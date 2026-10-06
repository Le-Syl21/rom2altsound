//! The AltSound pack: turns the extraction's WAV files into a folder that VPinball's
//! AltSound plugin (libaltsound) plays as is, dropped as `<table>/altsound/<rom>/`.
//!
//! - Loops: `<id>-<rom>.wav` (intro + one exact cycle) gets a WAV `smpl` chunk carrying the
//!   loop's start and end, sample-accurate, for editors and players that honour it.
//!   libaltsound only loops a whole file (libaltsound issue #14), so the CSVs point at:
//!   - for a loop with an intro of its own (a fanfare before the loop: AFM `0009` against
//!     `000A`, which share the loop), `<id>-<rom>-extended.wav`: the intro then whole
//!     cycles, sample-exact, `--intro-loop-secs` long (5 minutes by default), played once
//!     (LOOP 0) and carrying the same `smpl` loop points;
//!   - for the others, `<id>-<rom>-loop.wav` (the body alone), looped (LOOP 100).
//! - Twins: some ROMs hold commands that play the same audio (AFM lists every sound effect
//!   twice). They are detected with a strict test and recorded in `manifest.json`
//!   (`twin_of`); each command keeps its own file unless `--merge-twins` is given.
//! - `altsound.csv` (the "AltSound" format), `g-sound.csv` (the "G-Sound" format) and an
//!   `altsound.ini` that selects the AltSound format and turns off the ROM volume control:
//!   every file is recorded at the same reference level, so GAIN is 100 everywhere.
//! - DCS: CHANNEL, DUCK and STOP (AltSound), TYPE and the ducking profiles (G-Sound) come
//!   from the track programs (`dcsrom::command_effects`, in `manifest.json` as `dcs`): the
//!   music is DCS channel 0, the one voice channel without twins is the jingle channel
//!   (one line at a time), and DUCK is the depth the program lowers the music by. Other
//!   boards keep DUCK 100 and STOP 0: their sound programs are CPU code, nothing says how
//!   they mix. The rest of the artistic pass is left to an editor such as VPin Studio.

use std::fs;
use std::io;
use std::path::Path;

use crate::dcsrom;
use crate::extract::SoundInfo;

pub const ALTSOUND_CSV: &str = "altsound.csv";
pub const GSOUND_CSV: &str = "g-sound.csv";
pub const ALTSOUND_INI: &str = "altsound.ini";

/// Twin test thresholds. A pair is a twin only when all three hold.
/// - Lengths within one sample of each other.
/// - Integrated loudness within 0.01 LU.
/// - After aligning the two files to a fraction of a sample, the residual is at the level
///   of 16-bit dither / resampling noise relative to the signal.
pub const TWIN_MAX_LENGTH_DIFF: usize = 1;
pub const TWIN_MAX_LUFS_DIFF: f64 = 0.01;
pub const TWIN_MAX_RESIDUAL_DB: f64 = -60.0;
/// Integer lags tried before the sub-sample refinement (the boards can start a sound one
/// or two samples apart).
///
/// Measured on afm_113b: the 303 pairs that pass the length and loudness tests are at
/// -64 to -80 dB once aligned; different sounds of nearly the same length are above 0 dB.
const TWIN_MAX_LAG: isize = 3;

/// A loop is played from an extended file (intro + cycles) when its intro holds at least
/// this much audio of its own, before the repetition starts. Shorter, the "intro" is only
/// where the loop detector chose to start the body (it keeps one window clear of the
/// first cycle's edge, then looks for the quietest joint within a second).
pub const OWN_INTRO_MIN_SECS: f64 = 0.05;
/// ...and when that part carries sound: a peak above this (LSB, about -60 dBFS).
pub const OWN_INTRO_MIN_PEAK: i32 = 32;
/// Default length of an extended file (`--intro-loop-secs`): long enough for a game mode.
pub const DEFAULT_INTRO_LOOP_SECS: f64 = 300.0;

/// Which file a row plays, and why (`manifest.json`, `pack.file_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A loop with an intro of its own: intro + whole cycles, played once.
    IntroLoopExtended,
    /// A loop without an intro (or with `--intro-loop-secs 0`): the body alone, looped.
    BodyLoop,
    /// A sound that ends, or one cut at `--max-secs`: played once.
    OneShot,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::IntroLoopExtended => "intro_loop_extended",
            FileKind::BodyLoop => "body_loop",
            FileKind::OneShot => "one_shot",
        }
    }
}

/// A sound found to be the same audio as an earlier one.
#[derive(Debug, Clone, PartialEq)]
pub struct Twin {
    /// Index (in the sounds slice) of this sound.
    pub index: usize,
    /// Index of the earlier sound it duplicates (never itself a twin).
    pub of: usize,
    /// Residual energy after sub-sample alignment, relative to the signal (dB).
    pub residual_db: f64,
    /// Sub-sample offset that minimised the residual (samples).
    pub lag: f64,
    /// Length difference (samples).
    pub length_diff: usize,
    /// Integrated loudness difference (LU).
    pub lufs_diff: f64,
}

/// What the pack writer did, for the summary and the manifest.
#[derive(Debug, Default, serde::Serialize)]
pub struct PackReport {
    pub rows: usize,
    pub loops_with_smpl: usize,
    /// Loops with an intro of their own, played from an extended file.
    pub intro_loops_extended: usize,
    pub intro_loop_secs: f64,
    pub twins: usize,
    pub merged_twins: bool,
    pub files_referenced: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dcs: Option<DcsPack>,
}

/// DCS: how the track programs were mapped onto the pack.
#[derive(Debug, Default, serde::Serialize)]
pub struct DcsPack {
    /// The DCS channel whose rows are AltSound's jingle channel (CHANNEL 1) and G-Sound
    /// callouts: the channel holding most voice lines, without twins.
    pub voice_channel: Option<u8>,
    pub music_rows: usize,
    pub voice_rows: usize,
    pub ducking_rows: usize,
    /// Distinct DUCK values, deepest last.
    pub duck_values: Vec<u32>,
    pub stop_rows: usize,
    /// G-Sound ducking profiles: profile n (from 1) lowers the music to `music:<value>`.
    pub callout_profiles: Vec<u32>,
    pub sfx_profiles: Vec<u32>,
    /// What the pack formats cannot hold.
    pub limits: Vec<String>,
}

/// One row of both CSVs.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: u32,
    pub kind: Kind,
    pub looped: bool,
    /// Keeps playing, but its file is not looped in altsound.csv: music cut at
    /// `--max-secs` without an exact loop found, or a loop with an intro played from its
    /// extended file. "music" in g-sound.csv all the same (where every music file loops).
    pub continuous: bool,
    pub name: String,
    pub fname: String,
    /// AltSound CHANNEL: 0 music, 1 jingle, -1 sound effect / voice (polyphonic).
    pub channel: i8,
    /// AltSound DUCK: the music's volume (%) while this row plays; 100 = not ducked.
    pub duck: u32,
    /// AltSound STOP: a jingle that stops the music.
    pub stop: bool,
    /// G-Sound TYPE and DUCKING_PROFILE (0: none).
    pub gtype: GType,
    pub profile: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Music,
    Callout,
    Sfx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GType {
    Music,
    Callout,
    Sfx,
}

impl GType {
    pub fn as_str(self) -> &'static str {
        match self {
            GType::Music => "music",
            GType::Callout => "callout",
            GType::Sfx => "sfx",
        }
    }
}

impl Row {
    /// A row without DCS data: music on channel 0, everything else polyphonic, no
    /// ducking, no stop. In G-Sound every "music" sample loops, so only loops are music
    /// there; a one-shot "Music:" sound (a jingle) is an SFX.
    pub fn plain(
        id: u32,
        kind: Kind,
        looped: bool,
        continuous: bool,
        name: String,
        fname: String,
    ) -> Self {
        let gtype = match kind {
            Kind::Music if looped || continuous => GType::Music,
            Kind::Callout => GType::Callout,
            _ => GType::Sfx,
        };
        Row {
            id,
            kind,
            looped,
            continuous,
            name,
            fname,
            channel: if kind == Kind::Music { 0 } else { -1 },
            duck: 100,
            stop: false,
            gtype,
            profile: 0,
        }
    }
}

/// AltSound DUCK for a level change of `units` on the music (negative: lower):
/// `round(100 * 0.9733^-units)`, 100 when the music is not lowered.
pub fn duck_percent(units: f64) -> u32 {
    (100.0 * dcsrom::duck_factor(units))
        .round()
        .clamp(0.0, 100.0) as u32
}

/// Classifies a sound from its sounds.dat name and whether it keeps playing: loops and
/// sounds that never end are music; "Music:" names are music too; voice lines (quoted
/// text or "VOX:") are callouts; the rest is SFX.
pub fn classify(name: &str, looped: bool) -> Kind {
    let n = name.trim_start();
    let lower = n.to_ascii_lowercase();
    if looped || lower.starts_with("music:") {
        Kind::Music
    } else if n.starts_with('"') || lower.starts_with("vox:") || lower.starts_with("voice:") {
        Kind::Callout
    } else {
        Kind::Sfx
    }
}

/// A CSV-safe NAME: libaltsound splits on commas and deletes double quotes, so both go.
pub fn csv_name(name: &str, id: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| *c != '"' && *c != '\'' && !c.is_control())
        .map(|c| if c == ',' { ' ' } else { c })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        format!("sound {id}")
    } else {
        cleaned
    }
}

/// The `altsound.csv` text (header + rows).
pub fn altsound_csv(rows: &[Row]) -> String {
    let mut s = String::from("ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME\r\n");
    for r in rows {
        // CHANNEL: 0 = music (one at a time, replaces the current music); 1 = jingle (one
        // at a time, replaces the current jingle); empty = -1, a polyphonic voice/SFX
        // stream. DUCK: the music's volume while this one plays (100 = not ducked);
        // STOP 1: this jingle stops the music.
        let channel = match r.channel {
            -1 => String::new(),
            c => c.to_string(),
        };
        let looped = if r.looped { 100 } else { 0 };
        s.push_str(&format!(
            "0x{:04X},{channel},{},100,{looped},{},{},{}\r\n",
            r.id,
            r.duck,
            u8::from(r.stop),
            r.name,
            r.fname
        ));
    }
    s
}

/// The `g-sound.csv` text (header + rows).
pub fn gsound_csv(rows: &[Row]) -> String {
    let mut s = String::from("ID,TYPE,GAIN,DUCKING_PROFILE,FNAME\r\n");
    for r in rows {
        s.push_str(&format!(
            "0x{:04X},{},100,{},{}\r\n",
            r.id,
            r.gtype.as_str(),
            r.profile,
            r.fname
        ));
    }
    s
}

/// Numbers the G-Sound ducking profiles: per type, one profile per distinct DUCK value
/// below 100 (the lightest first), and each row gets the profile of its own value.
/// Returns the callout and sfx profiles (`music:<value>`).
pub fn assign_profiles(rows: &mut [Row]) -> (Vec<u32>, Vec<u32>) {
    let values = |t: GType, rows: &[Row]| {
        let mut v: Vec<u32> = rows
            .iter()
            .filter(|r| r.gtype == t && r.duck < 100)
            .map(|r| r.duck)
            .collect();
        v.sort_unstable_by(|a, b| b.cmp(a));
        v.dedup();
        v
    };
    let (callout, sfx) = (values(GType::Callout, rows), values(GType::Sfx, rows));
    for r in rows.iter_mut() {
        let list = match r.gtype {
            GType::Callout => &callout,
            GType::Sfx => &sfx,
            GType::Music => continue,
        };
        r.profile = list
            .iter()
            .position(|&v| v == r.duck)
            .map_or(0, |p| p as u32 + 1);
    }
    (callout, sfx)
}

/// `altsound.ini` for a DCS pack: as `altsound_ini`, but the callout and sfx types only
/// duck the music, with one profile per depth found in the ROM (`assign_profiles`).
/// libaltsound refuses a type that ducks without a profile, so a type with none ducks
/// nothing.
pub fn altsound_ini_dcs(callout: &[u32], sfx: &[u32]) -> String {
    let section = |ty: &str, values: &[u32]| {
        let mut s = format!("[{ty}]\n");
        s += if values.is_empty() {
            "ducks =\n"
        } else {
            "ducks = music\n"
        };
        if ty == "callout" {
            s += "pauses =\nstops =\n";
        }
        s += "group_vol = 100\n\n";
        if !values.is_empty() {
            s += &format!("[{ty}_ducking_profiles]\n;profile0 is reserved\n");
            for (i, v) in values.iter().enumerate() {
                s += &format!("ducking_profile{} = music:{v}\n", i + 1);
            }
            s += "\n";
        }
        s
    };
    format!(
        "; altsound.ini - written by rom2altsound (https://github.com/Le-Syl21/rom2altsound)\n\
         ;\n\
         ; rom_volume_ctrl = 0: every sample was recorded at the same reference volume, so the\n\
         ; ROM's own volume commands must not change the playback level.\n\
         ; format: this folder holds both altsound.csv and g-sound.csv; switch to\n\
         ; \"g-sound\" to use the latter.\n\
         ; The ducking (DUCK in altsound.csv, the G-Sound profiles below) is the ROM's own:\n\
         ; each profile lowers the music as much as the DCS track programs do.\n\
         \n\
         [system]\n\
         record_sound_cmds = 0\n\
         rom_volume_ctrl = 0\n\
         cmd_skip_count = 0\n\
         \n\
         [format]\n\
         format = altsound\n\
         \n\
         [logging]\n\
         logging_level = Error\n\
         \n\
         ; G-Sound behaviors (used only with format = g-sound)\n\
         [music]\n\
         group_vol = 100\n\
         \n\
         {}{}\
         [solo]\n\
         stops = music, overlay, callout\n\
         group_vol = 100\n\
         \n\
         [overlay]\n\
         ducks = music, sfx\n\
         group_vol = 100\n\
         \n\
         [overlay_ducking_profiles]\n\
         ;profile0 is reserved\n\
         ducking_profile1 = sfx:65, music:65\n\
         ducking_profile2 = sfx:80, music:50\n",
        section("callout", callout),
        section("sfx", sfx),
    )
}

/// `altsound.ini`: libaltsound's own template, with the AltSound format selected and the
/// ROM volume control off (the files already carry the right relative levels).
pub fn altsound_ini() -> String {
    "; altsound.ini - written by rom2altsound (https://github.com/Le-Syl21/rom2altsound)\n\
     ;\n\
     ; rom_volume_ctrl = 0: every sample was recorded at the same reference volume, so the\n\
     ; ROM's own volume commands must not change the playback level.\n\
     ; format: this folder holds both altsound.csv and g-sound.csv; switch to\n\
     ; \"g-sound\" to use the latter.\n\
     \n\
     [system]\n\
     record_sound_cmds = 0\n\
     rom_volume_ctrl = 0\n\
     cmd_skip_count = 0\n\
     \n\
     [format]\n\
     format = altsound\n\
     \n\
     [logging]\n\
     logging_level = Error\n\
     \n\
     ; G-Sound behaviors (used only with format = g-sound)\n\
     [music]\n\
     group_vol = 100\n\
     \n\
     [callout]\n\
     ducks = sfx, music, overlay\n\
     pauses =\n\
     stops =\n\
     group_vol = 100\n\
     \n\
     [callout_ducking_profiles]\n\
     ;profile0 is reserved\n\
     ducking_profile1 = sfx:65, music:50, overlay:50\n\
     \n\
     [sfx]\n\
     ducks = music\n\
     group_vol = 100\n\
     \n\
     [sfx_ducking_profiles]\n\
     ;profile0 is reserved\n\
     ducking_profile1 = music:50\n\
     \n\
     [solo]\n\
     stops = music, overlay, callout\n\
     group_vol = 100\n\
     \n\
     [overlay]\n\
     ducks = music, sfx\n\
     group_vol = 100\n\
     \n\
     [overlay_ducking_profiles]\n\
     ;profile0 is reserved\n\
     ducking_profile1 = sfx:65, music:65\n\
     ducking_profile2 = sfx:80, music:50\n"
        .replace("\n     ", "\n")
}

/// Appends a `smpl` chunk with one forward loop `[start, end]` (sample frames, `end`
/// inclusive, as the chunk defines it) to a RIFF/WAVE file, replacing any earlier one.
pub fn write_smpl(path: &Path, start: u32, end: u32) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let bad = |m: &str| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {m}", path.display()),
        )
    };
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    // Copy every chunk but an existing smpl.
    let mut out = bytes[..12].to_vec();
    let mut pos = 12;
    let mut rate = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end_chunk = pos + 8 + size + (size & 1);
        if end_chunk > bytes.len() + (size & 1) {
            return Err(bad("truncated chunk"));
        }
        let end_chunk = end_chunk.min(bytes.len());
        if id == b"fmt " && size >= 8 {
            rate = Some(u32::from_le_bytes(
                bytes[pos + 12..pos + 16].try_into().unwrap(),
            ));
        }
        if id != b"smpl" {
            out.extend_from_slice(&bytes[pos..end_chunk]);
        }
        pos = end_chunk;
    }
    let rate = rate.ok_or_else(|| bad("no fmt chunk"))?;
    out.extend_from_slice(&smpl_chunk(rate, start, end));
    let riff_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    fs::write(path, out)
}

/// The `smpl` chunk bytes (header included) for one infinite forward loop.
pub fn smpl_chunk(rate: u32, start: u32, end: u32) -> Vec<u8> {
    let mut c = Vec::with_capacity(68);
    c.extend_from_slice(b"smpl");
    c.extend_from_slice(&60u32.to_le_bytes());
    let period_ns = (1_000_000_000f64 / rate as f64).round() as u32;
    for v in [0u32, 0, period_ns, 60, 0, 0, 0, 1, 0] {
        // manufacturer, product, sample period, MIDI unity note, pitch fraction,
        // SMPTE format, SMPTE offset, number of loops, sampler data
        c.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0u32, 0, start, end, 0, 0] {
        // cue point id, type (0 = forward), start, end, fraction, play count (0 = forever)
        c.extend_from_slice(&v.to_le_bytes());
    }
    c
}

/// How many cycles of `period` frames follow an intro of `intro` frames in an extended
/// file of at least `total` frames (at least one).
pub fn extended_cycles(intro: usize, period: usize, total: usize) -> usize {
    total.saturating_sub(intro).div_ceil(period.max(1)).max(1)
}

/// The extended file of a loop: `samples` (interleaved, `ch` channels) holds the intro
/// (`intro` frames) then one body (`period` frames); the result is the intro then `cycles`
/// copies of the body, back to back. Each joint is the body's own end-to-start joint, the
/// one a player looping the body file plays.
pub fn extend_loop(
    samples: &[i16],
    ch: usize,
    intro: usize,
    period: usize,
    cycles: usize,
) -> Vec<i16> {
    let ch = ch.max(1);
    let (a, b) = (intro * ch, (intro + period) * ch);
    let body = &samples[a..b];
    let mut out = Vec::with_capacity(a + body.len() * cycles);
    out.extend_from_slice(&samples[..a]);
    for _ in 0..cycles {
        out.extend_from_slice(body);
    }
    out
}

/// Writes interleaved 16-bit samples as a WAV.
fn write_wav(path: &Path, samples: &[i16], channels: u16, rate: u32) -> io::Result<()> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let to_io = |e: hound::Error| io::Error::other(e.to_string());
    let mut w = hound::WavWriter::create(path, spec).map_err(to_io)?;
    let mut w16 = w.get_i16_writer(samples.len() as u32);
    for &v in samples {
        w16.write_sample(v);
    }
    w16.flush().map_err(to_io)?;
    w.finalize().map_err(to_io)
}

/// The extended file of a loop that has an intro of its own: `Ok(Some(name))` when
/// written, `Ok(None)` when the loop has no intro of its own (`OWN_INTRO_*`).
fn write_extended(
    out_dir: &Path,
    file: &str,
    l: &crate::extract::LoopInfo,
    secs: f64,
) -> io::Result<Option<(String, usize)>> {
    let path = out_dir.join(file);
    let mut r = hound::WavReader::open(&path).map_err(|e| io::Error::other(e.to_string()))?;
    let spec = r.spec();
    let (ch, rate) = (spec.channels as usize, spec.sample_rate);
    if (l.repeats_from_samples as f64) < OWN_INTRO_MIN_SECS * rate as f64 {
        return Ok(None);
    }
    let samples: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
    let own = (l.repeats_from_samples * ch).min(samples.len());
    let peak = samples[..own]
        .iter()
        .map(|&v| (v as i32).abs())
        .max()
        .unwrap_or(0);
    if peak <= OWN_INTRO_MIN_PEAK || (l.intro_samples + l.period_samples) * ch > samples.len() {
        return Ok(None);
    }
    let total = (secs * rate as f64).round() as usize;
    let cycles = extended_cycles(l.intro_samples, l.period_samples, total);
    let out = extend_loop(&samples, ch, l.intro_samples, l.period_samples, cycles);
    let name = extended_name(file);
    let p = out_dir.join(&name);
    write_wav(&p, &out, ch as u16, rate)?;
    let start = l.intro_samples as u32;
    let end = (l.intro_samples + l.period_samples).saturating_sub(1) as u32;
    write_smpl(&p, start, end)?;
    Ok(Some((name, cycles)))
}

/// `0x0009-afm_113b.wav` -> `0x0009-afm_113b-extended.wav`.
pub fn extended_name(file: &str) -> String {
    match file.strip_suffix(".wav") {
        Some(stem) => format!("{stem}-extended.wav"),
        None => format!("{file}-extended.wav"),
    }
}

/// Reads a WAV as interleaved 16-bit samples.
fn read_wav(path: &Path) -> Option<(Vec<i16>, usize)> {
    let mut r = hound::WavReader::open(path).ok()?;
    let ch = r.spec().channels as usize;
    let s: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
    Some((s, ch))
}

/// Mono mixdown in f64.
fn mono(s: &[i16], ch: usize) -> Vec<f64> {
    let ch = ch.max(1);
    s.chunks(ch)
        .map(|f| f.iter().map(|&v| v as f64).sum::<f64>() / ch as f64)
        .collect()
}

/// Half width (taps) of the windowed-sinc interpolator used for sub-sample alignment.
const SINC_HALF: isize = 24;

/// Windowed-sinc (Blackman) taps that evaluate a signal `frac` (0..1) samples after an
/// index, for k in `1 - SINC_HALF ..= SINC_HALF`.
fn sinc_taps(frac: f64) -> Vec<f64> {
    (1 - SINC_HALF..=SINC_HALF)
        .map(|k| {
            let x = k as f64 - frac;
            let sinc = if x == 0.0 {
                1.0
            } else {
                (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
            };
            let t = (x / SINC_HALF as f64 + 1.0) / 2.0; // 0..1 across the window
            let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos()
                + 0.08 * (4.0 * std::f64::consts::PI * t).cos();
            sinc * w
        })
        .collect()
}

/// Residual energy of `b` against `a` over `range` (indices of `a`) after shifting `b` by
/// `lag` samples (fractional lags are band-limited interpolation), relative to `a`'s
/// energy there (dB). Samples whose interpolation would leave `b` are skipped.
fn residual_at(a: &[f64], b: &[f64], lag: f64, range: std::ops::Range<usize>) -> f64 {
    let (mut e, mut p) = (0.0, 0.0);
    let il = lag.floor() as isize;
    let frac = lag - lag.floor();
    let taps = (frac != 0.0).then(|| sinc_taps(frac));
    let nb = b.len() as isize;
    for i in range {
        let j = i as isize + il;
        let bv = match &taps {
            None if (0..nb).contains(&j) => b[j as usize],
            Some(t) if j - SINC_HALF + 1 >= 0 && j + SINC_HALF < nb => {
                let s = (j - SINC_HALF + 1) as usize;
                t.iter().zip(&b[s..]).map(|(w, v)| w * v).sum()
            }
            _ => continue,
        };
        let d = a[i] - bv;
        e += d * d;
        p += a[i] * a[i];
    }
    if e <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p <= 0.0 {
        return 0.0;
    }
    10.0 * (e / p).log10()
}

/// Best alignment of `b` on `a`: integer lag first, then a sub-sample refinement (golden
/// section search between the neighbouring lags) on the loudest stretch of `a`, then the
/// residual over the whole overlap at that lag. Returns (residual dB, lag).
pub fn aligned_residual(a: &[f64], b: &[f64]) -> (f64, f64) {
    let n = a.len().min(b.len());
    let mut best = (f64::INFINITY, 0.0);
    for l in -TWIN_MAX_LAG..=TWIN_MAX_LAG {
        let r = residual_at(a, b, l as f64, 0..n);
        if r < best.0 {
            best = (r, l as f64);
        }
    }
    if best.0 == f64::NEG_INFINITY {
        return best;
    }
    const WINDOW: usize = 16384;
    let peak = a[..n]
        .iter()
        .enumerate()
        .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
        .map_or(0, |(i, _)| i);
    let w0 = peak
        .saturating_sub(WINDOW / 2)
        .min(n.saturating_sub(WINDOW));
    let win = w0..(w0 + WINDOW).min(n);
    let (mut lo, mut hi) = (best.1 - 1.0, best.1 + 1.0);
    let g = (5f64.sqrt() - 1.0) / 2.0;
    for _ in 0..24 {
        let m1 = hi - g * (hi - lo);
        let m2 = lo + g * (hi - lo);
        if residual_at(a, b, m1, win.clone()) < residual_at(a, b, m2, win.clone()) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let lag = (lo + hi) / 2.0;
    let r = residual_at(a, b, lag, 0..n);
    if r < best.0 { (r, lag) } else { best }
}

/// Finds twins among the written sounds. Each sound is compared to the earlier sounds
/// that are not twins themselves; the first that passes all three tests is its original.
pub fn find_twins(out_dir: &Path, sounds: &[SoundInfo]) -> Vec<Twin> {
    struct Cand {
        index: usize,
        lufs: f64,
        audio: Vec<f64>,
    }
    let mut originals: Vec<Cand> = Vec::new();
    let mut twins = Vec::new();
    for (index, s) in sounds.iter().enumerate() {
        let (Some(file), Some(lufs)) = (&s.file, s.lufs) else {
            continue;
        };
        let Some((samples, ch)) = read_wav(&out_dir.join(file)) else {
            continue;
        };
        let audio = mono(&samples, ch);
        let found = originals.iter().find_map(|o| {
            let length_diff = o.audio.len().abs_diff(audio.len());
            let lufs_diff = (o.lufs - lufs).abs();
            if length_diff > TWIN_MAX_LENGTH_DIFF || lufs_diff > TWIN_MAX_LUFS_DIFF {
                return None;
            }
            let (residual_db, lag) = aligned_residual(&o.audio, &audio);
            (residual_db <= TWIN_MAX_RESIDUAL_DB).then_some(Twin {
                index,
                of: o.index,
                residual_db,
                lag,
                length_diff,
                lufs_diff,
            })
        });
        match found {
            Some(t) => twins.push(t),
            None => originals.push(Cand { index, lufs, audio }),
        }
    }
    twins
}

/// Writes the pack into `out_dir` (where the WAVs already are) and annotates
/// `manifest.json`. With `merge_twins`, a twin's rows point at its original's file and the
/// twin's own WAV files are removed.
///
/// A loop with an intro of its own gets an extended file of `intro_loop_secs` (intro + whole
/// cycles, played once); 0 keeps the body alone, looped, for every loop.
pub fn write_pack(
    out_dir: &Path,
    sounds: &[SoundInfo],
    merge_twins: bool,
    intro_loop_secs: f64,
) -> Result<PackReport, String> {
    let err = |p: &Path, e: io::Error| format!("{}: {e}", p.display());
    let mut report = PackReport {
        merged_twins: merge_twins,
        intro_loop_secs,
        ..Default::default()
    };

    for s in sounds {
        let (Some(file), Some(l)) = (&s.file, &s.loop_info) else {
            continue;
        };
        let path = out_dir.join(file);
        let start = l.intro_samples as u32;
        let end = (l.intro_samples + l.period_samples).saturating_sub(1) as u32;
        write_smpl(&path, start, end).map_err(|e| err(&path, e))?;
        report.loops_with_smpl += 1;
    }

    // The extended files: (sound index) -> (file, cycles).
    let mut extended = std::collections::BTreeMap::new();
    if intro_loop_secs > 0.0 {
        for (i, s) in sounds.iter().enumerate() {
            let (Some(file), Some(l)) = (&s.file, &s.loop_info) else {
                continue;
            };
            if let Some(x) = write_extended(out_dir, file, l, intro_loop_secs)
                .map_err(|e| err(&out_dir.join(file), e))?
            {
                extended.insert(i, x);
            }
        }
    }
    report.intro_loops_extended = extended.len();

    let twins = find_twins(out_dir, sounds);
    report.twins = twins.len();
    let twin_of = |i: usize| twins.iter().find(|t| t.index == i);

    let mut rows = Vec::new();
    // The sound (index in `sounds`) of each row, and the file it plays with the reason.
    let mut row_sound = Vec::new();
    let mut row_file: Vec<(FileKind, String)> = Vec::new();
    let mut referenced = std::collections::BTreeSet::new();
    for (i, s) in sounds.iter().enumerate() {
        let Some(file) = &s.file else { continue };
        let Some(id) = parse_id(&s.id) else { continue };
        let src_i = match twin_of(i) {
            Some(t) if merge_twins => t.of,
            _ => i,
        };
        let src = &sounds[src_i];
        let rate =
            hound::WavReader::open(out_dir.join(file)).map_or(44100, |r| r.spec().sample_rate);
        let secs = |n: usize| n as f64 / rate as f64;
        let (fname, kind, reason) = match (&src.loop_info, extended.get(&src_i)) {
            (Some(l), Some((f, cycles))) => (
                f.clone(),
                FileKind::IntroLoopExtended,
                format!(
                    "intro of its own ({:.2} s before the loop starts): intro + {cycles} whole cycles ({:.0} s), played once, as AltSound loops whole files only",
                    secs(l.repeats_from_samples),
                    secs(l.intro_samples + cycles * l.period_samples),
                ),
            ),
            (Some(l), None) => {
                let own = secs(l.repeats_from_samples);
                let reason = if l.loop_file.is_none() {
                    "the loop starts with the sound: the whole file loops".to_string()
                } else if intro_loop_secs <= 0.0 && own >= OWN_INTRO_MIN_SECS {
                    format!(
                        "--intro-loop-secs 0: the body alone, looped; the intro ({own:.2} s of its own) is not played"
                    )
                } else {
                    format!(
                        "no intro of its own ({own:.3} s before the loop starts, or silence): the body alone, looped"
                    )
                };
                (
                    l.loop_file.clone().unwrap_or_else(|| file.clone()),
                    FileKind::BodyLoop,
                    reason,
                )
            }
            (None, _) => (
                src.file.clone().unwrap_or_else(|| file.clone()),
                FileKind::OneShot,
                if src.loop_unresolved.is_some() {
                    "no exact loop found: cut at --max-secs, played once".to_string()
                } else {
                    "ends by itself: played once".to_string()
                },
            ),
        };
        let looped = kind == FileKind::BodyLoop;
        let continuous = kind == FileKind::IntroLoopExtended
            || (src.loop_info.is_none() && src.loop_unresolved.is_some());
        referenced.insert(fname.clone());
        rows.push(Row::plain(
            id,
            classify(&s.name, looped || continuous),
            looped,
            continuous,
            csv_name(&s.name, &s.id),
            fname,
        ));
        row_sound.push(i);
        row_file.push((kind, reason));
    }
    report.rows = rows.len();
    report.files_referenced = referenced.len();
    report.dcs = apply_dcs(&mut rows, &row_sound, sounds, &twins);

    if merge_twins {
        for t in &twins {
            let s = &sounds[t.index];
            let own = [
                s.file.clone(),
                s.loop_info.as_ref().and_then(|l| l.loop_file.clone()),
                extended.get(&t.index).map(|x| x.0.clone()),
            ];
            for f in own.into_iter().flatten() {
                if !referenced.contains(&f) {
                    let _ = fs::remove_file(out_dir.join(&f));
                }
            }
        }
    }

    let ini = match &report.dcs {
        Some(d) => altsound_ini_dcs(&d.callout_profiles, &d.sfx_profiles),
        None => altsound_ini(),
    };
    for (name, text) in [
        (ALTSOUND_CSV, altsound_csv(&rows)),
        (GSOUND_CSV, gsound_csv(&rows)),
        (ALTSOUND_INI, ini),
    ] {
        let p = out_dir.join(name);
        fs::write(&p, text).map_err(|e| err(&p, e))?;
    }

    annotate_manifest(
        out_dir, sounds, &twins, &rows, &row_sound, &row_file, &report,
    )?;
    Ok(report)
}

/// DCS: CHANNEL, DUCK, STOP, TYPE and the ducking profiles from the track programs of the
/// rows' commands (`SoundInfo::dcs`). None when no row has DCS data (other boards keep the
/// plain rows).
fn apply_dcs(
    rows: &mut [Row],
    row_sound: &[usize],
    sounds: &[SoundInfo],
    twins: &[Twin],
) -> Option<DcsPack> {
    let fx = |r: usize| {
        sounds[row_sound[r]]
            .dcs
            .as_ref()
            .filter(|d| d.error.is_none())
    };
    if !(0..rows.len()).any(|r| fx(r).is_some()) {
        return None;
    }
    let voice_channel = voice_channel(rows, row_sound, sounds, twins);
    let mut pack = DcsPack {
        voice_channel,
        ..Default::default()
    };
    let mut lost_stops = Vec::new();
    for (r, row) in rows.iter_mut().enumerate() {
        let Some(d) = fx(r) else { continue };
        if d.is_music() {
            row.channel = 0;
            row.duck = 100;
            row.gtype = if row.looped || row.continuous {
                GType::Music
            } else {
                GType::Sfx
            };
            pack.music_rows += 1;
            continue;
        }
        let voice = Some(d.channel) == voice_channel;
        row.channel = if voice { 1 } else { -1 };
        row.gtype = if voice { GType::Callout } else { GType::Sfx };
        pack.voice_rows += usize::from(voice);
        row.duck = d.duck_on(0).map_or(100, |x| duck_percent(x.units));
        pack.ducking_rows += usize::from(row.duck < 100);
        // AltSound only stops the music, and only from a jingle.
        let stops_music = d.stops.contains(&0);
        row.stop = voice && stops_music;
        pack.stop_rows += usize::from(row.stop);
        if stops_music && !voice {
            lost_stops.push(format!("0x{:04X}", row.id));
        }
    }
    let mut values: Vec<u32> = rows.iter().map(|r| r.duck).filter(|&d| d < 100).collect();
    values.sort_unstable_by(|a, b| b.cmp(a));
    values.dedup();
    pack.duck_values = values;
    (pack.callout_profiles, pack.sfx_profiles) = assign_profiles(rows);
    pack.limits = vec![
        "AltSound gives the music its level back at once when the sound ends; the board fades it back (0.15 s for most commands)".into(),
        "AltSound keeps only the deepest of overlapping ducks; the board adds them (https://github.com/vpinball/libaltsound/issues/15)".into(),
        "only the music is ducked: ducks of other channels are in manifest.json (dcs.ducks) only".into(),
        "every DCS channel cuts its previous sound; AltSound keeps that for the music and one voice channel, the other channels play over each other".into(),
    ];
    if !lost_stops.is_empty() {
        pack.limits.push(format!(
            "these commands stop the music but are not on the jingle channel, which alone can stop it in AltSound: {}",
            lost_stops.join(" ")
        ));
    }
    Some(pack)
}

/// The DCS channel to play as AltSound's jingle channel / G-Sound's callouts: the one with
/// the most voice lines (quoted names), where voice lines are most of its rows and which
/// has no twin channel. Twin channels hold the same sounds (AFM: 1 and 2, 4 and 5) so that
/// two can play at once: making one exclusive would cut them. Channel `a` is the twin of
/// `b` when most of `a`'s sounds also play on `b` (a twin group, `twin_of`, holds both);
/// a few shared lines do not make a twin (AFM plays 41 of the General's 172 lines from
/// channel 3 on the Martians' channels 4 and 5 too).
fn voice_channel(
    rows: &[Row],
    row_sound: &[usize],
    sounds: &[SoundInfo],
    twins: &[Twin],
) -> Option<u8> {
    let home = |i: usize| sounds[i].dcs.as_ref().map(|d| d.channel);
    // The twin group of a sound: its original's index.
    let group = |i: usize| twins.iter().find(|t| t.index == i).map_or(i, |t| t.of);
    // The channels each group plays on.
    let mut group_channels: std::collections::BTreeMap<usize, u8> = Default::default();
    for (i, _) in sounds.iter().enumerate() {
        if let Some(c) = home(i) {
            *group_channels.entry(group(i)).or_default() |= 1 << c;
        }
    }
    let mut best: Option<(u8, usize)> = None;
    for ch in 1..8u8 {
        let members: Vec<usize> = (0..rows.len())
            .filter(|&r| {
                sounds[row_sound[r]]
                    .dcs
                    .as_ref()
                    .is_some_and(|d| d.channel == ch && !d.is_music())
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let voices = members
            .iter()
            .filter(|&&r| rows[r].kind == Kind::Callout)
            .count();
        let has_twin_channel = (0..8u8).filter(|&o| o != ch).any(|o| {
            let shared = members
                .iter()
                .filter(|&&r| group_channels[&group(row_sound[r])] & (1 << o) != 0)
                .count();
            shared * 2 > members.len()
        });
        if voices * 2 > members.len() && !has_twin_channel && best.is_none_or(|b| voices > b.1) {
            best = Some((ch, voices));
        }
    }
    best.map(|b| b.0)
}

/// Adds `twin_of` (+ the similarity measures) to the twins' entries of `manifest.json`,
/// and an `altsound` summary.
fn annotate_manifest(
    out_dir: &Path,
    sounds: &[SoundInfo],
    twins: &[Twin],
    rows: &[Row],
    row_sound: &[usize],
    row_file: &[(FileKind, String)],
    report: &PackReport,
) -> Result<(), String> {
    let path = out_dir.join("manifest.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut m: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(list) = m.get_mut("sounds").and_then(|v| v.as_array_mut()) {
        for t in twins {
            let id = &sounds[t.index].id;
            if let Some(e) = list.iter_mut().find(|e| e["id"] == *id) {
                e["twin_of"] = serde_json::json!(sounds[t.of].id);
                let home = |i: usize| sounds[i].dcs.as_ref().map(|d| d.channel);
                if let (Some(a), Some(b)) = (home(t.of), home(t.index)) {
                    e["twin_reason"] = serde_json::json!(if a == b {
                        format!("same sound, both on DCS channel {a}")
                    } else {
                        format!(
                            "same sound on DCS channels {a} and {b}: a new command on a channel cuts that channel's sound, so two copies can play at once"
                        )
                    });
                }
                e["twin"] = serde_json::json!({
                    "residual_db": round2(t.residual_db),
                    "lag_samples": round2(t.lag),
                    "length_diff_samples": t.length_diff,
                    "lufs_diff": (t.lufs_diff * 1000.0).round() / 1000.0,
                });
            }
        }
        // What each row became in the two CSVs.
        for ((row, &i), (kind, reason)) in rows.iter().zip(row_sound).zip(row_file) {
            if let Some(e) = list.iter_mut().find(|e| e["id"] == sounds[i].id) {
                e["pack"] = serde_json::json!({
                    "file": row.fname,
                    "file_kind": kind.as_str(),
                    "file_reason": reason,
                    "loop": u8::from(row.looped) * 100,
                    "channel": row.channel,
                    "duck": row.duck,
                    "stop": u8::from(row.stop),
                    "gsound_type": row.gtype.as_str(),
                    "ducking_profile": row.profile,
                });
            }
        }
    }
    m["altsound"] = serde_json::json!({
        "files": [ALTSOUND_CSV, GSOUND_CSV, ALTSOUND_INI],
        "rows": report.rows,
        "loops_with_smpl": report.loops_with_smpl,
        "intro_loops_extended": report.intro_loops_extended,
        "intro_loop_secs": report.intro_loop_secs,
        "twins": report.twins,
        "merged_twins": report.merged_twins,
        "files_referenced": report.files_referenced,
        "dcs": report.dcs,
        "twin_test": {
            "max_length_diff_samples": TWIN_MAX_LENGTH_DIFF,
            "max_lufs_diff": TWIN_MAX_LUFS_DIFF,
            "max_residual_db": TWIN_MAX_RESIDUAL_DB,
        },
    });
    fs::write(&path, serde_json::to_string_pretty(&m).unwrap())
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn round2(v: f64) -> f64 {
    if v.is_finite() {
        (v * 100.0).round() / 100.0
    } else {
        -999.0
    }
}

/// `0x0392` → 0x392. Ids that are not a plain hex number (multi-byte sequences) are left
/// out of the CSVs: libaltsound keys samples by a single command number.
pub fn parse_id(id: &str) -> Option<u32> {
    let h = id.strip_prefix("0x").or_else(|| id.strip_prefix("0X"))?;
    (h.len() <= 8).then(|| u32::from_str_radix(h, 16).ok())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_names() {
        assert_eq!(classify("Music: Main Theme", false), Kind::Music);
        assert_eq!(classify("SFX: Whack", true), Kind::Music);
        assert_eq!(classify("SFX: Whack", false), Kind::Sfx);
        assert_eq!(classify("\"Hey look\"", false), Kind::Callout);
        assert_eq!(classify("VOX: Jackpot", false), Kind::Callout);
    }

    #[test]
    fn names_have_no_commas_or_quotes() {
        let n = csv_name("\"Hey look, your shoe is untied.\"", "0x0392");
        assert_eq!(n, "Hey look your shoe is untied.");
        assert_eq!(csv_name("", "0x0001"), "sound 0x0001");
    }

    #[test]
    fn csv_rows() {
        let rows = vec![
            Row::plain(
                1,
                Kind::Music,
                true,
                false,
                "Music: Prelaunch Loop".into(),
                "0x0001-afm_113b-loop.wav".into(),
            ),
            Row::plain(
                0x392,
                Kind::Callout,
                false,
                false,
                "Hey".into(),
                "0x0392-afm_113b.wav".into(),
            ),
            Row::plain(
                0x24,
                Kind::Music,
                false,
                true,
                "sound 0x24".into(),
                "0x24-xfiles.wav".into(),
            ),
        ];
        let a = altsound_csv(&rows);
        assert!(a.starts_with("ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME\r\n"));
        assert!(
            a.contains("0x0001,0,100,100,100,0,Music: Prelaunch Loop,0x0001-afm_113b-loop.wav\r\n")
        );
        assert!(a.contains("0x0392,,100,100,0,0,Hey,0x0392-afm_113b.wav\r\n"));
        let g = gsound_csv(&rows);
        assert!(g.contains("0x0001,music,100,0,0x0001-afm_113b-loop.wav\r\n"));
        assert!(g.contains("0x0392,callout,100,0,0x0392-afm_113b.wav\r\n"));
        assert!(a.contains("0x0024,0,100,100,0,0,sound 0x24,0x24-xfiles.wav\r\n"));
        assert!(g.contains("0x0024,music,100,0,0x24-xfiles.wav\r\n"));
    }

    #[test]
    fn duck_values_follow_the_board() {
        // AFM: -10 units -> 76, -15 -> 67, -80 -> 11, -100 -> 7.
        assert_eq!(duck_percent(-10.0), 76);
        assert_eq!(duck_percent(-15.0), 67);
        assert_eq!(duck_percent(-20.0), 58);
        assert_eq!(duck_percent(-80.0), 11);
        assert_eq!(duck_percent(-100.0), 7);
        assert_eq!(duck_percent(0.0), 100);
        assert_eq!(duck_percent(12.0), 100);
    }

    fn effects(
        channel: u8,
        streams: &[u8],
        duck: Option<f64>,
        stops: &[u8],
    ) -> dcsrom::CommandEffects {
        dcsrom::CommandEffects {
            track_type: 1,
            channel,
            streams: streams.to_vec(),
            stops: stops.to_vec(),
            ducks: duck
                .map(|units| dcsrom::Duck {
                    channel: 0,
                    units,
                    db: units * dcsrom::DB_PER_LEVEL,
                    start_s: 0.0,
                    full_s: 0.0,
                    end_s: Some(1.0),
                    restore: "fade",
                    release_s: 0.15,
                })
                .into_iter()
                .collect(),
            deferred: None,
            queues: Vec::new(),
            own_level: None,
            length_s: Some(1.0),
            error: None,
        }
    }

    #[test]
    fn dcs_rows() {
        // music (ch0), two voice lines on ch3 (one ducks, one stops the music), an SFX
        // and its twin on ch1/ch2 (a fanfare ducking deep), a voice line on twinned ch4/5.
        let sounds = vec![
            effects(0, &[0], None, &[]),
            effects(3, &[3], Some(-10.0), &[]),
            effects(3, &[3], Some(-15.0), &[0]),
            effects(1, &[1], Some(-80.0), &[]),
            effects(2, &[2], Some(-80.0), &[]),
            effects(4, &[4], Some(-15.0), &[]),
            effects(5, &[5], Some(-15.0), &[]),
        ];
        let names = [
            "Music",
            "\"Jackpot\"",
            "\"Stop\"",
            "Music: Fanfare",
            "Music: Fanfare",
            "\"Ouch\"",
            "\"Ouch\"",
        ];
        let kinds = [
            Kind::Music,
            Kind::Callout,
            Kind::Callout,
            Kind::Music,
            Kind::Music,
            Kind::Callout,
            Kind::Callout,
        ];
        let mut rows: Vec<Row> = (0..sounds.len())
            .map(|i| {
                Row::plain(
                    i as u32,
                    kinds[i],
                    i == 0,
                    false,
                    csv_name(names[i], ""),
                    format!("{i}.wav"),
                )
            })
            .collect();
        let infos: Vec<SoundInfo> = sounds
            .into_iter()
            .enumerate()
            .map(|(i, e)| SoundInfo::for_test(&format!("0x{i:04X}"), names[i], e))
            .collect();
        let twin = |index, of| Twin {
            index,
            of,
            residual_db: -70.0,
            lag: 0.0,
            length_diff: 0,
            lufs_diff: 0.0,
        };
        let twins = [twin(4, 3), twin(6, 5)];
        let row_sound: Vec<usize> = (0..rows.len()).collect();
        let pack = apply_dcs(&mut rows, &row_sound, &infos, &twins).unwrap();
        assert_eq!(pack.voice_channel, Some(3));
        let a = altsound_csv(&rows);
        assert!(a.contains("0x0000,0,100,100,100,0,"), "{a}");
        assert!(a.contains("0x0001,1,76,100,0,0,"), "{a}");
        assert!(a.contains("0x0002,1,67,100,0,1,"), "{a}");
        assert!(a.contains("0x0003,,11,100,0,0,"), "{a}");
        assert!(a.contains("0x0005,,67,100,0,0,"), "{a}");
        let g = gsound_csv(&rows);
        assert!(g.contains("0x0000,music,100,0,"), "{g}");
        assert!(g.contains("0x0001,callout,100,1,"), "{g}");
        assert!(g.contains("0x0002,callout,100,2,"), "{g}");
        assert!(g.contains("0x0003,sfx,100,2,"), "{g}");
        assert!(g.contains("0x0005,sfx,100,1,"), "{g}");
        assert_eq!(pack.callout_profiles, vec![76, 67]);
        assert_eq!(pack.sfx_profiles, vec![67, 11]);
        assert_eq!(pack.duck_values, vec![76, 67, 11]);
        let ini = altsound_ini_dcs(&pack.callout_profiles, &pack.sfx_profiles);
        assert!(
            ini.contains("[callout]\nducks = music\npauses =\nstops =\ngroup_vol = 100\n"),
            "{ini}"
        );
        assert!(ini.contains("[callout_ducking_profiles]\n;profile0 is reserved\nducking_profile1 = music:76\nducking_profile2 = music:67\n"), "{ini}");
        assert!(ini.contains("[sfx_ducking_profiles]\n;profile0 is reserved\nducking_profile1 = music:67\nducking_profile2 = music:11\n"), "{ini}");
        assert!(!ini.contains("\n     "));
        // Without a profile, a type ducks nothing (libaltsound refuses `ducks` without one).
        let ini = altsound_ini_dcs(&[], &[50]);
        assert!(ini.contains("[callout]\nducks =\n"), "{ini}");
        assert!(!ini.contains("[callout_ducking_profiles]"), "{ini}");
    }

    #[test]
    fn ini_has_format_and_volume() {
        let ini = altsound_ini();
        assert!(ini.contains("\n[format]\nformat = altsound\n"));
        assert!(ini.contains("\nrom_volume_ctrl = 0\n"));
        assert!(!ini.contains("\n     "));
    }

    #[test]
    fn extended_file_joins_cycles_exactly() {
        // Stereo: an intro of 7 frames, then a body of 5 frames whose samples are distinct.
        let ch = 2;
        let (intro, period) = (7, 5);
        let samples: Vec<i16> = (0..(intro + period) * ch)
            .map(|v| v as i16 * 3 - 20)
            .collect();
        let total = 30; // frames
        let cycles = extended_cycles(intro, period, total);
        assert_eq!(cycles, 5); // 7 + 5 * 5 = 32 >= 30, and 4 cycles (27) is not enough
        let x = extend_loop(&samples, ch, intro, period, cycles);
        assert_eq!(x.len(), (intro + cycles * period) * ch);
        // The intro and the first cycle are the file as it was.
        assert_eq!(&x[..samples.len()], &samples[..]);
        // Every later cycle is the body again, sample for sample: each joint is the body's
        // own end-to-start joint.
        let body = &samples[intro * ch..];
        for k in 0..cycles {
            let a = (intro + k * period) * ch;
            assert_eq!(&x[a..a + period * ch], body, "cycle {k}");
        }
        // At least one cycle, even when the intro alone is longer than asked.
        assert_eq!(extended_cycles(100, 5, 30), 1);
        assert_eq!(extended_cycles(0, 10, 100), 10);
    }

    #[test]
    fn extended_file_carries_the_loop_points() {
        let dir = std::env::temp_dir().join(format!("r2a-ext-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let rate = 1000;
        let (intro, period) = (200usize, 300usize);
        // An intro of its own (a ramp), then one cycle of a tone.
        let s: Vec<i16> = (0..intro)
            .map(|n| (n as i16) * 50)
            .chain((0..period).map(|n| {
                (8000.0 * (n as f64 * 2.0 * std::f64::consts::PI * 3.0 / period as f64).sin())
                    as i16
            }))
            .collect();
        let file = "0x0009-test.wav";
        write_wav(&dir.join(file), &s, 1, rate).unwrap();
        let mut l = crate::extract::LoopInfo::for_test(intro, period);
        l.repeats_from_samples = 150; // 0.15 s of its own
        let (name, cycles) = write_extended(&dir, file, &l, 2.0)
            .unwrap()
            .expect("extended");
        assert_eq!(name, "0x0009-test-extended.wav");
        assert_eq!(cycles, 6); // 200 + 6 * 300 = 2000 frames = 2 s
        let bytes = fs::read(dir.join(&name)).unwrap();
        let at = bytes
            .windows(4)
            .position(|w| w == b"smpl")
            .expect("smpl chunk");
        let u = |o: usize| u32::from_le_bytes(bytes[at + o..at + o + 4].try_into().unwrap());
        assert_eq!((u(52), u(56)), (intro as u32, (intro + period - 1) as u32));
        let (x, ch) = read_wav(&dir.join(&name)).unwrap();
        assert_eq!((x.len(), ch), (intro + cycles * period, 1));
        // The joint between two cycles is the one between the body's end and its start.
        let j = intro + period;
        assert_eq!((x[j - 1], x[j]), (s[intro + period - 1], s[intro]));
        // No intro of its own (the repetition starts at once): no extended file.
        l.repeats_from_samples = 10;
        assert!(write_extended(&dir, file, &l, 2.0).unwrap().is_none());
        // An intro of its own that is silence: none either.
        let quiet: Vec<i16> = std::iter::repeat_n(3, intro)
            .chain(s[intro..].iter().copied())
            .collect();
        write_wav(&dir.join(file), &quiet, 1, rate).unwrap();
        l.repeats_from_samples = 150;
        assert!(write_extended(&dir, file, &l, 2.0).unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn smpl_layout() {
        let c = smpl_chunk(44100, 10, 99);
        assert_eq!(c.len(), 68);
        assert_eq!(&c[0..4], b"smpl");
        assert_eq!(u32::from_le_bytes(c[4..8].try_into().unwrap()), 60);
        assert_eq!(u32::from_le_bytes(c[36..40].try_into().unwrap()), 1); // loops
        assert_eq!(u32::from_le_bytes(c[52..56].try_into().unwrap()), 10); // start
        assert_eq!(u32::from_le_bytes(c[56..60].try_into().unwrap()), 99); // end
    }

    #[test]
    fn residual_finds_subsample_shift() {
        let a: Vec<f64> = (0..4000)
            .map(|i| (i as f64 * 0.05).sin() * 1000.0)
            .collect();
        let b: Vec<f64> = (0..4000)
            .map(|i| ((i as f64 + 0.4) * 0.05).sin() * 1000.0)
            .collect();
        let (r, lag) = aligned_residual(&a, &b);
        assert!(r < -50.0, "{r}");
        assert!((lag + 0.4).abs() < 0.05, "{lag}");
        let c: Vec<f64> = (0..4000)
            .map(|i| (i as f64 * 0.07).sin() * 1000.0)
            .collect();
        assert!(aligned_residual(&a, &c).0 > -10.0);
    }

    #[test]
    fn ids() {
        assert_eq!(parse_id("0x0392"), Some(0x392));
        assert_eq!(parse_id("0x55AAEF10"), Some(0x55AA_EF10));
        assert_eq!(parse_id("x"), None);
    }
}
