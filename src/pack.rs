//! The library's entry point: extract one ROM ([`extract`]) and read what it made ([`Pack`]).

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Extraction options.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Where ROM set names (and a clone's parent zip, BSMT2000 firmware) are looked up
    /// [default: the ROM's own folder, then the current directory and ./roms].
    pub roms: Option<PathBuf>,
    /// More arguments of the `rom2altsound` command line (`--volume reference`,
    /// `--only 0x0002`...), as typed.
    pub args: Vec<String>,
}

/// Extracts one ROM: `rom` is a ROM zip or a set name (looked up in `o.roms`), the pack goes
/// to `<out>/<rom>/`.
///
/// The extraction runs in a copy of the current program (PinMAME runs one machine per
/// process): the program must call [`crate::run_worker_if_requested`] first thing in
/// `main`. Progress lines of the copy go to `<out>/<rom>/` (its log). Blocks until done.
pub fn extract(rom: &str, out: &Path, o: &Options) -> Result<Pack, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this program: {e}"))?;
    let mut argv: Vec<String> = vec![
        "rom2altsound".into(),
        rom.into(),
        "--out".into(),
        out.to_string_lossy().into_owned(),
    ];
    if let Some(r) = &o.roms {
        argv.push("--roms".into());
        argv.push(r.to_string_lossy().into_owned());
    }
    argv.extend(o.args.iter().cloned());
    let cli = <crate::Cli as clap::Parser>::try_parse_from(&argv).map_err(|e| e.to_string())?;
    let outcomes = crate::batch::run_with(&cli, &exe, None, &mut |_| {});
    let oc = outcomes
        .into_iter()
        .next()
        .ok_or_else(|| format!("{rom}: nothing was extracted"))?;
    if let Some(e) = oc.error {
        return Err(format!("{}: {e}", oc.rom));
    }
    Pack::open(&oc.out)
}

/// An extracted pack, read from its folder.
#[derive(Clone, Debug)]
pub struct Pack {
    /// The pack's folder (an AltSound pack: drop it as `<table>/altsound/<rom>/`).
    pub dir: PathBuf,
    /// The ROM set name.
    pub rom: String,
    /// Sample rate of the files.
    pub sample_rate: u32,
    /// One entry per sound command (or game sound id) that played.
    pub sounds: Vec<Sound>,
    /// The whole `manifest.json` (boards, volumes, loudness, every detail).
    pub manifest: Value,
}

/// One sound of a pack.
#[derive(Clone, Debug)]
pub struct Sound {
    /// The command (`0x0186`), or the game's sound id on a board that takes no command.
    pub id: String,
    /// Its name (from PinMAME's sounds.dat, the names applied, or the tagger), may be empty.
    pub name: String,
    /// The recording, when one was written (a blip under 20 ms has none).
    pub file: Option<PathBuf>,
    /// Length of the recording in seconds.
    pub duration: f64,
    /// For a music loop: the file is the intro then one loop body.
    pub loop_points: Option<LoopPoints>,
}

/// Where a looping recording repeats, in sample frames from the start of its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopPoints {
    /// The loop body starts here (the intro before it plays once).
    pub start: usize,
    /// One loop body: it plays from `start` to `start + length`, then again from `start`.
    pub length: usize,
}

/// Decoded samples of a recording.
#[derive(Clone, Debug)]
pub struct Samples {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved 16-bit samples.
    pub data: Vec<i16>,
}

impl Pack {
    /// Reads the pack in `dir` (its `manifest.json`).
    pub fn open(dir: &Path) -> Result<Pack, String> {
        let mp = dir.join("manifest.json");
        let text = std::fs::read_to_string(&mp).map_err(|e| format!("{}: {e}", mp.display()))?;
        let manifest: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", mp.display()))?;
        let rom = manifest["rom"].as_str().unwrap_or_default().to_string();
        let sample_rate = manifest["sample_rate"].as_u64().unwrap_or(44_100) as u32;
        let sounds = manifest["sounds"]
            .as_array()
            .map(|v| v.iter().map(|s| Sound::from_json(dir, s)).collect())
            .unwrap_or_default();
        Ok(Pack {
            dir: dir.to_path_buf(),
            rom,
            sample_rate,
            sounds,
            manifest,
        })
    }
}

impl Sound {
    fn from_json(dir: &Path, s: &Value) -> Sound {
        let lp = &s["loop"];
        let loop_points = match (lp["intro_samples"].as_u64(), lp["period_samples"].as_u64()) {
            (Some(a), Some(n)) => Some(LoopPoints {
                start: a as usize,
                length: n as usize,
            }),
            _ => None,
        };
        Sound {
            id: s["id"].as_str().unwrap_or_default().to_string(),
            name: s["name"].as_str().unwrap_or_default().to_string(),
            file: s["file"].as_str().map(|f| dir.join(f)),
            duration: s["duration"].as_f64().unwrap_or(0.0),
            loop_points,
        }
    }

    /// Decodes the recording.
    pub fn samples(&self) -> Result<Samples, String> {
        let p = self
            .file
            .as_ref()
            .ok_or_else(|| format!("{}: no recording", self.id))?;
        let err = |e: hound::Error| format!("{}: {e}", p.display());
        let mut r = hound::WavReader::open(p).map_err(err)?;
        let spec = r.spec();
        let data = match spec.sample_format {
            hound::SampleFormat::Int => r
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?,
            hound::SampleFormat::Float => r
                .samples::<f32>()
                .map(|v| v.map(|x| (x.clamp(-1.0, 1.0) * 32767.0) as i16))
                .collect::<Result<Vec<_>, _>>()
                .map_err(err)?,
        };
        Ok(Samples {
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            data,
        })
    }
}
