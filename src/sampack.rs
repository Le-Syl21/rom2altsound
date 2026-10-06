//! Stern SAM ROMs: the static extraction (`sam`), written as the other families' output
//! (WAV files, manifest.json, the AltSound files), plus the factory volume, read from the
//! DAC writes of a warm boot in PinMAME.
//!
//! - Files: every distinct stream of the sound scripts at its own rate (24 or 12 kHz),
//!   named after the first sample id that plays it (`s0123-<rom>.wav`, `s0123-l2-<rom>.wav`
//!   for a language other than the first), and every music script (song versions, loop
//!   beds) as one continuous 24 kHz file (`s0172-<rom>.wav`), joins declicked, with a
//!   `smpl` loop when the script loops. All at full scale: the samples as decoded, which
//!   is what the DAC plays at 0 dB (attenuation FF).
//! - Factory volume: the game sets its master volume in the PCM1755 DAC (registers
//!   0x10/0x11), from the operator setting in its nvram. The cold boot (child process)
//!   writes the factory nvram, the warm boot from it logs the DAC writes (shim hook).
//! - AltSound: rows keyed by sound call id, one row per sample the call picks from. PinMAME
//!   cannot play them today: SAM sends no sound command (no AltSound hook for SAM).

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, c_int, c_void};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{Value, json};

use crate::altsound::{self, Kind, Row};
use crate::sam::{self, BASE_RATE, Catalog};
use crate::{COLD_BOOT_REPORT, Cli, Job, ffi, loudness};

/// PCM1755 attenuation: FF = 0 dB, 0.5 dB per step down to 81 (-63 dB); 80 and below mute
/// (TI PCM1755 datasheet, registers 16/17; sam.c says the same).
pub fn dac_db(v: u8) -> Option<f64> {
    (v > 0x80).then_some(0.5 * (v as f64 - 255.0))
}

/// What PinMAME plays for the same register: a linear mixer level of `(v & 7F) * 100 / 7F`
/// percent (sam.c), in dB.
pub fn pinmame_db(v: u8) -> Option<f64> {
    let pct = if v > 0x80 {
        (v & 0x7F) as i32 * 100 / 0x7F
    } else {
        0
    };
    (pct > 0).then(|| 20.0 * (pct as f64 / 100.0).log10())
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn hex2(v: u8) -> String {
    format!("{v:02X}")
}

// ------------------------------------------------------------------------- the boot

/// One write to a PCM1755 register.
#[derive(Debug, Clone, Serialize)]
pub struct DacWrite {
    pub at: f64,
    pub reg: String,
    pub value: String,
}

/// A boot of the SAM machine in PinMAME, with the DAC writes it made.
#[derive(Debug, Clone, Serialize)]
pub struct Boot {
    /// Emulated seconds.
    pub secs: f64,
    /// "dac-quiet": no DAC write for `QUIET_SECS` after `boot_secs`; "max": `boot_max_secs`
    /// reached; "no-dac-hook": the port handler could not be hooked.
    pub ended_by: &'static str,
    pub wall_secs: f64,
    pub hooked: bool,
    pub writes: Vec<DacWrite>,
    /// The last value of each register (hex), when written.
    pub left: Option<String>,
    pub right: Option<String>,
    pub soft_mute: Option<String>,
    pub dac_off: Option<String>,
}

impl Boot {
    fn last(writes: &[(f64, u8, u8)], reg: u8) -> Option<u8> {
        writes.iter().rev().find(|w| w.1 == reg).map(|w| w.2)
    }
}

const QUIET_SECS: f64 = 3.0;

static SAMPLES: AtomicU64 = AtomicU64::new(0);
static RATE: AtomicU32 = AtomicU32::new(0);
static HOOKED: AtomicI32 = AtomicI32::new(-1);

unsafe extern "C" fn audio_available(info: *mut ffi::AudioInfo, _: *mut c_void) -> c_int {
    let info = unsafe { &*info };
    RATE.store(info.sample_rate as u32, Ordering::Relaxed);
    // The emulation thread, before the CPU runs: put the DAC hook in place.
    HOOKED.store(unsafe { ffi::shim_sam_hook_dac() }, Ordering::Release);
    info.samples_per_frame
}

unsafe extern "C" fn audio_updated(_: *mut c_void, samples: c_int, _: *mut c_void) -> c_int {
    unsafe { ffi::throttle = 0 };
    SAMPLES.fetch_add(samples.max(0) as u64, Ordering::Relaxed);
    samples
}

/// Sets libpinmame's configuration for a SAM boot (before `stage_rom`, which needs it).
fn configure(vpm: &Path, verbose: bool) {
    unsafe { ffi::shim_log_min_level = if verbose { 1 } else { 2 } };
    let config = ffi::Config {
        audio_format: ffi::AUDIO_FORMAT_INT16,
        sample_rate: 44100,
        vpm_path: ffi::vpm_path(vpm),
        on_state_updated: None,
        on_display_available: None,
        on_display_updated: None,
        on_audio_available: Some(audio_available),
        on_audio_updated: Some(audio_updated),
        on_mech_available: None,
        on_mech_updated: None,
        on_solenoid_updated: None,
        on_console_data_updated: None,
        is_key_pressed: None,
        on_log_message: ffi::shim_log as *const c_void,
        on_sound_command: None,
    };
    unsafe { ffi::PinmameSetConfig(&config) };
}

/// Boots the machine (nvram as it is in the configured vpm) until the DAC has been quiet
/// for `QUIET_SECS` after `min_secs`, or `max_secs`, then stops it (which writes the
/// nvram). libpinmame runs one machine per process: call it once per process.
pub fn boot(rom: &str, min_secs: f64, max_secs: f64) -> Result<Boot, String> {
    let t0 = Instant::now();
    let rom_c = CString::new(rom).unwrap();
    let st = unsafe { ffi::PinmameRun(rom_c.as_ptr()) };
    if st != ffi::STATUS_OK {
        return Err(format!("PinmameRun failed (status {st})"));
    }
    let emulated = || {
        let r = RATE.load(Ordering::Relaxed);
        if r == 0 {
            0.0
        } else {
            SAMPLES.load(Ordering::Relaxed) as f64 / r as f64
        }
    };
    let (mut seen, mut last_change) = (0, 0.0);
    let mut started = false;
    let ended_by = loop {
        std::thread::sleep(Duration::from_millis(50));
        let running = unsafe { ffi::PinmameIsRunning() } != 0;
        started |= running;
        if started && !running {
            break "stopped";
        }
        if HOOKED.load(Ordering::Acquire) == 0 {
            break "no-dac-hook";
        }
        let t = emulated();
        let n = unsafe { ffi::shim_sam_dac_count() };
        if n != seen {
            seen = n;
            last_change = t;
        }
        if t >= min_secs && seen > 0 && t - last_change >= QUIET_SECS {
            break "dac-quiet";
        }
        if t >= max_secs {
            break "max";
        }
    };
    let secs = emulated();
    unsafe { ffi::PinmameStop() };
    let mut writes = Vec::new();
    for i in 0..unsafe { ffi::shim_sam_dac_count() } {
        let (mut at, mut reg, mut val) = (0.0, 0u8, 0u8);
        if unsafe { ffi::shim_sam_dac_get(i, &mut at, &mut reg, &mut val) } != 0 {
            writes.push((at, reg, val));
        }
    }
    let last = |r| Boot::last(&writes, r).map(hex2);
    Ok(Boot {
        secs: round1(secs),
        ended_by,
        wall_secs: round1(t0.elapsed().as_secs_f64()),
        hooked: HOOKED.load(Ordering::Acquire) == 1,
        left: last(0x10),
        right: last(0x11),
        soft_mute: last(0x12),
        dac_off: last(0x13),
        writes: writes
            .iter()
            .map(|&(at, r, v)| DacWrite {
                at: round3(at),
                reg: hex2(r),
                value: hex2(v),
            })
            .collect(),
    })
}

/// Links the ROM (and its parent) into the private vpm, as the other families do.
fn stage_rom(job: &Job, vpm: &Path) -> Result<Option<String>, String> {
    let rom_c = CString::new(job.rom.clone()).unwrap();
    let mut game: (Option<String>, bool) = (None, false);
    unsafe extern "C" fn on_game(game: *mut ffi::Game, user: *mut c_void) {
        let game = unsafe { &*game };
        let out = unsafe { &mut *(user as *mut (Option<String>, bool)) };
        *out = (ffi::cstr(game.clone_of), game.found != 0);
    }
    let st =
        unsafe { ffi::PinmameGetGame(rom_c.as_ptr(), on_game, &mut game as *mut _ as *mut c_void) };
    if st != ffi::STATUS_OK {
        return Err(format!("unknown game {} (status {st})", job.rom));
    }
    let parent = game.0.filter(|p| !p.is_empty());
    for set in std::iter::once(&job.rom).chain(parent.as_ref()) {
        crate::link_rom(&job.roms, vpm, set)?;
    }
    Ok(parent)
}

// ------------------------------------------------------------------------- files

/// A written file's levels.
struct Measured {
    duration: f64,
    lufs: Option<f64>,
    true_peak_dbtp: Option<f64>,
    level_lufs: Option<f64>,
    peak_dbfs: Option<f64>,
    rms_dbfs: Option<f64>,
    clipped_samples: usize,
    dc_offset: i32,
    loud: loudness::FileLoudness,
}

fn measure(pcm: &[i16], rate: u32) -> Measured {
    let blocked = crate::extract::dc_block(pcm, 1, &[0], rate);
    let (peak, rms) = crate::extract::levels(&blocked);
    let loud = loudness::measure_fast(&blocked, 1, rate);
    Measured {
        duration: round3(pcm.len() as f64 / rate as f64),
        lufs: loud.lufs,
        true_peak_dbtp: loud.true_peak_dbtp,
        level_lufs: loud.level_lufs,
        peak_dbfs: peak.map(round2),
        rms_dbfs: rms.map(round2),
        clipped_samples: pcm
            .iter()
            .filter(|&&s| s == i16::MAX || s == i16::MIN)
            .count(),
        dc_offset: if pcm.is_empty() {
            0
        } else {
            (pcm.iter().map(|&s| s as i64).sum::<i64>() / pcm.len() as i64) as i32
        },
        loud,
    }
}

/// A music file's loop.
#[derive(Clone, Serialize)]
struct SamLoop {
    /// The file holds the intro then one cycle of the loop (sample frames), as the
    /// `smpl` chunk says.
    intro_samples: usize,
    period_samples: usize,
    repeats_from_samples: usize,
    period_secs: f64,
    cycles: u32,
    /// "sam-script": the loop of the music script (`07` mark, `03` jump back), exact.
    method: &'static str,
    confidence: f64,
    /// The cycle alone (written when there is an intro).
    loop_file: Option<String>,
    /// The intro then whole cycles, `--intro-loop-secs` long (written when there is an
    /// intro and `--intro-loop-secs` is not 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    extended_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extended_cycles: Option<usize>,
}

/// What one written item became.
struct Written {
    file: String,
    rate: u32,
    m: Measured,
    loop_info: Option<SamLoop>,
}

fn sound_file(rom: &str, sample: u16, lang: u8) -> String {
    if lang == 0 {
        format!("s{sample:04X}-{rom}.wav")
    } else {
        format!("s{sample:04X}-l{lang}-{rom}.wav")
    }
}

fn write(out: &Path, file: &str, pcm: &[i16], rate: u32) -> Result<(), String> {
    crate::extract::write_wav(&out.join(file), pcm, 1, rate)
        .map_err(|e| format!("{}: {e}", out.join(file).display()))
}

/// One cycle of a loop body that follows another cycle: its reset ramp (`ramp` samples,
/// declicked against the intro's end in the rendered file) is drawn again from the end
/// of the previous cycle.
fn next_cycle(body: &[i16], ramp: usize) -> Vec<i16> {
    let mut c = body.to_vec();
    if let Some(&prev) = body.last() {
        sam::declick(&mut c, ramp, prev);
    }
    c
}

fn write_music(
    rom: &[u8],
    m: &sam::Music,
    out: &Path,
    file: &str,
    intro_loop_secs: f64,
) -> Result<Written, String> {
    let r = sam::render(rom, &m.timeline, true);
    let pcm = r.pcm;
    write(out, file, &pcm, BASE_RATE)?;
    let mut loop_info = None;
    if let (Some(start), Some(_)) = (m.timeline.loop_start, m.timeline.loop_end) {
        let (start, end) = (start as usize, pcm.len());
        if end > start + 1 {
            let p = out.join(file);
            altsound::write_smpl(&p, start as u32, (end - 1) as u32)
                .map_err(|e| format!("{}: {e}", p.display()))?;
            let period = end - start;
            let mut l = SamLoop {
                intro_samples: start,
                period_samples: period,
                repeats_from_samples: start,
                period_secs: round3(period as f64 / BASE_RATE as f64),
                cycles: 1,
                method: "sam-script",
                confidence: 1.0,
                loop_file: None,
                extended_file: None,
                extended_cycles: None,
            };
            if start as f64 >= altsound::OWN_INTRO_MIN_SECS * BASE_RATE as f64 {
                let body = next_cycle(&pcm[start..end], r.loop_ramp);
                let name = match file.strip_suffix(".wav") {
                    Some(s) => format!("{s}-loop.wav"),
                    None => format!("{file}-loop.wav"),
                };
                write(out, &name, &body, BASE_RATE)?;
                l.loop_file = Some(name);
                if intro_loop_secs > 0.0 {
                    let total = (intro_loop_secs * BASE_RATE as f64).round() as usize;
                    let cycles = altsound::extended_cycles(start, period, total);
                    let mut x = Vec::with_capacity(start + cycles * period);
                    x.extend_from_slice(&pcm[..end]);
                    for _ in 1..cycles {
                        x.extend_from_slice(&body);
                    }
                    let name = altsound::extended_name(file);
                    write(out, &name, &x, BASE_RATE)?;
                    let p = out.join(&name);
                    altsound::write_smpl(&p, start as u32, (end - 1) as u32)
                        .map_err(|e| format!("{}: {e}", p.display()))?;
                    l.extended_file = Some(name);
                    l.extended_cycles = Some(cycles);
                }
            }
            loop_info = Some(l);
        }
    }
    Ok(Written {
        file: file.to_owned(),
        rate: BASE_RATE,
        m: measure(&pcm, BASE_RATE),
        loop_info,
    })
}

/// Work item: a sound stream or a music script (index in the catalog).
#[derive(Clone, Copy)]
enum Item {
    Sound(usize),
    Music(usize),
}

/// Writes every file, a few at a time; results in item order.
fn write_all(
    rom: &[u8],
    cat: &Catalog,
    items: &[(Item, String)],
    out: &Path,
    intro_loop_secs: f64,
) -> Result<Vec<Written>, String> {
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Result<Written, String>)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 4);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((item, file)) = items.get(i) else {
                        break;
                    };
                    let r = match *item {
                        Item::Sound(k) => {
                            let st = &cat.sounds[k].stream;
                            let pcm = st.decode(rom);
                            write(out, file, &pcm, st.rate()).map(|()| Written {
                                file: file.clone(),
                                rate: st.rate(),
                                m: measure(&pcm, st.rate()),
                                loop_info: None,
                            })
                        }
                        Item::Music(k) => {
                            write_music(rom, &cat.music[k], out, file, intro_loop_secs)
                        }
                    };
                    results.lock().unwrap().push((i, r));
                }
            });
        }
    });
    let mut r = results.into_inner().unwrap();
    r.sort_by_key(|x| x.0);
    r.into_iter().map(|x| x.1).collect()
}

// ------------------------------------------------------------------------- run

/// The image of a SAM ROM zip: the largest member.
fn load_image(job: &Job) -> Result<(PathBuf, crate::zipread::Entry, Vec<u8>), String> {
    let zip = job.roms.join(format!("{}.zip", job.rom));
    let entries = crate::zipread::list(&zip)?;
    let e = entries
        .into_iter()
        .max_by_key(|e| e.size)
        .ok_or_else(|| format!("{}: empty zip", zip.display()))?;
    let bytes = crate::zipread::read(&zip, &e)?;
    Ok((zip, e, bytes))
}

/// `--cold-boot-only` on a SAM: boot without nvram (the game writes its factory settings)
/// and write the report the parent reads.
fn cold_boot_only(cli: &Cli, job: &Job, vpm: &Path) -> Result<(), String> {
    configure(vpm, cli.verbose);
    stage_rom(job, vpm)?;
    let b = boot(&job.rom, cli.boot_secs, cli.boot_max_secs)?;
    let report = json!({
        "boot": { "secs": b.secs, "ended_by": b.ended_by, "wall_secs": b.wall_secs },
        "dac": b,
    });
    let path = job.out.join(COLD_BOOT_REPORT);
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The factory volume: cold boot in a child process (writes the factory nvram), then a
/// warm boot from it here, logging the DAC writes.
fn factory(cli: &Cli, job: &Job, vpm: &Path) -> (Option<Value>, Option<Boot>, Option<String>) {
    let cold = match crate::cold_boot(cli, job, vpm) {
        Ok(c) => c,
        Err(e) => return (None, None, Some(format!("cold boot failed: {e}"))),
    };
    configure(vpm, cli.verbose);
    if let Err(e) = stage_rom(job, vpm) {
        return (Some(cold), None, Some(e));
    }
    eprintln!("factory: warm boot of {} from the factory nvram", job.rom);
    match boot(&job.rom, cli.boot_secs, cli.boot_max_secs) {
        Ok(b) => (Some(cold), Some(b), None),
        Err(e) => (Some(cold), None, Some(format!("warm boot failed: {e}"))),
    }
}

/// Extracts a SAM ROM (`run` hands it over when the set is in PinMAME's SAM driver).
pub fn run(cli: &Cli, job: &Job, vpm: &Path) -> Result<(), String> {
    if cli.cold_boot_only {
        return cold_boot_only(cli, job, vpm);
    }
    let wall = Instant::now();
    for (set, what) in [
        (cli.only.is_some(), "--only"),
        (cli.limit.is_some(), "--limit"),
        (cli.factory_volume, "--factory-volume"),
        (cli.merge_twins, "--merge-twins"),
        (cli.check_ducking, "--check-ducking"),
        (cli.dc_block, "--dc-block"),
    ] {
        if set {
            eprintln!("note: {what} does not apply to Stern SAM (static extraction), ignored");
        }
    }
    let (zip, member, rom) = load_image(job)?;
    let pinmame = sam::sam_set(&job.rom);
    eprintln!(
        "SAM: {} ({} bytes, {:.1} banks of 8 MB, CRC32 {:08x}{}) from {}",
        member.name,
        rom.len(),
        rom.len() as f64 / sam::BANK as f64,
        member.crc32,
        match pinmame {
            Some((_, crc, _)) if crc == member.crc32 => " = PinMAME's",
            Some((_, 0, _)) => ", not dumped in PinMAME",
            Some(_) => ", NOT the image PinMAME expects",
            None => "",
        },
        zip.display()
    );
    let t_cat = Instant::now();
    let cat = sam::catalog(&rom)?;
    let catalog_secs = t_cat.elapsed().as_secs_f64();
    eprintln!(
        "SAM: directory at 0x{:x} ({} words, {} languages), {} scripts ({} sound streams, {} music scripts, {} stubs), {} calls ({:.1} s)",
        cat.directory.start,
        cat.directory.n,
        cat.nlang,
        cat.scripts,
        cat.sounds.len(),
        cat.music.len(),
        cat.stubs,
        cat.calls.len(),
        catalog_secs
    );

    // File names: sounds after their first owner, music after its sample.
    let rom_name = job.rom.as_str();
    let mut items: Vec<(Item, String)> = Vec::new();
    let mut names = BTreeSet::new();
    for (i, s) in cat.sounds.iter().enumerate() {
        let (sample, lang) = s.owners[0];
        let f = sound_file(rom_name, sample, lang);
        names.insert(f.clone());
        items.push((Item::Sound(i), f));
    }
    for (i, m) in cat.music.iter().enumerate() {
        let mut f = sound_file(rom_name, m.sample(), 0);
        if names.contains(&f) {
            f = format!("s{:04X}-music-{rom_name}.wav", m.sample());
        }
        names.insert(f.clone());
        items.push((Item::Music(i), f));
    }
    std::fs::create_dir_all(&job.out).map_err(|e| e.to_string())?;
    let t_files = Instant::now();
    let written = write_all(&rom, &cat, &items, &job.out, cli.intro_loop_secs)?;
    let files_secs = t_files.elapsed().as_secs_f64();

    let (cold, warm, factory_note) = if cli.factory() {
        factory(cli, job, vpm)
    } else {
        (
            None,
            None,
            Some("--no-factory: the game was not booted".into()),
        )
    };

    let calls_of = cat.calls_of();
    let speech = cat.localized();
    // (sample, lang) -> item index.
    let mut file_of: BTreeMap<(u16, u8), usize> = BTreeMap::new();
    for (i, (item, _)) in items.iter().enumerate() {
        let owners = match *item {
            Item::Sound(k) => &cat.sounds[k].owners,
            Item::Music(k) => &cat.music[k].owners,
        };
        for &o in owners {
            file_of.entry(o).or_insert(i);
        }
    }

    // The AltSound rows: per call, one row per sample it picks from (language 0).
    let mut rows: Vec<Row> = Vec::new();
    let mut row_item: Vec<usize> = Vec::new();
    let mut missing = 0;
    for c in &cat.calls {
        for &s in &c.samples {
            let Some(&i) = file_of.get(&(s, 0)) else {
                missing += 1;
                continue;
            };
            let w = &written[i];
            let (kind, label) = match items[i].0 {
                Item::Music(k) => {
                    let m = &cat.music[k];
                    let what = match m.song() {
                        Some(song) => format!("song {song} {}", m.role),
                        None => format!("music {}", m.role),
                    };
                    (Kind::Music, what)
                }
                Item::Sound(_) if speech.contains(&s) => (Kind::Callout, "voice".to_string()),
                Item::Sound(_) => (Kind::Sfx, "sound".to_string()),
            };
            let name = altsound::csv_name(
                &format!("call {:04X} {label} sample {s:04X}", c.id),
                &format!("{:04X}", c.id),
            );
            let (fname, looped, continuous) = match &w.loop_info {
                None => (w.file.clone(), false, false),
                Some(l) => match (&l.extended_file, &l.loop_file) {
                    (Some(x), _) => (x.clone(), false, true),
                    (None, Some(body)) => (body.clone(), true, false),
                    (None, None) => (w.file.clone(), true, false),
                },
            };
            rows.push(Row::plain(c.id, kind, looped, continuous, name, fname));
            row_item.push(i);
        }
    }

    // Per file: the calls that reach it and how the pack plays it.
    let mut pack_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (r, &i) in row_item.iter().enumerate() {
        pack_of.entry(i).or_default().push(r);
    }

    // Factory volume.
    let dac = warm.as_ref().and_then(|b| {
        let l = b
            .left
            .as_deref()
            .and_then(|v| u8::from_str_radix(v, 16).ok());
        let r = b
            .right
            .as_deref()
            .and_then(|v| u8::from_str_radix(v, 16).ok());
        l.or(r).map(|l| (l, r.unwrap_or(l)))
    });
    let offset = dac.and_then(|(l, r)| match (dac_db(l), dac_db(r)) {
        (Some(a), Some(b)) => Some((a + b) / 2.0),
        _ => None,
    });
    let factory_volume = warm.as_ref().map(|b| {
        json!({
            "seen": dac.is_some(),
            "source": "the PCM1755 attenuation registers (0x10 left, 0x11 right) as the game last wrote them on a warm boot from the cold boot's nvram (factory settings)",
            "left": b.left, "right": b.right, "soft_mute": b.soft_mute, "dac_off": b.dac_off,
            "attenuation_db": dac.map(|(l, r)| json!({"left": dac_db(l), "right": dac_db(r)})),
            "attenuation_scale": "PCM1755 datasheet: FF = 0 dB, -0.5 dB per step, 80 and below = mute",
            "pinmame_db": dac.map(|(l, r)| json!({"left": pinmame_db(l).map(round1), "right": pinmame_db(r).map(round1)})),
            "pinmame_note": "PinMAME maps the register linearly to its mixer ((v & 7F) * 100 / 7F percent, sam.c), not in 0.5 dB steps: its level differs from the machine's",
            "boot": b,
        })
    });

    // Sounds in the manifest.
    let mut sounds_json = Vec::new();
    let mut loud_all = Vec::new();
    let mut loud_once = Vec::new();
    let (mut secs_all, mut secs_once) = (0.0, 0.0);
    let mut lufs_once = Vec::new();
    let mut clipped = 0;
    for (i, ((item, file), w)) in items.iter().zip(&written).enumerate() {
        let (owners, detail) = match *item {
            Item::Sound(k) => {
                let s = &cat.sounds[k];
                let st = &s.stream;
                let check = (s.script.secs() - st.secs()).abs() < 0.01;
                (
                    &s.owners,
                    json!({
                        "kind": if speech.contains(&s.owners[0].0) { "voice" } else { "sound" },
                        "rate": st.rate(),
                        "stream_samples": st.count,
                        "stream_off": format!("0x{:x}", st.off),
                        "script_off": format!("0x{:x}", s.script.off),
                        "script_secs": round3(s.script.secs()),
                        "voice_mask": format!("{:02X}", s.script.mask),
                        "voices": s.script.voices,
                        "len_check": if check { "ok".to_string() } else {
                            format!("script {:.3} s vs stream {:.3} s", s.script.secs(), st.secs())
                        },
                    }),
                )
            }
            Item::Music(k) => {
                let m = &cat.music[k];
                let tl = &m.timeline;
                let secs = |f: u64| round3(f as f64 / BASE_RATE as f64);
                (
                    &m.owners,
                    json!({
                        "kind": "music",
                        "role": m.role,
                        "song": m.song(),
                        "rate": BASE_RATE,
                        "chunks": tl.events.len(),
                        "chunk_index_first": m.first_index(),
                        "chunk_index_last": m.last_index(),
                        "chunk_rates": tl.events.iter().map(|e| e.1.rate()).collect::<BTreeSet<_>>(),
                        "script_off": format!("0x{:x}", m.script.off),
                        "script_secs": round3(m.script.secs()),
                        "markers": tl.markers.iter().map(|x| json!({"secs": secs(x.at), "value": x.value})).collect::<Vec<_>>(),
                        "joins_declicked": true,
                    }),
                )
            }
        };
        let (sample, lang) = owners[0];
        let id = if lang == 0 {
            format!("s{sample:04X}")
        } else {
            format!("s{sample:04X}-l{lang}")
        };
        let mut calls: Vec<u32> = owners
            .iter()
            .filter(|o| o.1 == 0)
            .flat_map(|o| calls_of.get(&o.0).cloned().unwrap_or_default())
            .collect();
        calls.sort_unstable();
        calls.dedup();
        let looping = w.loop_info.is_some();
        let counted = lang == 0 || matches!(item, Item::Music(_));
        if counted {
            loud_all.push(&w.m.loud);
            secs_all += w.m.duration;
            if !looping {
                loud_once.push(&w.m.loud);
                secs_once += w.m.duration;
                lufs_once.extend(w.m.lufs);
            }
        }
        if w.m.clipped_samples > 0 {
            clipped += 1;
        }
        let pack = pack_of.get(&i).map(|rs| {
            let r = &rows[rs[0]];
            json!({
                "calls": rs.iter().map(|&k| format!("0x{:04X}", rows[k].id)).collect::<Vec<_>>(),
                "file": r.fname,
                "file_kind": match &w.loop_info {
                    None => "one-shot",
                    Some(l) if l.extended_file.is_some() => "intro-loop-extended",
                    Some(_) => "body-loop",
                },
                "loop": u8::from(r.looped) * 100,
                "channel": r.channel,
                "gsound_type": r.gtype.as_str(),
            })
        });
        let mut e = json!({
            "id": id,
            "name": match *item {
                Item::Sound(_) => format!("sample 0x{sample:04X}"),
                Item::Music(k) => match cat.music[k].song() {
                    Some(s) => format!("song {s} {}", cat.music[k].role),
                    None => format!("music {} 0x{sample:04X}", cat.music[k].role),
                },
            },
            "file": file,
            "duration": w.m.duration,
            "blip": false,
            "lufs": w.m.lufs,
            "true_peak_dbtp": w.m.true_peak_dbtp,
            "level_lufs": w.m.level_lufs,
            "peak_dbfs": w.m.peak_dbfs,
            "rms_dbfs": w.m.rms_dbfs,
            "clipped_samples": w.m.clipped_samples,
            "dc_offset": w.m.dc_offset,
            "ended_by": if looping { "loop" } else { "end" },
            "looping_or_truncated": looping,
            "board": "SAM",
            "sample_rate": w.rate,
            "sam": detail,
        });
        e["sam"]["sample"] = json!(format!("0x{sample:04X}"));
        e["sam"]["lang"] = json!(lang);
        e["sam"]["also"] = json!(
            owners[1..]
                .iter()
                .map(|(s, l)| format!("0x{s:04X}/{l}"))
                .collect::<Vec<_>>()
        );
        e["sam"]["calls"] = json!(
            calls
                .iter()
                .map(|c| format!("0x{c:04X}"))
                .collect::<Vec<_>>()
        );
        if let Some(l) = &w.loop_info {
            e["loop"] = json!(l);
        }
        if let Some(p) = pack {
            e["pack"] = p;
        }
        sounds_json.push(e);
    }
    let aggregate = |l: &[&loudness::FileLoudness], secs: f64| {
        let mut a = loudness::aggregate(l.iter().copied(), BASE_RATE);
        a.seconds = round3(secs);
        a
    };
    let all = aggregate(&loud_all, secs_all);
    let once = aggregate(&loud_once, secs_once);
    let median = loudness::median(lufs_once);
    let shift = |x: Option<f64>, o: f64| x.map(|v| round3(v + o));
    let as_shipped = offset.map(|o| {
        json!({
            "factory_offset_db": o,
            "all_lufs": shift(all.lufs, o),
            "excluding_loops_lufs": shift(once.lufs, o),
            "median_file_lufs": shift(median, o),
            "loudest_true_peak_dbtp": shift(all.true_peak_dbtp, o),
        })
    });

    let loops = written.iter().filter(|w| w.loop_info.is_some()).count();
    let extended = written
        .iter()
        .filter(|w| {
            w.loop_info
                .as_ref()
                .is_some_and(|l| l.extended_file.is_some())
        })
        .count();
    let mut songs: BTreeMap<u32, BTreeMap<&str, String>> = BTreeMap::new();
    for m in &cat.music {
        if let Some(s) = m.song() {
            songs
                .entry(s)
                .or_default()
                .insert(m.role, format!("s{:04X}", m.sample()));
        }
    }
    let referenced: BTreeSet<&String> = rows.iter().map(|r| &r.fname).collect();
    let altsound_note = "PinMAME cannot play a SAM AltSound pack today: the SAM CPU mixes its sounds itself and sends no sound command, so PinMAME has no AltSound hook for SAM. altsound.csv / g-sound.csv are keyed by the game's sound call ids (sam.call_table) for editing and measurement, one row per sample a call picks from (language 0); they need a PinMAME change that reports the call ids to play.";
    let call_table = cat.calls.first().map(|c| {
        json!({
            "offset": format!("0x{:x}", c.off),
            "calls": cat.calls.len(),
            "empty_calls": cat.calls.iter().filter(|c| c.samples.is_empty()).count(),
            "record_bytes": 20,
            "note": "call id = record index from the table's start (call 0 plays nothing); +8 points to the 0-terminated list of sample ids the call picks one from; flags = record bytes 12-19",
            "list": cat.calls.iter().map(|c| json!({
                "id": format!("0x{:04X}", c.id),
                "samples": c.samples.iter().map(|s| format!("0x{s:04X}")).collect::<Vec<_>>(),
                "flags": c.raw[12..].iter().map(|b| format!("{b:02x}")).collect::<String>(),
            })).collect::<Vec<_>>(),
        })
    });
    let manifest = json!({
        "rom": job.rom,
        "parent": Value::Null,
        "mode": if cli.factory() { "reference (Stern SAM: static extraction, full scale; factory volume from a warm boot)" } else { "static (Stern SAM: sounds read from the flash image)" },
        "factory": cold,
        "boards": ["SAM (software mixer, PCM1755 DAC)"],
        "sample_rate": BASE_RATE,
        "channels": 1,
        "factory_volume": factory_volume,
        "volume_init": Value::Null,
        "levels_note": "Every file holds the decoded samples at full scale: what the DAC plays at 0 dB (attenuation FF), the reference volume. The game's own per-voice volume ramps (script opcode 09) are not applied. Add factory_offset_db to a level to get it at the factory volume.",
        "factory_offset_db": offset,
        "factory_offset": {
            "method": "PCM1755 attenuation (0.5 dB per step from FF) of the master volume the game writes on a warm boot from its factory nvram; the mean of left and right",
            "reference_volume": "FF",
            "factory_volume": dac.map(|(l, r)| format!("{} {}", hex2(l), hex2(r))),
            "verified": false,
            "note": factory_note.clone().unwrap_or_else(|| "the last attenuation the game wrote during the warm boot (factory_volume.boot.writes). That it follows the operator's volume setting is not verified: the game may also scale its mix in software".into()),
        },
        "counts": {
            "written": written.len(),
            "sounds": cat.sounds.len(),
            "music": cat.music.len(),
            "loops": loops,
            "songs": songs.len(),
            "stub_scripts": cat.stubs,
            "clipped": clipped,
            "blips": 0,
        },
        "loudness": {
            "measured_as": "DC-blocked mono file duplicated to 2 identical channels (VPX playback); the first language's sounds and every music file",
            "all": all,
            "excluding_loops": once,
            "median_file_lufs": median,
            "as_shipped": as_shipped,
        },
        "sam": {
            "image": {
                "zip": zip, "member": member.name, "bytes": rom.len(),
                "banks": round1(rom.len() as f64 / sam::BANK as f64),
                "crc32": format!("{:08x}", member.crc32),
                "pinmame_crc32": pinmame.map(|p| format!("{:08x}", p.1)),
            },
            "directory": {
                "offset": format!("0x{:x}", cat.directory.start),
                "words": cat.directory.n,
                "languages": cat.nlang,
                "samples": cat.directory.n / cat.nlang.max(1),
                "entries": cat.entries.len(),
            },
            "scripts": {
                "distinct": cat.scripts,
                "sound_streams": cat.sounds.len(),
                "music": cat.music.len(),
                "stubs": cat.stubs,
                "unknown_opcodes": cat.unknown_opcodes,
            },
            "call_table": call_table,
            "songs": songs,
            "format": "reverse engineered by Ashram56 on Tron LE (https://github.com/Ashram56/Tron-Legacy-LE-ROM-Decryption); see docs/how-it-works.md, Stern SAM",
            "timing": { "catalog_secs": round1(catalog_secs), "files_secs": round1(files_secs) },
        },
        "altsound": if cli.no_altsound { Value::Null } else { json!({
            "files": [altsound::ALTSOUND_CSV, altsound::GSOUND_CSV, altsound::ALTSOUND_INI],
            "rows": rows.len(),
            "calls": cat.calls.iter().filter(|c| !c.samples.is_empty()).count(),
            "samples_without_a_file": missing,
            "loops_with_smpl": loops,
            "intro_loops_extended": extended,
            "intro_loop_secs": cli.intro_loop_secs,
            "files_referenced": referenced.len(),
            "twins": 0,
            "playable_in_pinmame": false,
            "note": altsound_note,
        }) },
        "sounds": sounds_json,
    });
    let path = job.out.join("manifest.json");
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !cli.no_altsound {
        for (name, text) in [
            (altsound::ALTSOUND_CSV, altsound::altsound_csv(&rows)),
            (altsound::GSOUND_CSV, altsound::gsound_csv(&rows)),
            (altsound::ALTSOUND_INI, sam_ini()),
        ] {
            let p = job.out.join(name);
            std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }

    // Summary.
    let music_secs: f64 = items
        .iter()
        .zip(&written)
        .filter(|(it, _)| matches!(it.0, Item::Music(_)))
        .map(|(_, w)| w.m.duration)
        .sum();
    println!(
        "{}: Stern SAM, static: {} sound stream(s), {} music script(s) ({} songs, {:.1} min), {} loop(s) ({} extended), {} file(s) reaching full scale as decoded; {} languages; {} calls; {:.1} s wall",
        job.rom,
        cat.sounds.len(),
        cat.music.len(),
        songs.len(),
        music_secs / 60.0,
        loops,
        extended,
        clipped,
        cat.nlang,
        cat.calls.len(),
        wall.elapsed().as_secs_f64()
    );
    match (&dac, offset) {
        (Some((l, r)), Some(o)) => println!(
            "  factory_volume: PCM1755 attenuation {} {} = {o:+.1} dB (reference FF = 0 dB; PinMAME plays it at {})",
            hex2(*l),
            hex2(*r),
            pinmame_db(*l).map_or("mute".into(), |v| format!("{v:+.1} dB"))
        ),
        (Some((l, r)), None) => println!(
            "  factory_volume: PCM1755 attenuation {} {} (mute)",
            hex2(*l),
            hex2(*r)
        ),
        _ => println!(
            "  factory_volume: not measured ({})",
            factory_note.as_deref().unwrap_or("no DAC write seen")
        ),
    }
    let fmt = |a: &loudness::Aggregate| {
        format!(
            "{} files, {:.1} s: {}, loudest true peak {}",
            a.files,
            a.seconds,
            a.lufs.map_or("n/a".into(), |v| format!("{v:.1} LUFS")),
            a.true_peak_dbtp
                .map_or("n/a".into(), |v| format!("{v:.1} dBTP"))
        )
    };
    println!("  loudness (full scale): all {}", fmt(&all));
    println!("    without loops:     {}", fmt(&once));
    if !cli.no_altsound {
        println!(
            "  altsound: {} row(s) for {} call(s), {} file(s) referenced (not playable by PinMAME today: SAM sends no sound command)",
            rows.len(),
            cat.calls.iter().filter(|c| !c.samples.is_empty()).count(),
            referenced.len()
        );
    }
    Ok(())
}

/// The plain altsound.ini, with a SAM note on top.
fn sam_ini() -> String {
    format!(
        "; Stern SAM: PinMAME cannot play this pack today (SAM sends no sound command).\n\
         ; The IDs are the game's sound call ids, for editing and measurement.\n{}",
        altsound::altsound_ini()
    )
}

/// The G-Sound type a music row gets (kept here for the tests of the SAM rows).
#[cfg(test)]
fn music_gtype(looped: bool, continuous: bool) -> altsound::GType {
    Row::plain(
        0,
        Kind::Music,
        looped,
        continuous,
        String::new(),
        String::new(),
    )
    .gtype
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altsound::GType;

    #[test]
    fn dac_attenuation_scale() {
        assert_eq!(dac_db(0xFF), Some(0.0));
        assert_eq!(dac_db(0xFE), Some(-0.5));
        assert_eq!(dac_db(0x81), Some(-63.0));
        assert_eq!(dac_db(0x80), None);
        assert_eq!(dac_db(0x00), None);
        // PinMAME: FF -> 100 %, C0 -> 64 * 100 / 127 = 50 % (-6.0 dB).
        assert_eq!(pinmame_db(0xFF), Some(0.0));
        assert!((pinmame_db(0xC0).unwrap() + 6.02).abs() < 0.01);
        assert_eq!(pinmame_db(0x80), None);
    }

    #[test]
    fn music_rows_types() {
        assert_eq!(music_gtype(true, false), GType::Music);
        assert_eq!(music_gtype(false, true), GType::Music);
        assert_eq!(music_gtype(false, false), GType::Sfx);
    }

    #[test]
    fn later_cycles_are_declicked_from_the_previous_cycle() {
        let body = vec![0i16, 0, 0, 900, 950, 1000, 800];
        let c = next_cycle(&body, 3);
        // From 800 (the body's end) to 900.
        assert_eq!(c, vec![825, 850, 875, 900, 950, 1000, 800]);
        assert_eq!(next_cycle(&body, 0), body);
    }

    #[test]
    fn synthetic_image_files_and_rows() {
        let s = sam::tests::synthetic();
        let cat = sam::catalog(&s.rom).unwrap();
        let dir = std::env::temp_dir().join(format!("rom2altsound-sam-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut items: Vec<(Item, String)> = (0..cat.sounds.len())
            .map(|i| {
                let (s, l) = cat.sounds[i].owners[0];
                (Item::Sound(i), sound_file("t", s, l))
            })
            .collect();
        items.push((Item::Music(0), sound_file("t", cat.music[0].sample(), 0)));
        let w = write_all(&s.rom, &cat, &items, &dir, 1.0).unwrap();
        assert_eq!(w.len(), 13);
        // Sound files: the stream at its own rate, sample-exact.
        let r = hound::WavReader::open(dir.join("s0001-t.wav")).unwrap();
        assert_eq!(r.spec().sample_rate, 12000);
        assert_eq!(r.len() as usize, cat.sounds[1].stream.count);
        // The song: 21000 frames, loop from 6000 to the end, a body file and an extended
        // file of at least 1 s (intro + whole cycles).
        let l = w[12].loop_info.as_ref().unwrap();
        assert_eq!((l.intro_samples, l.period_samples), (6000, 15000));
        assert_eq!(l.loop_file.as_deref(), Some("s000C-t-loop.wav"));
        let x = hound::WavReader::open(dir.join("s000C-t-extended.wav")).unwrap();
        assert_eq!(x.len() as usize, 6000 + 2 * 15000);
        let bytes = std::fs::read(dir.join("s000C-t.wav")).unwrap();
        let tail = &bytes[bytes.len() - 68..];
        assert_eq!(&tail[..4], b"smpl");
        assert_eq!(u32::from_le_bytes(tail[52..56].try_into().unwrap()), 6000);
        assert_eq!(u32::from_le_bytes(tail[56..60].try_into().unwrap()), 20999);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
