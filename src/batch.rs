//! Several ROMs in one command: each ROM is extracted by a child process of this program
//! (libpinmame runs one machine per process), up to `--jobs` at a time. A failing ROM
//! does not stop the others; the exit code says whether any failed.

use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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
    opt("--stop", cli.stop.clone());
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
struct Outcome {
    rom: String,
    out: PathBuf,
    secs: f64,
    error: Option<String>,
}

struct Running {
    rom: RomSpec,
    out: PathBuf,
    child: Child,
    start: Instant,
}

/// Extracts every ROM of the command line; returns the process exit code.
pub fn run(cli: &Cli) -> i32 {
    let root = cli.out.clone().unwrap_or_else(|| PathBuf::from("."));
    let mut outcomes = Vec::new();
    let mut queue = Vec::new();
    for arg in &cli.rom_args {
        match resolve(arg, cli.roms.as_deref()) {
            Ok(r) => queue.push(r),
            Err(e) => {
                eprintln!("{arg}: {e}");
                outcomes.push(Outcome {
                    rom: arg.clone(),
                    out: PathBuf::new(),
                    secs: 0.0,
                    error: Some(e),
                });
            }
        }
    }
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot find this program's path: {e}");
            return 1;
        }
    };
    let jobs = (cli.jobs as usize).min(queue.len()).max(1);
    // One at a time: the child writes to the terminal. Several: each to a log file in its
    // ROM folder, and only the summaries come here.
    let live = jobs == 1;
    queue.reverse();
    let mut running: Vec<Running> = Vec::new();
    while !queue.is_empty() || !running.is_empty() {
        while running.len() < jobs {
            let Some(rom) = queue.pop() else { break };
            let out = root.join(&rom.name);
            match spawn(&exe, cli, &rom, &out, live) {
                Ok(child) => {
                    if !live {
                        eprintln!("{}: started (log: {})", rom.name, out.join(LOG).display());
                    }
                    running.push(Running {
                        rom,
                        out,
                        child,
                        start: Instant::now(),
                    })
                }
                Err(e) => {
                    eprintln!("{}: {e}", rom.name);
                    outcomes.push(Outcome {
                        rom: rom.name,
                        out,
                        secs: 0.0,
                        error: Some(e),
                    });
                }
            }
        }
        let mut i = 0;
        while i < running.len() {
            match running[i].child.try_wait() {
                Ok(None) => i += 1,
                r => {
                    let job = running.swap_remove(i);
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
                    println!("{}", line(&o));
                    outcomes.push(o);
                }
            }
        }
        if !running.is_empty() {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    let code = recap(&outcomes);
    if !cli.no_html && !cli.cold_boot_only && outcomes.iter().any(|o| o.error.is_none()) {
        match crate::listen::write_index(&root) {
            Ok(Some(p)) => println!("pages of every ROM: {}", p.display()),
            Ok(None) => {}
            Err(e) => eprintln!("cannot write the index page: {e}"),
        }
    }
    code
}

const LOG: &str = "rom2altsound.log";

fn spawn(exe: &Path, cli: &Cli, rom: &RomSpec, out: &Path, live: bool) -> Result<Child, String> {
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
    cmd.spawn().map_err(|e| format!("cannot start: {e}"))
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
fn line(o: &Outcome) -> String {
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
