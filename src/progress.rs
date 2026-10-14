//! Machine-readable progress of one ROM's extraction, for a front end (the window program).
//!
//! The child process that extracts a ROM writes one JSON object per line to the file named
//! by [`ENV`], when it is set; nothing is written otherwise, and the terminal output never
//! changes. The batch engine reads the file back ([`Tracker`]) while the child runs.
//!
//! `--progress json` hands the same steps on to another program (VPin Studio...): the
//! parent process writes them, every ROM's, one [`Line`] per line on its stdout.

use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

/// The environment variable naming the progress file (JSON lines, appended).
pub const ENV: &str = "ROM2ALTSOUND_PROGRESS";

fn sink() -> Option<&'static Mutex<File>> {
    static SINK: OnceLock<Option<Mutex<File>>> = OnceLock::new();
    SINK.get_or_init(|| {
        let path = std::env::var_os(ENV).filter(|p| !p.is_empty())?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(Mutex::new)
    })
    .as_ref()
}

fn emit(v: Value) {
    if let Some(f) = sink()
        && let Ok(mut f) = f.lock()
    {
        let _ = writeln!(f, "{v}");
        let _ = f.flush();
    }
}

/// A stage of the extraction began: "cold-boot" (the factory settings), "boot", "pack".
pub fn phase(name: &str) {
    emit(json!({ "event": "phase", "phase": name }));
}

/// A command is being played: `pass` names the pass ("Main", "Retry", "Chips"...), `left`
/// is how many commands of that pass come after this one.
pub fn command(pass: &str, left: usize, id: &str, name: &str) {
    emit(json!({ "event": "command", "pass": pass, "left": left, "id": id, "name": name }));
}

/// Where one ROM stands, as its progress lines tell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Step {
    /// The stage: "cold-boot", "boot", "pack", or the pass of the commands ("Main"...).
    pub stage: String,
    /// The command being played in the pass, from 1, and how many the pass has (0 outside
    /// the commands).
    pub n: usize,
    pub total: usize,
    /// The command's id and name.
    pub id: String,
    pub name: String,
}

/// Turns progress lines into [`Step`]s (a pass's size is known from its first command).
#[derive(Debug, Default)]
pub struct Tracker {
    step: Step,
}

impl Tracker {
    /// Reads one line; returns the new step when the line is a progress event.
    pub fn line(&mut self, line: &str) -> Option<&Step> {
        let v: Value = serde_json::from_str(line).ok()?;
        let s = &mut self.step;
        match v["event"].as_str()? {
            "phase" => {
                *s = Step {
                    stage: v["phase"].as_str()?.to_owned(),
                    ..Step::default()
                };
            }
            "command" => {
                let pass = v["pass"].as_str()?;
                let left = v["left"].as_u64()? as usize;
                // A new pass, or the same one refilled: more commands left than there were.
                if s.stage != pass || left >= s.total.saturating_sub(s.n) {
                    s.stage = pass.to_owned();
                    s.total = left + 1;
                }
                s.n = s.total - left;
                s.id = v["id"].as_str().unwrap_or_default().to_owned();
                s.name = v["name"].as_str().unwrap_or_default().to_owned();
            }
            _ => return None,
        }
        Some(&self.step)
    }
}

/// `--progress`: how the parent reports every ROM's progress on its stdout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// One JSON object per line (JSON Lines), flushed per line.
    Json,
}

/// How a ROM ended, for [`Line::Done`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Failed,
    /// The set has nothing to extract (its manifest says why).
    NotApplicable,
}

/// One line of `--progress json`.
#[derive(Debug, Clone, PartialEq)]
pub enum Line<'a> {
    /// A ROM's process started.
    Start { rom: &'a str, out: &'a str },
    /// Where a ROM stands: a stage, or the command of a pass being recorded.
    Step { rom: &'a str, step: &'a Step },
    /// A ROM ended (or could not start, or was skipped by `--skip-done`).
    Done {
        rom: &'a str,
        status: Status,
        /// The sound files written (0 without a manifest).
        written: u64,
        /// Its folder (empty when the ROM was not found).
        out: &'a str,
        secs: f64,
        /// Why it failed, or why it is not applicable.
        message: Option<&'a str>,
        skipped: bool,
    },
    /// Every ROM ended.
    AllDone {
        ok: usize,
        failed: usize,
        not_applicable: usize,
    },
}

/// The fields of every line, in the order they are written (`rom` and `stage` first);
/// the ones a line does not have are left out.
#[derive(Default, serde::Serialize)]
struct Wire<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    rom: Option<&'a str>,
    stage: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    out: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    ok: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    not_applicable: Option<usize>,
}

impl Line<'_> {
    /// The JSON object, on one line (without its newline).
    pub fn to_json(&self) -> String {
        let w = match *self {
            Line::Start { rom, out } => Wire {
                rom: Some(rom),
                stage: "start",
                out: Some(out),
                ..Wire::default()
            },
            Line::Step { rom, step } if step.total > 0 => Wire {
                rom: Some(rom),
                stage: "record",
                pass: Some(kebab(&step.stage)),
                done: Some(step.n),
                total: Some(step.total),
                id: Some(&step.id),
                name: Some(&step.name),
                ..Wire::default()
            },
            Line::Step { rom, step } => Wire {
                rom: Some(rom),
                stage: &step.stage,
                ..Wire::default()
            },
            Line::Done {
                rom,
                status,
                written,
                out,
                secs,
                message,
                skipped,
            } => Wire {
                rom: Some(rom),
                stage: "done",
                status: Some(match status {
                    Status::Ok => "ok",
                    Status::Failed => "failed",
                    Status::NotApplicable => "not_applicable",
                }),
                written: Some(written),
                out: Some(out),
                secs: Some((secs * 10.0).round() / 10.0),
                error: message.filter(|_| status == Status::Failed),
                reason: message.filter(|_| status == Status::NotApplicable),
                skipped,
                ..Wire::default()
            },
            Line::AllDone {
                ok,
                failed,
                not_applicable,
            } => Wire {
                stage: "all_done",
                ok: Some(ok),
                failed: Some(failed),
                not_applicable: Some(not_applicable),
                ..Wire::default()
            },
        };
        serde_json::to_string(&w).unwrap_or_default()
    }
}

/// A pass as the extraction names it ("VolumeCheck"), in the stages' style
/// ("volume-check").
fn kebab(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_counts_each_pass() {
        let mut t = Tracker::default();
        assert_eq!(
            t.line(r#"{"event":"phase","phase":"boot"}"#).unwrap().stage,
            "boot"
        );
        let cmd = |pass: &str, left: usize| {
            format!(
                r#"{{"event":"command","pass":"{pass}","left":{left},"id":"0x0001","name":"x"}}"#
            )
        };
        let s = t.line(&cmd("Main", 2)).unwrap().clone();
        assert_eq!((s.n, s.total), (1, 3));
        let s = t.line(&cmd("Main", 0)).unwrap().clone();
        assert_eq!((s.n, s.total), (3, 3));
        let s = t.line(&cmd("Retry", 1)).unwrap().clone();
        assert_eq!((s.stage.as_str(), s.n, s.total), ("Retry", 1, 2));
        // The same pass again, refilled (a second round): counted afresh.
        let s = t.line(&cmd("Retry", 0)).unwrap().clone();
        assert_eq!((s.n, s.total), (2, 2));
        let s = t.line(&cmd("Retry", 4)).unwrap().clone();
        assert_eq!((s.n, s.total), (1, 5));
        assert!(t.line("not json").is_none());
    }

    #[test]
    fn json_lines() {
        let rom = "afm_113b";
        let boot = Step {
            stage: "cold-boot".into(),
            ..Step::default()
        };
        assert_eq!(
            Line::Step { rom, step: &boot }.to_json(),
            r#"{"rom":"afm_113b","stage":"cold-boot"}"#
        );
        let cmd = Step {
            stage: "VolumeCheck".into(),
            n: 42,
            total: 180,
            id: "0x0042".into(),
            name: "Say \"ack\"\nnow".into(),
        };
        assert_eq!(
            Line::Step { rom, step: &cmd }.to_json(),
            r#"{"rom":"afm_113b","stage":"record","pass":"volume-check","done":42,"total":180,"id":"0x0042","name":"Say \"ack\"\nnow"}"#
        );
        assert_eq!(
            Line::Start {
                rom,
                out: "packs/afm_113b"
            }
            .to_json(),
            r#"{"rom":"afm_113b","stage":"start","out":"packs/afm_113b"}"#
        );
        let done = |status, message, skipped| {
            Line::Done {
                rom,
                status,
                written: 176,
                out: "packs/afm_113b",
                secs: 312.46,
                message,
                skipped,
            }
            .to_json()
        };
        assert_eq!(
            done(Status::Ok, None, false),
            r#"{"rom":"afm_113b","stage":"done","status":"ok","written":176,"out":"packs/afm_113b","secs":312.5}"#
        );
        assert_eq!(
            done(Status::Failed, Some("boom"), false),
            r#"{"rom":"afm_113b","stage":"done","status":"failed","written":176,"out":"packs/afm_113b","secs":312.5,"error":"boom"}"#
        );
        assert_eq!(
            done(Status::NotApplicable, Some("chimes"), true),
            r#"{"rom":"afm_113b","stage":"done","status":"not_applicable","written":176,"out":"packs/afm_113b","secs":312.5,"reason":"chimes","skipped":true}"#
        );
        assert_eq!(
            Line::AllDone {
                ok: 2,
                failed: 1,
                not_applicable: 0
            }
            .to_json(),
            r#"{"stage":"all_done","ok":2,"failed":1,"not_applicable":0}"#
        );
        // One object per line: a newline in a name stays escaped.
        assert!(!Line::Step { rom, step: &cmd }.to_json().contains('\n'));
    }
}
