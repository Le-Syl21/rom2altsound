//! What a front end shows of each ROM zip before an extraction: what `rom2altsound roms`
//! finds in it (the PinMAME set, complete or not), its sound board family, how far
//! rom2altsound gets with that family (docs/board-support.md) and whether VPinball plays
//! the pack (docs/vpx_playback.json). Nothing is computed here that the check, the catalog
//! or the survey do not already say: this module only gathers their answers per zip.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::catalog::{self, Support};
use crate::drivers::{self, Board, Driver};
use crate::romcheck::{self, Index, SetMatch, SetStatus};

/// The families whose packs VPinball does not play today, and why.
const VPX_PLAYBACK: &str = include_str!("../docs/vpx_playback.json");

pub use crate::catalog::Support as SupportLevel;

/// Why VPinball does not play a family's packs, in English and in French.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotPlayed {
    pub en: String,
    pub fr: String,
}

/// The PinMAME set a zip holds, as the check found it.
#[derive(Debug, Clone)]
pub struct SetInfo {
    pub set: String,
    pub description: String,
    pub manufacturer: String,
    pub year: String,
    /// Every file, right CRC.
    pub complete: bool,
    /// A clone holding only its own files, complete with its parent's zip next to it.
    pub split: bool,
    /// Some files have a wrong CRC.
    pub bad_dump: bool,
    /// How many of its files are good, of how many.
    pub good: usize,
    pub required: usize,
    /// Its missing files are in other zips of the folder under other names
    /// (`rom2altsound roms --fix-names` writes it complete).
    pub completable: bool,
    /// The family label of the survey ("DCS95", "S11XS+S11CS", "NONE (Stern SAM)").
    pub family: String,
    pub support: Support,
    /// `Some` when VPinball does not play this family's packs today.
    pub vpx_not_played: Option<NotPlayed>,
}

/// One zip (or folder of ROM files) and what it holds.
#[derive(Debug, Clone)]
pub struct Item {
    pub path: PathBuf,
    /// The set name PinMAME looks it up by (the zip's name without `.zip`).
    pub name: String,
    /// The check's verdict: "ok", "misnamed", "bad-dump", "incomplete", "split",
    /// "not-pinmame", "support" or "error".
    pub status: &'static str,
    /// The check's notes, in its words.
    pub issues: Vec<String>,
    /// The set it holds (or the closest one).
    pub set: Option<SetInfo>,
}

impl Item {
    /// An extraction can start: the zip holds a complete game set under its own name.
    pub fn extractable(&self) -> bool {
        matches!(self.status, "ok" | "split")
            && self
                .set
                .as_ref()
                .is_some_and(|s| (s.complete || s.split) && s.set.eq_ignore_ascii_case(&self.name))
    }
}

/// The PinMAME tables the check needs, read once (the sound boards take a few seconds).
pub struct Checker {
    drivers: Vec<Driver>,
    boards: HashMap<String, Board>,
    support: HashMap<String, catalog::SupportRow>,
    vpx: HashMap<String, NotPlayed>,
}

impl Checker {
    /// Reads PinMAME's driver table; `exe` is this program, whose child processes read the
    /// sound board of every driver (see `drivers::boards`).
    pub fn new(exe: &Path) -> Self {
        let drivers = drivers::load();
        let boards = drivers::boards(exe).unwrap_or_default();
        Self {
            drivers,
            boards,
            support: catalog::parse_support(catalog::BOARD_SUPPORT),
            vpx: vpx_not_played(VPX_PLAYBACK),
        }
    }

    /// Checks zips and folders (a folder: each zip in it, as `rom2altsound roms` does).
    /// The zips of a clone's parent and of its system sets, when they sit next to it, are
    /// read too (so that a split set is seen complete) but not listed.
    pub fn check(&self, paths: &[PathBuf]) -> Vec<Item> {
        let ix = Index::new(&self.drivers, self.boards.clone());
        let (mut units, _) = romcheck::collect_units(paths);
        let listed: BTreeSet<PathBuf> = units.iter().map(|(p, _)| p.clone()).collect();
        // Their parents' and systems' zips, from a first look.
        let first = romcheck::scan(&ix, &units, false);
        let mut extra = BTreeSet::new();
        for (u, (p, _)) in first.iter().zip(&units) {
            let dir = p.parent().unwrap_or(Path::new("."));
            for s in u.sets.iter().filter(|s| s.status == SetStatus::Incomplete) {
                for other in s.parent.iter().chain(&s.systems) {
                    let z = dir.join(format!("{other}.zip"));
                    if z.is_file() && !listed.contains(&z) {
                        extra.insert(z);
                    }
                }
            }
        }
        let scanned = if extra.is_empty() {
            first
        } else {
            units.extend(extra.into_iter().map(|p| (p, true)));
            romcheck::scan(&ix, &units, false)
        };
        scanned
            .into_iter()
            .filter(|u| listed.contains(Path::new(&u.path)))
            .map(|u| {
                let set = best_set(&u.sets, &u.stem).map(|s| self.set_info(s));
                Item {
                    path: PathBuf::from(&u.path),
                    name: u.stem.clone(),
                    status: u.status,
                    issues: match &u.error {
                        Some(e) => vec![e.clone()],
                        None => u.issues.clone(),
                    },
                    set,
                }
            })
            .collect()
    }

    fn set_info(&self, s: &SetMatch) -> SetInfo {
        let driver = self.drivers.iter().find(|d| d.name == s.set);
        let is_sam = crate::sam::sam_set(&s.set).is_some()
            || driver.is_some_and(|d| d.source.ends_with("sam.c"));
        let family = catalog::family_label(s.sound.board.board.as_deref(), is_sam);
        let support = self
            .support
            .get(&family)
            .map_or(Support::Untested, |r| r.support);
        SetInfo {
            set: s.set.clone(),
            description: s.description.clone(),
            manufacturer: s.manufacturer.clone(),
            year: s.year.clone(),
            complete: s.status == SetStatus::Good,
            split: s.status == SetStatus::Split,
            bad_dump: s.status == SetStatus::BadDump,
            good: s.good,
            required: s.required,
            completable: !s.complete_with.is_empty(),
            vpx_not_played: self.vpx.get(&family).cloned(),
            family,
            support,
        }
    }
}

/// The set a unit is about: the one named like it, else the first (the best match).
fn best_set<'a>(sets: &'a [SetMatch], stem: &str) -> Option<&'a SetMatch> {
    sets.iter()
        .find(|s| s.set.eq_ignore_ascii_case(stem))
        .or_else(|| sets.first())
}

/// docs/vpx_playback.json: family label -> why its packs do not play.
fn vpx_not_played(text: &str) -> HashMap<String, NotPlayed> {
    let v: Value = serde_json::from_str(text).unwrap_or_default();
    let mut out = HashMap::new();
    if let Some(fams) = v["families"].as_object() {
        for (label, key) in fams {
            let r = &v["reasons"][key.as_str().unwrap_or_default()];
            out.insert(
                label.clone(),
                NotPlayed {
                    en: r["en"].as_str().unwrap_or_default().to_owned(),
                    fr: r["fr"].as_str().unwrap_or_default().to_owned(),
                },
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpx_playback_reasons_are_read() {
        let m = vpx_not_played(VPX_PLAYBACK);
        assert!(!m.is_empty());
        let sam = &m["NONE (Stern SAM)"];
        assert!(sam.en.contains("SAM") && sam.fr.contains("SAM"));
        assert!(m.values().all(|r| !r.en.is_empty() && !r.fr.is_empty()));
    }
}
