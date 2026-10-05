//! The extraction state machine. It runs entirely inside libpinmame's audio callback,
//! i.e. on the emulation thread, once per emulated video frame: the callback is both
//! our clock (the mixed samples of that frame) and the place where we touch the
//! emulated hardware (halt lines, sound board latches) between two frames, exactly
//! where PinMAME's own sound commander does it.
//!
//! Four passes: every command once; then every command that played nothing once more
//! (`retried`); then the files far louder than the ROM's median are played again at
//! another master volume, to find the ones that ignore it (`ignores_master_volume`); then,
//! when the files are recorded at the reference volume, a few of them are played again at
//! the game's factory volume, to measure the offset between the two (`factory_offset`).
//!
//! In the first two passes, a sound that keeps playing is analysed as it records, until one
//! exact cycle of its loop is confirmed (`looping`, and on DCS the track program's own period
//! from `dcsrom::track_run`); the file is then its intro and one loop body.

use std::collections::VecDeque;
use std::ffi::c_int;
use std::path::PathBuf;

use serde::Serialize;

use crate::loudness::{self, Aggregate, FileLoudness};
use crate::soundsdat::{Entry, SoundsDat};
use crate::volume::{self, VolumeCmd};
use crate::{dcsrom, ffi, looping};

/// A sample within this many LSB of the idle level is digital silence: the upstream mixer
/// adds +/-1 LSB TPDF dither.
const SILENCE: i16 = 2;
/// PinMAME's commander sends one manual command every 4th frame (snd_cmd.c `playCmd`).
const FRAMES_PER_SEND: u32 = 4;
/// DCS boards get one byte per frame instead, closer to the game, which sends both bytes
/// of a command within a millisecond. The DCS firmware drops the first byte when the
/// second comes 13 main-loop passes (13 x 7.68 ms, about 100 ms) after it (mjrgh's
/// DCSExplorer, `dataPortTimeout`): with 6 frames (100 ms) between the bytes every
/// command of rs_l6 was lost; 4 frames (67 ms) is within that limit.
///
/// At any pacing, about one command in 200 (rs_l6) plays nothing on its first try and
/// plays normally later, depending on the boot's timing (the WPC clock comes from the
/// host's, so two boots differ by a frame or two): with 4 frames rs_l6 lost 060D, or 080E
/// and 08BE, depending on the run, with 1 frame 0247, 024E and 0803. The DSP did read
/// every byte (IRQ2 was never still pending at the next byte). Hence the retry pass.
const DCS_FRAMES_PER_SEND: u32 = 1;
/// End a recording after this much emulated silence.
const END_SILENCE_SECS: f64 = 2.0;
/// Loop search (main and retry passes): the recording is analysed this often (emulated
/// time), from `FIRST_LOOP_CHECK_SECS` on, and every `LATE_LOOP_CHECK_SECS` past a minute.
const FIRST_LOOP_CHECK_SECS: f64 = looping::CONFIRM_SECS + 2.0;
const LOOP_CHECK_SECS: f64 = 5.0;
const LATE_LOOP_CHECK_SECS: f64 = 10.0;
/// A DCS track whose program loops is recorded until the audio can confirm that loop
/// (`looping::HINT_CONFIRM_MAX_SECS`), up to this long, even past `loop_max_secs`.
const LOOP_HINT_MAX_SECS: f64 = 900.0;
/// DCS frame length: 240 samples at 31250 Hz.
const DCS_FRAME_SECS: f64 = 240.0 / 31250.0;
/// Between two commands: required silence, and how long we wait for it at most.
const QUIET_SECS: f64 = 0.5;
const QUIET_MAX_SECS: f64 = 10.0;
/// After a board reset the board reboots and may play its boot sound ~2 s later.
const QUIET_AFTER_RESET_SECS: f64 = 4.0;
/// Stop commands for families that have no section in sounds.dat, measured on whirl_l3
/// (System 11B, one WMSS11 + one WMSS11C board):
/// - WMSS11: the game sends 00 at power-up; 00 cuts a looping sound within 0.6 s.
/// - WMSS11C: 00 does nothing. A sweep of all 256 bytes against the looping 0x22 found 20
///   (and 93/94/98/9E) silent within 0.5 s; 20 stops all 22 looping commands of the board.
///   On S11_SNDOVERLAY games the game never sends 00-1F to this board (s11.c `pia5cb2_w`
///   routes them to the solenoid overlay), so 20 is the lowest byte the board really gets.
/// - WMSS11J: unmeasured guess.
///
/// If a stop does not work, the boards are reset (see `Quiet`), so a wrong guess costs
/// time, not a contaminated file.
/// - BSMT (Data East / Sega / Stern Whitestar): 00, which the game sends at power-up and
///   which altsound treats as "stop music" on Whitestar (snd_alt.cpp `postprocess_commands`).
const BUILTIN_STOPS: &[(&str, &[u8])] = &[
    ("WMSS11", &[0x00]),
    ("WMSS11C", &[0x20]),
    ("WMSS11J", &[0x00]),
    ("BSMT", &[0x00]),
];
/// WPCS bytes that change the board's state instead of playing a sound (sounds.dat `wpcs:`):
/// tempo, DAC and FM volumes, the master volume prefix 79 and the 16-bit prefix 7A.
const WPCS_STATE: &[std::ops::RangeInclusive<u8>] = &[0x1E..=0x2F, 0x60..=0x72, 0x79..=0x7A];
/// DCS sweep range when the ROM's track catalog cannot be read.
const DCS_FALLBACK_LAST: u16 = 0x03FF;
/// Whitestar/Data East bytes FC..FF start a two-byte command (`FE xx FD` is the volume), so
/// the sweep stops at FB. Probed: `FF xx` plays the same sound as `xx` (apollo13, xfiles;
/// after every silent `xx` of four full sweeps, `FF xx` was silent too), and `FC xx` starts
/// a loop for every `xx` on apollo13, gnr_300 and xfiles alike.
const BSMT_LAST: u8 = 0xFB;
/// Boot ends once no game sound byte has arrived for this long (after `boot_secs`).
const BOOT_QUIET_SECS: f64 = 3.0;
/// Recordings shorter than this after trimming are blips: counted, not written.
const BLIP_SECS: f64 = 0.020;
/// Boards whose manual-command handler exists but does nothing (src/wpc/sam.c `man3_w`).
const NOOP_MANCMD: &[&str] = &["SAM"];
/// Boards reset by a write to their control port (the game's own reset path, which also
/// reboots the DSP on DCS), instead of a plain CPU reset line.
const CTRL_RESET: &[&str] = &["DCS", "WPCS"];
/// After halting the game CPUs, let any half-sent command expire (DCS drops it after 100 ms).
const SETTLE_SECS: f64 = 0.5;
/// High-pass corner of the DC blocker used for the levels (and `--dc-block`), in Hz.
const DC_BLOCK_HZ: f64 = 10.0;
/// End trimming: a sound that ends on a held DC level (Data East and Whitestar BSMT boards
/// step between flat offsets for seconds after some sounds: gnr_300 `67`, xfiles `69`)
/// ends where the first held level starts. A held level is flat for at least
/// `DC_HOLD_MIN_SECS`; a step between two levels is a burst of moving samples (gaps under
/// `DC_STEP_GAP_SECS`) no longer than `DC_STEP_MAX_SECS` (measured: 1 to 6.5 ms; gnr_300 `67`
/// rings back to 0 in two parts 4.6 ms apart).
const DC_HOLD_MIN_SECS: f64 = 0.040;
const DC_STEP_MAX_SECS: f64 = 0.010;
const DC_STEP_GAP_SECS: f64 = 0.010;
/// A held level drifts slightly (gnr_300 `67`, xfiles `69`: spans of 3 to 8 LSB over
/// 0.1 to 1.6 s); it stays within this span, in LSB (-66 dBFS).
const DC_HOLD_MAX_SPAN: i32 = 16;
/// A held level is the upstream mixer's dither around a constant: its standard deviation,
/// in LSB, stays at about 0.5 (gnr_300 `67`, xfiles `56` and `69`: 0.50 to 0.53). A quiet
/// decaying tail also fits in `DC_HOLD_MAX_SPAN` but moves more (apollo13 `76`: sd 2.0 to
/// 3.7 around 0, about -74 dBFS), and is sound, not a held level.
const DC_HOLD_MAX_SD: f64 = 1.0;
/// Master volume check: files louder than the ROM's median file by more than this (15 LU
/// missed xfiles' click `1E`, 9.3 LU above; 5 LU plays 3 to 19 files again per ROM) are
/// played again at another master volume...
const VOLUME_CHECK_ABOVE_MEDIAN_LU: f64 = 5.0;
/// ...that many levels away from the current one (about 10 dB on DCS, 1.3 dB per level)...
const VOLUME_CHECK_LEVELS: u32 = 8;
/// ...together with a reference, the non-loop file closest to the median, on the same board.
/// A file is flagged `ignores_master_volume` when its level moved differently from the
/// reference's by more than half the reference's move (and at least this much). Measured:
/// cv_20h's click `03DE` did not move at all (+0.0 LU for +8 levels); apollo13's ADPCM
/// sound `5F` moves the wrong way and not monotonically (-8.7 LUFS at level 0, -11.4 at 3,
/// -39.9 at 11, -31.6 at 23, -15.8 at 31), where its other sounds follow the volume.
const VOLUME_CHECK_MIN_DELTA_LU: f64 = 3.0;
/// The check needs this many written non-loop files: with fewer, the median is the loud
/// file itself or close to it, and there is no other file to serve as the reference.
const VOLUME_CHECK_MIN_FILES: usize = 3;
/// DCS master volume levels: `vv = level * 8 + 7`, level 8..31 (`47`..`FF`).
const DCS_LEVELS: std::ops::RangeInclusive<u32> = 8..=31;

pub struct Options {
    pub out_dir: PathBuf,
    pub rom: String,
    pub parent: Option<String>,
    pub only: Option<Vec<String>>,
    pub limit: Option<usize>,
    pub volume: VolumeInit,
    pub boot_secs: f64,
    pub boot_max_secs: f64,
    /// Longest file of a sound whose loop is not found (its first `max_secs`).
    pub max_secs: f64,
    /// How long a sound that has not ended is recorded while looking for its loop (0: no
    /// loop search, the recording stops at `max_secs`).
    pub loop_max_secs: f64,
    /// End a recording when nothing was heard this long after the last command byte.
    pub no_sound_secs: f64,
    /// Only boot (no halt, no extraction): the cold boot of `--factory`.
    pub cold_boot_only: bool,
    /// Factory mode: the cold boot's report, stored in the manifest.
    pub factory: Option<serde_json::Value>,
    /// Explicit stop command (same notation as `only`), overriding the family's.
    pub stop: Option<String>,
    /// Write DC-blocked audio instead of the raw emulated output.
    pub dc_block: bool,
    pub verbose: bool,
}

/// Which master volume the sound boards play at.
#[derive(Clone, Copy, Debug)]
pub enum VolumeInit {
    /// Send `55 AA vv ~vv` to every DCS board once booted (and again after a board reset).
    Dcs(u8),
    /// Keep what the game itself sent at boot; after a board reset (which loses it), the
    /// game's own last volume command is sent again, byte for byte.
    Game,
    /// The default of `--factory`: record every board at its reference volume, the loudest
    /// master volume that does not clip in emulation (DCS `55 AA vv ~vv` with this `vv`,
    /// Whitestar `FE xx FD` with this `xx`, re-sent before every command as the game
    /// re-sends its own).
    /// The game's own (factory) volume is only reported, and the dB offset between the two
    /// is measured on a few files played again at the factory volume (`factory_offset`).
    /// Boards without a master volume (Data East, System 11) are unchanged.
    Reference { dcs: u8, whitestar: u8 },
}

/// The DCS master volume a board keeps when the game sends none: its reset default.
const DCS_RESET_DEFAULT: u8 = 0x67;
/// Factory offset: this many non-loop files per board are played again at the factory
/// volume (the loudest at or below `VOLUME_CHECK_ABOVE_MEDIAN_LU` over the median, at least
/// `OFFSET_MIN_SECS` long, not clipped), and the offset is the median of their level moves.
const OFFSET_FILES: usize = 5;
const OFFSET_MIN_SECS: f64 = 1.0;

/// One action for the sound board, paced like PinMAME's commander.
#[derive(Clone, Copy, Debug)]
enum Send {
    /// `sndbrd_manCmd(board, byte)`: the commander's entry point.
    Byte(c_int, c_int),
    /// `sndbrd_data_w(board, byte)`: the game CPU's own entry point, for single bytes on
    /// boards whose manual handler only takes pairs (WPCS).
    Data(c_int, c_int),
    /// Reset every sound board (control-port reset or audio CPU reset line).
    Reset,
    /// Scenario (`--only 0x000C+2.5+0x0390`): wait this many seconds (in milliseconds)
    /// before the next send, keeping the recording open.
    Wait(u32),
}

/// What the commands being played are for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    /// Every command once.
    Main,
    /// The commands that played nothing, once more.
    Retry,
    /// The loudest files again, at another master volume.
    VolumeCheck,
    /// A few files again at the game's factory volume (`VolumeInit::Reference` only).
    FactoryOffset,
}

#[derive(Clone)]
struct Cmd {
    id: String,
    name: String,
    /// The board's type string, and its number.
    board: String,
    board_no: c_int,
    sends: Vec<Send>,
    /// Retry and volume check: the index of the result this command updates.
    slot: Option<usize>,
    /// Volume check: the other master volume, sent before the command.
    alt: Option<AltVolume>,
}

/// Another master volume for the volume check.
#[derive(Clone)]
struct AltVolume {
    sends: Vec<Send>,
    /// The command bytes, hex.
    bytes: String,
    /// Levels away from the current master volume.
    levels: i32,
    /// This is the reference file of its board, not a suspect.
    reference: bool,
}

struct Recording {
    cmd: Cmd,
    samples: Vec<i16>,
    frames: u64,
    first_loud: Option<u64>,
    last_loud_end: u64,
    /// Per-channel idle level when the command was sent (start state of the DC blocker).
    start_idle: Vec<i32>,
    /// False if the previous sound was still playing when this command was sent.
    clean_start: bool,
    /// `frames` when the last command byte went out.
    drained_at: Option<u64>,
    /// Main and retry passes: looking for the loop of a sound that does not end.
    search: Option<LoopSearch>,
    /// The loop found, which ended the recording.
    found: Option<LoopFound>,
    /// Why a recording that reached the cap has no loop.
    unresolved: Option<String>,
}

/// Looking for one cycle of a sound that keeps playing.
struct LoopSearch {
    /// DCS: what the track program does, and the period it predicts.
    dcs: Option<DcsTrack>,
    hint: Option<Hint>,
    /// Recording frames (since the command) of the next analysis, and the last one.
    next_check: u64,
    cap: u64,
}

/// A period from the DCS track program, to check against the audio.
struct Hint {
    /// In output samples, with its fraction.
    period: f64,
    /// Recording frames by which the audio can confirm it.
    ready_at: u64,
}

struct LoopFound {
    l: looping::Loop,
    method: &'static str,
    note: Option<String>,
}

/// What a DCS track's program does (`dcsrom::track_run`), from a silent board.
#[derive(Clone, Serialize)]
pub struct DcsTrack {
    /// "loops", "ends" or "unknown".
    pub program: &'static str,
    /// Loops: frames (7.68 ms) before the board state first repeats, and the period.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intro_frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub period_frames: Option<u32>,
    /// Ends: after this many frames.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frames: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One cycle of a looping sound, as written.
#[derive(Clone, Serialize)]
pub struct LoopInfo {
    /// The file holds `intro_samples` then one loop body of `period_samples` (sample
    /// frames); the body's last frame joins its first.
    pub intro_samples: usize,
    pub period_samples: usize,
    /// The period with its fraction of a sample (the cycles are that far apart; the body
    /// is rounded to whole samples).
    pub period_exact_samples: f64,
    pub period_secs: f64,
    /// Cycles in the body: more than 1 for a short loop whose cycle is not a whole number
    /// of samples (the body is then that many cycles, which is; `period_*` are the body's).
    pub cycles: u32,
    /// "dcs-catalog": the period of the DCS track program (`dcs_track`), confirmed on the
    /// audio. "audio": found in the audio alone (`looping`).
    pub method: &'static str,
    /// 1 minus the worst window's residual-to-signal ratio when one cycle is compared with
    /// the next (`residual_db`): 0.999 at -60 dB, 0.968 at -30 dB (the acceptance limit),
    /// lower only for a near-silent loop judged within the dither.
    pub confidence: f64,
    pub residual_db: f64,
    /// How long the repetition was checked over.
    pub verified_secs: f64,
    /// The body alone, written when there is an intro.
    pub loop_file: Option<String>,
    pub seam: SeamInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dcs_track: Option<DcsTrack>,
    /// dcs-catalog: the period the audio alone gives on the same recording (a cross-check;
    /// null when the recording holds too few cycles for it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_period_samples: Option<Option<usize>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The joint of a loop body (`looping::Seam`), in LSB of the written samples.
#[derive(Clone, Serialize)]
pub struct SeamInfo {
    /// The step played at the joint (last frame of the body to its first).
    pub joint_step: i32,
    /// The step the recording makes at that point (last frame of the body to the next).
    pub natural_step: i32,
    /// The difference between the two: the discontinuity the loop adds.
    pub error: i32,
    pub body_p99_step: i32,
    pub body_max_step: i32,
}

/// Waiting for silence between two commands.
struct Quiet {
    since: u64,
    deadline: u64,
    /// When the last queued byte went out; silence is only counted after that.
    drained_at: Option<u64>,
    need: u64,
    /// This wait follows a board reset: a further failure is not retried.
    after_reset: bool,
    /// Send the volume once quiet (initially for `VolumeInit::Dcs` and `Reference`, and after a reset,
    /// which loses it).
    then_volume: bool,
    /// Send what goes before the next command (volume refresh, retry and check volumes)
    /// once quiet.
    then_pre: bool,
}

enum Phase {
    Boot,
    Settle { until: u64 },
    Quiet(Quiet),
    Record(Box<Recording>),
    Done,
}

#[derive(Serialize)]
pub struct SoundInfo {
    pub id: String,
    pub name: String,
    pub file: Option<String>,
    pub duration: f64,
    /// Shorter than 20 ms after trimming: not written.
    pub blip: bool,
    /// EBU R128 integrated loudness and true peak of the written file, with a mono stream
    /// measured as two identical channels (VPX's playback), on the DC-blocked signal.
    /// Null when not measurable.
    pub lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    /// `lufs`, or for a file under 400 ms the loudness of the file padded with silence to
    /// 400 ms: the level used to compare files with each other.
    pub level_lufs: Option<f64>,
    /// Peak and rms of the DC-blocked signal (what an AC-coupled output plays).
    pub peak_dbfs: Option<f64>,
    pub rms_dbfs: Option<f64>,
    /// Raw samples at full scale (+32767 / -32768): the emulated output clipped.
    pub clipped_samples: usize,
    /// Mean of the raw samples, in LSB (System 11 DACs idle far from 0).
    pub dc_offset: i32,
    pub ended_by: &'static str,
    /// The sound does not end by itself: `ended_by` "loop" (one cycle found, `loop`) or
    /// "max" (cut at `max_secs`, `loop_unresolved` says why).
    pub looping_or_truncated: bool,
    #[serde(rename = "loop", skip_serializing_if = "Option::is_none")]
    pub loop_info: Option<LoopInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_unresolved: Option<String>,
    /// False if the previous sound could not be stopped: the file may contain it.
    pub clean_start: bool,
    /// The first try played nothing, so the command was played once more (after a stop and
    /// the volume); the fields are those of the second try.
    pub retried: bool,
    /// The file was played again at another master volume (see `master_volume_check`) and
    /// its level did not follow it the way the reference file's did (it did not move, or
    /// moved the wrong way): it is left out of the ROM's loudness totals.
    pub ignores_master_volume: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_volume_check: Option<VolumeCheck>,
    pub board: String,
    pub volume_init: Option<String>,
    /// Idle (DC) level of the output when the recording ended, in LSB.
    pub idle_level: i32,
    /// Emulated time from the first command byte to the first non-silent sample.
    pub onset: Option<f64>,
}

/// A file played again at another master volume.
#[derive(Clone, Serialize)]
pub struct VolumeCheck {
    /// The master volume command sent before it, hex.
    pub replayed_at: String,
    pub levels_away: i32,
    pub level_lufs: Option<f64>,
    /// Its `level_lufs` minus the file's.
    pub delta_lu: Option<f64>,
    /// The reference file played at the same volume, and how much its level moved; null
    /// on the reference itself.
    pub reference: Option<String>,
    pub reference_delta_lu: Option<f64>,
}

/// One file played again at the factory volume.
#[derive(Clone, Serialize)]
pub struct OffsetSample {
    pub id: String,
    /// `level_lufs` at the reference volume (the written file) and at the factory volume.
    pub reference_lufs: f64,
    pub factory_lufs: Option<f64>,
    /// Factory minus reference, in dB.
    pub delta_db: Option<f64>,
}

/// The dB offset between the reference volume (what the files are recorded at) and the
/// game's factory volume, for one board.
#[derive(Clone, Serialize)]
pub struct BoardOffset {
    pub board: c_int,
    pub family: String,
    /// The master volume commands, hex.
    pub reference_volume: String,
    pub factory_volume: String,
    /// Where the factory volume comes from: the game's own command at boot, or the board's
    /// reset default when the game sent none.
    pub factory_volume_from: String,
    /// Median of the samples' `delta_db` (negative: the factory volume is quieter).
    pub factory_offset_db: Option<f64>,
    pub spread_db: Option<f64>,
    pub samples: Vec<OffsetSample>,
}

/// What the files would measure at the factory volume: the reference totals shifted by
/// `factory_offset_db` (true peaks too).
#[derive(Serialize)]
pub struct AsShipped {
    pub factory_offset_db: f64,
    pub all_lufs: Option<f64>,
    pub excluding_loops_lufs: Option<f64>,
    pub median_file_lufs: Option<f64>,
    pub loudest_true_peak_dbtp: Option<f64>,
}

/// The volume the game set at boot, as found in its command stream.
#[derive(Clone, Serialize)]
pub struct VolumeReport {
    pub seen: bool,
    /// The last volume command per board and kind (master, DCS channel, Data East music).
    pub commands: Vec<VolumeCmd>,
    /// Why there is no master volume, when there is none.
    pub note: Option<String>,
}

/// Every byte the game sent to one board during boot, as `seconds:byte`.
#[derive(Serialize)]
pub struct BootLog {
    pub board: c_int,
    pub family: String,
    pub count: usize,
    pub bytes: String,
}

#[derive(Serialize)]
pub struct BootReport {
    pub secs: f64,
    /// After `boot_secs` (and, on DCS, once the game's volume was seen): "quiet" when no
    /// byte arrived for 3 s, "repeats" when the last 3 s only repeated byte pairs already
    /// sent earlier in the boot (afm_113b polls `03 D3`, a silent track, three times a
    /// second forever); "max" when `boot_max_secs` was reached first.
    pub ended_by: &'static str,
    pub bytes: usize,
    pub log: Vec<BootLog>,
}

#[derive(Serialize, Default)]
pub struct Counts {
    pub tried: usize,
    pub with_sound: usize,
    pub written: usize,
    pub blips: usize,
    pub no_sound: usize,
    pub loops: usize,
    /// Loops written as one exact cycle, by method, and loops cut at `max_secs`.
    pub loops_exact_dcs_catalog: usize,
    pub loops_exact_audio: usize,
    pub loops_unresolved: usize,
    pub not_clean: usize,
    /// Written files with clipped samples.
    pub clipped: usize,
    /// Commands played a second time because the first try played nothing, and how many
    /// of them played something then.
    pub retried: usize,
    pub recovered_by_retry: usize,
    pub ignores_master_volume: usize,
}

#[derive(Serialize)]
pub struct LoudnessReport {
    pub measured_as: &'static str,
    /// All written files, without those flagged `ignores_master_volume`.
    pub all: Aggregate,
    /// The same without the loops (`looping_or_truncated`).
    pub excluding_loops: Aggregate,
    /// Median of the per-file `lufs` of the files in `excluding_loops`.
    pub median_file_lufs: Option<f64>,
    /// Left out of the totals: they ignore the master volume.
    pub excluded_ignoring_master_volume: Vec<String>,
    /// What the master volume check did, or why it did not run.
    pub master_volume_check: Option<String>,
    /// Reference mode: the totals above shifted by `factory_offset_db`, i.e. at the volume
    /// the game ships with. Null when the offset could not be measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_shipped: Option<AsShipped>,
}

#[derive(Serialize)]
struct Manifest<'a> {
    rom: &'a str,
    parent: Option<&'a str>,
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    factory: Option<&'a serde_json::Value>,
    boards: &'a [String],
    sample_rate: u32,
    channels: usize,
    boot: BootReport,
    /// In factory mode: the volume the game itself set on the warm boot.
    #[serde(skip_serializing_if = "Option::is_none")]
    factory_volume: Option<VolumeReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_volume: Option<VolumeReport>,
    volume_init: Option<String>,
    /// Reference mode: what the levels below are relative to, and how to get them as shipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    levels_note: Option<&'static str>,
    /// Reference mode: the factory level minus the reference level, in dB (negative), for
    /// the ROM; 0 when no board has a master volume (the files are at the only level there
    /// is). Null when it could not be measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    factory_offset_db: Option<Option<f64>>,
    /// Reference mode: the offset per board with a master volume, and how it was measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    factory_offset: Option<FactoryOffsetReport<'a>>,
    /// The longest recording, and what happens to the sounds that reach it.
    recording_cap: RecordingCap,
    /// How many times a volume was sent again (after a reset, before a retry, or before
    /// each command).
    volume_replays: u32,
    /// Sent before every command, outside the recording, so that no command's volume
    /// change leaks into the next.
    refreshed_before_each_command: &'a [String],
    commands_from: &'a [String],
    counts: Counts,
    loudness: LoudnessReport,
    stop: String,
    board_resets: u32,
    dc_blocked_wav: bool,
    sounds: &'a [SoundInfo],
}

#[derive(Serialize)]
struct FactoryOffsetReport<'a> {
    method: String,
    boards: &'a [BoardOffset],
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Serialize)]
struct RecordingCap {
    max_secs: f64,
    loop_max_secs: f64,
    loop_hint_max_secs: f64,
    note: &'static str,
}

pub struct Extractor {
    opts: Options,
    dat: SoundsDat,
    pub rate: u32,
    pub channels: usize,
    /// Emulated time, in sample frames since the first audio callback.
    pub t: u64,
    silent_run: u64,
    /// Per-channel idle (DC) level, see `track_idle_level`.
    idle: Vec<i32>,
    phase: Phase,
    pass: Pass,
    sender: VecDeque<Send>,
    cooldown: u32,
    /// The end of the `Send::Wait` at the front of `sender`.
    wait_until: Option<u64>,
    queue: VecDeque<Cmd>,
    /// The main pass's commands, by result index.
    main_cmds: Vec<Cmd>,
    mask: u8,
    boards: Vec<String>,
    stop: Vec<Send>,
    /// Sent before every command (`refreshed_before_each_command`).
    refresh: Vec<Send>,
    refresh_labels: Vec<String>,
    /// The previous sound did not stop, even after a reset.
    dirty: bool,
    pub board_resets: u32,
    /// Sound commands written by the game CPU during boot, as (time, board, byte).
    boot_log: Vec<(u64, c_int, u8)>,
    /// (board, previous byte, byte) seen during boot, and when the last unseen one arrived.
    boot_pairs: std::collections::HashSet<(c_int, Option<u8>, u8)>,
    last_novel: u64,
    /// The last few bytes per board, for recognizing volume commands.
    recent: [Vec<u8>; 2],
    /// Every volume command the game sent during boot.
    volumes: Vec<VolumeCmd>,
    pub boot_end: Option<u64>,
    boot_ended_by: &'static str,
    /// Each board's type string, captured at the end of the boot.
    families: [String; 2],
    /// Data East game: its BSMT board takes 20..2F as a music volume.
    data_east: bool,
    volume_sent: Option<String>,
    /// Our own volume (`VolumeInit::Dcs` or `Reference`) went out once.
    own_volume_sent: bool,
    volume_replays: u32,
    /// How the command list was made, per board.
    commands_from: Vec<String>,
    volume_check_note: Option<String>,
    /// Volume check references: (board, id, how much its level moved).
    check_refs: Vec<(c_int, String, Option<f64>)>,
    /// Reference mode: the factory offset per board (filled by the `FactoryOffset` pass).
    offsets: Vec<BoardOffset>,
    offset_note: Option<String>,
    pub results: Vec<SoundInfo>,
    /// The loudness of each written file, by result index.
    loud: Vec<Option<FileLoudness>>,
    pub done: bool,
    pub error: Option<String>,
}

/// What a recording contains, once trimmed.
struct Analysis {
    /// Trimmed raw samples, interleaved.
    raw: Vec<i16>,
    /// The same, DC-blocked.
    blocked: Vec<f64>,
    peak: Option<f64>,
    rms: Option<f64>,
    clipped_samples: usize,
    dc_offset: i32,
    duration: f64,
    blip: bool,
    /// A loop: the body's frames within `raw`.
    body: Option<std::ops::Range<usize>>,
}

impl Extractor {
    pub fn new(opts: Options, dat: SoundsDat) -> Self {
        Self {
            opts,
            dat,
            rate: 0,
            channels: 0,
            t: 0,
            silent_run: 0,
            idle: Vec::new(),
            phase: Phase::Boot,
            pass: Pass::Main,
            sender: VecDeque::new(),
            cooldown: 0,
            wait_until: None,
            queue: VecDeque::new(),
            main_cmds: Vec::new(),
            mask: 0,
            boards: Vec::new(),
            stop: Vec::new(),
            refresh: Vec::new(),
            refresh_labels: Vec::new(),
            dirty: false,
            board_resets: 0,
            boot_log: Vec::new(),
            boot_pairs: Default::default(),
            last_novel: 0,
            recent: [Vec::new(), Vec::new()],
            volumes: Vec::new(),
            boot_end: None,
            boot_ended_by: "",
            families: Default::default(),
            data_east: false,
            volume_sent: None,
            own_volume_sent: false,
            volume_replays: 0,
            commands_from: Vec::new(),
            volume_check_note: None,
            check_refs: Vec::new(),
            offsets: Vec::new(),
            offset_note: None,
            results: Vec::new(),
            loud: Vec::new(),
            done: false,
            error: None,
        }
    }

    fn secs(&self, s: f64) -> u64 {
        (s * self.rate as f64) as u64
    }

    fn t_secs(&self, t: u64) -> f64 {
        t as f64 / self.rate.max(1) as f64
    }

    /// Called by libpinmame for every sound command the game CPU sends (on the emulation
    /// thread, from the CPU's write). Only the boot is logged: afterwards the game CPUs are
    /// halted and the bytes seen here are the tool's own `sndbrd_data_w` calls.
    pub fn on_game_command(&mut self, board: c_int, cmd: c_int) {
        if !matches!(self.phase, Phase::Boot) {
            return;
        }
        let byte = cmd as u8;
        self.boot_log.push((self.t, board, byte));
        let prev = self.recent[(board & 1) as usize].last().copied();
        if self.boot_pairs.insert((board, prev, byte)) {
            self.last_novel = self.t;
        }
        let recent = &mut self.recent[(board & 1) as usize];
        recent.push(byte);
        if recent.len() > 8 {
            recent.remove(0);
        }
        let family = board_typestr(board).unwrap_or_default();
        let at = self.t as f64 / self.rate.max(1) as f64;
        let v = if family == "BSMT" && ffi::is_data_east() {
            volume::decode_de_music(board, byte, at)
        } else {
            volume::decode(&family, board, recent, at)
        };
        self.volumes.extend(v);
    }

    /// The game's last volume command per board and kind (master first), in that order.
    fn last_volumes(&self) -> Vec<VolumeCmd> {
        let mut last: Vec<VolumeCmd> = Vec::new();
        for v in self.volumes.iter().rev() {
            if !last.iter().any(|l| l.board == v.board && l.kind == v.kind) {
                last.push(v.clone());
            }
        }
        last.sort_by_key(|v| (v.board, v.kind != "master", v.kind.clone()));
        last
    }

    fn last_master(&self, board: c_int) -> Option<VolumeCmd> {
        self.last_volumes()
            .into_iter()
            .find(|v| v.board == board && v.kind == "master")
    }

    /// The board's family as reported: the type string, plus "(Data East)" for the BSMT
    /// board of a Data East game.
    fn family_label(&self, board: c_int) -> String {
        let f = &self.families[board as usize];
        if self.is_de_board(board) {
            format!("{f} (Data East)")
        } else {
            f.clone()
        }
    }

    fn is_de_board(&self, board: c_int) -> bool {
        self.data_east && self.families[board as usize] == "BSMT"
    }

    pub fn volume_report(&self) -> VolumeReport {
        let commands = self.last_volumes();
        let seen = commands.iter().any(|v| v.kind == "master");
        let note = (!seen).then(|| {
            self.board_list()
                .map(|b| {
                    let f = self.family_label(b);
                    let mut reason = volume::none_reason(&f).to_string();
                    if self.is_de_board(b) && !commands.iter().any(|v| v.board == b) {
                        reason += "; no 20..2F music volume at boot (the board's default is 20)";
                    }
                    format!("board {b} ({f}): {reason}")
                })
                .collect::<Vec<_>>()
                .join("; ")
        });
        VolumeReport {
            seen,
            commands,
            note,
        }
    }

    pub fn boot_report(&self) -> BootReport {
        let log = (0..2)
            .filter_map(|b| {
                let bytes: Vec<String> = self
                    .boot_log
                    .iter()
                    .filter(|e| e.1 == b)
                    .map(|&(t, _, v)| format!("{:.3}:{v:02X}", self.t_secs(t)))
                    .collect();
                (!bytes.is_empty()).then(|| BootLog {
                    board: b,
                    family: self.families[b as usize].clone(),
                    count: bytes.len(),
                    bytes: bytes.join(" "),
                })
            })
            .collect();
        BootReport {
            secs: round3(self.t_secs(self.boot_end.unwrap_or(self.t))),
            ended_by: self.boot_ended_by,
            bytes: self.boot_log.len(),
            log,
        }
    }

    pub fn counts(&self) -> Counts {
        let r = &self.results;
        let written = || r.iter().filter(|s| s.file.is_some());
        Counts {
            tried: r.len(),
            with_sound: r.iter().filter(|s| s.onset.is_some()).count(),
            written: written().count(),
            blips: r.iter().filter(|s| s.blip).count(),
            no_sound: r.iter().filter(|s| s.ended_by == "no_sound").count(),
            loops: written().filter(|s| s.looping_or_truncated).count(),
            loops_exact_dcs_catalog: written()
                .filter(|s| {
                    s.loop_info
                        .as_ref()
                        .is_some_and(|l| l.method == "dcs-catalog")
                })
                .count(),
            loops_exact_audio: written()
                .filter(|s| s.loop_info.as_ref().is_some_and(|l| l.method == "audio"))
                .count(),
            loops_unresolved: written().filter(|s| s.loop_unresolved.is_some()).count(),
            not_clean: r.iter().filter(|s| !s.clean_start).count(),
            clipped: written().filter(|s| s.clipped_samples > 0).count(),
            retried: r.iter().filter(|s| s.retried).count(),
            recovered_by_retry: r.iter().filter(|s| s.retried && s.onset.is_some()).count(),
            ignores_master_volume: r.iter().filter(|s| s.ignores_master_volume).count(),
        }
    }

    /// Indices of the written files that count in the loudness totals.
    fn counted(&self, with_loops: bool) -> impl Iterator<Item = usize> + '_ {
        (0..self.results.len()).filter(move |&i| {
            let s = &self.results[i];
            s.file.is_some()
                && self.loud[i].is_some()
                && !s.ignores_master_volume
                && (with_loops || !s.looping_or_truncated)
        })
    }

    /// The median per-file loudness of the written files that are not loops.
    fn median_lufs(&self) -> Option<f64> {
        loudness::median(
            self.counted(false)
                .filter_map(|i| self.results[i].lufs)
                .collect(),
        )
    }

    pub fn loudness_report(&self) -> LoudnessReport {
        let rate = self.rate;
        let total = |with_loops| {
            loudness::aggregate(
                self.counted(with_loops)
                    .filter_map(|i| self.loud[i].as_ref()),
                rate,
            )
        };
        let mut r = LoudnessReport {
            measured_as: if self.channels == 1 {
                "DC-blocked mono ROM stream duplicated to 2 identical channels (VPX playback)"
            } else {
                "DC-blocked stereo ROM stream"
            },
            all: total(true),
            excluding_loops: total(false),
            median_file_lufs: self.median_lufs(),
            excluded_ignoring_master_volume: self
                .results
                .iter()
                .filter(|s| s.ignores_master_volume)
                .map(|s| s.id.clone())
                .collect(),
            master_volume_check: self.volume_check_note.clone(),
            as_shipped: None,
        };
        if self.is_reference()
            && let Some(o) = self.rom_offset()
        {
            let shift = |x: Option<f64>| x.map(|v| round3(v + o));
            r.as_shipped = Some(AsShipped {
                factory_offset_db: o,
                all_lufs: shift(r.all.lufs),
                excluding_loops_lufs: shift(r.excluding_loops.lufs),
                median_file_lufs: shift(r.median_file_lufs),
                loudest_true_peak_dbtp: shift(r.all.true_peak_dbtp),
            });
        }
        r
    }

    pub fn is_factory(&self) -> bool {
        self.opts.factory.is_some()
    }

    pub fn commands_from(&self) -> &[String] {
        &self.commands_from
    }

    pub fn offsets(&self) -> &[BoardOffset] {
        &self.offsets
    }

    pub fn offset_note(&self) -> Option<&str> {
        self.offset_note.as_deref()
    }

    pub fn refreshed(&self) -> &[String] {
        &self.refresh_labels
    }

    /// One emulated frame of mixed audio (interleaved, `channels` per frame).
    pub fn on_audio(&mut self, buf: &[i16]) {
        let ch = self.channels.max(1);
        let frames = (buf.len() / ch) as u64;
        self.track_idle_level(buf, ch);
        for frame in buf.chunks_exact(ch) {
            let loud = frame
                .iter()
                .zip(&self.idle)
                .any(|(&s, &dc)| (s as i32 - dc).abs() > SILENCE as i32);
            self.silent_run = if loud { 0 } else { self.silent_run + 1 };
            if let Phase::Record(rec) = &mut self.phase {
                rec.samples.extend_from_slice(frame);
                rec.frames += 1;
                if loud {
                    rec.first_loud.get_or_insert(rec.frames - 1);
                    rec.last_loud_end = rec.frames;
                }
            }
        }
        self.t += frames;
        self.step();
        self.tick_sender();
    }

    /// Some boards idle at a constant non-zero level (whirl_l3 sits at +2056 after its boot
    /// sound). Silence is "within the dither of the idle level": a whole frame whose span stays
    /// within the dither is silent and updates the per-channel idle level.
    fn track_idle_level(&mut self, buf: &[i16], ch: usize) {
        self.idle.resize(ch, 0);
        for c in 0..ch {
            let it = || buf.iter().skip(c).step_by(ch).map(|&s| s as i32);
            let (Some(min), Some(max)) = (it().min(), it().max()) else {
                continue;
            };
            if max - min <= 2 * SILENCE as i32 {
                self.idle[c] = (min + max) / 2;
            }
        }
    }

    fn tick_sender(&mut self) {
        if self.cooldown > 0 {
            self.cooldown -= 1;
            return;
        }
        if let Some(&Send::Wait(ms)) = self.sender.front() {
            let until = *self
                .wait_until
                .get_or_insert(self.t + self.secs(f64::from(ms) / 1000.0));
            if self.t >= until {
                self.sender.pop_front();
                self.wait_until = None;
            }
            return;
        }
        if let Some(s) = self.sender.pop_front() {
            let board = match s {
                Send::Wait(_) => unreachable!(),
                Send::Byte(board, byte) => {
                    unsafe { ffi::sndbrd_manCmd(board, byte) };
                    Some(board)
                }
                Send::Data(board, byte) => {
                    unsafe { ffi::sndbrd_data_w(board, byte) };
                    Some(board)
                }
                Send::Reset => {
                    self.reset_boards();
                    None
                }
            };
            let dcs = board.is_some_and(|b| self.families[(b & 1) as usize] == "DCS");
            self.cooldown = if dcs {
                DCS_FRAMES_PER_SEND
            } else {
                FRAMES_PER_SEND
            } - 1;
        }
    }

    /// DCS and WPCS are reset through their control port, as the game does; the other
    /// boards by pulsing the reset line of the audio CPUs (once for all of them).
    fn reset_boards(&mut self) {
        self.board_resets += 1;
        let mut cpu_reset = false;
        for b in self.board_list() {
            if CTRL_RESET.contains(&board_typestr(b).unwrap_or_default().as_str()) {
                unsafe { ffi::sndbrd_ctrl_w(b, 0) };
            } else if !cpu_reset {
                unsafe { ffi::shim_reset_audio_cpus() };
                cpu_reset = true;
            }
        }
    }

    fn step(&mut self) {
        match &mut self.phase {
            Phase::Boot => {
                // Wait for the game to finish talking to its boards: at least `boot_secs`,
                // then until no byte for BOOT_QUIET_SECS and, on DCS, until the game's own
                // master volume was seen. Capped by `boot_max_secs`.
                let has_dcs = (0..2).any(|b| board_typestr(b).as_deref() == Some("DCS"));
                let last_byte = self.boot_log.last().map_or(0, |e| e.0);
                let quiet = self.t - last_byte.min(self.t) >= self.secs(BOOT_QUIET_SECS);
                let repeats = self.t - self.last_novel.min(self.t) >= self.secs(BOOT_QUIET_SECS);
                let dcs_volume = !has_dcs
                    || self
                        .volumes
                        .iter()
                        .any(|v| v.family == "DCS" && v.kind == "master");
                if self.t >= self.secs(self.opts.boot_secs) && repeats && dcs_volume {
                    self.end_boot(if quiet { "quiet" } else { "repeats" });
                } else if self.t >= self.secs(self.opts.boot_max_secs) {
                    self.end_boot("max");
                }
            }
            Phase::Settle { until } => {
                if self.t >= *until {
                    self.send_stop();
                }
            }
            Phase::Quiet(q) => {
                if self.sender.is_empty() && q.drained_at.is_none() {
                    q.drained_at = Some(self.t);
                }
                let quiet =
                    q.drained_at.is_some_and(|d| self.t - d >= q.need) && self.silent_run >= q.need;
                if quiet {
                    if self.opts.verbose {
                        eprintln!(
                            "  (quiet {:.2} s after the stop)",
                            (self.t - q.since) as f64 / self.rate as f64
                        );
                    }
                    let (then_volume, then_pre) = (q.then_volume, q.then_pre);
                    self.on_quiet(then_volume, then_pre);
                } else if self.t >= q.deadline {
                    if q.after_reset {
                        eprintln!(
                            "  warning: still not silent after a board reset; the next file is flagged clean_start=false"
                        );
                        self.dirty = true;
                        self.on_quiet(false, true);
                    } else {
                        eprintln!(
                            "  warning: the stop command did not silence the board within {QUIET_MAX_SECS} s: resetting the board(s)"
                        );
                        self.sender.clear();
                        self.sender.push_back(Send::Reset);
                        self.phase = Phase::Quiet(self.quiet(true));
                    }
                }
            }
            Phase::Record(rec) => {
                let end_silence = (END_SILENCE_SECS * self.rate as f64) as u64;
                let max_secs = (self.opts.max_secs * self.rate as f64) as u64;
                // A loop search records past `max_secs`; a replay of a loop file (volume
                // check, factory offset) is as long as the written file.
                let max = match (&rec.search, rec.cmd.slot) {
                    (Some(search), _) => search.cap,
                    (None, Some(i)) => match &self.results[i] {
                        s if s.loop_info.is_some() => {
                            ((s.onset.unwrap_or(0.0) + s.duration.max(BLIP_SECS))
                                * self.rate as f64) as u64
                        }
                        _ => max_secs,
                    },
                    _ => max_secs,
                };
                let no_sound = (self.opts.no_sound_secs * self.rate as f64) as u64;
                if rec.drained_at.is_none() && self.sender.is_empty() {
                    rec.drained_at = Some(rec.frames);
                }
                let silent_end =
                    self.sender.is_empty() && rec.frames - rec.last_loud_end >= end_silence;
                // The last check is at the cap itself, so that a search never ends between
                // two checks; a DCS program period is checked as soon as it can be.
                let check_due = !silent_end
                    && rec.first_loud.is_some()
                    && rec
                        .search
                        .as_ref()
                        .is_some_and(|s| rec.frames >= s.next_check || rec.frames >= s.cap);
                if check_due {
                    let (ch, rate) = (self.channels.max(1), self.rate);
                    rec.found = loop_check(rec, ch, rate).map(|f| {
                        let len = (rec.frames - rec.first_loud.unwrap_or(0)) as usize;
                        LoopFound {
                            l: looping::whole_cycles(f.l, rate, len),
                            ..f
                        }
                    });
                    let late = rec.frames >= (60 * rate) as u64;
                    let step = if late {
                        LATE_LOOP_CHECK_SECS
                    } else {
                        LOOP_CHECK_SECS
                    };
                    let frames = rec.frames;
                    if let Some(search) = &mut rec.search {
                        let mut next = frames + (step * rate as f64) as u64;
                        if let Some(h) = search.hint.as_ref().filter(|h| h.ready_at > frames) {
                            next = next.min(h.ready_at);
                        }
                        search.next_check = next;
                    }
                }
                let ended_by = if rec.first_loud.is_none()
                    && rec.drained_at.is_some_and(|d| rec.frames - d >= no_sound)
                {
                    Some("no_sound")
                } else if silent_end {
                    Some("silence")
                } else if rec.found.is_some() {
                    Some("loop")
                } else if rec.frames >= max {
                    Some("max")
                } else {
                    None
                };
                if let Some(ended_by) = ended_by {
                    let Phase::Record(mut rec) = std::mem::replace(&mut self.phase, Phase::Done)
                    else {
                        unreachable!()
                    };
                    if ended_by == "max"
                        && let Some(search) = &rec.search
                    {
                        // No loop: keep the first `max_secs`, as without a search.
                        rec.unresolved = Some(unresolved_reason(search, rec.frames, self.rate));
                        let keep = max_secs.min(rec.frames);
                        rec.samples.truncate(keep as usize * self.channels.max(1));
                        rec.last_loud_end = rec.last_loud_end.min(keep);
                        rec.frames = keep;
                    }
                    self.finish(*rec, ended_by);
                    self.send_stop();
                }
            }
            Phase::Done => {}
        }
    }

    /// The boards are quiet: send the volume or what goes before the next command (each
    /// followed by another wait for quiet), or start the next command.
    fn on_quiet(&mut self, then_volume: bool, then_pre: bool) {
        if then_volume && self.send_volume(then_pre) {
            return;
        }
        if !self.fill_queue() {
            self.phase = Phase::Done;
            self.done = true;
            return;
        }
        if then_pre {
            let pre = self.pre_sends();
            if !pre.is_empty() {
                self.sender.extend(pre);
                let mut q = self.quiet(false);
                (q.then_volume, q.then_pre) = (false, false);
                self.phase = Phase::Quiet(q);
                return;
            }
        }
        self.next_command();
    }

    /// Sends the stop command and waits for silence.
    fn send_stop(&mut self) {
        self.sender.clear();
        self.sender.extend(self.stop.iter().copied());
        let resets = self.stop.iter().any(|s| matches!(s, Send::Reset));
        self.phase = Phase::Quiet(self.quiet(resets));
    }

    fn quiet(&self, after_reset: bool) -> Quiet {
        let need = if after_reset {
            QUIET_AFTER_RESET_SECS
        } else {
            QUIET_SECS
        };
        Quiet {
            since: self.t,
            deadline: self.t + self.secs(QUIET_MAX_SECS + need),
            drained_at: None,
            need: self.secs(need),
            after_reset,
            // Our own volume is sent on the first wait; any volume (ours or the game's) is
            // sent again after a reset, which loses it.
            then_volume: after_reset || self.sets_own_volume() && !self.own_volume_sent,
            then_pre: true,
        }
    }

    /// The volume the boards should be at: our own master volume where we set one (see
    /// `our_master`), and the game's own last volume commands for everything else (its
    /// master volume on the other boards, DCS channel levels, the Data East music volume).
    fn volume_sends(&self) -> Vec<Send> {
        let mut v = Vec::new();
        for cmd in self.last_volumes() {
            if cmd.kind != "master" || self.our_master(cmd.board).is_none() {
                v.extend(board_sends(self.mask, cmd.board, &parse_id(&cmd.replay)));
            }
        }
        for b in self.board_list() {
            if let Some(bytes) = self.our_master(b) {
                v.extend(board_sends(self.mask, b, &bytes));
            }
        }
        v
    }

    /// The master volume the tool sets on a board itself, if any: `VolumeInit::Dcs` on DCS
    /// boards; `VolumeInit::Reference` on DCS and Whitestar boards.
    fn our_master(&self, board: c_int) -> Option<Vec<u8>> {
        match (self.opts.volume, self.families[board as usize].as_str()) {
            (VolumeInit::Dcs(vv) | VolumeInit::Reference { dcs: vv, .. }, "DCS") => {
                Some(vec![0x55, 0xAA, vv, !vv])
            }
            (VolumeInit::Reference { whitestar, .. }, "BSMT" | "AT91")
                if !self.is_de_board(board) =>
            {
                Some(vec![0xFE, whitestar, volume::BSMT_END])
            }
            _ => None,
        }
    }

    fn sets_own_volume(&self) -> bool {
        !matches!(self.opts.volume, VolumeInit::Game)
    }

    pub fn is_reference(&self) -> bool {
        matches!(self.opts.volume, VolumeInit::Reference { .. })
    }

    /// Our own master volumes, as reported in `volume_init`.
    fn own_volume_label(&self) -> Option<String> {
        let parts: Vec<String> = self
            .board_list()
            .filter_map(|b| {
                let bytes = hex(&self.our_master(b)?);
                Some(if self.mask == 3 {
                    format!("board {b} {bytes}")
                } else {
                    bytes
                })
            })
            .collect();
        if parts.is_empty() {
            return None;
        }
        let label = parts.join(" ");
        Some(if self.is_reference() {
            format!("reference {label}")
        } else {
            label
        })
    }

    /// The master volume value (`vv` or `xx`) the board is playing at: ours, else the game's.
    fn current_master(&self, board: c_int) -> Option<u8> {
        match self.our_master(board) {
            Some(bytes) => bytes.get(if bytes[0] == 0x55 { 2 } else { 1 }).copied(),
            None => self.last_master(board).map(|m| m.value),
        }
    }

    /// The game's factory master volume for a board, as bytes to send, and where it comes
    /// from: its last master volume at boot, or on DCS the board's reset default when the
    /// game sent none.
    fn factory_master(&self, board: c_int) -> Option<(Vec<u8>, String)> {
        if let Some(m) = self.last_master(board) {
            return Some((
                parse_id(&m.replay),
                format!(
                    "the game's last master volume at boot ({} at {:.1} s)",
                    m.bytes, m.at
                ),
            ));
        }
        (self.families[board as usize] == "DCS").then(|| {
            let v = DCS_RESET_DEFAULT;
            (
                vec![0x55, 0xAA, v, !v],
                "none sent by the game: the board's reset default".to_string(),
            )
        })
    }

    /// Queues the volume and waits for quiet again. Returns false when there is nothing
    /// to send.
    fn send_volume(&mut self, then_pre: bool) -> bool {
        let v = self.volume_sends();
        if v.is_empty() {
            return false;
        }
        if self.sets_own_volume() && !self.own_volume_sent {
            self.own_volume_sent = true;
            self.volume_sent = self.own_volume_label();
        } else {
            self.volume_replays += 1;
        }
        self.sender.extend(v);
        let mut q = self.quiet(false);
        (q.then_volume, q.then_pre) = (false, then_pre);
        self.phase = Phase::Quiet(q);
        true
    }

    /// What goes before the next command, outside its recording: before a retry the stop
    /// and the volume again; before every command the refresh; before a volume check the
    /// other master volume.
    fn pre_sends(&mut self) -> Vec<Send> {
        let mut v = Vec::new();
        if self.pass == Pass::Retry {
            v.extend(self.stop.iter().copied());
            let vol = self.volume_sends();
            self.volume_replays += u32::from(!vol.is_empty());
            v.extend(vol);
        }
        self.volume_replays += u32::from(!self.refresh.is_empty());
        v.extend(self.refresh.iter().copied());
        if let Some(alt) = self.queue.front().and_then(|c| c.alt.as_ref()) {
            v.extend(alt.sends.iter().copied());
        }
        v
    }

    fn end_boot(&mut self, ended_by: &'static str) {
        self.boot_end = Some(self.t);
        self.boot_ended_by = ended_by;
        self.mask = board_mask();
        self.boards = self.board_list().filter_map(board_typestr).collect();
        // The reports are also written after PinmameStop, when the boards are gone.
        self.families = [0, 1].map(|b| board_typestr(b).unwrap_or_default());
        self.data_east = ffi::is_data_east();
        let boot = self.boot_report();
        eprintln!(
            "boot: {:.1} s emulated (ended by {ended_by}), game sent {} sound byte(s), boards {:?}",
            boot.secs, boot.bytes, self.boards
        );
        for l in &boot.log {
            eprintln!(
                "  board {} ({}), {} byte(s), seconds:byte: {}",
                l.board, l.family, l.count, l.bytes
            );
        }
        let vol = self.volume_report();
        for c in &vol.commands {
            eprintln!(
                "  volume: board {} ({}) {} {} = level {}/{} at {:.1} s",
                c.board, c.family, c.kind, c.bytes, c.level, c.level_max, c.at
            );
        }
        if let Some(note) = &vol.note {
            eprintln!("  volume: no master volume seen ({note})");
        }
        if self.opts.cold_boot_only {
            self.phase = Phase::Done;
            self.done = true;
            return;
        }
        // Ducking study: dump the DCS sound region for `dcs-effects`.
        if let (Ok(path), Some(region)) = (std::env::var("R2A_DUMP_REGION"), ffi::sound_region()) {
            if let Err(e) = std::fs::write(&path, region) {
                eprintln!("cannot dump the sound region to {path}: {e}");
            }
        }
        let halted = unsafe { ffi::shim_halt_game_cpus(1) };
        eprintln!("halted {halted} game CPU(s)");
        // The halt can fall between a Whitestar `FE xx` and the `FD` that completes it (the
        // game sends them in different frames); the board would then swallow everything.
        for b in self.board_list() {
            let recent = &self.recent[b as usize];
            let open = recent.iter().rposition(|&c| c == 0xFE)
                > recent.iter().rposition(|&c| c == volume::BSMT_END);
            if matches!(self.families[b as usize].as_str(), "BSMT" | "AT91")
                && !self.is_de_board(b)
                && open
            {
                eprintln!("  completing the game's half-sent FE command with FD");
                unsafe { ffi::sndbrd_manCmd(target(self.mask, b), volume::BSMT_END as c_int) };
            }
        }
        if self.mask == 0 {
            return self.fail("no sound board on this machine".into());
        }
        for b in self.board_list() {
            let typestr = board_typestr(b).unwrap_or_default();
            if unsafe { ffi::shim_board_has_mancmd(b) } == 0
                || NOOP_MANCMD.contains(&typestr.as_str())
            {
                return self.fail(format!(
                    "sound board {b} ({typestr}) has no manual command handler in PinMAME: nothing can be driven"
                ));
            }
        }
        self.stop = self.stop_sends();
        self.set_refresh();
        for l in &self.refresh_labels {
            eprintln!("  before every command: {l}");
        }
        self.main_cmds = self.build_commands();
        self.queue = self.main_cmds.clone().into();
        eprintln!("{} command(s) to extract", self.queue.len());
        self.phase = Phase::Settle {
            until: self.t + self.secs(SETTLE_SECS),
        };
    }

    /// What is sent before every command, so that no command's state leaks into the next:
    /// - Data East BSMT: the music volume (the game's last `20`..`2F` at boot, else the
    ///   board's default `20`), then the stop `00`. Without the volume a pure volume byte
    ///   (`21`..`2F`), or a music track that fades out as it ends (gnr_300 `10`, btmn_106
    ///   `01`), left every later music 17 to 50 dB too quiet, or silent. Without the `00`
    ///   (which keeps the music volume) btmn_106's output stayed on a held DC level after
    ///   the `20` (+14462 LSB), so its files started far from 0 and `7B` clipped.
    /// - A master volume the game kept re-sending during boot (Whitestar `FE xx FD` every
    ///   0.5 s), or ours instead where we set one (always on Whitestar): on xfiles, music `0F` ends after 22 s with the board muted, and without
    ///   the refresh every later command was silent (one-byte sweep: 17 sounds instead of
    ///   170).
    fn set_refresh(&mut self) {
        let last = self.last_volumes();
        for b in self.board_list() {
            if self.is_de_board(b) {
                let music = last.iter().find(|v| v.board == b && v.kind == "music");
                let byte = music.map_or(*volume::DE_MUSIC_VOLUME.start(), |v| v.value);
                self.refresh.extend(addressed(self.mask, b, &[byte, 0x00]));
                self.refresh_labels.push(format!(
                    "board {b}: {byte:02X} 00 (Data East music volume, {}, then the stop)",
                    if music.is_some() {
                        "the game's last one at boot"
                    } else {
                        "the board's default: the game sent none at boot"
                    }
                ));
                continue;
            }
            let master = last.iter().find(|v| v.board == b && v.kind == "master");
            let repeated = master.is_some_and(|m| {
                self.volumes
                    .iter()
                    .filter(|v| v.board == b && v.bytes == m.bytes)
                    .count()
                    >= 2
            });
            let whitestar = matches!(self.families[b as usize].as_str(), "BSMT" | "AT91");
            if let Some(ours) = self.our_master(b) {
                if repeated || whitestar {
                    self.refresh.extend(board_sends(self.mask, b, &ours));
                    self.refresh_labels.push(format!(
                        "board {b}: {} (our master volume, re-sent as the game re-sends its own)",
                        hex(&ours)
                    ));
                }
            } else if let Some(m) = master
                && repeated
            {
                self.refresh
                    .extend(board_sends(self.mask, b, &parse_id(&m.replay)));
                self.refresh_labels.push(format!(
                    "board {b}: {} (the game's master volume, which it kept re-sending at boot)",
                    m.replay
                ));
            }
        }
    }

    fn fail(&mut self, msg: String) {
        self.error = Some(msg);
        self.phase = Phase::Done;
        self.done = true;
    }

    /// Makes sure a command is queued, moving on to the next pass when the current one is
    /// over. Returns false when everything is done.
    fn fill_queue(&mut self) -> bool {
        while self.queue.is_empty() {
            match self.pass {
                Pass::Main => {
                    self.pass = Pass::Retry;
                    self.queue = self.retry_commands().into();
                    if !self.queue.is_empty() {
                        eprintln!(
                            "retry: {} command(s) played nothing, playing them once more",
                            self.queue.len()
                        );
                    }
                }
                Pass::Retry => {
                    self.pass = Pass::VolumeCheck;
                    self.queue = self.volume_checks().into();
                    if let Some(n) = &self.volume_check_note {
                        eprintln!("master volume check: {n}");
                    }
                }
                Pass::VolumeCheck => {
                    self.pass = Pass::FactoryOffset;
                    self.queue = self.factory_offset_commands().into();
                    if let Some(n) = &self.offset_note {
                        eprintln!("factory offset: {n}");
                    }
                }
                Pass::FactoryOffset => return false,
            }
        }
        true
    }

    fn retry_commands(&self) -> Vec<Cmd> {
        self.results
            .iter()
            .enumerate()
            .filter(|(_, s)| s.ended_by == "no_sound" && !s.retried)
            .filter_map(|(i, _)| {
                let mut c = self.main_cmds.get(i)?.clone();
                c.slot = Some(i);
                Some(c)
            })
            .collect()
    }

    /// The written files more than `VOLUME_CHECK_ABOVE_MEDIAN_LU` above the median file,
    /// to be played again at another master volume, where the board has one.
    fn volume_checks(&mut self) -> Vec<Cmd> {
        let files = self.counted(false).count();
        if files < VOLUME_CHECK_MIN_FILES {
            self.volume_check_note = Some(format!(
                "not run: {files} non-loop file(s), too few for a median and a reference (a sweep of a few commands with --only cannot flag a file)"
            ));
            return Vec::new();
        }
        let Some(median) = self.median_lufs() else {
            self.volume_check_note = Some("no file to compare".into());
            return Vec::new();
        };
        let threshold = median + VOLUME_CHECK_ABOVE_MEDIAN_LU;
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut unchecked = Vec::new();
        let mut references = 0;
        for (i, s) in self.results.iter().enumerate() {
            if s.file.is_none() || s.level_lufs.is_none_or(|l| l <= threshold) {
                continue;
            }
            let Some(cmd) = self.main_cmds.get(i) else {
                continue;
            };
            match self.alt_volume(cmd.board_no) {
                Some(alt) => {
                    // The board's reference goes first.
                    let has_ref = cmds.iter().any(|c| c.board_no == cmd.board_no);
                    if !has_ref && let Some(r) = self.reference_file(cmd.board_no, median) {
                        let mut c = self.main_cmds[r].clone();
                        c.slot = Some(r);
                        c.alt = Some(AltVolume {
                            reference: true,
                            ..alt.clone()
                        });
                        cmds.push(c);
                        references += 1;
                    }
                    let mut c = cmd.clone();
                    c.slot = Some(i);
                    c.alt = Some(alt);
                    cmds.push(c);
                }
                None => unchecked.push(format!("{} (board {})", s.id, cmd.board_no)),
            }
        }
        let checked = cmds.len() - references;
        let mut note = format!(
            "{} file(s) more than {VOLUME_CHECK_ABOVE_MEDIAN_LU} LU above the median file ({median:.1} LUFS)",
            checked + unchecked.len()
        );
        if checked > 0 {
            note += &format!(
                "; {checked} played again {VOLUME_CHECK_LEVELS} master volume levels away with a reference file, flagged when their level did not follow the reference's"
            );
        }
        if !unchecked.is_empty() {
            note += &format!(
                "; not checked, no known master volume on their board: {}",
                unchecked.join(" ")
            );
        }
        self.volume_check_note = Some(note);
        cmds
    }

    /// Reference mode: per board with a master volume of ours, a few written non-loop files
    /// played again at the game's factory volume, to measure the offset between the two.
    /// The loudest files at most `VOLUME_CHECK_ABOVE_MEDIAN_LU` over the median (louder
    /// ones are the volume check's suspects), at least `OFFSET_MIN_SECS` long, not clipped
    /// and not flagged: loud, so that they stay well above the silence threshold at a low
    /// factory volume (apollo13 plays at level 3/31).
    fn factory_offset_commands(&mut self) -> Vec<Cmd> {
        if !self.is_reference() {
            return Vec::new();
        }
        let median = self.median_lufs();
        let mut cmds = Vec::new();
        let mut notes = Vec::new();
        for b in self.board_list() {
            let Some(ours) = self.our_master(b) else {
                notes.push(format!(
                    "board {b} ({}): no master volume, the files are at the board's only level (offset 0)",
                    self.family_label(b)
                ));
                continue;
            };
            let Some((factory, from)) = self.factory_master(b) else {
                notes.push(format!(
                    "board {b} ({}): the game sent no master volume at boot, offset not measured",
                    self.family_label(b)
                ));
                continue;
            };
            let mut files: Vec<usize> = self
                .counted(false)
                .filter(|&i| self.main_cmds.get(i).is_some_and(|c| c.board_no == b))
                .filter(|&i| {
                    let s = &self.results[i];
                    s.clipped_samples == 0
                        && s.duration >= OFFSET_MIN_SECS
                        && s.lufs.is_some_and(|l| {
                            median.is_none_or(|m| l <= m + VOLUME_CHECK_ABOVE_MEDIAN_LU)
                        })
                })
                .collect();
            files.sort_by(|&x, &y| {
                let l = |i: usize| self.results[i].lufs.unwrap_or(f64::NEG_INFINITY);
                l(y).total_cmp(&l(x))
            });
            files.truncate(OFFSET_FILES);
            if files.is_empty() {
                notes.push(format!(
                    "board {b}: no non-loop file of at least {OFFSET_MIN_SECS} s to play again, offset not measured"
                ));
            }
            let alt = AltVolume {
                sends: board_sends(self.mask, b, &factory),
                bytes: hex(&factory),
                levels: master_level(&factory) - master_level(&ours),
                reference: false,
            };
            for &i in &files {
                let mut c = self.main_cmds[i].clone();
                c.slot = Some(i);
                c.alt = Some(alt.clone());
                cmds.push(c);
            }
            self.offsets.push(BoardOffset {
                board: b,
                family: self.family_label(b),
                reference_volume: hex(&ours),
                factory_volume: hex(&factory),
                factory_volume_from: from,
                factory_offset_db: None,
                spread_db: None,
                samples: Vec::new(),
            });
        }
        if !cmds.is_empty() {
            notes.insert(
                0,
                format!("{} file(s) played again at the factory volume", cmds.len()),
            );
        }
        self.offset_note = (!notes.is_empty()).then(|| notes.join("; "));
        cmds
    }

    /// The ROM's factory offset: 0 when no board has a master volume of ours, the one
    /// board's offset otherwise (the median of all samples if several boards have one).
    pub fn rom_offset(&self) -> Option<f64> {
        if !self.board_list().any(|b| self.our_master(b).is_some()) {
            return Some(0.0);
        }
        let deltas: Vec<f64> = self
            .offsets
            .iter()
            .flat_map(|o| o.samples.iter().filter_map(|s| s.delta_db))
            .collect();
        loudness::median(deltas)
    }

    /// The written non-loop file of a board whose loudness is closest to the median.
    fn reference_file(&self, board: c_int, median: f64) -> Option<usize> {
        self.counted(false)
            .filter(|&i| self.main_cmds.get(i).is_some_and(|c| c.board_no == board))
            .filter_map(|i| Some((i, (self.results[i].lufs? - median).abs())))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Another master volume for a board, `VOLUME_CHECK_LEVELS` away from the current one
    /// (down if possible, the files checked being loud). None when the board has no known
    /// master volume.
    fn alt_volume(&self, board: c_int) -> Option<AltVolume> {
        let away = |level: u32, range: std::ops::RangeInclusive<u32>| {
            let down = level.checked_sub(VOLUME_CHECK_LEVELS);
            match down.filter(|l| range.contains(l)) {
                Some(l) => Some(l),
                None => Some(level + VOLUME_CHECK_LEVELS).filter(|l| range.contains(l)),
            }
        };
        let (bytes, level, alt) = match self.families[board as usize].as_str() {
            "DCS" => {
                let vv = self.current_master(board)?;
                let level = u32::from(vv.saturating_sub(7)) / 8;
                let alt = away(level, DCS_LEVELS)?;
                let v = (alt * 8 + 7) as u8;
                (vec![0x55, 0xAA, v, !v], level, alt)
            }
            "BSMT" | "AT91" if !self.is_de_board(board) => {
                let level = u32::from(0x2F - self.current_master(board)?.min(0x2F));
                let alt = away(level, 0..=31)?;
                (vec![0xFE, (0x2F - alt) as u8, volume::BSMT_END], level, alt)
            }
            _ => return None,
        };
        Some(AltVolume {
            sends: board_sends(self.mask, board, &bytes),
            bytes: hex(&bytes),
            levels: alt as i32 - level as i32,
            reference: false,
        })
    }

    fn next_command(&mut self) {
        let Some(cmd) = self.queue.pop_front() else {
            self.phase = Phase::Done;
            self.done = true;
            return;
        };
        let what = match (self.pass, &cmd.alt) {
            (Pass::Retry, _) => format!("{} (retry)", cmd.id),
            (Pass::VolumeCheck, Some(a)) => {
                format!("{} (at {}, {:+} levels)", cmd.id, a.bytes, a.levels)
            }
            (Pass::FactoryOffset, Some(a)) => {
                format!("{} (at factory {}, {:+} levels)", cmd.id, a.bytes, a.levels)
            }
            _ => cmd.id.clone(),
        };
        eprint!("  {what} {:<32}", cmd.name);
        self.sender.extend(cmd.sends.iter().copied());
        let search = (matches!(self.pass, Pass::Main | Pass::Retry)
            && self.opts.loop_max_secs > 0.0)
            .then(|| self.loop_search(&cmd));
        self.phase = Phase::Record(Box::new(Recording {
            cmd,
            samples: Vec::new(),
            frames: 0,
            first_loud: None,
            last_loud_end: 0,
            start_idle: self.idle.clone(),
            clean_start: !std::mem::take(&mut self.dirty),
            drained_at: None,
            search,
            found: None,
            unresolved: None,
        }));
    }

    /// The loop search of one command: on DCS, the track program's own loop first.
    fn loop_search(&self, cmd: &Cmd) -> LoopSearch {
        let mut cap = self.secs(self.opts.loop_max_secs.max(self.opts.max_secs));
        let id = parse_id(&cmd.id);
        let dcs_track = (self.families[(cmd.board_no & 1) as usize] == "DCS" && id.len() == 2)
            .then(|| u16::from_be_bytes([id[0], id[1]]));
        let (mut dcs, mut hint) = (None, None);
        if let Some(track) = dcs_track
            && let Some(region) = ffi::sound_region()
        {
            let max_frames = (LOOP_HINT_MAX_SECS / DCS_FRAME_SECS) as u32;
            dcs = Some(match dcsrom::track_run(region, track, max_frames) {
                dcsrom::TrackRun::Loops(l) => {
                    let (intro, period) = (
                        l.intro_frames as f64 * DCS_FRAME_SECS,
                        l.period_frames as f64 * DCS_FRAME_SECS,
                    );
                    // The audio confirms the period over a whole cycle (at most a minute)
                    // after the intro; plus the onset and a margin.
                    let need = period.clamp(looping::CONFIRM_SECS, looping::HINT_CONFIRM_MAX_SECS);
                    let ready = intro + period + need + 2.0;
                    if ready <= LOOP_HINT_MAX_SECS {
                        let ready_at = self.secs(ready);
                        cap = cap.max(ready_at + self.secs(LOOP_CHECK_SECS));
                        hint = Some(Hint {
                            period: period * self.rate as f64,
                            ready_at,
                        });
                    }
                    DcsTrack {
                        program: "loops",
                        intro_frames: Some(l.intro_frames),
                        period_frames: Some(l.period_frames),
                        frames: None,
                        reason: (ready > LOOP_HINT_MAX_SECS)
                            .then(|| format!("too long to confirm within {LOOP_HINT_MAX_SECS} s")),
                    }
                }
                dcsrom::TrackRun::Ends(frames) => DcsTrack {
                    program: "ends",
                    intro_frames: None,
                    period_frames: None,
                    frames: Some(frames),
                    reason: None,
                },
                dcsrom::TrackRun::Unknown(reason) => DcsTrack {
                    program: "unknown",
                    intro_frames: None,
                    period_frames: None,
                    frames: None,
                    reason: Some(reason),
                },
            });
        }
        LoopSearch {
            dcs,
            hint,
            next_check: self.secs(FIRST_LOOP_CHECK_SECS),
            cap,
        }
    }

    fn board_list(&self) -> impl Iterator<Item = c_int> + use<> {
        let mask = self.mask;
        (0..2).filter(move |&b| mask & (1 << b) != 0)
    }

    /// The family's own stop command from sounds.dat ("All sound off" for DCS, "Reset Sound
    /// System" for WPCS), else from `BUILTIN_STOPS`; boards without one are reset instead.
    fn stop_sends(&self) -> Vec<Send> {
        if let Some(stop) = &self.opts.stop {
            let e = Entry {
                bytes: parse_id(stop),
                name: String::new(),
            };
            return self.game_cmd(&e).sends;
        }
        let mut v = Vec::new();
        for b in self.board_list() {
            let typestr = board_typestr(b).unwrap_or_default();
            let stop = self.dat.family_entries(&typestr).into_iter().find(|e| {
                let n = e.name.to_ascii_lowercase();
                n.contains("sound off") || n.contains("reset sound")
            });
            let stop = stop.map(|e| e.bytes).or_else(|| {
                BUILTIN_STOPS
                    .iter()
                    .find(|(t, _)| *t == typestr)
                    .map(|(_, bytes)| bytes.to_vec())
            });
            match stop {
                Some(bytes) => v.extend(board_sends(self.mask, b, &bytes)),
                None if !v.iter().any(|s| matches!(s, Send::Reset)) => v.push(Send::Reset),
                None => {}
            }
        }
        v
    }

    fn build_commands(&mut self) -> Vec<Cmd> {
        let entries = self
            .dat
            .game_entries(&self.opts.rom, self.opts.parent.as_deref());
        let mut cmds: Vec<Cmd> = if entries.is_empty() {
            let (cmds, notes) = sweep(self.mask);
            eprintln!(
                "no sounds.dat section for {}: sweeping raw commands",
                self.opts.rom
            );
            for n in &notes {
                eprintln!("  {n}");
            }
            self.commands_from = notes;
            cmds
        } else {
            let mut cmds: Vec<Cmd> = entries.iter().map(|e| self.game_cmd(e)).collect();
            self.commands_from = vec![format!(
                "sounds.dat section of {} ({} commands)",
                self.opts.parent.as_deref().unwrap_or(&self.opts.rom),
                entries.len()
            )];
            // A sounds.dat section is not always complete (afm_113b leaves out 15 populated
            // tracks, among them 0013, a 120 s loop): on DCS, add the catalog's other tracks.
            if let Some(dcs) = self
                .board_list()
                .find(|&b| self.families[b as usize] == "DCS")
                && let Some((count, tracks)) = ffi::sound_region().and_then(dcsrom::tracks)
            {
                let extra: Vec<Cmd> = tracks
                    .iter()
                    .filter(|&&t| t != 0)
                    .map(|t| t.to_be_bytes().to_vec())
                    .filter(|bytes| !entries.iter().any(|e| &e.bytes == bytes))
                    .map(|bytes| {
                        let mut c = self.game_cmd(&Entry {
                            bytes,
                            name: String::new(),
                        });
                        c.board_no = dcs;
                        c
                    })
                    .collect();
                self.commands_from.push(format!(
                    "board {dcs} (DCS): + {} populated tracks of the ROM catalog's {count} not in sounds.dat (sorted in by track number)",
                    extra.len()
                ));
                cmds.extend(extra);
                cmds.sort_by_key(|c| parse_id(&c.id));
            }
            cmds
        };
        if let Some(only) = &self.opts.only {
            cmds = only
                .iter()
                .map(|want| {
                    if want.contains('+') {
                        return self.scenario_cmd(want);
                    }
                    let bytes = parse_id(want);
                    let id = format!("0x{}", hex(&bytes));
                    cmds.iter()
                        .find(|c| c.id == id)
                        .cloned()
                        .unwrap_or_else(|| {
                            self.game_cmd(&Entry {
                                bytes,
                                name: "(not in sounds.dat)".into(),
                            })
                        })
                })
                .collect();
            self.commands_from = vec![format!("--only ({} commands)", cmds.len())];
        }
        if let Some(n) = self.opts.limit {
            cmds.truncate(n);
        }
        cmds
    }

    /// A scenario for the ducking study: `0x000C+2.5+0x0390` sends 000C, waits 2.5 s, then
    /// sends 0390, all in one recording.
    fn scenario_cmd(&self, want: &str) -> Cmd {
        let mut sends = Vec::new();
        let mut board = 0;
        for part in want.split('+') {
            if part.starts_with("0x") || part.starts_with("0X") {
                let c = self.game_cmd(&Entry {
                    bytes: parse_id(part),
                    name: String::new(),
                });
                board = c.board_no;
                sends.extend(c.sends);
            } else {
                let secs: f64 = part.parse().unwrap_or(0.0);
                sends.push(Send::Wait((secs * 1000.0).round() as u32));
            }
        }
        Cmd {
            id: want.to_string(),
            name: "(scenario)".into(),
            board: board_typestr(board).unwrap_or_default(),
            board_no: board,
            sends,
            slot: None,
            alt: None,
        }
    }

    /// A game-section command: plain bytes on one-board machines, (board, byte) pairs on
    /// two-board machines (the layout PinMAME's commander expects).
    fn game_cmd(&self, e: &Entry) -> Cmd {
        let mask = self.mask;
        let (sends, board) = if mask == 3 {
            let sends: Vec<Send> = e
                .bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[b, v]| Send::Byte(*b as c_int, *v as c_int))
                .collect();
            let board = e.bytes.first().map_or(0, |&b| (b & 1) as c_int);
            (sends, board)
        } else {
            let b = if mask == 2 { 1 } else { 0 };
            (board_sends(mask, b, &e.bytes), b)
        };
        Cmd {
            id: format!("0x{}", hex(&e.bytes)),
            name: e.name.clone(),
            board: board_typestr(board).unwrap_or_default(),
            board_no: board,
            sends,
            slot: None,
            alt: None,
        }
    }

    /// Trims a recording and measures its levels.
    fn analyze(&self, rec: &Recording) -> Analysis {
        let ch = self.channels.max(1);
        let frames = match (rec.first_loud, &rec.found) {
            // A loop: the intro and one body, cut at the exact sample.
            (Some(first), Some(f)) => {
                let first = first as usize;
                first..first + f.l.intro + f.l.period
            }
            (Some(first), None) => {
                let (first, end) = (first as usize, rec.last_loud_end as usize);
                first..held_dc_start(&rec.samples, ch, first, end, self.rate)
            }
            (None, _) => 0..0,
        };
        let range = frames.start * ch..frames.end * ch;
        let raw = rec.samples[range.clone()].to_vec();
        let blocked = dc_block(&rec.samples, ch, &rec.start_idle, self.rate)[range].to_vec();
        let (peak, rms) = levels(&blocked);
        let clipped_samples = raw
            .iter()
            .filter(|&&s| s == i16::MAX || s == i16::MIN)
            .count();
        let dc_offset = if raw.is_empty() {
            0
        } else {
            (raw.iter().map(|&s| s as i64).sum::<i64>() / raw.len() as i64) as i32
        };
        let duration = (raw.len() / ch) as f64 / self.rate as f64;
        Analysis {
            body: rec
                .found
                .as_ref()
                .map(|f| f.l.intro..f.l.intro + f.l.period),
            blip: !raw.is_empty() && duration < BLIP_SECS,
            raw,
            blocked,
            peak,
            rms,
            clipped_samples,
            dc_offset,
            duration,
        }
    }

    fn finish(&mut self, rec: Recording, ended_by: &'static str) {
        let a = self.analyze(&rec);
        match self.pass {
            Pass::VolumeCheck => self.finish_check(rec, a, ended_by),
            Pass::FactoryOffset => self.finish_offset(rec, a, ended_by),
            Pass::Main | Pass::Retry => self.finish_sound(rec, a, ended_by),
        }
        self.write_manifest();
    }

    /// A main or retry recording: written, measured, stored.
    fn finish_sound(&mut self, rec: Recording, a: Analysis, ended_by: &'static str) {
        let ch = self.channels.max(1);
        let rate = self.rate;
        let file =
            (!a.raw.is_empty() && !a.blip).then(|| format!("{}-{}.wav", rec.cmd.id, self.opts.rom));
        let rounded: Vec<i16>;
        let samples = if self.opts.dc_block {
            rounded = a
                .blocked
                .iter()
                .map(|&x| x.round().clamp(-32768.0, 32767.0) as i16)
                .collect();
            &rounded[..]
        } else {
            &a.raw[..]
        };
        let loud = file.as_ref().map(|f| {
            if let Err(e) = write_wav(&self.opts.out_dir.join(f), samples, ch as u16, rate) {
                eprintln!("  cannot write {f}: {e}");
            }
            loudness::measure(&a.blocked, ch, rate)
        });
        let loop_info = match (&rec.found, &a.body, &file) {
            (Some(found), Some(body), Some(_)) => {
                Some(self.loop_info(&rec, found, body.clone(), samples))
            }
            _ => None,
        };
        let lufs = loud.as_ref().and_then(|l| l.lufs);
        let tp = loud.as_ref().and_then(|l| l.true_peak_dbtp);
        eprintln!(
            " {:7.2} s  peak {}  rms {}  {}{}{}{}  [{ended_by}]",
            a.duration,
            fmt_db(a.peak),
            fmt_db(a.rms),
            match (lufs, tp) {
                (Some(l), Some(p)) => format!("{l:6.1} LUFS {p:5.1} dBTP"),
                (None, Some(p)) => format!("   (<400ms) {p:5.1} dBTP"),
                _ => String::new(),
            },
            if a.blip { "  blip (not written)" } else { "" },
            if a.clipped_samples > 0 {
                format!("  {} clipped", a.clipped_samples)
            } else {
                String::new()
            },
            if rec.clean_start { "" } else { "  NOT CLEAN" }
        );
        if let Some(l) = &loop_info {
            eprintln!(
                "      loop ({}): intro {:.3} s + body {:.3} s, residual {:.1} dB over {:.1} s, seam error {} LSB (step {} for {} in the recording){}",
                l.method,
                l.intro_samples as f64 / rate as f64,
                l.period_secs,
                l.residual_db,
                l.verified_secs,
                l.seam.error,
                l.seam.joint_step,
                l.seam.natural_step,
                l.note.as_ref().map_or(String::new(), |n| format!("; {n}"))
            );
        } else if let Some(r) = rec.unresolved.as_ref().filter(|_| ended_by == "max") {
            eprintln!("      loop not found: {r}");
        }
        let info = SoundInfo {
            id: rec.cmd.id,
            name: rec.cmd.name,
            file,
            duration: round3(a.duration),
            blip: a.blip,
            lufs,
            true_peak_dbtp: tp,
            level_lufs: loud.as_ref().and_then(|l| l.level_lufs),
            peak_dbfs: a.peak.map(round3),
            rms_dbfs: a.rms.map(round3),
            clipped_samples: a.clipped_samples,
            dc_offset: a.dc_offset,
            ended_by,
            looping_or_truncated: ended_by == "max" || ended_by == "loop",
            loop_unresolved: rec.unresolved.clone().filter(|_| ended_by == "max"),
            loop_info,
            clean_start: rec.clean_start,
            retried: self.pass == Pass::Retry,
            ignores_master_volume: false,
            master_volume_check: None,
            board: rec.cmd.board,
            volume_init: self.volume_label(),
            idle_level: self.idle.first().copied().unwrap_or(0),
            onset: rec.first_loud.map(|f| round3(f as f64 / rate as f64)),
        };
        match rec.cmd.slot {
            // A retry that played something replaces the first try.
            Some(i) if info.onset.is_some() => {
                self.results[i] = info;
                self.loud[i] = loud;
            }
            Some(i) => self.results[i].retried = true,
            None => {
                self.results.push(info);
                self.loud.push(loud);
            }
        }
    }

    /// Writes the body of a loop alone when there is an intro, checks its joint and
    /// describes it.
    fn loop_info(
        &self,
        rec: &Recording,
        found: &LoopFound,
        body: std::ops::Range<usize>,
        written: &[i16],
    ) -> LoopInfo {
        let (ch, rate) = (self.channels.max(1), self.rate);
        let l = &found.l;
        let loop_file = (l.intro > 0).then(|| {
            let f = format!("{}-{}-loop.wav", rec.cmd.id, self.opts.rom);
            let samples = &written[body.start * ch..body.end * ch];
            if let Err(e) = write_wav(&self.opts.out_dir.join(&f), samples, ch as u16, rate) {
                eprintln!("  cannot write {f}: {e}");
            }
            f
        });
        // The joint, on the raw recording (which goes on past the body).
        let first = rec.first_loud.unwrap_or(0) as usize;
        let recorded = &rec.samples[first * ch..];
        let seam = looping::seam(recorded, ch, l.intro, l.period);
        let audio_period_samples = (found.method == "dcs-catalog")
            .then(|| looping::find(&looping::mono(recorded, ch), rate, None).map(|a| a.period));
        let dcs_track = rec.search.as_ref().and_then(|s| s.dcs.clone());
        LoopInfo {
            intro_samples: l.intro,
            period_samples: l.period,
            period_exact_samples: round3(l.period_exact),
            period_secs: round3(l.period as f64 / rate as f64),
            cycles: l.cycles,
            method: found.method,
            confidence: round3(1.0 - 10f64.powf(l.residual_db / 20.0)),
            residual_db: round3(l.residual_db),
            verified_secs: round3(l.matched as f64 / rate as f64),
            loop_file,
            seam: SeamInfo {
                joint_step: seam.joint,
                natural_step: seam.natural,
                error: seam.error,
                body_p99_step: seam.p99,
                body_max_step: seam.max,
            },
            dcs_track,
            audio_period_samples,
            note: found.note.clone(),
        }
    }

    /// A volume check recording: compared with the file, not written.
    fn finish_check(&mut self, rec: Recording, a: Analysis, ended_by: &'static str) {
        let Some(i) = rec.cmd.slot else { return };
        let level = (!a.raw.is_empty())
            .then(|| loudness::measure(&a.blocked, self.channels.max(1), self.rate))
            .and_then(|l| l.level_lufs);
        let delta = level
            .zip(self.results[i].level_lufs)
            .map(|(l, f)| round3(l - f));
        let Some(alt) = rec.cmd.alt else { return };
        let board = rec.cmd.board_no;
        let reference = if alt.reference {
            self.check_refs.push((board, rec.cmd.id.clone(), delta));
            None
        } else {
            self.check_refs.iter().find(|r| r.0 == board).cloned()
        };
        let ignores = !alt.reference
            && delta.is_some_and(|d| match reference.as_ref().and_then(|r| r.2) {
                Some(e) => (d - e).abs() > (e.abs() / 2.0).max(VOLUME_CHECK_MIN_DELTA_LU),
                // Without a usable reference: flagged when the level did not move.
                None => d.abs() < VOLUME_CHECK_MIN_DELTA_LU,
            });
        eprintln!(
            " level {} ({}){}  [{ended_by}]",
            level.map_or("n/a".into(), |l| format!("{l:.1} LUFS")),
            delta.map_or("n/a".into(), |d| format!("{d:+.1} LU")),
            if alt.reference {
                "  (reference)"
            } else if ignores {
                "  DOES NOT FOLLOW THE MASTER VOLUME"
            } else {
                ""
            }
        );
        let s = &mut self.results[i];
        s.ignores_master_volume = ignores;
        s.master_volume_check = Some(VolumeCheck {
            replayed_at: alt.bytes,
            levels_away: alt.levels,
            level_lufs: level,
            delta_lu: delta,
            reference: reference.as_ref().map(|r| r.1.clone()),
            reference_delta_lu: reference.and_then(|r| r.2),
        });
    }

    /// A factory offset recording: its level against the written file's, not written.
    fn finish_offset(&mut self, rec: Recording, a: Analysis, ended_by: &'static str) {
        let Some(i) = rec.cmd.slot else { return };
        let level = (!a.raw.is_empty())
            .then(|| loudness::measure(&a.blocked, self.channels.max(1), self.rate))
            .and_then(|l| l.level_lufs);
        let Some(reference) = self.results[i].level_lufs else {
            return;
        };
        let delta = level.map(|l| round3(l - reference));
        eprintln!(
            " level {} ({})  [{ended_by}]",
            level.map_or("n/a".into(), |l| format!("{l:.1} LUFS")),
            delta.map_or("n/a".into(), |d| format!("{d:+.1} dB")),
        );
        let Some(o) = self
            .offsets
            .iter_mut()
            .find(|o| o.board == rec.cmd.board_no)
        else {
            return;
        };
        o.samples.push(OffsetSample {
            id: rec.cmd.id,
            reference_lufs: reference,
            factory_lufs: level,
            delta_db: delta,
        });
        let deltas: Vec<f64> = o.samples.iter().filter_map(|s| s.delta_db).collect();
        o.factory_offset_db = loudness::median(deltas.clone());
        o.spread_db = deltas
            .iter()
            .copied()
            .reduce(f64::max)
            .zip(deltas.iter().copied().reduce(f64::min))
            .map(|(max, min)| round3(max - min));
    }

    /// The volume the boards play at: ours, or the game's own.
    pub fn volume_label(&self) -> Option<String> {
        if let Some(v) = &self.volume_sent {
            return Some(v.clone());
        }
        let game: Vec<String> = self
            .last_volumes()
            .iter()
            .map(|v| match v.kind.as_str() {
                "master" => format!("game {}", v.bytes),
                k => format!("game {} ({k})", v.bytes),
            })
            .collect();
        (!game.is_empty()).then(|| game.join(" "))
    }

    pub fn write_manifest(&self) {
        let stop: Vec<String> = self
            .stop
            .iter()
            .map(|s| match s {
                Send::Byte(b, v) => format!("manCmd({b},{v:02X})"),
                Send::Data(b, v) => format!("data_w({b},{v:02X})"),
                Send::Reset => "board reset".into(),
                Send::Wait(ms) => format!("wait {ms} ms"),
            })
            .collect();
        let vol = self.volume_report();
        let factory = self.is_factory();
        let m = Manifest {
            rom: &self.opts.rom,
            parent: self.opts.parent.as_deref(),
            mode: if factory { "factory" } else { "normal" },
            factory: self.opts.factory.as_ref(),
            boards: &self.boards,
            sample_rate: self.rate,
            channels: self.channels,
            boot: self.boot_report(),
            factory_volume: factory.then(|| vol.clone()),
            game_volume: (!factory).then_some(vol),
            volume_init: self.volume_label(),
            levels_note: self.is_reference().then_some(
                "every level (per sound and in loudness) is measured on the files, recorded at the reference volume (volume_init); add factory_offset_db for the level at the game's factory volume (loudness.as_shipped)",
            ),
            factory_offset_db: self.is_reference().then(|| self.rom_offset()),
            factory_offset: self.is_reference().then(|| FactoryOffsetReport {
                method: format!(
                    "per board with a master volume: up to {OFFSET_FILES} written non-loop files (the loudest at most {VOLUME_CHECK_ABOVE_MEDIAN_LU} LU above the median file, at least {OFFSET_MIN_SECS} s, not clipped) played again at the factory volume; offset = median of (factory level_lufs - reference level_lufs)"
                ),
                boards: &self.offsets,
                note: self.offset_note.clone(),
            }),
            recording_cap: RecordingCap {
                max_secs: self.opts.max_secs,
                loop_max_secs: self.opts.loop_max_secs,
                loop_hint_max_secs: LOOP_HINT_MAX_SECS,
                note: "a sound that keeps playing is recorded until one exact cycle of its loop is confirmed (ended_by loop: the file is the intro and one loop body, the body alone also goes to <id>-<rom>-loop.wav when there is an intro; see loop), for at most loop_max_secs (a DCS track whose program loops: until its period can be confirmed, at most loop_hint_max_secs); without a loop it is cut at max_secs (ended_by max, loop_unresolved says why). looping_or_truncated is true for both",
            },
            volume_replays: self.volume_replays,
            refreshed_before_each_command: &self.refresh_labels,
            commands_from: &self.commands_from,
            counts: self.counts(),
            loudness: self.loudness_report(),
            stop: stop.join(" "),
            board_resets: self.board_resets,
            dc_blocked_wav: self.opts.dc_block,
            sounds: &self.results,
        };
        let path = self.opts.out_dir.join("manifest.json");
        if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()) {
            eprintln!("cannot write {}: {e}", path.display());
        }
    }
}

fn board_mask() -> u8 {
    unsafe { (ffi::sndbrd_exists(0) != 0) as u8 | (((ffi::sndbrd_exists(1) != 0) as u8) << 1) }
}

fn board_typestr(board: c_int) -> Option<String> {
    ffi::cstr(unsafe { ffi::sndbrd_typestr(board) })
}

/// The board number that goes with each byte: on two-board machines the commander sends
/// (board, byte) pairs, otherwise the single existing board gets everything.
fn target(mask: u8, board: c_int) -> c_int {
    match mask {
        3 => board,
        2 => 1,
        _ => 0,
    }
}

/// Bytes for one board through its manual-command handler.
fn addressed(mask: u8, board: c_int, bytes: &[u8]) -> Vec<Send> {
    let t = target(mask, board);
    bytes.iter().map(|&b| Send::Byte(t, b as c_int)).collect()
}

/// A command for one board. Boards flagged SNDBRD_DOUBLECMD (WPCS) only act on byte pairs
/// in `sndbrd_manCmd`, and `wpcs_manCmd_w` writes both bytes of the pair to the board. A
/// single-byte command therefore goes through `sndbrd_data_w`, the path the WPC game CPU
/// itself uses (wpc.c `WPC_SND_DATA`), instead of being padded with a byte that the board
/// would also execute (00 is "Reset Sound System").
fn board_sends(mask: u8, board: c_int, bytes: &[u8]) -> Vec<Send> {
    let double = unsafe { ffi::shim_board_flags(board) } & ffi::SNDBRD_DOUBLECMD != 0;
    if double && bytes.len() % 2 == 1 {
        let t = target(mask, board);
        bytes.iter().map(|&b| Send::Data(t, b as c_int)).collect()
    } else {
        addressed(mask, board, bytes)
    }
}

/// Raw command sweep for games without a sounds.dat section, leaving out the commands that
/// change the board's state instead of playing something. Returns the commands and one
/// line per board saying what was swept.
fn sweep(mask: u8) -> (Vec<Cmd>, Vec<String>) {
    let mut v = Vec::new();
    let mut notes = Vec::new();
    for b in (0..2).filter(|&b| mask & (1 << b) != 0) {
        let typestr = board_typestr(b).unwrap_or_default();
        let (list, note): (Vec<Vec<u8>>, String) = match typestr.as_str() {
            // 16-bit track numbers: only the tracks populated in the ROM's catalog. 0000 is
            // "all sound off", and the 55 xx specials (volume...) are far above the count.
            "DCS" => match ffi::sound_region().and_then(dcsrom::tracks) {
                Some((count, tracks)) => {
                    let list: Vec<Vec<u8>> = tracks
                        .iter()
                        .filter(|&&t| t != 0)
                        .map(|t| t.to_be_bytes().to_vec())
                        .collect();
                    let n = format!(
                        "board {b} (DCS): {} populated tracks of the ROM catalog's {count} (0001..{:04X}), 0000 (stop) excluded",
                        list.len(),
                        count.saturating_sub(1)
                    );
                    (list, n)
                }
                None => (
                    (1..=DCS_FALLBACK_LAST)
                        .map(|t| t.to_be_bytes().to_vec())
                        .collect(),
                    format!(
                        "board {b} (DCS): no track catalog found in U2, swept 0001..{DCS_FALLBACK_LAST:04X}"
                    ),
                ),
            },
            // Data East's music volume bytes 20..2F are swept too: before every command the
            // music volume is set back (see `set_refresh`), so a pure volume byte ends as
            // no_sound or a blip, and the real sounds among them (gnr_300 `2E`) are kept.
            "BSMT" | "AT91" => (
                (1..=BSMT_LAST).map(|c| vec![c]).collect(),
                format!(
                    "board {b} ({typestr}): bytes 01..{BSMT_LAST:02X} (00 = stop; FC..FF start two-byte commands, FE xx FD = volume)"
                ),
            ),
            "WPCS" => (
                (1..=0xFFu8)
                    .filter(|c| !WPCS_STATE.iter().any(|r| r.contains(c)))
                    .map(|c| vec![c])
                    .collect(),
                format!(
                    "board {b} (WPCS): bytes 01..FF without tempo/volume/prefix bytes 1E-2F, 60-72, 79, 7A"
                ),
            ),
            _ => (
                (1..=0xFFu8).map(|c| vec![c]).collect(),
                format!("board {b} ({typestr}): bytes 01..FF"),
            ),
        };
        notes.push(note);
        for bytes in list {
            let id_bytes: Vec<u8> = if mask == 3 {
                [&[b as u8][..], &bytes].concat()
            } else {
                bytes.clone()
            };
            v.push(Cmd {
                id: format!("0x{}", hex(&id_bytes)),
                name: String::new(),
                board: typestr.clone(),
                board_no: b,
                sends: board_sends(mask, b, &bytes),
                slot: None,
                alt: None,
            });
        }
    }
    (v, notes)
}

/// Looks for the loop in what was recorded so far (from the first sound on). On DCS, the
/// track program's period is tried first once the recording is long enough to confirm it;
/// a period the audio alone gives is taken when there is no program period, when it
/// divides it, or once the program's period failed on the audio.
fn loop_check(rec: &Recording, ch: usize, rate: u32) -> Option<LoopFound> {
    let search = rec.search.as_ref()?;
    let first = rec.first_loud? as usize;
    let x = looping::mono(&rec.samples[first * ch..], ch);
    let hint = search.hint.as_ref();
    if let Some(h) = hint
        && rec.frames >= h.ready_at
        && let Some(l) = looping::find(&x, rate, Some(h.period))
    {
        return Some(LoopFound {
            l,
            method: "dcs-catalog",
            note: None,
        });
    }
    let l = looping::find(&x, rate, None)?;
    let Some(h) = hint else {
        return Some(LoopFound {
            l,
            method: "audio",
            note: None,
        });
    };
    let cycles = h.period / l.period_exact;
    let divides = cycles.round() >= 1.0 && (cycles - cycles.round()).abs() * l.period_exact < 2.0;
    let secs = |p: f64| p / rate as f64;
    if divides && cycles.round() == 1.0 {
        // The program's period, found by the audio alone (stricter: two whole cycles).
        Some(LoopFound {
            l,
            method: "dcs-catalog",
            note: None,
        })
    } else if divides {
        Some(LoopFound {
            note: Some(format!(
                "the audio repeats {} times per cycle of the track program ({:.3} s)",
                cycles.round(),
                secs(h.period)
            )),
            l,
            method: "audio",
        })
    } else if rec.frames >= h.ready_at {
        Some(LoopFound {
            note: Some(format!(
                "the track program's period ({:.3} s) does not hold in the audio",
                secs(h.period)
            )),
            l,
            method: "audio",
        })
    } else {
        // Wait until the program's period can be checked.
        None
    }
}

/// Why a sound that kept playing until the cap has no loop.
fn unresolved_reason(search: &LoopSearch, frames: u64, rate: u32) -> String {
    let mut reason = format!(
        "no exact repetition within {:.0} s (the period must hold over two cycles and at least {:.0} s)",
        frames as f64 / rate as f64,
        looping::CONFIRM_SECS
    );
    if let Some(d) = &search.dcs {
        match (d.program, d.period_frames) {
            ("loops", Some(p)) => {
                reason += &format!(
                    "; the DCS track program repeats every {:.3} s ({p} frames) after {:.3} s, which the audio did not confirm",
                    p as f64 * DCS_FRAME_SECS,
                    d.intro_frames.unwrap_or(0) as f64 * DCS_FRAME_SECS
                )
            }
            ("ends", _) => {
                reason += &format!(
                    "; the DCS track program ends after {:.3} s",
                    d.frames.unwrap_or(0) as f64 * DCS_FRAME_SECS
                )
            }
            _ => {
                if let Some(r) = &d.reason {
                    reason += &format!("; DCS track program: {r}");
                }
            }
        }
        if let Some(r) = d.reason.as_ref().filter(|_| d.program == "loops") {
            reason += &format!(" ({r})");
        }
    }
    reason
}

/// Where a sound that ends on held DC levels really ends (see `DC_HOLD_MIN_SECS`): the
/// recording's frames `first..end` are walked back from the end over steps between held
/// levels (short bursts of moving samples that change the level, preceded by a long flat
/// run); the sound ends where the first of those held levels starts. Returns `end` when
/// the sound does not end that way. A click that comes back to its level is not a step.
fn held_dc_start(s: &[i16], ch: usize, first: usize, end: usize, rate: u32) -> usize {
    let secs = |x: f64| (x * rate as f64) as usize;
    let (hold_min, step_max, gap) = (
        secs(DC_HOLD_MIN_SECS),
        secs(DC_STEP_MAX_SECS),
        secs(DC_STEP_GAP_SECS).max(1),
    );
    let frame = |i: usize| &s[i * ch..(i + 1) * ch];
    let differ = |a: &[i16], b: &[i16]| {
        a.iter()
            .zip(b)
            .any(|(&x, &y)| (x as i32 - y as i32).abs() > 2 * SILENCE as i32)
    };
    // Frames `from..to` hold one level on every channel: a narrow span, and no more motion
    // than the dither.
    let held = |from: usize, to: usize| {
        let n = to.saturating_sub(from) as f64;
        (0..ch).all(|c| {
            let it = || (from..to).map(|i| s[i * ch + c] as i32);
            let flat = it()
                .max()
                .zip(it().min())
                .is_some_and(|(max, min)| max - min <= DC_HOLD_MAX_SPAN);
            flat && {
                let mean = it().map(f64::from).sum::<f64>() / n;
                let var = it().map(|x| (f64::from(x) - mean).powi(2)).sum::<f64>() / n;
                var.sqrt() <= DC_HOLD_MAX_SD
            }
        })
    };
    // Bursts of moving frames, as [start, last] frame indices.
    let mut bursts: Vec<(usize, usize)> = Vec::new();
    for i in first + 1..end {
        if differ(frame(i), frame(i - 1)) {
            match bursts.last_mut() {
                Some((_, last)) if i - *last <= gap => *last = i,
                _ => bursts.push((i, i)),
            }
        }
    }
    let mut cut = end;
    for k in (1..bursts.len()).rev() {
        let (start, last) = bursts[k];
        let prev_last = bursts[k - 1].1;
        let after = frame((last + 1).min(end - 1));
        // The level reached at `prev_last` is held until `start`.
        let is_step = last - start <= step_max
            && start - prev_last > hold_min
            && held(prev_last, start)
            && differ(frame(start - 1), after);
        if !is_step {
            break;
        }
        cut = prev_last;
    }
    cut
}

/// "0x0186" or "0186" -> [01, 86]; an odd digit count gets a leading zero.
fn parse_id(s: &str) -> Vec<u8> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    let s = if s.len() % 2 == 1 {
        format!("0{s}")
    } else {
        s.to_string()
    };
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// One-pole DC blocker (`y = x - x[-1] + r * y[-1]`, corner `DC_BLOCK_HZ`) per channel,
/// starting from the idle level the output had before the command, so that a DC step
/// caused by the sound itself is kept as the transient it would be on AC-coupled hardware.
fn dc_block(s: &[i16], ch: usize, start: &[i32], rate: u32) -> Vec<f64> {
    let r = (-2.0 * std::f64::consts::PI * DC_BLOCK_HZ / rate as f64).exp();
    let mut prev_x: Vec<f64> = (0..ch)
        .map(|c| start.get(c).copied().unwrap_or(0) as f64)
        .collect();
    let mut prev_y = vec![0.0; ch];
    s.iter()
        .enumerate()
        .map(|(i, &x)| {
            let c = i % ch;
            let x = x as f64;
            let y = x - prev_x[c] + r * prev_y[c];
            prev_x[c] = x;
            prev_y[c] = y;
            y
        })
        .collect()
}

/// Peak and rms of a (DC-blocked) signal, in dBFS.
fn levels(s: &[f64]) -> (Option<f64>, Option<f64>) {
    if s.is_empty() {
        return (None, None);
    }
    let peak = s.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let rms = (s.iter().map(|x| x * x).sum::<f64>() / s.len() as f64).sqrt();
    let db = |x: f64| 20.0 * (x / 32768.0).log10();
    (Some(db(peak)), Some(db(rms)))
}

fn fmt_db(x: Option<f64>) -> String {
    x.map_or("   -inf".into(), |v| format!("{v:6.1} dBFS"))
}

/// The level of a master volume command on its game's 0..31 scale: DCS `55 AA vv ~vv`
/// (`vv = level * 8 + 7`), Whitestar `FE xx FD` (`xx = 2F - level`).
fn master_level(bytes: &[u8]) -> i32 {
    match *bytes {
        [0x55, 0xAA, vv, ..] => i32::from(vv.saturating_sub(7)) / 8,
        [0xFE, xx, ..] => 0x2F - i32::from(xx.min(0x2F)),
        _ => 0,
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn write_wav(
    path: &std::path::Path,
    samples: &[i16],
    channels: u16,
    rate: u32,
) -> hound::Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)?;
    let mut w16 = w.get_i16_writer(samples.len() as u32);
    for &s in samples {
        w16.write_sample(s);
    }
    w16.flush()?;
    w.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_blocker_removes_offset_and_keeps_steps() {
        let rate = 44100;
        // Idle at 2056, then a step to 10248 held for 2 s.
        let s: Vec<i16> = std::iter::repeat_n(10248, 2 * rate as usize).collect();
        let y = dc_block(&s, 1, &[2056], rate);
        assert!(
            (y[0] - 8192.0).abs() < 1.0,
            "the step is kept as a transient"
        );
        assert!(y.last().unwrap().abs() < 1.0, "the offset decays away");
        let (peak, _) = levels(&y);
        assert!((peak.unwrap() - 20.0 * (8192.0f64 / 32768.0).log10()).abs() < 0.01);
    }

    /// 0.5 s of a 440 Hz tone, mono.
    fn tone() -> Vec<i16> {
        (0..22050)
            .map(|i| (8000.0 * (i as f64 * 440.0 * std::f64::consts::TAU / 44100.0).sin()) as i16)
            .collect()
    }

    #[test]
    fn trim_stops_where_held_dc_levels_start() {
        // gnr_300 0x67's shape: the sound, then -3786 held, a step to -1891 held, and the
        // final step back to 0 (with a few samples of ringing).
        let mut s = tone();
        let sound_end = s.len();
        s.extend(std::iter::repeat_n(-3786, 3000));
        s.extend(std::iter::repeat_n(-1891, 12000));
        s.extend([48, -55, -39, 25, 33, -10, -24, 0]);
        s.extend(std::iter::repeat_n(0, 100));
        let cut = held_dc_start(&s, 1, 0, s.len(), 44100);
        assert_eq!(cut, sound_end);
    }

    #[test]
    fn trim_keeps_a_sound_without_held_dc() {
        let s = tone();
        assert_eq!(held_dc_start(&s, 1, 0, s.len(), 44100), s.len());
        // A click after a pause comes back to its level: it is sound, not a DC step.
        let mut s = tone();
        s.extend(std::iter::repeat_n(0, 4000));
        s.extend([9000, -9000, 4000]);
        s.extend(std::iter::repeat_n(0, 100));
        assert_eq!(held_dc_start(&s, 1, 0, s.len(), 44100), s.len());
    }

    /// A deterministic noise, uniform over -`amp`..=`amp` LSB (sd 0.82 for 1, 1.41 for 2).
    fn noise(n: usize, amp: i32) -> impl Iterator<Item = i16> {
        let mut x: u32 = 12345;
        (0..n).map(move |_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((x >> 16) as i32 % (2 * amp + 1) - amp) as i16
        })
    }

    #[test]
    fn trim_keeps_a_quiet_decaying_tail() {
        // apollo13 0x76's shape: the sound, then a tail around 0 that fits in the held span
        // but moves more than the dither (sd about 1.4 here), between short bursts that
        // leave it at another sample value.
        let mut s = tone();
        for _ in 0..3 {
            s.extend([3, 1]);
            s.extend(noise(4000, 2));
            s.extend([-1, -2, 9, -8, 7]);
        }
        let end = s.len();
        assert_eq!(held_dc_start(&s, 1, 0, end, 44100), end);
        // The same steps between dithered levels (sd 0.5) are held levels.
        let mut s = tone();
        let sound_end = s.len();
        for level in [-121, 19] {
            s.extend(noise(4000, 1).map(|x| x + level));
        }
        s.extend([48, -55, -39, 25, 0]);
        let end = s.len();
        assert_eq!(held_dc_start(&s, 1, 0, end, 44100), sound_end);
    }

    #[test]
    fn trim_handles_stereo() {
        let mut s: Vec<i16> = tone().iter().flat_map(|&x| [x, x / 2]).collect();
        let sound_end = s.len() / 2;
        s.extend(std::iter::repeat_n([-121, -121], 5000).flatten());
        s.extend(std::iter::repeat_n([0, 0], 50).flatten());
        assert_eq!(held_dc_start(&s, 2, 0, s.len() / 2, 44100), sound_end);
    }

    #[test]
    fn master_levels() {
        assert_eq!(master_level(&[0x55, 0xAA, 0xFF, 0x00]), 31);
        assert_eq!(master_level(&[0x55, 0xAA, 0x67, 0x98]), 12);
        assert_eq!(master_level(&[0xFE, 0x10, 0xFD]), 31);
        assert_eq!(master_level(&[0xFE, 0x2C, 0xFD]), 3);
    }

    #[test]
    fn ids() {
        assert_eq!(parse_id("0x0186"), vec![0x01, 0x86]);
        assert_eq!(parse_id("122"), vec![0x01, 0x22]);
        assert_eq!(hex(&[0x55, 0xAA, 0xF7, 0x08]), "55AAF708");
    }

    /// A recording of `secs` seconds of a loop of `period` samples, fraction included
    /// (whole harmonics of the cycle, so it is smooth across the joint), after 0.5 s of an
    /// intro, with a loop search carrying `hint`.
    fn looping_recording(period: f64, secs: f64, hint: Option<(f64, u64)>) -> Recording {
        let rate = 44100.0;
        let intro = 22050;
        let mut seed = 7u64;
        let samples = (0..(secs * rate) as usize)
            .map(|n| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let dither = (seed >> 40) as f64 / (1u64 << 24) as f64 - 0.5;
                let v = if n < intro {
                    3000.0 * (n as f64 * 0.01).sin()
                } else {
                    let ph = ((n - intro) as f64 % period) / period;
                    let k = |hz: f64| (hz * period / rate).round();
                    6000.0
                        * (std::f64::consts::TAU * k(220.0) * ph).sin()
                        * (0.6 + 0.4 * (std::f64::consts::TAU * ph).sin())
                        + 2000.0 * (std::f64::consts::TAU * k(1500.0) * ph).sin()
                };
                (v + dither).round() as i16
            })
            .collect::<Vec<i16>>();
        Recording {
            cmd: Cmd {
                id: "0x0001".into(),
                name: String::new(),
                board: "DCS".into(),
                board_no: 0,
                sends: Vec::new(),
                slot: None,
                alt: None,
            },
            frames: samples.len() as u64,
            last_loud_end: samples.len() as u64,
            samples,
            first_loud: Some(0),
            start_idle: vec![0],
            clean_start: true,
            drained_at: Some(0),
            search: Some(LoopSearch {
                dcs: None,
                hint: hint.map(|(period, ready_at)| Hint { period, ready_at }),
                next_check: 0,
                cap: u64::MAX,
            }),
            found: None,
            unresolved: None,
        }
    }

    #[test]
    fn loop_check_methods() {
        let p = 3.0 * 44100.0 + 0.4;
        // No program period: the audio's.
        let f = loop_check(&looping_recording(p, 30.0, None), 1, 44100).expect("loop");
        assert_eq!((f.method, f.l.period), ("audio", p.round() as usize));
        // The program's period, once the recording can confirm it.
        let f = loop_check(&looping_recording(p, 30.0, Some((p, 0))), 1, 44100).expect("loop");
        assert_eq!((f.method, f.l.period), ("dcs-catalog", p.round() as usize));
        // A program period of two cycles, not yet checkable: the audio's divides it.
        let r = looping_recording(p, 30.0, Some((2.0 * p, u64::MAX)));
        let f = loop_check(&r, 1, 44100).expect("loop");
        assert_eq!(f.method, "audio");
        assert!(
            f.note.as_deref().unwrap_or("").contains("2 times"),
            "{:?}",
            f.note
        );
        // A program period the audio contradicts: wait for it to be checkable...
        let r = looping_recording(p, 30.0, Some((1.7 * p, u64::MAX)));
        assert!(loop_check(&r, 1, 44100).is_none());
        // ...and once it failed, take the audio's.
        let r = looping_recording(p, 30.0, Some((1.7 * p, 0)));
        let f = loop_check(&r, 1, 44100).expect("loop");
        assert!(
            f.note.as_deref().unwrap_or("").contains("does not hold"),
            "{:?}",
            f.note
        );
    }
}
