//! rom2altsound: extract a pinball ROM's sounds by running PinMAME in-process and
//! driving its sound board directly (no keyboard, no patch to PinMAME).
//!
//! The library behind both programs: `rom2altsound` (the command line, [`cli_main`]) and
//! `rom2altsound-gui` (the window, the `gui/` package), which drives the same batch
//! engine ([`batch`]) and the same ROM check ([`inventory`]).

mod altsound;
pub mod batch;
mod bsmtfw;
mod catalog;
mod dcsrom;
mod drivers;
mod ducking;
mod extract;
mod ffi;
mod gamesound;
pub mod inventory;
mod listen;
mod looping;
mod loudness;
mod names;
pub mod progress;
mod romcheck;
mod sam;
mod sampack;
mod seqloop;
mod seqstate;
mod sha1;
mod soundsdat;
mod volume;
mod zipread;

use std::ffi::{CString, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use clap::Parser;

use extract::{Extractor, Options, VolumeInit};

const ABOUT: &str = "Turn a pinball ROM's sound board into an AltSound pack";

const LONG_ABOUT: &str = "\
Turn a pinball ROM's sound board into an AltSound pack.

rom2altsound runs PinMAME in-process, plays every sound command of the ROM on the
emulated sound board, and writes each one to its own WAV file at the volume the
game itself sets from its factory settings. Music loops are cut to their intro plus one exact cycle. Each ROM gets a
folder that VPinball's AltSound plugin reads as is: drop it as
<table folder>/altsound/<rom>/.

Supported boards: Williams/Bally DCS, Pinball 2000 (DCS2), WPC (WPC89/WPCS), System 11, Data East
(BSMT), Sega/Stern Whitestar, Bally Cheap Squeak / Turbo Cheap Squeak and the
early Bally boards (Sounds Plus -51/-56, Squawk & Talk -61). Stern SAM has no sound board: its sounds are read from the ROM image (every sound, every
song as one file, at full scale); its AltSound files, keyed by the game's sound
calls, do not play in PinMAME today (SAM sends no sound command). The boards that
take no sound command (Stern SB-300, Atari) are game-driven:
the game, left running, is asked for each of its sounds as its own code does; their
files are keyed by the game's internal sound ids and do not play in VPinball.

BSMT boards (Data East, Sega, Whitestar): with the chip's own program,
bsmt2000.zip (bsmt2000.bin, CRC c2a265af, not distributed), next to the ROM zip,
in --roms or in ./roms, the real chip runs; without it, PinMAME's older emulation.";

const AFTER_HELP: &str = "\
Examples:
  rom2altsound afm_113b                 afm_113b.zip from . or ./roms, pack in ./afm_113b/
  rom2altsound ~/roms/mm_109c.zip       ROM given by its zip, pack in ./mm_109c/
  rom2altsound afm_113b cv_20h rs_l6 --roms ~/vpinball/roms --out ~/packs
                                        three ROMs, 2 at a time, packs in ~/packs/<rom>/
  rom2altsound roms ~/vpinball/roms     check a ROM folder: what each zip holds, bad dumps,
                                        misnamed zips (rom2altsound roms --help)
  rom2altsound names afm_113b names.csv put the names typed on the listening page (its
                                        \"Export names\") in the pack (rom2altsound names --help)
  rom2altsound catalog ~/vpinball/roms --out catalog.json
                                        the sound ROM catalog of the web site (rom2altsound
                                        catalog --help)

Each ROM folder holds the WAV files, altsound.csv, g-sound.csv, altsound.ini,
manifest.json, cold-boot.json and factory-nvram/.

The pack is a starting point: every sound plays at the same level. On DCS boards
the channels and the ducking come from the ROM's own track programs (the music is
lowered under a sound by as much as the real board lowers it); on WPCS and System 11
boards they are measured chip by chip (the chips pass); on the other boards nothing
is ducked or stopped. Do the artistic pass (ducking, stops, gains) in an
AltSound editor such as VPin Studio. AltSound loops whole files only
(https://github.com/vpinball/libaltsound/issues/14): a loop with an intro of its own
(a fanfare before the loop) plays from an extended file, the intro then whole cycles
for --intro-loop-secs, once; the other loops play their body, looped. The intro +
one cycle file, with its loop points, is kept next to them.

Project: https://github.com/Le-Syl21/rom2altsound
Discord: https://discord.gg/T37DYHmt2j (channel #rom2altsound)";

#[derive(Parser)]
#[command(version, about = ABOUT, long_about = LONG_ABOUT, after_help = AFTER_HELP)]
pub struct Cli {
    /// ROMs to extract: a path to a ROM zip (e.g. ./afm_113b.zip), or a ROM set name
    /// (e.g. afm_113b) looked up in --roms
    #[arg(value_name = "ROM", required = true)]
    rom_args: Vec<String>,
    /// Where ROM set names are looked up [default: the current directory, then ./roms]
    #[arg(long, value_name = "DIR")]
    roms: Option<PathBuf>,
    /// Output directory: each ROM goes in <DIR>/<rom>/ [default: the current directory]
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// How many ROMs are extracted at the same time (one process each)
    #[arg(long, short, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..))]
    jobs: u32,
    /// Only these command ids, e.g. 0x0186,0x0002
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,
    /// Stop after N commands
    #[arg(long)]
    limit: Option<usize>,
    /// Skip the factory settings. By default each ROM is booted cold in a fresh private
    /// PinMAME directory (no nvram) so that the game runs its factory reset and writes its
    /// nvram, then warm from that nvram; every sound is recorded at the volume the game
    /// sets there (its factory volume, see --volume)
    #[arg(long)]
    no_factory: bool,
    /// Which master volume the files are at, with the factory settings. factory: the one
    /// the game itself sets at boot (DCS 55 AA 67 98 on most games, WPCS 79 0C F3,
    /// Whitestar FE 2C FD on apollo13), on every board: recorded and analysed at the
    /// reference volume, then scaled by the factory offset, measured on a few files played
    /// again at the factory volume. reference: the loudest master volume that does not clip
    /// in emulation (DCS 55 AA EF 10, Whitestar FE 11 FD, WPCS 79 0C F3). Boards without a
    /// volume stage are at full scale either way
    /// [default: factory, or reference when --dcs-volume, --whitestar-volume or --wpcs-volume
    /// is given]
    #[arg(long, value_enum, conflicts_with_all = ["no_factory", "no_volume_init", "cold_boot_only"])]
    volume: Option<VolumeMode>,
    /// Same as --volume factory (kept for older scripts)
    #[arg(long, hide = true, conflicts_with_all = ["volume", "dcs_volume", "whitestar_volume", "wpcs_volume", "no_factory", "no_volume_init", "cold_boot_only"])]
    factory_volume: bool,
    /// Do not set the DCS master volume (keep what the game or the board's reset set);
    /// implies --no-factory
    #[arg(long)]
    no_volume_init: bool,
    /// DCS master volume byte sent as `55 AA vv ~vv` (FF = 0 dB, one step = 08; Pinball
    /// 2000: the words `55AA vv~vv`); with the factory settings, the DCS reference volume
    /// (implies --volume reference unless --volume factory is given) [default: EF, or FF
    /// with --no-factory; Pinball 2000: A0 either way]
    #[arg(long, value_parser = parse_hex_byte)]
    dcs_volume: Option<u8>,
    /// The Whitestar reference volume byte, sent as `FE xx FD` (10 = level 31, the
    /// loudest; 2F = silent; implies --volume reference unless --volume factory is given)
    /// [default: 11]
    #[arg(long, conflicts_with_all = ["no_factory", "no_volume_init", "cold_boot_only"], value_parser = parse_whitestar)]
    whitestar_volume: Option<u8>,
    /// The WPCS reference volume byte, sent as `79 vv ~vv` (00 = silent, 1F = the loudest;
    /// the board ignores 20 and above; implies --volume reference unless --volume factory
    /// is given) [default: 0C]
    #[arg(long, conflicts_with_all = ["no_factory", "no_volume_init", "cold_boot_only"], value_parser = parse_hex_byte)]
    wpcs_volume: Option<u8>,
    /// Minimum emulated boot time before halting the game CPUs; the boot then lasts until
    /// no game sound byte has arrived for 3 s (and, on DCS, until the game's volume)
    #[arg(long, default_value_t = 15.0)]
    boot_secs: f64,
    /// Longest boot (emulated seconds)
    #[arg(long, default_value_t = 60.0)]
    boot_max_secs: f64,
    /// Longest file of a sound that keeps playing and whose loop is not found (emulated
    /// seconds; it is cut there)
    #[arg(long, default_value_t = 120.0)]
    max_secs: f64,
    /// How long a sound that keeps playing is recorded while looking for one exact cycle
    /// of its loop (emulated seconds; a DCS track whose program loops is recorded until its
    /// period can be confirmed, up to 900 s). 0: no loop search, cut at --max-secs
    #[arg(long, default_value_t = 240.0)]
    loop_max_secs: f64,
    /// End a recording when no sound started this long after the last command byte
    /// (emulated seconds)
    #[arg(long, default_value_t = 1.5)]
    no_sound_secs: f64,
    /// Private PinMAME directory (roms are linked or copied in, nvram/cfg are written there)
    /// [default: the user cache directory, then rom2altsound/vpm-factory/<rom>, or
    /// rom2altsound/vpm with --no-factory]
    #[arg(long)]
    vpm: Option<PathBuf>,
    /// Only boot the ROM and stop, which writes its nvram (the cold boot of the factory
    /// settings); the boot report goes to cold-boot.json
    #[arg(long)]
    cold_boot_only: bool,
    /// A sounds.dat to name the commands with [default: the one of the PinMAME version
    /// built in]
    #[arg(long)]
    sounds_dat: Option<PathBuf>,
    /// Stop command sent after each recording, e.g. 0x0000 (default: the family's
    /// "All sound off" / "Reset Sound System" from sounds.dat, else an audio CPU reset)
    #[arg(long)]
    stop: Option<String>,
    /// BSMT boards: use PinMAME's older BSMT2000 emulation (HLE) even when the chip's own
    /// program (bsmt2000.zip) is found
    #[arg(long)]
    bsmt_hle: bool,
    /// Internal: where bsmt2000.zip (or a bsmt2000/ folder) is looked for, in order
    /// [default: the ROM's directory, then ./roms]
    #[arg(long, hide = true, value_name = "DIR")]
    firmware_dir: Vec<PathBuf>,
    /// Write DC-blocked audio (10 Hz high-pass, like an AC-coupled output) instead of the
    /// raw emulated samples
    #[arg(long)]
    dc_block: bool,
    /// Keep PinMAME's real-time pacing (for comparison; default is to run unthrottled)
    #[arg(long)]
    throttled: bool,
    /// Show libpinmame info messages
    #[arg(long)]
    verbose: bool,
    /// Length of the file a loop with an intro of its own (a fanfare, then the loop) plays
    /// from: the intro then whole cycles, played once, as AltSound loops whole files only
    /// (seconds; 0: the body alone, looped, the intro not played)
    #[arg(long, default_value_t = altsound::DEFAULT_INTRO_LOOP_SECS, value_name = "SECS")]
    intro_loop_secs: f64,
    /// Do not write the AltSound pack (altsound.csv, g-sound.csv, altsound.ini, the loop
    /// points in the WAV files): only the WAV files and manifest.json
    #[arg(long)]
    no_altsound: bool,
    /// Do not write index.html, the page that lists and plays the sounds in a browser (and,
    /// for several ROMs, the one linking their pages at the output root)
    #[arg(long)]
    no_html: bool,
    /// Sound names to apply to the pack, from a names.csv (the listening page's "Export
    /// names"): they replace sounds.dat's names for the ids the file lists, in altsound.csv,
    /// manifest.json and the page (one ROM only). A file made for another sound ROM is
    /// refused (see --force-names)
    #[arg(long, value_name = "NAMES.CSV", conflicts_with = "cold_boot_only")]
    names: Option<PathBuf>,
    /// Apply --names even when the file was made for another sound ROM (its sound ROM id
    /// differs from the pack's)
    #[arg(long, requires = "names")]
    force_names: bool,
    /// Commands that play the same audio (twins, listed as `twin_of` in manifest.json) share
    /// the first one's file in the CSVs, and the twins' own WAV files are not kept. Off by
    /// default: every command keeps its own file (on DCS, twins are the same sound on two
    /// channels, so that two of them can play at once)
    #[arg(long, conflicts_with = "no_altsound")]
    merge_twins: bool,
    /// DCS: after the extraction, play the loudest music loop with one command per duck
    /// depth on top, and check that the music is lowered as much as the ROM's track
    /// programs say (a difference over 0.5 dB is flagged in manifest.json and the summary)
    #[arg(long)]
    check_ducking: bool,
    /// Write PinMAME's sound region (the DCS ROM image) to this file once booted, for the
    /// `dcs-effects` diagnostic
    #[arg(long, hide = true, value_name = "FILE")]
    dump_sound_region: Option<PathBuf>,
    /// Stern SAM diagnostic: boot from the vpm's nvram, press the coin door's Plus button N
    /// times (Minus when negative), print the DAC writes and save the nvram
    #[arg(long, hide = true, value_name = "N", allow_negative_numbers = true)]
    sam_volume_test: Option<i32>,
    /// WPCS and System 11: skip the chips pass (each sound again with only its voice chip
    /// heard, and over a music with only the music chip heard), which finds the voice lines,
    /// the ducking and the stops for the pack
    #[arg(long)]
    no_chip_check: bool,
    /// Diagnostic: mute every mixer channel whose name does not contain this (e.g. YM2151)
    #[arg(long, hide = true, value_name = "NAME")]
    solo: Option<String>,
    /// Diagnostic: send the volume (and, on Data East, the music volume and stop) once
    /// after the boot instead of before every command
    #[arg(long, hide = true)]
    no_refresh: bool,
    /// Internal: extract the single ROM given in this process; --roms is its directory and
    /// --out its own folder (libpinmame runs one machine per process)
    #[arg(long, hide = true)]
    in_process: bool,
}

impl Cli {
    /// The factory settings are on unless turned off, directly or by an option that
    /// cannot go with them.
    fn factory(&self) -> bool {
        !(self.no_factory || self.no_volume_init || self.cold_boot_only)
    }

    /// With the factory settings: the volume the files are recorded at.
    fn volume_mode(&self) -> VolumeMode {
        match self.volume {
            Some(m) => m,
            None if self.factory_volume => VolumeMode::Factory,
            None if self.dcs_volume.is_some()
                || self.whitestar_volume.is_some()
                || self.wpcs_volume.is_some() =>
            {
                VolumeMode::Reference
            }
            None => VolumeMode::Factory,
        }
    }
}

/// `--volume`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum VolumeMode {
    /// The master volume the game itself sets at boot from its factory settings.
    Factory,
    /// The loudest master volume that does not clip in emulation, per board family.
    Reference,
}

/// One ROM to extract: its set name, the directory holding its zip, its output folder.
struct Job {
    rom: String,
    roms: PathBuf,
    out: PathBuf,
}

static STATE: Mutex<Option<Extractor>> = Mutex::new(None);
static DONE: AtomicBool = AtomicBool::new(false);
static UNTHROTTLE: AtomicBool = AtomicBool::new(true);

fn with_state<R>(f: impl FnOnce(&mut Extractor) -> R) -> Option<R> {
    STATE.lock().ok()?.as_mut().map(f)
}

unsafe extern "C" fn on_audio_available(info: *mut ffi::AudioInfo, _: *mut c_void) -> c_int {
    let info = unsafe { &*info };
    with_state(|x| {
        x.rate = info.sample_rate as u32;
        x.channels = info.channels as usize;
    });
    eprintln!(
        "audio: {} Hz, {} channel(s), {} fps, {} samples/frame",
        info.sample_rate, info.channels, info.frames_per_second, info.samples_per_frame
    );
    info.samples_per_frame
}

unsafe extern "C" fn on_audio_updated(buf: *mut c_void, samples: c_int, _: *mut c_void) -> c_int {
    // Same thread as the emulator: run as fast as possible from now on.
    if UNTHROTTLE.load(Ordering::Relaxed) {
        unsafe { ffi::throttle = 0 };
    }
    with_state(|x| {
        let n = samples as usize * x.channels.max(1);
        let buf = unsafe { std::slice::from_raw_parts(buf as *const i16, n) };
        x.on_audio(buf);
        if x.done {
            DONE.store(true, Ordering::Release);
        }
    });
    samples
}

unsafe extern "C" fn on_sound_command(board: c_int, cmd: c_int, _: *mut c_void) {
    // Our own `sndbrd_data_w` (one-byte WPCS commands) reports the byte here too, from
    // inside `on_audio`, which holds the state: a blocking lock would deadlock. The game's
    // bytes never arrive while the state is held, so a busy state is always our own send.
    if let Ok(mut s) = STATE.try_lock()
        && let Some(x) = s.as_mut()
    {
        x.on_game_command(board, cmd);
    }
}

unsafe extern "C" fn on_game(game: *mut ffi::Game, user: *mut c_void) {
    let game = unsafe { &*game };
    let out = unsafe { &mut *(user as *mut (Option<String>, bool)) };
    *out = (ffi::cstr(game.clone_of), game.found != 0);
}

/// The extraction's options as the command line defines them (names, help, defaults),
/// for a front end that offers every one of them.
pub fn command() -> clap::Command {
    <Cli as clap::CommandFactory>::command()
}

/// Parses an extraction command line (`args` without the program name), as the command
/// line program would.
pub fn parse_args<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(
        std::iter::once(std::ffi::OsString::from("rom2altsound"))
            .chain(args.into_iter().map(Into::into)),
    )
}

/// The command line program: reads its arguments, runs, and exits with its code.
pub fn cli_main() {
    // Diagnostic: `rom2altsound loop-scan [--hint SECS | --hint-frames F] <wav>...` runs the loop detector on
    // existing recordings.
    if std::env::args().nth(1).as_deref() == Some("loop-scan") {
        loop_scan(std::env::args().skip(2).collect());
        return;
    }
    // DCS ducking diagnostics (docs/how-it-works.md, "Ducking, stops and channels").
    match std::env::args().nth(1).as_deref() {
        Some("dcs-effects") => return ducking::dcs_effects(std::env::args().skip(2).collect()),
        Some("duck-fit") => return ducking::duck_fit(std::env::args().skip(2).collect()),
        Some("drift-check") => return ducking::drift_check(std::env::args().skip(2).collect()),
        Some("seq-scan") => return seqloop::scan_cli(std::env::args().skip(2).collect()),
        Some("seq-audio") => return seqloop::audio_cli(std::env::args().skip(2).collect()),
        Some("page") => return listen::page_cli(std::env::args().skip(2).collect()),
        Some("names") => std::process::exit(names::cli(std::env::args().skip(2).collect())),
        Some("roms") => std::process::exit(romcheck::cli(std::env::args().skip(2).collect())),
        Some("catalog") => std::process::exit(catalog::cli(std::env::args().skip(2).collect())),
        Some("__driver-boards") => {
            return drivers::print_boards(std::env::args().skip(2).collect());
        }
        _ => {}
    }
    let cli = Cli::parse();
    if cli.bsmt_hle {
        // SAFETY: still single-threaded (nothing has been started yet). PinMAME reads it when
        // the BSMT2000 starts, and the child processes inherit it.
        unsafe { std::env::set_var(bsmtfw::HLE_ENV, "1") };
    }
    if cli.in_process {
        let [rom] = cli.rom_args.as_slice() else {
            eprintln!("error: --in-process takes exactly one ROM");
            std::process::exit(2);
        };
        let job = Job {
            rom: rom.clone(),
            roms: cli.roms.clone().unwrap_or_else(|| PathBuf::from(".")),
            out: cli.out.clone().unwrap_or_else(|| PathBuf::from(rom)),
        };
        // The sound ROM id goes in the manifest (a names file says which one it is for); a
        // names file for another sound ROM is refused before the extraction, not after.
        let sound_rom_id = names::sound_rom_id_of(&job.rom);
        let names_file = match &cli.names {
            Some(p) => match names::load(p).and_then(|n| {
                let m = serde_json::json!({ "rom": job.rom, "sound_rom_id": sound_rom_id });
                names::check_rom(&n, &m, cli.force_names, "--force-names").map(|w| (n, w))
            }) {
                Ok((n, warnings)) => {
                    for w in warnings {
                        eprintln!("warning: {w}");
                    }
                    Some((p, n))
                }
                Err(e) => {
                    eprintln!("error: --names: {e}");
                    std::process::exit(1);
                }
            },
            None => None,
        };
        if let Err(e) = run(&cli, &job) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        if !cli.cold_boot_only {
            let named =
                names::stamp_sound_rom_id(&job.out, sound_rom_id.as_deref()).and_then(|()| {
                    match &names_file {
                        Some((p, n)) => {
                            names::apply_dir(&job.out, n, p, cli.force_names, "--force-names")
                                .map(|(r, _)| println!("  {}", names::summary(&r)))
                        }
                        None => Ok(()),
                    }
                });
            if let Err(e) = named {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        if !cli.no_html && !cli.cold_boot_only {
            match listen::write_page(&job.out) {
                Ok(p) => println!("  page: {}", p.display()),
                Err(e) => eprintln!("cannot write the listening page: {e}"),
            }
        }
        return;
    }
    std::process::exit(batch::run(&cli));
}

fn run(cli: &Cli, job: &Job) -> Result<(), String> {
    let wall = Instant::now();
    let dat = soundsdat::SoundsDat::parse(&match &cli.sounds_dat {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => soundsdat::BUILT_IN.to_owned(),
    });
    std::fs::create_dir_all(&job.out).map_err(|e| e.to_string())?;
    let vpm = match &cli.vpm {
        Some(v) => std::path::absolute(v).map_err(|e| e.to_string())?,
        None if cli.factory() => work_dir()?.join("vpm-factory").join(&job.rom),
        None => work_dir()?.join("vpm"),
    };
    for d in ["roms", "nvram", "cfg"] {
        std::fs::create_dir_all(vpm.join(d)).map_err(|e| e.to_string())?;
    }
    // Stern SAM: no sound board to drive; the sounds are read from the flash image.
    if sam::sam_set(&job.rom).is_some() {
        progress::phase("sam");
        return sampack::run(cli, job, &vpm);
    }
    let factory = if cli.factory() {
        progress::phase("cold-boot");
        Some(cold_boot(cli, job, &vpm)?)
    } else {
        None
    };

    UNTHROTTLE.store(!cli.throttled, Ordering::Relaxed);
    unsafe {
        ffi::shim_log_min_level = if cli.verbose { 1 } else { 2 };
    }
    let vpm_path = ffi::vpm_path(&vpm);
    let config = ffi::Config {
        audio_format: ffi::AUDIO_FORMAT_INT16,
        sample_rate: 44100,
        vpm_path,
        on_state_updated: None,
        on_display_available: None,
        on_display_updated: None,
        on_audio_available: Some(on_audio_available),
        on_audio_updated: Some(on_audio_updated),
        on_mech_available: None,
        on_mech_updated: None,
        on_solenoid_updated: None,
        on_console_data_updated: None,
        is_key_pressed: None,
        on_log_message: ffi::shim_log as *const c_void,
        on_sound_command: Some(on_sound_command),
    };
    unsafe { ffi::PinmameSetConfig(&config) };

    // Parent set (sounds.dat sections are often under the parent name) and private ROM links.
    let rom_c = CString::new(job.rom.clone()).unwrap();
    let mut game: (Option<String>, bool) = (None, false);
    let st =
        unsafe { ffi::PinmameGetGame(rom_c.as_ptr(), on_game, &mut game as *mut _ as *mut c_void) };
    if st != ffi::STATUS_OK {
        return Err(format!("unknown game {} (status {st})", job.rom));
    }
    let parent = game.0.filter(|p| !p.is_empty()); // parents point at the nameless root driver
    for set in std::iter::once(&job.rom).chain(parent.as_ref()) {
        link_rom(&job.roms, &vpm, set)?;
    }
    let sets: Vec<&str> = std::iter::once(job.rom.as_str())
        .chain(parent.as_deref())
        .collect();
    let bsmt = bsmtfw::stage(
        &firmware_dirs(cli, job),
        &vpm.join("roms"),
        &sets,
        bsmtfw::forced_hle(cli.bsmt_hle),
    );
    for r in &bsmt.rejected {
        eprintln!("BSMT2000 program: ignored {r}");
    }
    if let Some(src) = &bsmt.source {
        eprintln!(
            "BSMT2000 program: {} (from {})",
            bsmtfw::FILE,
            src.display()
        );
    }

    *STATE.lock().unwrap() = Some(Extractor::new(
        Options {
            out_dir: job.out.clone(),
            rom: job.rom.clone(),
            parent: parent.clone(),
            only: cli.only.clone(),
            limit: cli.limit,
            volume: if cli.no_volume_init {
                VolumeInit::Game
            } else if cli.factory() {
                let r = extract::Ref {
                    dcs: cli.dcs_volume.unwrap_or(DCS_REFERENCE),
                    p2k: cli.dcs_volume.unwrap_or(P2K_REFERENCE),
                    whitestar: cli.whitestar_volume.unwrap_or(WHITESTAR_REFERENCE),
                    wpcs: cli.wpcs_volume.unwrap_or(WPCS_REFERENCE),
                };
                match cli.volume_mode() {
                    VolumeMode::Factory => VolumeInit::Factory(r),
                    VolumeMode::Reference => VolumeInit::Reference(r),
                }
            } else {
                VolumeInit::Dcs(
                    cli.dcs_volume.unwrap_or(0xFF),
                    cli.dcs_volume.unwrap_or(P2K_REFERENCE),
                )
            },
            boot_secs: cli.boot_secs,
            boot_max_secs: cli.boot_max_secs,
            max_secs: cli.max_secs,
            loop_max_secs: cli.loop_max_secs,
            no_sound_secs: cli.no_sound_secs,
            cold_boot_only: cli.cold_boot_only,
            factory,
            stop: cli.stop.clone(),
            dc_block: cli.dc_block,
            check_ducking: cli.check_ducking,
            dump_region: cli.dump_sound_region.clone(),
            verbose: cli.verbose,
            no_refresh: cli.no_refresh,
            solo: cli.solo.clone(),
            chip_check: !cli.no_chip_check,
            bsmt,
        },
        dat,
    ));

    progress::phase("boot");
    let st = unsafe { ffi::PinmameRun(rom_c.as_ptr()) };
    if st != ffi::STATUS_OK {
        return Err(format!("PinmameRun failed (status {st})"));
    }
    eprintln!(
        "running {} (parent {:?}) from {}",
        job.rom,
        parent,
        vpm.display()
    );
    let mut started = false;
    while !DONE.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(50));
        let running = unsafe { ffi::PinmameIsRunning() } != 0;
        started |= running;
        if started && !running {
            break; // emulation ended on its own (e.g. missing ROM)
        }
    }
    unsafe { ffi::PinmameStop() };

    let mut x = STATE.lock().unwrap().take().unwrap();
    if let Some(e) = &x.error {
        return Err(e.clone());
    }
    if !x.done {
        return Err("emulation stopped before extraction finished".into());
    }
    if cli.cold_boot_only {
        let report = serde_json::json!({
            "boot": x.boot_report(),
            "volume": x.volume_report(),
        });
        let path = job.out.join(COLD_BOOT_REPORT);
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(());
    }
    progress::phase("pack");
    // Factory mode: the levels at the factory volume (the files follow after the pack).
    x.apply_factory_gain();
    x.write_manifest();
    summary(&job.rom, &x, wall.elapsed().as_secs_f64());
    let counts = x.counts();
    if counts.written == 0 {
        // Nothing to pack: a run where every command stayed silent is a failure, not a
        // pack (the board was not driven, or only silent commands were asked for).
        return Err(format!(
            "no sound was recorded: {} command(s) tried, {} with sound, none written (manifest.json kept for diagnosis)",
            counts.tried, counts.with_sound
        ));
    }
    let mut extended = Vec::new();
    if !cli.no_altsound {
        let r = altsound::write_pack(
            &job.out,
            &x.results,
            cli.merge_twins,
            cli.intro_loop_secs,
            x.pack_aliases(),
        )?;
        extended = r.extended.clone();
        println!(
            "  altsound: {} row(s), {} loop(s) with loop points ({} with an intro of their own, extended to {:.0} s), {} twin(s){}, {} file(s) referenced",
            r.rows,
            r.loops_with_smpl,
            r.intro_loops_extended,
            r.intro_loop_secs,
            r.twins,
            if r.merged_twins {
                " merged"
            } else {
                " kept separate"
            },
            r.files_referenced
        );
        if let (n @ 1.., Some(kind)) = (r.aliases, r.aliases_kind) {
            println!(
                "  altsound: {n} alias row(s) ({kind}), the ids AltSound looks up in VPinball"
            );
        }
        if let Some(c) = &r.chips {
            println!(
                "  altsound (chips): voice chip -> CHANNEL 1 {} row(s) ({} callouts), {} row(s) duck the music (DUCK {}), {} STOP, {} on the music channel (they end the music)",
                c.voice_chip_rows,
                c.callout_rows,
                c.ducking_rows,
                c.duck_values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                c.stop_rows,
                c.music_channel_rows,
            );
        }
        if let Some(d) = &r.dcs {
            println!(
                "  altsound (DCS): music channel 0 {} row(s), voice channel {} -> CHANNEL 1 {} row(s), {} row(s) duck the music (DUCK {}), {} STOP",
                d.music_rows,
                d.voice_channel
                    .map_or("none".into(), |c| format!("DCS {c}")),
                d.voice_rows,
                d.ducking_rows,
                d.duck_values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                d.stop_rows,
            );
        }
    }
    // Factory mode: the pack was built from the recordings at the reference volume (loops,
    // twins); now every file goes to the factory volume.
    if let Some(r) = x.scale_files(&extended, !cli.no_altsound)?
        && r.files > 0
    {
        println!(
            "  factory volume: {} file(s) written again at the factory volume{}",
            r.files,
            if r.clamped_samples > 0 {
                format!(", {} sample(s) clamped at full scale", r.clamped_samples)
            } else {
                String::new()
            }
        );
    }
    Ok(())
}

const COLD_BOOT_REPORT: &str = "cold-boot.json";
/// The reference volumes of `--factory`: per board family, the loudest master volume at
/// which no file of our ROMs clips in emulation, apart from isolated clicks (README,
/// "Reference volume"). DCS `55 AA EF 10` (level 29/31): at `FF`, 5 to 24 files per ROM
/// clipped (mm_109c `01AB`: 99 samples; cv_20h's loop `0016`: 575); at `EF`, one sample of
/// afm_113b `0186` and cv_20h's click `03DE`, which clips at any volume.
const DCS_REFERENCE: u8 = 0xEF;
/// The Pinball 2000 reference master volume (`55AA vv ~vv`, `vv` = level * 8, FF = 31):
/// level 20, 8 levels (about 11 dB) above the factory volume `60`. At the factory volume
/// the loudest of swep1_130's 683 sounds peaks at -16.4 dBFS, so at 20 at about -5 dBFS;
/// FF (31) clips most sounds.
const P2K_REFERENCE: u8 = 0xA0;
/// Whitestar `FE 11 FD` (level 30/31): at `FE 10 FD`, apollo13 `5C` clipped 172 samples and
/// `68` 13; at `FE 11`, nothing but xfiles' click `1F`, which ignores the master volume.
const WHITESTAR_REFERENCE: u8 = 0x11;
/// WPCS `79 vv ~vv` (`vv` 00..1F; the board ignores 20 and above): `79 0C F3`, the game's
/// own factory level. With the DAC AC-coupled (shim.c `shim_dac_ac_couple`), no source
/// clips on its own even at 1F, but their sum does on five taf_l5 effects that play speech,
/// FM and DAC together (C6, C7, CD, D3, D4: 945 samples in all at 1F, 43 at 14, 2 at 0D);
/// 0C is the loudest level at which no file of taf_l5 or tz_94h clips.
const WPCS_REFERENCE: u8 = 0x0C;

/// The first half of `--factory`: wipe this ROM's nvram and cfg from the private vpm, boot
/// it cold in a child process (libpinmame runs one machine per process) until the game has
/// gone quiet, and stop it, which writes the nvram the game initialized with its factory
/// settings. That nvram is copied to `<out>/factory-nvram/<rom>.nv` before the warm boot,
/// which rewrites the vpm's copy when it stops. Returns the report stored in the manifest.
fn cold_boot(cli: &Cli, job: &Job, vpm: &Path) -> Result<serde_json::Value, String> {
    let rom = &job.rom;
    let nvram = vpm.join("nvram").join(format!("{rom}.nv"));
    let cfg = vpm.join("cfg").join(format!("{rom}.cfg"));
    for f in [&nvram, &cfg] {
        match std::fs::remove_file(f) {
            Ok(()) => eprintln!("factory: removed {}", f.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", f.display())),
        }
    }
    eprintln!(
        "factory: cold boot of {rom} (no nvram) in {}",
        vpm.display()
    );
    let t0 = Instant::now();
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe);
    child
        .arg(rom)
        .arg("--roms")
        .arg(&job.roms)
        .arg("--out")
        .arg(&job.out)
        .arg("--vpm")
        .arg(vpm)
        .arg("--boot-secs")
        .arg(cli.boot_secs.to_string())
        .arg("--boot-max-secs")
        .arg(cli.boot_max_secs.to_string())
        .arg("--cold-boot-only")
        .arg("--in-process");
    // Its boot is this process's "cold-boot" stage: it reports no progress of its own.
    child.env_remove(progress::ENV);
    for d in firmware_dirs(cli, job) {
        child.arg("--firmware-dir").arg(d);
    }
    if cli.bsmt_hle {
        child.arg("--bsmt-hle");
    }
    if let Some(dat) = &cli.sounds_dat {
        child.arg("--sounds-dat").arg(dat);
    }
    if cli.verbose {
        child.arg("--verbose");
    }
    let status = child.status().map_err(|e| format!("cold boot: {e}"))?;
    if !status.success() {
        return Err(format!("cold boot of {rom} failed ({status})"));
    }
    let size = std::fs::metadata(&nvram).map(|m| m.len()).map_err(|_| {
        format!(
            "cold boot of {rom} wrote no nvram ({} missing): the game never saved its settings; try a longer --boot-max-secs",
            nvram.display()
        )
    })?;
    let saved = job.out.join("factory-nvram").join(format!("{rom}.nv"));
    std::fs::create_dir_all(saved.parent().unwrap_or(&job.out))
        .and_then(|()| std::fs::copy(&nvram, &saved))
        .map_err(|e| format!("{}: {e}", saved.display()))?;
    let report_path = job.out.join(COLD_BOOT_REPORT);
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report_path)
            .map_err(|e| format!("{}: {e}", report_path.display()))?,
    )
    .map_err(|e| format!("{}: {e}", report_path.display()))?;
    eprintln!(
        "factory: cold boot done in {:.1} s wall ({} s emulated, ended by {}), nvram {} ({size} bytes, saved as {}); warm boot from it",
        t0.elapsed().as_secs_f64(),
        report["boot"]["secs"],
        report["boot"]["ended_by"].as_str().unwrap_or("?"),
        nvram.display(),
        saved.display(),
    );
    Ok(serde_json::json!({
        "vpm": vpm,
        "nvram": std::path::absolute(&saved).unwrap_or(saved),
        "nvram_note": "the cold boot's nvram as written, copied before the warm boot (which rewrites the vpm's copy when it stops)",
        "nvram_bytes": size,
        "cfg_written": cfg.exists(),
        "cold_boot_wall_secs": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "cold_boot": report,
    }))
}

fn summary(rom: &str, x: &Extractor, wall: f64) {
    let emulated = x.t as f64 / x.rate.max(1) as f64;
    let audio: f64 = x
        .results
        .iter()
        .filter(|r| r.file.is_some())
        .map(|r| r.duration)
        .sum();
    let c = x.counts();
    let boot = x.boot_report();
    println!(
        "{rom}: {} command(s) tried, {} with sound, {} written, {} blip(s), {} silent, {} loop(s) ({} exact cycles from the DCS catalog, {} from the audio, {} from the sound CPU's state, {} cut at --max-secs), {} retried ({} recovered); {:.1} s of audio; {:.1} s emulated in {:.1} s wall (x{:.1} real time); boot {:.1} s ({}); {} board reset(s), {} not clean, {} written file(s) clipped",
        c.tried,
        c.with_sound,
        c.written,
        c.blips,
        c.no_sound,
        c.loops,
        c.loops_exact_dcs_catalog,
        c.loops_exact_audio,
        c.loops_sequencer_state,
        c.loops_unresolved,
        c.retried,
        c.recovered_by_retry,
        audio,
        emulated,
        wall,
        emulated / wall,
        boot.secs,
        boot.ended_by,
        x.board_resets,
        c.not_clean,
        c.clipped,
    );
    for n in x.commands_from() {
        println!("  commands: {n}");
    }
    for r in x.sweep_results() {
        println!("  {}", r.line());
    }
    for r in x.refreshed() {
        println!("  before every command: {r}");
    }
    let vol = x.volume_report();
    let label = if x.is_factory() {
        "factory_volume"
    } else {
        "game_volume"
    };
    for v in &vol.commands {
        println!(
            "  {label}: board {} ({}) {} {} = level {}/{} (sent at {:.1} s)",
            v.board, v.family, v.kind, v.bytes, v.level, v.level_max, v.at
        );
    }
    if let Some(note) = &vol.note {
        println!("  {label}: no master volume seen ({note})");
    }
    if let Some(b) = x.bsmt_report() {
        println!("  BSMT2000: {}", b.label());
    }
    println!(
        "  recorded at: {}",
        x.volume_label()
            .unwrap_or_else(|| "the boards' own level (no volume command)".into())
    );
    for r in x.recorded_volumes() {
        println!(
            "    board {} ({}): {} ({})",
            r.board, r.family, r.volume, r.from
        );
    }
    if x.compares_volumes() {
        println!("  reference volume: {}", x.reference_volume());
    }
    let clipped = x.clipped_files();
    if !clipped.is_empty() {
        let list: Vec<String> = clipped
            .iter()
            .take(12)
            .map(|c| format!("{} ({})", c.id, c.clipped_samples))
            .collect();
        println!(
            "  CLIPPED in the recording (PinMAME's own mix at the recording volume, not lowered): {} file(s), {} sample(s): {}{}",
            clipped.len(),
            clipped.iter().map(|c| c.clipped_samples).sum::<usize>(),
            list.join(" "),
            if clipped.len() > 12 { " ..." } else { "" }
        );
    }
    let l = x.loudness_report();
    let fmt = |a: &loudness::Aggregate| {
        format!(
            "{} files, {:.1} s: {} integrated, loudest true peak {}",
            a.files,
            a.seconds,
            a.lufs.map_or("n/a".into(), |v| format!("{v:.1} LUFS")),
            a.true_peak_dbtp
                .map_or("n/a".into(), |v| format!("{v:.1} dBTP")),
        )
    };
    println!("  loudness ({}):", l.measured_as);
    println!("    all sounds:        {}", fmt(&l.all));
    println!("    without loops:     {}", fmt(&l.excluding_loops));
    println!(
        "    median file:       {} (without loops)",
        l.median_file_lufs
            .map_or("n/a".into(), |v| format!("{v:.1} LUFS"))
    );
    if let Some(n) = &l.master_volume_check {
        println!("    master volume check: {n}");
    }
    if x.compares_volumes() {
        for o in x.offsets() {
            println!(
                "    factory offset: board {} ({}) factory {} vs reference {}: {} (spread {}, {} file(s))",
                o.board,
                o.family,
                o.factory_volume,
                o.reference_volume,
                o.factory_offset_db
                    .map_or("n/a".into(), |v| format!("{v:+.1} dB")),
                o.spread_db.map_or("n/a".into(), |v| format!("{v:.1} dB")),
                o.samples.len(),
            );
        }
        if let Some(n) = x.offset_note() {
            println!("    factory offset: {n}");
        }
        for g in x.gains().unwrap_or_default() {
            println!(
                "    factory gain: board {} ({}): {}{} ({} file(s)): {}",
                g.board,
                g.family,
                g.gain_db
                    .map_or("not applied".into(), |d| format!("{d:+.3} dB")),
                g.gain.map_or(String::new(), |k| format!(" (x{k:.5})")),
                g.files,
                g.note
            );
        }
        let own: Vec<String> = x
            .own_gains()
            .iter()
            .map(|o| {
                format!(
                    "{} {}",
                    o.id,
                    o.gain_db
                        .map_or("0 dB (not measured)".into(), |d| format!("{d:+.2} dB"))
                )
            })
            .collect();
        if !own.is_empty() && x.gains().is_some() {
            println!(
                "    factory gain: own gain for the file(s) that do not follow the master volume: {}",
                own.join(", ")
            );
        }
        for o in x.offsets() {
            let e: Vec<f64> = o
                .samples
                .iter()
                .filter_map(|s| s.scaled_minus_replay_db)
                .collect();
            if let (Some(lo), Some(hi)) = (
                e.iter().copied().reduce(f64::min),
                e.iter().copied().reduce(f64::max),
            ) {
                println!(
                    "    factory check: board {}: scaled files minus their replays at the factory volume {lo:+.3} to {hi:+.3} dB ({} file(s))",
                    o.board,
                    e.len()
                );
            }
        }
        match &l.as_shipped {
            Some(s) => println!(
                "    as shipped ({:+.1} dB): all {}, without loops {}, median file {}, loudest true peak {}",
                s.factory_offset_db,
                fmt_lufs(s.all_lufs),
                fmt_lufs(s.excluding_loops_lufs),
                fmt_lufs(s.median_file_lufs),
                s.loudest_true_peak_dbtp
                    .map_or("n/a".into(), |v| format!("{v:.1} dBTP")),
            ),
            None if x.is_reference() => {
                println!("    as shipped: n/a (factory offset not measured)")
            }
            None => {}
        }
    }
    if let Some(r) = x.mix_report() {
        println!(
            "  chips: {} sound(s) over music {}: {} voice line(s), {} on the music chip, {} duck the music, {} stop it{}",
            r.measured,
            if r.music.is_empty() { "-" } else { &r.music },
            r.voice_lines,
            r.on_music_chip,
            r.ducking,
            r.stopping,
            r.note.as_ref().map_or(String::new(), |n| format!(" ({n})"))
        );
    }
    if let Some(r) = x.duck_report() {
        match &r.note {
            Some(n) if r.checks.is_empty() => println!("  ducking check: {n}"),
            _ => {
                println!(
                    "  ducking check: music {}, {} depth(s), {} mismatch(es) over {} dB",
                    r.music,
                    r.checks.len(),
                    r.mismatches,
                    r.tolerance_db
                );
                for c in &r.checks {
                    println!(
                        "    {} {:+.2} dB predicted, {}{}",
                        c.id,
                        c.predicted_db,
                        c.measured_db
                            .map_or("not measured".into(), |m| format!("{m:+.2} dB measured")),
                        if c.mismatch {
                            "  MISMATCH".to_string()
                        } else {
                            c.note.as_ref().map_or(String::new(), |n| format!(" ({n})"))
                        }
                    );
                }
            }
        }
    }
    if !l.excluded_ignoring_master_volume.is_empty() {
        println!(
            "    left out of the totals (do not follow the master volume): {}",
            l.excluded_ignoring_master_volume.join(" ")
        );
    }
}

fn fmt_lufs(x: Option<f64>) -> String {
    x.map_or("n/a".into(), |v| format!("{v:.1} LUFS"))
}

fn parse_whitestar(s: &str) -> Result<u8, String> {
    let v = parse_hex_byte(s)?;
    (0x10..=0x2F)
        .contains(&v)
        .then_some(v)
        .ok_or_else(|| format!("{v:02X}: the Whitestar volume is 10 (loudest) to 2F (silent)"))
}

fn parse_hex_byte(s: &str) -> Result<u8, String> {
    u8::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16)
        .map_err(|e| e.to_string())
}

/// Where the private PinMAME directories go when `--vpm` is not given: the user cache
/// directory (`$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS,
/// `%LOCALAPPDATA%` on Windows), never the current directory.
fn work_dir() -> Result<PathBuf, String> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let cache = if cfg!(windows) {
        var("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Caches"))
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|h| h.join(".cache")))
    }
    .ok_or("no user cache directory found (HOME / LOCALAPPDATA unset): pass --vpm")?;
    Ok(cache.join("rom2altsound"))
}

/// Where the BSMT2000's program is looked for: as handed down by the parent process, else
/// the ROM's directory, then `./roms`.
fn firmware_dirs(cli: &Cli, job: &Job) -> Vec<PathBuf> {
    if !cli.firmware_dir.is_empty() {
        return cli.firmware_dir.clone();
    }
    vec![job.roms.clone(), PathBuf::from("roms")]
}

/// Puts `<roms>/<set>.zip` into the private `<vpm>/roms`, if it exists: a symbolic link
/// on Unix, a copy on Windows (where creating links needs a privilege).
fn link_rom(roms: &Path, vpm: &Path, set: &str) -> Result<(), String> {
    let src = std::path::absolute(roms.join(format!("{set}.zip"))).map_err(|e| e.to_string())?;
    let dst = vpm.join("roms").join(format!("{set}.zip"));
    if !src.exists() {
        return Ok(());
    }
    let _ = std::fs::remove_file(&dst);
    #[cfg(unix)]
    let r = std::os::unix::fs::symlink(&src, &dst);
    #[cfg(not(unix))]
    let r = std::fs::copy(&src, &dst).map(|_| ());
    r.map_err(|e| format!("{}: {e}", dst.display()))
}

/// `loop-scan`: the loop detector on WAV files, one line per file.
fn loop_scan(args: Vec<String>) {
    let mut hint = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        // A known period: in seconds, or in DCS frames (7.68 ms).
        if a == "--hint" || a == "--hint-frames" {
            let unit = if a == "--hint" {
                1.0
            } else {
                dcsrom::SAMPLES_PER_FRAME_44K1 / 44100.0
            };
            hint = it
                .next()
                .and_then(|h| h.parse::<f64>().ok())
                .map(|h| h * unit);
            continue;
        }
        let Ok(mut r) = hound::WavReader::open(&a) else {
            eprintln!("{a}: cannot read");
            continue;
        };
        let spec = r.spec();
        let s: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
        let ch = spec.channels as usize;
        let rate = spec.sample_rate;
        let t0 = Instant::now();
        let x = looping::mono(&s, ch);
        let found = looping::find(&x, rate, hint.map(|h| h * rate as f64));
        let secs = |n: usize| n as f64 / rate as f64;
        match found {
            Some(l) => {
                let seam = looping::seam(&s, ch, l.intro, l.period);
                println!(
                    "{a}: {:.2} s: intro {:.3} s, period {} ({:.3} s, exact {:.2}), residual {:.1} dB, matched {:.1} s, seam joint {} natural {} error {} (body p99 {} max {}) ({:.2} s cpu)",
                    secs(x.len()),
                    secs(l.intro),
                    l.period,
                    secs(l.period),
                    l.period_exact,
                    l.residual_db,
                    secs(l.matched),
                    seam.joint,
                    seam.natural,
                    seam.error,
                    seam.p99,
                    seam.max,
                    t0.elapsed().as_secs_f64()
                );
            }
            None => println!(
                "{a}: {:.2} s: no loop ({:.2} s cpu)",
                secs(x.len()),
                t0.elapsed().as_secs_f64()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(args: &[&str]) -> VolumeMode {
        let argv = std::iter::once("rom2altsound").chain(args.iter().copied());
        Cli::try_parse_from(argv).unwrap().volume_mode()
    }

    #[test]
    fn factory_volume_by_default() {
        assert_eq!(mode(&["afm_113b"]), VolumeMode::Factory);
        assert_eq!(mode(&["afm_113b", "--factory-volume"]), VolumeMode::Factory);
        assert_eq!(
            mode(&["afm_113b", "--volume", "reference"]),
            VolumeMode::Reference
        );
        // A reference byte asks for the reference volume, unless factory is explicit.
        assert_eq!(
            mode(&["taf_l5", "--wpcs-volume", "16"]),
            VolumeMode::Reference
        );
        assert_eq!(
            mode(&["taf_l5", "--wpcs-volume", "16", "--volume", "factory"]),
            VolumeMode::Factory
        );
        assert!(
            Cli::try_parse_from(["rom2altsound", "x", "--volume", "reference", "--no-factory"])
                .is_err()
        );
    }

    #[test]
    fn names_options() {
        let c = Cli::try_parse_from(["rom2altsound", "afm_113b", "--names", "n.csv"]).unwrap();
        assert_eq!(c.names.as_deref(), Some(Path::new("n.csv")));
        assert!(!c.force_names);
        assert!(
            Cli::try_parse_from([
                "rom2altsound",
                "afm_113b",
                "--names",
                "n.csv",
                "--force-names"
            ])
            .unwrap()
            .force_names
        );
        assert!(Cli::try_parse_from(["rom2altsound", "afm_113b", "--force-names"]).is_err());
        assert!(
            Cli::try_parse_from(["rom2altsound", "x", "--names", "n.csv", "--cold-boot-only"])
                .is_err()
        );
    }
}
