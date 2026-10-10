//! Several ROMs in one command: each ROM is extracted by a child process of this program
//! (libpinmame runs one machine per process), up to `--jobs` at a time. A failing ROM
//! does not stop the others; the exit code says whether any failed.

use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::Cli;

/// Where a ROM argument points: its set name and the directory holding its zip.
#[derive(Debug, PartialEq)]
pub struct RomSpec {
    pub name: String,
    pub dir: PathBuf,
}

/// Resolves a ROM argument: a path to a zip (its directory is the ROM directory), or a
/// set name looked up in `roms`, else in the current directory, then in `./roms`.
pub fn resolve(arg: &str, roms: Option<&Path>) -> Result<RomSpec, String> {
    let p = Path::new(arg);
    let is_zip = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if is_zip || p.is_file() {
        if !p.is_file() {
            return Err(format!("{arg}: no such file"));
        }
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{arg}: not a ROM zip name"))?
            .to_owned();
        let dir = p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf();
        return Ok(RomSpec { name, dir });
    }
    let dirs: Vec<PathBuf> = match roms {
        Some(d) => vec![d.to_path_buf()],
        None => vec![PathBuf::from("."), PathBuf::from("roms")],
    };
    for d in &dirs {
        if d.join(format!("{arg}.zip")).is_file() {
            return Ok(RomSpec {
                name: arg.to_owned(),
                dir: d.clone(),
            });
        }
    }
    let where_ = dirs
        .iter()
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(" or ");
    Err(format!(
        "{arg}.zip not found in {where_} (give the zip's path, or --roms <DIR>)"
    ))
}

/// The arguments of the child process that extracts one ROM: every option of `cli`,
/// passed on as given.
fn child_args(cli: &Cli, rom: &RomSpec, out: &Path) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec![
        rom.name.clone().into(),
        "--in-process".into(),
        "--roms".into(),
        rom.dir.clone().into(),
        "--out".into(),
        out.into(),
    ];
    let mut opt = |k: &str, v: Option<String>| {
        if let Some(v) = v {
            a.push(k.into());
            a.push(v.into());
        }
    };
    opt("--only", cli.only.as_ref().map(|v| v.join(",")));
    opt("--limit", cli.limit.map(|v| v.to_string()));
    opt(
        "--volume",
        cli.volume.map(|m| {
            clap::ValueEnum::to_possible_value(&m)
                .map_or_else(String::new, |p| p.get_name().to_owned())
        }),
    );
    opt("--dcs-volume", cli.dcs_volume.map(|v| format!("{v:02X}")));
    opt(
        "--whitestar-volume",
        cli.whitestar_volume.map(|v| format!("{v:02X}")),
    );
    opt("--wpcs-volume", cli.wpcs_volume.map(|v| format!("{v:02X}")));
    opt("--boot-secs", Some(cli.boot_secs.to_string()));
    opt("--boot-max-secs", Some(cli.boot_max_secs.to_string()));
    opt("--max-secs", Some(cli.max_secs.to_string()));
    opt("--loop-max-secs", Some(cli.loop_max_secs.to_string()));
    opt("--no-sound-secs", Some(cli.no_sound_secs.to_string()));
    opt("--intro-loop-secs", Some(cli.intro_loop_secs.to_string()));
    opt(
        "--vpm",
        cli.vpm.as_ref().map(|v| v.to_string_lossy().into_owned()),
    );
    opt(
        "--sounds-dat",
        cli.sounds_dat
            .as_ref()
            .map(|v| v.to_string_lossy().into_owned()),
    );
    opt(
        "--names",
        cli.names.as_ref().map(|v| v.to_string_lossy().into_owned()),
    );
    opt("--stop", cli.stop.clone());
    opt("--sound-rom-from", cli.sound_rom_from.clone());
    opt("--solo", cli.solo.clone());
    opt(
        "--sam-volume-test",
        cli.sam_volume_test.map(|n| n.to_string()),
    );
    opt(
        "--dump-sound-region",
        cli.dump_sound_region
            .as_ref()
            .map(|v| v.to_string_lossy().into_owned()),
    );
    for (flag, on) in [
        ("--no-factory", cli.no_factory),
        ("--factory-volume", cli.factory_volume),
        ("--no-volume-init", cli.no_volume_init),
        ("--cold-boot-only", cli.cold_boot_only),
        ("--dc-block", cli.dc_block),
        ("--throttled", cli.throttled),
        ("--verbose", cli.verbose),
        ("--no-altsound", cli.no_altsound),
        ("--no-html", cli.no_html),
        ("--merge-twins", cli.merge_twins),
        ("--check-ducking", cli.check_ducking),
        ("--no-refresh", cli.no_refresh),
        ("--no-chip-check", cli.no_chip_check),
        ("--bsmt-hle", cli.bsmt_hle),
        ("--force-names", cli.force_names),
        ("--force-sound-rom", cli.force_sound_rom),
    ] {
        if on {
            a.push(flag.into());
        }
    }
    for d in firmware_dirs(cli, rom) {
        a.push("--firmware-dir".into());
        a.push(d.into());
    }
    a
}

/// Where the BSMT2000's program (bsmt2000.zip or a bsmt2000/ folder) is looked for: next to
/// the ROM's zip, in --roms, then in ./roms (absolute: the child may not resolve them alike).
fn firmware_dirs(cli: &Cli, rom: &RomSpec) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    for d in [
        Some(rom.dir.as_path()),
        cli.roms.as_deref(),
        Some(Path::new("roms")),
    ]
    .into_iter()
    .flatten()
    {
        let d = std::path::absolute(d).unwrap_or_else(|_| d.to_path_buf());
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

/// How one ROM went.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub rom: String,
    /// Its folder (empty when the ROM was not found).
    pub out: PathBuf,
    pub secs: f64,
    pub error: Option<String>,
}

/// What the engine reports while it runs.
pub enum Event<'a> {
    /// A ROM could not be started (not found, or its process could not start).
    NotStarted(&'a Outcome),
    /// A ROM's process started; `log` is its log file when its output goes there.
    Started {
        rom: &'a str,
        out: &'a Path,
        log: Option<&'a Path>,
    },
    /// A line of a ROM's log (watched runs only).
    Log { rom: &'a str, line: &'a str },
    /// Where a ROM stands (watched runs only).
    Progress {
        rom: &'a str,
        step: &'a crate::progress::Step,
    },
    /// A ROM's process ended.
    Finished(&'a Outcome),
}

/// A watched run (the window program): every ROM's output goes to its log file and is read
/// back as [`Event::Log`] lines, its progress comes as [`Event::Progress`], and setting
/// `cancel` stops everything (each ROM's processes are killed, and the ROMs not started yet
/// are not started).
#[derive(Default)]
pub struct Watch {
    pub cancel: Arc<AtomicBool>,
}

/// The message of a ROM stopped by [`Watch::cancel`].
pub const CANCELLED: &str = "cancelled";

struct Running {
    rom: RomSpec,
    out: PathBuf,
    child: Child,
    start: Instant,
    log: Option<Tail>,
    progress: Option<(Tail, crate::progress::Tracker)>,
}

/// The ROM arguments that cannot go together; `Err` is the message.
pub fn check(cli: &Cli) -> Result<(), String> {
    if cli.names.is_some() && cli.rom_args.len() > 1 {
        // Names belong to one sound ROM.
        return Err("--names goes with one ROM (each sound ROM has its own names)".into());
    }
    Ok(())
}

/// The folder every ROM's folder goes in.
pub fn root(cli: &Cli) -> PathBuf {
    cli.out.clone().unwrap_or_else(|| PathBuf::from("."))
}

/// Extracts every ROM of the command line; returns the process exit code.
pub fn run(cli: &Cli) -> i32 {
    if let Err(e) = check(cli) {
        eprintln!("error: {e}");
        return 2;
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot find this program's path: {e}");
            return 1;
        }
    };
    let outcomes = run_with(cli, &exe, None, &mut |e| match e {
        Event::NotStarted(o) => eprintln!("{}: {}", o.rom, o.error.as_deref().unwrap_or("")),
        Event::Started {
            rom, log: Some(l), ..
        } => eprintln!("{rom}: started (log: {})", l.display()),
        Event::Finished(o) => println!("{}", line(o)),
        _ => {}
    });
    let code = recap(&outcomes);
    match write_index(cli, &outcomes) {
        Some(Ok(p)) => println!("pages of every ROM: {}", p.display()),
        Some(Err(e)) => eprintln!("cannot write the index page: {e}"),
        None => {}
    }
    code
}

/// The page linking every ROM's page at the output root, once some ROM went through (and
/// pages are written); its path, or why it could not be written.
pub fn write_index(cli: &Cli, outcomes: &[Outcome]) -> Option<Result<PathBuf, String>> {
    if cli.no_html || cli.cold_boot_only || !outcomes.iter().any(|o| o.error.is_none()) {
        return None;
    }
    crate::listen::write_index(&root(cli)).transpose()
}

/// Extracts every ROM of `cli` with child processes of `exe` (this program), reporting what
/// happens to `on`; returns how each ROM went, in the order they ended.
pub fn run_with(
    cli: &Cli,
    exe: &Path,
    watch: Option<&Watch>,
    on: &mut dyn FnMut(Event),
) -> Vec<Outcome> {
    let root = root(cli);
    let mut outcomes = Vec::new();
    let mut queue = Vec::new();
    for arg in &cli.rom_args {
        match resolve(arg, cli.roms.as_deref()) {
            Ok(r) => queue.push(r),
            Err(e) => {
                let o = Outcome {
                    rom: arg.clone(),
                    out: PathBuf::new(),
                    secs: 0.0,
                    error: Some(e),
                };
                on(Event::NotStarted(&o));
                outcomes.push(o);
            }
        }
    }
    let cancelled = || watch.is_some_and(|w| w.cancel.load(Ordering::Relaxed));
    let jobs = (cli.jobs as usize).min(queue.len()).max(1);
    // One at a time on a terminal: the child writes to it. Otherwise: each to a log file in
    // its ROM folder, and only the summaries come here.
    let live = jobs == 1 && watch.is_none();
    queue.reverse();
    let mut running: Vec<Running> = Vec::new();
    while !queue.is_empty() || !running.is_empty() {
        while running.len() < jobs && !cancelled() {
            let Some(rom) = queue.pop() else { break };
            let out = root.join(&rom.name);
            let progress = watch.map(|_| progress_file(&rom.name));
            match spawn(exe, cli, &rom, &out, live, progress.as_deref()) {
                Ok(child) => {
                    let log = (!live).then(|| out.join(LOG));
                    on(Event::Started {
                        rom: &rom.name,
                        out: &out,
                        log: log.as_deref(),
                    });
                    running.push(Running {
                        log: watch.and(log).map(Tail::new),
                        progress: progress.map(|p| (Tail::new(p), Default::default())),
                        rom,
                        out,
                        child,
                        start: Instant::now(),
                    })
                }
                Err(e) => {
                    let o = Outcome {
                        rom: rom.name,
                        out,
                        secs: 0.0,
                        error: Some(e),
                    };
                    on(Event::NotStarted(&o));
                    outcomes.push(o);
                }
            }
        }
        if cancelled() {
            for mut job in running.drain(..) {
                kill_tree(&mut job.child);
                follow(&mut job, on);
                let o = Outcome {
                    rom: job.rom.name,
                    out: job.out,
                    secs: job.start.elapsed().as_secs_f64(),
                    error: Some(CANCELLED.into()),
                };
                if let Some((p, _)) = &job.progress {
                    let _ = std::fs::remove_file(&p.path);
                }
                on(Event::Finished(&o));
                outcomes.push(o);
            }
            break;
        }
        let mut i = 0;
        while i < running.len() {
            follow(&mut running[i], on);
            match running[i].child.try_wait() {
                Ok(None) => i += 1,
                r => {
                    let mut job = running.swap_remove(i);
                    follow(&mut job, on);
                    if let Some((p, _)) = &job.progress {
                        let _ = std::fs::remove_file(&p.path);
                    }
                    let error = match r {
                        Ok(Some(st)) if st.success() => None,
                        Ok(Some(st)) => Some(match last_error(&job.out, live) {
                            Some(l) => format!("{l} ({st})"),
                            None => format!("failed ({st})"),
                        }),
                        Err(e) => Some(e.to_string()),
                        Ok(None) => unreachable!(),
                    };
                    let o = Outcome {
                        rom: job.rom.name,
                        out: job.out,
                        secs: job.start.elapsed().as_secs_f64(),
                        error,
                    };
                    on(Event::Finished(&o));
                    outcomes.push(o);
                }
            }
        }
        if !running.is_empty() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    outcomes
}

/// Reports what a watched ROM wrote since the last look.
fn follow(job: &mut Running, on: &mut dyn FnMut(Event)) {
    let rom = job.rom.name.as_str();
    if let Some(t) = &mut job.log {
        for l in t.lines() {
            on(Event::Log { rom, line: &l });
        }
    }
    if let Some((t, tracker)) = &mut job.progress {
        for l in t.lines() {
            if let Some(step) = tracker.line(&l) {
                on(Event::Progress { rom, step });
            }
        }
    }
}

/// The progress file of a watched ROM (a few lines, removed when the ROM ends).
fn progress_file(rom: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "rom2altsound-progress-{}-{rom}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// A file another process appends to, read line by line as it grows.
struct Tail {
    path: PathBuf,
    pos: u64,
    partial: Vec<u8>,
}

impl Tail {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            pos: 0,
            partial: Vec::new(),
        }
    }

    /// The whole lines written since the last call.
    fn lines(&mut self) -> Vec<String> {
        let Ok(mut f) = File::open(&self.path) else {
            return Vec::new();
        };
        if f.seek(SeekFrom::Start(self.pos)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.pos += buf.len() as u64;
        self.partial.extend_from_slice(&buf);
        let mut out = Vec::new();
        while let Some(i) = self.partial.iter().position(|&b| b == b'\n') {
            let l: Vec<u8> = self.partial.drain(..=i).collect();
            out.push(
                String::from_utf8_lossy(&l)
                    .trim_end_matches(['\r', '\n'])
                    .to_owned(),
            );
        }
        out
    }
}

/// Stops a ROM's process and the ones it started (the factory cold boot).
fn kill_tree(child: &mut Child) {
    // Its process group (see `spawn`): the child and every process it started.
    #[cfg(unix)]
    if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: a plain system call on a process group this program created.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

const LOG: &str = "rom2altsound.log";

fn spawn(
    exe: &Path,
    cli: &Cli,
    rom: &RomSpec,
    out: &Path,
    live: bool,
    progress: Option<&Path>,
) -> Result<Child, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut cmd = Command::new(exe);
    cmd.args(child_args(cli, rom, out));
    if !live {
        let log = out.join(LOG);
        let f = File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
        let g = f
            .try_clone()
            .map_err(|e| format!("{}: {e}", log.display()))?;
        cmd.stdout(Stdio::from(f)).stderr(Stdio::from(g));
    }
    if let Some(p) = progress {
        cmd.env(crate::progress::ENV, p).stdin(Stdio::null());
        // A process group of its own, so that a cancel stops the processes it starts too.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
    }
    cmd.spawn().map_err(|e| format!("cannot start: {e}"))
}

/// A ROM's listening page, when it was written.
pub fn page_of(out: &Path) -> Option<PathBuf> {
    let p = out.join(crate::listen::PAGE);
    p.is_file().then_some(p)
}

/// The child's last `error:` line, from its log file (when it was not live).
fn last_error(out: &Path, live: bool) -> Option<String> {
    if live {
        return None;
    }
    let text = std::fs::read_to_string(out.join(LOG)).ok()?;
    text.lines()
        .rev()
        .find(|l| l.starts_with("error: "))
        .map(|l| l.trim_start_matches("error: ").to_owned())
}

/// One line per ROM, from its manifest.
pub fn line(o: &Outcome) -> String {
    if let Some(e) = &o.error {
        return format!("{}: FAILED: {e}", o.rom);
    }
    let m: Option<serde_json::Value> = std::fs::read_to_string(o.out.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let detail = m.map_or_else(String::new, |m| {
        let c = &m["counts"];
        let mut d = format!(
            ": {} sound(s), {} loop(s)",
            c["written"].as_u64().unwrap_or(0),
            c["loops"].as_u64().unwrap_or(0)
        );
        if let Some(t) = m["altsound"]["twins"].as_u64() {
            d.push_str(&format!(", {t} twin(s)"));
        }
        if let Some(e) = m["bsmt2000"]["emulation"].as_str() {
            d.push_str(&format!(", BSMT2000 {}", e.to_uppercase()));
        }
        d
    });
    format!(
        "{}: OK in {:.0} s{detail} -> {}",
        o.rom,
        o.secs,
        o.out.display()
    )
}

/// The final recap; returns the exit code (1 if any ROM failed).
fn recap(outcomes: &[Outcome]) -> i32 {
    let failed = outcomes.iter().filter(|o| o.error.is_some()).count();
    if outcomes.len() > 1 {
        println!();
        println!(
            "{} ROM(s): {} done, {} failed",
            outcomes.len(),
            outcomes.len() - failed,
            failed
        );
        for o in outcomes {
            println!("  {}", line(o));
        }
    }
    i32::from(failed > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_zip_path_and_name() {
        let dir = std::env::temp_dir().join(format!("rom2altsound-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("afm_113b.zip");
        std::fs::write(&zip, b"").unwrap();
        let r = resolve(zip.to_str().unwrap(), None).unwrap();
        assert_eq!(r.name, "afm_113b");
        assert_eq!(r.dir, dir);
        let r = resolve("afm_113b", Some(&dir)).unwrap();
        assert_eq!(r.dir, dir);
        assert!(resolve("nope_10", Some(&dir)).is_err());
        assert!(resolve(dir.join("nope.zip").to_str().unwrap(), None).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
