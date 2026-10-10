//! Machine-readable progress of one ROM's extraction, for a front end (the window program).
//!
//! The child process that extracts a ROM writes one JSON object per line to the file named
//! by [`ENV`], when it is set; nothing is written otherwise, and the terminal output never
//! changes. The batch engine reads the file back ([`Tracker`]) while the child runs.

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
}
