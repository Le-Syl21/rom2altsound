//! `rom2altsound roms <dir|zip>...`: identifies ROM zips (and unzipped ROM folders) by
//! their content against the ROM tables of the PinMAME linked in, checks them, and gives
//! every verified game its sound board family and its sound ROM id.
//!
//! A zip's members are matched by CRC32 and size (what the zip's directory lists; `--deep`
//! also decompresses them and checks their SHA-1), never by the zip's or the files' names:
//! a zip named `sfightii.zip` that holds `sfight2` is reported as such. The original files
//! are only read; `--fix-names` writes correctly named copies (or links) elsewhere.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;

use crate::drivers::{self, Board, Driver};
use crate::zipread;

#[derive(Parser)]
#[command(
    name = "rom2altsound roms",
    about = "Identify and check ROM zips against the PinMAME built in",
    long_about = "Identify and check ROM zips against the PinMAME built in.

Every zip (and every folder of unzipped ROMs) found in the given folders, or given
directly, is identified by its content: each file's CRC32 and size are looked up in the
ROM tables of the PinMAME version rom2altsound runs, so a zip is recognised whatever its
name. For each one: the set(s) it holds, and whether they are complete (good), hold a file
with a wrong CRC (a bad dump) or miss files; files under another name; files that belong
to no set (a .vpx, a readme); a zip named after another set; a merged zip (a game and
its clones, at its root or in subfolders); a clone zip that needs its parent's zip (a
split set).

Each verified game gets its sound board (PinMAME's SNDBRD_* type) and its sound ROM id:
the SHA-1 of its sound ROMs' SHA-1s (sorted, one per line), the same for every revision
of a game that kept its sound ROMs.

Nothing is changed in the folders read. --fix-names writes, in another folder, one zip per
complete set, named after the set: a link to the original zip when it is already right,
else a new zip with the set's files under their PinMAME names.",
    after_help = "Examples:
  rom2altsound roms ~/vpinball/roms
  rom2altsound roms ~/vpinball/roms --json roms.json --fix-names ~/roms-fixed
  rom2altsound roms sfightii.zip --deep"
)]
struct RomsCli {
    /// ROM zips, ROM folders, or folders holding them (each *.zip and each subfolder of a
    /// folder is one unit)
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<PathBuf>,
    /// Also write the full report as JSON to this file ("-": standard output, and the text
    /// report is not printed)
    #[arg(long, value_name = "FILE")]
    json: Option<PathBuf>,
    /// Write one correctly named zip per complete set to this folder (links to the zips
    /// that are already right; never inside a folder being checked)
    #[arg(long, value_name = "DIR")]
    fix_names: Option<PathBuf>,
    /// Decompress every matched file and check its SHA-1 too (slower)
    #[arg(long)]
    deep: bool,
    /// Only print the units that are not plain OK
    #[arg(long, short)]
    quiet: bool,
    /// Write PinMAME's whole driver table (every set, its ROMs, sound board and sound ROM
    /// id) as JSON to this file
    #[arg(long, value_name = "FILE")]
    dump_table: Option<PathBuf>,
}

/// Where a member's bytes are.
#[derive(Debug, Clone)]
pub enum Origin {
    Zip(PathBuf, zipread::Entry),
    File(PathBuf),
}

/// One file of a unit (a zip member, or a file of a ROM folder).
#[derive(Debug, Clone)]
pub struct Member {
    /// Its path inside the unit, with `/` separators.
    pub path: String,
    pub size: u64,
    pub crc: u32,
    pub origin: Option<Origin>,
}

impl Member {
    fn base(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
    fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(d, _)| d)
    }
}

/// The ROM tables, indexed by (CRC32, size).
pub struct Index<'a> {
    pub drivers: &'a [Driver],
    by_hash: HashMap<(u32, u64), Vec<usize>>,
    /// ROMs PinMAME knows no dump of (`NO_DUMP`: Stern SAM's colour mods, acd_168hc...),
    /// by lowercase name: the loader takes any file of that name (another length is only a
    /// warning).
    by_no_dump: HashMap<String, Vec<usize>>,
    by_name: HashMap<&'a str, usize>,
    by_sound_id: HashMap<String, Vec<usize>>,
    boards: HashMap<String, Board>,
    /// For each system set, how many sets load from it.
    system_users: HashMap<String, usize>,
}

impl<'a> Index<'a> {
    pub fn new(drivers: &'a [Driver], boards: HashMap<String, Board>) -> Self {
        let mut system_users: HashMap<String, usize> = HashMap::new();
        for d in drivers {
            for s in &d.systems {
                *system_users.entry(s.clone()).or_default() += 1;
            }
        }
        let mut by_hash: HashMap<(u32, u64), Vec<usize>> = HashMap::new();
        let mut by_no_dump: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_sound_id: HashMap<String, Vec<usize>> = HashMap::new();
        for (d, drv) in drivers.iter().enumerate() {
            for (_, r) in drv.required() {
                let v = by_hash.entry((r.crc.unwrap(), r.size)).or_default();
                if v.last() != Some(&d) {
                    v.push(d);
                }
            }
            for r in drv.roms.iter().filter(|r| r.no_dump && !r.optional) {
                let v = by_no_dump.entry(r.name.to_ascii_lowercase()).or_default();
                if v.last() != Some(&d) {
                    v.push(d);
                }
            }
            if let Some(id) = drv.sound_rom_id().filter(|_| !drv.system) {
                by_sound_id.entry(id).or_default().push(d);
            }
        }
        let by_name = drivers
            .iter()
            .enumerate()
            .map(|(i, d)| (d.name.as_str(), i))
            .collect();
        Self {
            drivers,
            by_hash,
            by_no_dump,
            by_name,
            by_sound_id,
            boards,
            system_users,
        }
    }

    fn driver(&self, name: &str) -> Option<&Driver> {
        self.by_name.get(name).map(|&i| &self.drivers[i])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SetStatus {
    /// Every file, with the right CRC.
    Good,
    /// Every file is there, but some with a wrong CRC (or SHA-1).
    BadDump,
    /// Files are missing.
    Incomplete,
    /// A clone zip holding only its own files, complete with its parent's zip.
    Split,
}

#[derive(Debug, Clone, Serialize)]
pub struct WrongFile {
    pub file: String,
    pub member: String,
    pub expected_crc: String,
    pub expected_size: u64,
    pub found_crc: String,
    pub found_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingFile {
    pub file: String,
    pub crc: String,
    pub size: u64,
    pub region: String,
    pub sound: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Renamed {
    pub file: String,
    pub member: String,
}

/// A game's sound board(s), as PinMAME sets them up.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BoardInfo {
    /// The SNDBRD_* names of its sound boards joined by `+` ("SNDBRD_DCS",
    /// "SNDBRD_S11XS+SNDBRD_S11CS", "SNDBRD_NONE"); None when the game's init could not be
    /// read.
    pub board: Option<String>,
    /// Their `SNDBRD_TYPE` values.
    pub board_values: Vec<u32>,
    /// PinMAME's own names of the board interfaces ("WMSDCS").
    pub interfaces: Vec<String>,
    /// `core_gameData->gen`, hex.
    pub generation: Option<String>,
    /// The CPU family's machine init when it picks the board itself ("wpc", "s11"...).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_init: Option<String>,
}

impl BoardInfo {
    fn of(b: Option<&Board>) -> Self {
        let Some(b) = b else {
            return Self::default();
        };
        let boards = b.sound_boards();
        Self {
            board: Some(b.family()),
            interfaces: boards
                .iter()
                .filter_map(|&v| drivers::board_interface(v))
                .collect(),
            board_values: boards,
            generation: Some(format!("0x{:x}", b.generation)),
            core_init: b.core_init.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Sound {
    #[serde(flatten)]
    pub board: BoardInfo,
    pub sound_rom_id: Option<String>,
    pub sound_roms: usize,
    /// Every sound ROM of the set is present with the right CRC.
    pub sound_roms_good: bool,
    /// The other PinMAME sets with the same sound ROM id.
    pub shared_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetMatch {
    pub set: String,
    pub parent: Option<String>,
    pub description: String,
    pub manufacturer: String,
    pub year: String,
    pub status: SetStatus,
    pub good: usize,
    pub required: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wrong: Vec<WrongFile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<MissingFile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub renamed: Vec<Renamed>,
    /// The folders inside the unit its files come from ("" is the root).
    pub folders: Vec<String>,
    /// Its files PinMAME knows no dump of, found by name and size: not verifiable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub no_dump: Vec<String>,
    /// The other zips of the same folder the set's missing files are in: its parent's
    /// (a split set) and the system sets' it loads from (see `systems`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub split_with: Vec<String>,
    /// An incomplete set whose missing files are all in other zips of the same folder that
    /// PinMAME would not look in (MAME's Pinball 2000 base zips `rfmpb`, `swe1pb`: the shared
    /// sound and Prism ROMs of every version, under other names), found by content:
    /// `--fix-names` writes it complete from them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub complete_with: Vec<String>,
    /// A shared system ROM set (gts80s, allied...), not a game.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub system: bool,
    /// For a system set: how many PinMAME sets load from it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_users: Option<usize>,
    /// The system sets PinMAME also looks the set's files up in, nearest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<String>,
    pub sound: Sound,
    /// For each required ROM (table order): the member holding it.
    #[serde(skip)]
    files: Vec<(usize, Option<usize>)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Extra {
    pub member: String,
    pub size: u64,
    pub crc: String,
    /// "rom-of-other-set", "duplicate", "not-a-rom" (by its extension: a table, a text...)
    /// or "unknown" (no PinMAME ROM has its CRC and size).
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Unit {
    pub path: String,
    /// "zip" or "folder".
    pub kind: &'static str,
    /// The name PinMAME looks the set up by (the zip's name without .zip, the folder's name).
    pub stem: String,
    /// "ok", "misnamed", "bad-dump", "incomplete", "split", "not-pinmame", "support",
    /// "error": the worst issue.
    pub status: &'static str,
    pub issues: Vec<String>,
    /// The sets found complete (or, for a bad dump / incomplete unit, the closest ones).
    pub sets: Vec<SetMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also_closest: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extras: Vec<Extra>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub members: Vec<Member>,
}

/// Extensions of files that are no ROM, for the report's wording.
const NOT_ROM_EXT: &[&str] = &[
    "vpx",
    "vpt",
    "directb2s",
    "txt",
    "nfo",
    "diz",
    "ini",
    "cfg",
    "nv",
    "pdf",
    "jpg",
    "jpeg",
    "png",
    "gif",
    "bmp",
    "wav",
    "mp3",
    "ogg",
    "exe",
    "dll",
    "url",
    "htm",
    "html",
    "md",
    "csv",
    "zip",
    "7z",
    "rar",
    "db",
];

fn crc_hex(c: Option<u32>) -> String {
    c.map_or_else(String::new, |c| format!("{c:08x}"))
}

/// Compares the members of a unit with one driver's ROMs.
fn compare(d: &Driver, members: &[Member]) -> SetMatch {
    let mut files = Vec::new();
    let mut wrong = Vec::new();
    let mut missing = Vec::new();
    let mut renamed = Vec::new();
    let mut good = 0;
    let mut required = 0;
    for (ri, r) in d.required() {
        required += 1;
        let crc = r.crc.unwrap();
        let same_name = |m: &Member| m.base().eq_ignore_ascii_case(&r.name);
        let by_hash: Vec<usize> = (0..members.len())
            .filter(|&i| members[i].crc == crc && members[i].size == r.size)
            .collect();
        if let Some(&i) = by_hash
            .iter()
            .find(|&&i| same_name(&members[i]))
            .or(by_hash.first())
        {
            good += 1;
            if !same_name(&members[i]) {
                renamed.push(Renamed {
                    file: r.name.clone(),
                    member: members[i].path.clone(),
                });
            }
            files.push((ri, Some(i)));
        } else if let Some(i) = (0..members.len()).find(|&i| same_name(&members[i])) {
            wrong.push(WrongFile {
                file: r.name.clone(),
                member: members[i].path.clone(),
                expected_crc: crc_hex(r.crc),
                expected_size: r.size,
                found_crc: crc_hex(Some(members[i].crc)),
                found_size: members[i].size,
                detail: None,
            });
            files.push((ri, Some(i)));
        } else {
            missing.push(MissingFile {
                file: r.name.clone(),
                crc: crc_hex(r.crc),
                size: r.size,
                region: r.region.clone(),
                sound: r.sound,
                optional: r.optional,
            });
            files.push((ri, None));
        }
    }
    // Files with no known dump: taken by name, as PinMAME's loader does (a missing one, or
    // another length, is only a warning there).
    let mut no_dump = Vec::new();
    for (ri, r) in d.roms.iter().enumerate().filter(|(_, r)| r.no_dump) {
        if let Some(i) =
            (0..members.len()).find(|&i| members[i].base().eq_ignore_ascii_case(&r.name))
        {
            no_dump.push(if members[i].size == r.size {
                r.name.clone()
            } else {
                format!(
                    "{} ({} bytes, PinMAME's table says {})",
                    r.name, members[i].size, r.size
                )
            });
            files.push((ri, Some(i)));
        }
    }
    let status = if missing.iter().any(|m| !m.optional) {
        SetStatus::Incomplete
    } else if !wrong.is_empty() {
        SetStatus::BadDump
    } else {
        SetStatus::Good
    };
    let mut folders: Vec<String> = files
        .iter()
        .filter_map(|(_, m)| m.map(|i| members[i].folder().to_owned()))
        .collect();
    folders.sort();
    folders.dedup();
    SetMatch {
        set: d.name.clone(),
        parent: d.parent.clone(),
        description: d.description.clone(),
        manufacturer: d.manufacturer.clone(),
        year: d.year.clone(),
        status,
        good,
        required,
        wrong,
        missing,
        renamed,
        folders,
        no_dump,
        split_with: Vec::new(),
        complete_with: Vec::new(),
        system: d.system,
        system_users: None,
        systems: d.systems.clone(),
        sound: Sound {
            board: BoardInfo::default(),
            sound_rom_id: None,
            sound_roms: 0,
            sound_roms_good: false,
            shared_with: Vec::new(),
        },
        files,
    }
}

impl SetMatch {
    /// The members this set uses (good and wrong-CRC files).
    fn used(&self) -> HashSet<usize> {
        self.files.iter().filter_map(|(_, m)| *m).collect()
    }

    fn fill_sound(&mut self, ix: &Index, members: &[Member]) {
        let Some(d) = ix.driver(&self.set) else {
            return;
        };
        let b = ix.boards.get(&d.name);
        let id = d.sound_rom_id();
        let good_members: HashSet<usize> = self
            .files
            .iter()
            .filter_map(|(ri, m)| {
                let m = (*m)?;
                let r = &d.roms[*ri];
                (members[m].crc == r.crc? && members[m].size == r.size).then_some(*ri)
            })
            .collect();
        let sound_roms: Vec<usize> = d
            .required()
            .filter(|(_, r)| r.sound)
            .map(|(i, _)| i)
            .collect();
        let shared_with = id
            .as_ref()
            .and_then(|id| ix.by_sound_id.get(id))
            .map(|v| {
                v.iter()
                    .map(|&i| ix.drivers[i].name.clone())
                    .filter(|n| *n != d.name)
                    .collect()
            })
            .unwrap_or_default();
        self.sound = Sound {
            board: BoardInfo::of(b),
            sound_rom_id: id,
            sound_roms_good: sound_roms.iter().all(|i| good_members.contains(i)),
            sound_roms: sound_roms.len(),
            shared_with,
        };
    }
}

fn stem_of(path: &Path) -> String {
    let n = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match n.rsplit_once('.') {
        Some((s, e)) if e.eq_ignore_ascii_case("zip") => s.to_owned(),
        _ => n,
    }
}

/// Identifies one unit from its members (the verifier's core; no I/O).
pub fn identify(
    ix: &Index,
    path: &str,
    kind: &'static str,
    stem: &str,
    members: Vec<Member>,
) -> Unit {
    let mut unit = Unit {
        path: path.to_owned(),
        kind,
        stem: stem.to_owned(),
        status: "ok",
        issues: Vec::new(),
        sets: Vec::new(),
        also_closest: Vec::new(),
        extras: Vec::new(),
        error: None,
        members,
    };
    let members = &unit.members;
    let mut candidates: Vec<usize> = members
        .iter()
        .filter_map(|m| ix.by_hash.get(&(m.crc, m.size)))
        .flatten()
        .copied()
        .collect();
    candidates.extend(
        members
            .iter()
            .filter_map(|m| ix.by_no_dump.get(&m.base().to_ascii_lowercase()))
            .flatten()
            .copied(),
    );
    candidates.sort_unstable();
    candidates.dedup();
    let compared: Vec<SetMatch> = candidates
        .iter()
        .map(|&d| compare(&ix.drivers[d], members))
        .collect();
    let complete_systems: Vec<&SetMatch> = compared
        .iter()
        .filter(|s| s.status == SetStatus::Good && s.system)
        .collect();
    let mut complete: Vec<&SetMatch> = compared
        .iter()
        .filter(|s| s.status == SetStatus::Good && !s.system)
        .collect();
    // A system set's zip (gts80s.zip): the shared ROMs of a generation, not a game. It is
    // one when named after a complete system set, or when no game is complete in it. Games
    // whose files are all in it (Allied's, which have no ROM of their own) are noted.
    let support_named: Vec<&SetMatch> = complete_systems
        .iter()
        .copied()
        .filter(|s| s.set.eq_ignore_ascii_case(stem))
        .collect();
    let mut support_games: Vec<String> = Vec::new();
    if !support_named.is_empty() || (complete.is_empty() && !complete_systems.is_empty()) {
        support_games = complete.iter().map(|s| s.set.clone()).collect();
        complete = if support_named.is_empty() {
            complete_systems
        } else {
            support_named
        };
    }
    let mut chosen: Vec<SetMatch> = if !complete.is_empty() {
        // Leave out a set whose files are all part of another complete set's (a set with
        // fewer files that the zip holds as a side effect).
        let used: Vec<HashSet<usize>> = complete.iter().map(|s| s.used()).collect();
        complete
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                !used
                    .iter()
                    .enumerate()
                    .any(|(j, u)| j != *i && used[*i].is_subset(u) && used[*i].len() < u.len())
            })
            .map(|(_, s)| (*s).clone())
            .collect()
    } else {
        // The closest sets: the most files found, then the fewest wrong or missing.
        let score = |s: &SetMatch| {
            let bad = s.wrong.len() + s.missing.iter().filter(|m| !m.optional).count();
            (s.good + s.wrong.len(), usize::MAX - bad)
        };
        let best = compared.iter().map(score).max();
        let mut best: Vec<SetMatch> = compared
            .iter()
            .filter(|s| Some(score(s)) == best)
            .cloned()
            .collect();
        // The one named like the unit first, then parents, then by name.
        best.sort_by_key(|s| {
            (
                !s.set.eq_ignore_ascii_case(stem),
                s.parent.is_some(),
                s.set.clone(),
            )
        });
        if best.len() > 1 {
            unit.also_closest = best[1..].iter().map(|s| s.set.clone()).collect();
            best.truncate(1);
        }
        best
    };
    chosen.sort_by_key(|s| (s.parent.is_some(), s.set.clone()));
    for s in &mut chosen {
        s.fill_sound(ix, members);
        if s.system {
            s.system_users = Some(ix.system_users.get(&s.set).copied().unwrap_or(0));
        }
    }

    // Extras: members no chosen set uses.
    let used: HashSet<usize> = chosen.iter().flat_map(|s| s.used()).collect();
    let used_hashes: HashSet<(u32, u64)> = used
        .iter()
        .map(|&i| (members[i].crc, members[i].size))
        .collect();
    for (i, m) in members.iter().enumerate() {
        if used.contains(&i) {
            continue;
        }
        let sets: Vec<String> = ix
            .by_hash
            .get(&(m.crc, m.size))
            .map(|v| {
                v.iter()
                    .take(6)
                    .map(|&d| ix.drivers[d].name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let kind = if used_hashes.contains(&(m.crc, m.size)) {
            "duplicate"
        } else if !sets.is_empty() {
            "rom-of-other-set"
        } else if m
            .base()
            .rsplit_once('.')
            .is_some_and(|(_, e)| NOT_ROM_EXT.iter().any(|x| e.eq_ignore_ascii_case(x)))
        {
            "not-a-rom"
        } else {
            "unknown"
        };
        unit.extras.push(Extra {
            member: m.path.clone(),
            size: m.size,
            crc: crc_hex(Some(m.crc)),
            kind,
            sets,
        });
    }
    unit.sets = chosen;
    unit.classify();
    if unit.status == "support" && !support_games.is_empty() {
        support_games.sort();
        unit.issues.push(format!(
            "also every file of {} game(s) with no ROM of their own: {}",
            support_games.len(),
            support_games.join(", ")
        ));
    }
    unit
}

impl Unit {
    /// Sets `issues` and `status` from what was found.
    fn classify(&mut self) {
        let mut issues = Vec::new();
        let bsmt = self
            .members
            .iter()
            .any(|m| m.crc == crate::bsmtfw::CRC && m.size == 8192);
        if self.sets.is_empty() {
            if bsmt {
                self.status = "support";
                self.issues =
                    vec!["BSMT2000 program (bsmt2000.bin) for rom2altsound, not a game".into()];
            } else {
                self.status = "not-pinmame";
                self.issues = vec!["no file of any PinMAME set".into()];
            }
            return;
        }
        if self
            .sets
            .iter()
            .all(|s| s.system && s.status == SetStatus::Good)
        {
            self.status = "support";
            let s = &self.sets[0];
            self.issues = vec![format!(
                "system ROMs, not a game: {} ({} {}), loaded by {} PinMAME set(s)",
                s.description,
                s.manufacturer,
                s.year,
                s.system_users.unwrap_or(0)
            )];
            if !self
                .sets
                .iter()
                .any(|s| s.set.eq_ignore_ascii_case(&self.stem))
            {
                self.issues.push(format!(
                    "misnamed: named {}, holds {}",
                    self.stem, self.sets[0].set
                ));
            }
            return;
        }
        let complete: Vec<&SetMatch> = self
            .sets
            .iter()
            .filter(|s| matches!(s.status, SetStatus::Good | SetStatus::Split))
            .collect();
        let named = self
            .sets
            .iter()
            .any(|s| s.set.eq_ignore_ascii_case(&self.stem));
        if self.sets.len() > 1 {
            let parents: HashSet<&str> = self
                .sets
                .iter()
                .map(|s| s.parent.as_deref().unwrap_or(&s.set))
                .collect();
            issues.push(format!(
                "merged: holds {} sets ({})",
                self.sets.len(),
                self.sets
                    .iter()
                    .map(|s| s.set.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            if !named && !(parents.len() == 1 && parents.contains(self.stem.as_str())) {
                issues.push(format!(
                    "misnamed: named {}, holds {}",
                    self.stem, self.sets[0].set
                ));
            }
        } else if !named {
            issues.push(format!(
                "misnamed: named {}, holds {}",
                self.stem, self.sets[0].set
            ));
        }
        for s in &self.sets {
            if !s.no_dump.is_empty() {
                issues.push(format!(
                    "not verifiable: PinMAME knows no dump of {} (NO_DUMP), taken by name",
                    s.no_dump.join(", ")
                ));
            }
            if s.folders.iter().any(|f| !f.is_empty()) {
                issues.push(format!(
                    "subfolder: {} is in {}/",
                    s.set,
                    s.folders
                        .iter()
                        .filter(|f| !f.is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("/, ")
                ));
            }
            if !s.renamed.is_empty() {
                issues.push(format!(
                    "files under another name: {}",
                    s.renamed
                        .iter()
                        .map(|r| format!("{} (PinMAME: {})", r.member, r.file))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        let not_rom: Vec<&Extra> = self
            .extras
            .iter()
            .filter(|e| e.kind == "not-a-rom")
            .collect();
        if !not_rom.is_empty() {
            issues.push(format!(
                "not ROM content: {}",
                not_rom
                    .iter()
                    .map(|e| e.member.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let unknown: Vec<&Extra> = self.extras.iter().filter(|e| e.kind == "unknown").collect();
        if !unknown.is_empty() {
            issues.push(format!(
                "unknown files (no PinMAME ROM has their CRC): {}",
                unknown
                    .iter()
                    .map(|e| e.member.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let other: Vec<&Extra> = self
            .extras
            .iter()
            .filter(|e| matches!(e.kind, "rom-of-other-set" | "duplicate"))
            .collect();
        if !other.is_empty() {
            issues.push(format!(
                "extra ROM files: {}",
                other
                    .iter()
                    .map(|e| match e.sets.first() {
                        Some(s) => format!("{} ({s})", e.member),
                        None => format!("{} ({})", e.member, e.kind),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        self.status = if !complete.is_empty() {
            if complete.iter().any(|s| s.status == SetStatus::Split) {
                "split"
            } else if issues.iter().any(|i| i.starts_with("misnamed")) {
                "misnamed"
            } else {
                "ok"
            }
        } else if self.sets.iter().all(|s| s.status == SetStatus::BadDump) {
            "bad-dump"
        } else {
            "incomplete"
        };
        self.issues = issues;
    }
}

/// Lists the members of a zip (directories left out).
fn zip_members(path: &Path) -> Result<Vec<Member>, String> {
    Ok(zipread::list(path)?
        .into_iter()
        .filter(|e| !e.name.ends_with('/'))
        .map(|e| Member {
            path: e.name.replace('\\', "/"),
            size: e.size,
            crc: e.crc32,
            origin: Some(Origin::Zip(path.to_path_buf(), e)),
        })
        .collect())
}

/// Lists the files of a ROM folder (recursively), reading each one for its CRC32.
fn folder_members(dir: &Path) -> Result<Vec<Member>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&d)
            .map_err(|e| format!("{}: {e}", d.display()))?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file() {
                let data = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                let rel = p.strip_prefix(dir).unwrap_or(&p);
                out.push(Member {
                    path: rel.to_string_lossy().replace('\\', "/"),
                    size: data.len() as u64,
                    crc: zipread::crc32(&data),
                    origin: Some(Origin::File(p.clone())),
                });
            }
        }
    }
    Ok(out)
}

fn member_bytes(m: &Member) -> Result<Vec<u8>, String> {
    match &m.origin {
        Some(Origin::Zip(p, e)) => zipread::read(p, e),
        Some(Origin::File(p)) => std::fs::read(p).map_err(|e| format!("{}: {e}", p.display())),
        None => Err(format!("{}: no data", m.path)),
    }
}

/// `--deep`: decompresses the files of the sets found and checks their SHA-1 (and, through
/// the decompression, that the stored CRC is the data's).
fn deep_check(ix: &Index, unit: &mut Unit) {
    for s in &mut unit.sets {
        let Some(d) = ix.driver(&s.set) else { continue };
        let mut bad = Vec::new();
        for (ri, m) in &s.files {
            let Some(m) = *m else { continue };
            let r = &d.roms[*ri];
            let mem = &unit.members[m];
            if r.crc != Some(mem.crc) || r.size != mem.size {
                continue; // already a wrong file
            }
            let detail = match member_bytes(mem) {
                Err(e) => Some(format!("unreadable: {e}")),
                Ok(data) => match &r.sha1 {
                    Some(want) if crate::sha1::hex(&data) != *want => Some(format!(
                        "CRC matches but SHA-1 is {}",
                        crate::sha1::hex(&data)
                    )),
                    _ => None,
                },
            };
            if let Some(detail) = detail {
                bad.push(WrongFile {
                    file: r.name.clone(),
                    member: mem.path.clone(),
                    expected_crc: crc_hex(r.crc),
                    expected_size: r.size,
                    found_crc: crc_hex(Some(mem.crc)),
                    found_size: mem.size,
                    detail: Some(detail),
                });
            }
        }
        if !bad.is_empty() {
            s.good -= bad.len();
            s.wrong.extend(bad);
            if s.status == SetStatus::Good {
                s.status = SetStatus::BadDump;
            }
            s.fill_sound(ix, &unit.members);
        }
    }
    unit.classify();
}

/// The units of the command line: each zip, each folder of ROM files, and in a folder each
/// *.zip and each subfolder.
fn collect_units(paths: &[PathBuf]) -> (Vec<(PathBuf, bool)>, Vec<String>) {
    let mut units = Vec::new();
    let mut ignored = Vec::new();
    for p in paths {
        let is_zip =
            |p: &Path| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        if is_zip(p) {
            units.push((p.clone(), true));
        } else if p.is_dir() {
            let mut entries: Vec<PathBuf> = match std::fs::read_dir(p) {
                Ok(r) => r.filter_map(|e| e.ok().map(|e| e.path())).collect(),
                Err(e) => {
                    ignored.push(format!("{}: {e}", p.display()));
                    continue;
                }
            };
            entries.sort();
            let has_zip = entries.iter().any(|e| is_zip(e));
            let has_dir = entries.iter().any(|e| e.is_dir());
            if !has_zip && !has_dir {
                // A folder of loose ROM files: one unit.
                units.push((p.clone(), false));
                continue;
            }
            for e in entries {
                if is_zip(&e) {
                    units.push((e, true));
                } else if e.is_dir() {
                    units.push((e, false));
                } else {
                    ignored.push(e.display().to_string());
                }
            }
        } else {
            ignored.push(format!("{}: not a zip or a folder", p.display()));
        }
    }
    (units, ignored)
}

/// Sets that only hold their own files, complete with the zips PinMAME's ROM loader also
/// opens, found among the units of the same folder: their parent's (a split clone) and
/// their system sets' (gts80s.zip: the ROMs a generation's games share).
fn find_split_sets(units: &mut [Unit]) {
    for u in 0..units.len() {
        if units[u].sets.len() != 1 || units[u].sets[0].status != SetStatus::Incomplete {
            continue;
        }
        let s = &units[u].sets[0];
        if !s.wrong.is_empty() || (s.parent.is_none() && s.systems.is_empty()) {
            continue;
        }
        let chain: Vec<String> = s.parent.iter().chain(&s.systems).cloned().collect();
        let dir = Path::new(&units[u].path).parent().map(Path::to_path_buf);
        let mut need: Vec<(u32, u64)> = s
            .missing
            .iter()
            .filter(|m| !m.optional)
            .filter_map(|m| u32::from_str_radix(&m.crc, 16).ok().map(|c| (c, m.size)))
            .collect();
        // Like the loader: each name of the chain in turn, the files still missing.
        let mut with: Vec<(usize, bool)> = Vec::new();
        for (k, name) in chain.iter().enumerate() {
            if need.is_empty() {
                break;
            }
            let Some(v) = (0..units.len()).find(|&v| {
                v != u
                    && Path::new(&units[v].path).parent().map(Path::to_path_buf) == dir
                    && (units[v].stem.eq_ignore_ascii_case(name)
                        || units[v].sets.iter().any(|s| s.set == *name))
            }) else {
                continue;
            };
            let before = need.len();
            need.retain(|h| !units[v].members.iter().any(|m| (m.crc, m.size) == *h));
            if need.len() < before {
                with.push((v, k == 0 && s.parent.is_some()));
            }
        }
        if !need.is_empty() {
            // Not loadable as is: the rest may still be in some other zip of the folder.
            let mut donors: Vec<usize> = Vec::new();
            for (v, other) in units.iter().enumerate() {
                if need.is_empty() {
                    break;
                }
                if v == u
                    || with.iter().any(|&(w, _)| w == v)
                    || Path::new(&other.path).parent().map(Path::to_path_buf) != dir
                {
                    continue;
                }
                let before = need.len();
                need.retain(|h| !other.members.iter().any(|m| (m.crc, m.size) == *h));
                if need.len() < before {
                    donors.push(v);
                }
            }
            if need.is_empty() {
                let paths: Vec<String> = with
                    .iter()
                    .map(|&(v, _)| v)
                    .chain(donors.iter().copied())
                    .map(|v| units[v].path.clone())
                    .collect();
                let donor_paths: Vec<String> =
                    donors.iter().map(|&v| units[v].path.clone()).collect();
                let s = &mut units[u].sets[0];
                s.complete_with = paths;
                let set = s.set.clone();
                units[u].issues.push(format!(
                    "completable: {set}'s missing files are all in {} (found by content, under other names; PinMAME does not look there): --fix-names writes the complete set",
                    donor_paths.join(", ")
                ));
            }
            continue;
        }
        if with.is_empty() {
            continue;
        }
        let paths: Vec<String> = with.iter().map(|&(v, _)| units[v].path.clone()).collect();
        let split = with.iter().any(|&(_, parent)| parent);
        let s = &mut units[u].sets[0];
        // A clone needs its parent's zip: split. A game whose only missing files are its
        // system set's is complete as PinMAME loads it.
        s.status = if split {
            SetStatus::Split
        } else {
            SetStatus::Good
        };
        s.split_with = paths.clone();
        // Every missing file is in those zips, with the right CRC.
        s.sound.sound_roms_good = s.sound.sound_rom_id.is_some();
        let set = s.set.clone();
        units[u].classify();
        for (&(_, parent), p) in with.iter().zip(&paths) {
            units[u].issues.push(if parent {
                format!("split set: {set} needs its parent's zip {p}")
            } else {
                format!("system ROMs: {set} loads its shared ROMs from {p}")
            });
        }
    }
}

/// `--fix-names`: one zip per complete set in `out`.
fn fix_names(ix: &Index, units: &[Unit], out: &Path) -> Vec<String> {
    let mut log = Vec::new();
    if let Err(e) = std::fs::create_dir_all(out) {
        return vec![format!("error: {}: {e}", out.display())];
    }
    let mut written: HashSet<String> = HashSet::new();
    for u in units {
        for s in &u.sets {
            if !matches!(s.status, SetStatus::Good | SetStatus::Split) && s.complete_with.is_empty()
            {
                continue;
            }
            let Some(d) = ix.driver(&s.set) else { continue };
            let dst = out.join(format!("{}.zip", s.set));
            if written.contains(&s.set) || dst.exists() {
                log.push(format!(
                    "{}: {} already written, {} skipped",
                    s.set,
                    dst.display(),
                    u.path
                ));
                continue;
            }
            let as_is = u.kind == "zip"
                && u.sets.len() == 1
                && s.status == SetStatus::Good
                && s.split_with.is_empty()
                && s.renamed.is_empty()
                && u.extras.is_empty()
                && s.folders.iter().all(|f| f.is_empty());
            let r = if as_is {
                link_or_copy(Path::new(&u.path), &dst).map(|how| format!("{how} {}", u.path))
            } else {
                write_set(ix, d, s, u, units, &dst).map(|n| format!("new zip, {n} file(s)"))
            };
            match r {
                Ok(how) => {
                    written.insert(s.set.clone());
                    log.push(format!("{}: {} ({how})", s.set, dst.display()));
                }
                Err(e) => log.push(format!("{}: error: {e}", s.set)),
            }
        }
    }
    log
}

fn link_or_copy(src: &Path, dst: &Path) -> Result<&'static str, String> {
    let src = std::fs::canonicalize(src).map_err(|e| format!("{}: {e}", src.display()))?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&src, dst)
            .map(|_| "link to")
            .map_err(|e| format!("{}: {e}", dst.display()))
    }
    #[cfg(not(unix))]
    {
        std::fs::copy(&src, dst)
            .map(|_| "copy of")
            .map_err(|e| format!("{}: {e}", dst.display()))
    }
}

/// Writes a set's files under their PinMAME names, copied as stored when from a zip.
fn write_set(
    ix: &Index,
    d: &Driver,
    s: &SetMatch,
    u: &Unit,
    units: &[Unit],
    dst: &Path,
) -> Result<usize, String> {
    let parent_members: Vec<&Member> = s
        .split_with
        .iter()
        .chain(&s.complete_with)
        .filter_map(|p| units.iter().find(|v| v.path == *p))
        .flat_map(|v| v.members.iter())
        .collect();
    let _ = ix;
    let mut w = zipread::Writer::default();
    let mut n = 0;
    let mut names = HashSet::new();
    for (ri, r) in d.required() {
        if !names.insert(r.name.to_ascii_lowercase()) {
            continue; // the same file loaded twice
        }
        let want = (r.crc.unwrap(), r.size);
        let m = s
            .files
            .iter()
            .find(|(i, _)| *i == ri)
            .and_then(|(_, m)| m.map(|m| &u.members[m]))
            .filter(|m| (m.crc, m.size) == want)
            .or_else(|| {
                parent_members
                    .iter()
                    .copied()
                    .find(|m| (m.crc, m.size) == want)
            });
        let Some(m) = m else {
            if r.optional {
                continue;
            }
            return Err(format!("{} not found", r.name));
        };
        match &m.origin {
            Some(Origin::Zip(p, e)) if matches!(e.method, 0 | 8) => {
                let raw = zipread::read_raw(p, e)?;
                w.add_raw(&r.name, e.method, e.crc32, e.size, &raw);
            }
            _ => w.add(&r.name, &member_bytes(m)?),
        }
        n += 1;
    }
    // The files with no known dump, found by name.
    for (ri, m) in &s.files {
        let (true, Some(m)) = (d.roms[*ri].no_dump, m) else {
            continue;
        };
        let r = &d.roms[*ri];
        if !names.insert(r.name.to_ascii_lowercase()) {
            continue;
        }
        let m = &u.members[*m];
        match &m.origin {
            Some(Origin::Zip(p, e)) if matches!(e.method, 0 | 8) => {
                let raw = zipread::read_raw(p, e)?;
                w.add_raw(&r.name, e.method, e.crc32, e.size, &raw);
            }
            _ => w.add(&r.name, &member_bytes(m)?),
        }
        n += 1;
    }
    std::fs::write(dst, w.finish()).map_err(|e| format!("{}: {e}", dst.display()))?;
    Ok(n)
}

/// One line per unit, and the problems under it.
fn print_unit(u: &Unit, quiet: bool) {
    if quiet && u.status == "ok" && u.issues.is_empty() {
        return;
    }
    let name = Path::new(&u.path)
        .file_name()
        .map_or_else(|| u.path.clone(), |n| n.to_string_lossy().into_owned());
    let head = match u.status {
        "ok" => "OK",
        "misnamed" => "MISNAMED",
        "bad-dump" => "BAD DUMP",
        "incomplete" => "INCOMPLETE",
        "split" => "SPLIT",
        "not-pinmame" => "NOT PINMAME",
        "support" => "SUPPORT",
        _ => "ERROR",
    };
    if let Some(e) = &u.error {
        println!("{name}: ERROR {e}");
        return;
    }
    let sets = u
        .sets
        .iter()
        .map(|s| {
            let mut t = format!(
                "{} ({}, {} {})",
                s.set, s.description, s.manufacturer, s.year
            );
            if s.status != SetStatus::Good {
                t.push_str(&format!(" [{}/{} files]", s.good, s.required));
            }
            t
        })
        .collect::<Vec<_>>()
        .join("; ");
    println!("{name}: {head} {sets}");
    if !u.also_closest.is_empty() {
        println!(
            "    as close: {}{}",
            u.also_closest
                .iter()
                .take(8)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if u.also_closest.len() > 8 {
                format!(" and {} more", u.also_closest.len() - 8)
            } else {
                String::new()
            }
        );
    }
    for s in &u.sets {
        for w in &s.wrong {
            println!(
                "    wrong file: {} ({}) is {} bytes CRC {}, expected {} bytes CRC {}{}",
                w.file,
                w.member,
                w.found_size,
                w.found_crc,
                w.expected_size,
                w.expected_crc,
                w.detail
                    .as_ref()
                    .map_or_else(String::new, |d| format!(": {d}"))
            );
        }
        for m in &s.missing {
            println!(
                "    missing: {} ({} bytes CRC {}, {}{}{})",
                m.file,
                m.size,
                m.crc,
                m.region,
                if m.sound { ", sound" } else { "" },
                if m.optional { ", optional" } else { "" }
            );
        }
    }
    // One line per distinct sound ROM id (the sets of a merged zip usually share theirs).
    let mut seen: Vec<Option<&str>> = Vec::new();
    for s in &u.sets {
        let id = s.sound.sound_rom_id.as_deref();
        if s.system || seen.contains(&id) || (s.sound.board.board.is_none() && id.is_none()) {
            continue;
        }
        seen.push(id);
        let here: Vec<&str> = u
            .sets
            .iter()
            .filter(|t| t.sound.sound_rom_id.as_deref() == id)
            .map(|t| t.set.as_str())
            .collect();
        let others = s
            .sound
            .shared_with
            .iter()
            .filter(|n| !here.contains(&n.as_str()))
            .count();
        println!(
            "    sound: {}{}, sound ROM id {}{}{}",
            s.sound.board.board.as_deref().unwrap_or("board unknown"),
            if s.sound.board.interfaces.is_empty() {
                String::new()
            } else {
                format!(" ({})", s.sound.board.interfaces.join("+"))
            },
            id.map_or("none (no sound ROM)", |i| &i[..12]),
            if here.len() > 1 && u.sets.len() > 1 {
                format!(" for {}", here.join(", "))
            } else {
                String::new()
            },
            if others > 0 {
                format!(", also used by {others} other PinMAME set(s)")
            } else {
                String::new()
            },
        );
        if id.is_some() && !s.sound.sound_roms_good {
            println!("    sound ROMs not all good: no sound ROM id for this zip");
        }
    }
    for i in &u.issues {
        println!("    {i}");
    }
}

#[derive(Serialize)]
struct Report<'a> {
    pinmame_sets: usize,
    units: &'a [Unit],
    ignored: &'a [String],
    summary: BTreeMap<&'static str, usize>,
    /// Verified sets (sound ROMs all good), by sound ROM id.
    sound_groups: Vec<SoundGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fix_names: Vec<String>,
}

#[derive(Serialize)]
struct SoundGroup {
    sound_rom_id: String,
    board: Option<String>,
    sets_found: Vec<String>,
    pinmame_sets: Vec<String>,
}

#[derive(Serialize)]
struct TableRow<'a> {
    #[serde(flatten)]
    driver: &'a Driver,
    #[serde(flatten)]
    board: BoardInfo,
    sound_rom_id: Option<String>,
}

/// Entry point of `rom2altsound roms`; returns the exit code.
pub fn cli(args: Vec<String>) -> i32 {
    let cli = RomsCli::parse_from(std::iter::once("rom2altsound roms".to_owned()).chain(args));
    let to_stdout = cli.json.as_deref() == Some(Path::new("-"));
    let table = drivers::load();
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot find this program's path: {e}");
            return 1;
        }
    };
    let boards = match drivers::boards(&exe) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("warning: sound boards unknown: {e}");
            HashMap::new()
        }
    };
    let ix = Index::new(&table, boards);
    if let Some(p) = &cli.dump_table {
        let rows: Vec<TableRow> = table
            .iter()
            .map(|d| TableRow {
                driver: d,
                board: BoardInfo::of(ix.boards.get(&d.name)),
                sound_rom_id: d.sound_rom_id(),
            })
            .collect();
        let text = serde_json::to_string_pretty(&rows).unwrap();
        if let Err(e) = std::fs::write(p, text) {
            eprintln!("error: {}: {e}", p.display());
            return 1;
        }
        eprintln!("driver table ({} sets): {}", rows.len(), p.display());
    }
    let (paths, ignored) = collect_units(&cli.paths);
    if let Some(out) = &cli.fix_names {
        let out_abs = std::fs::canonicalize(out).ok();
        let scanned: Vec<PathBuf> = cli
            .paths
            .iter()
            .filter(|p| p.is_dir())
            .filter_map(|p| std::fs::canonicalize(p).ok())
            .collect();
        if out_abs.is_some_and(|o| scanned.contains(&o)) {
            eprintln!("error: --fix-names must not be a folder being checked");
            return 2;
        }
    }
    let mut units: Vec<Unit> = Vec::new();
    for (p, is_zip) in &paths {
        let members = if *is_zip {
            zip_members(p)
        } else {
            folder_members(p)
        };
        let path = p.display().to_string();
        let stem = stem_of(p);
        let kind = if *is_zip { "zip" } else { "folder" };
        let mut u = match members {
            Ok(m) => identify(&ix, &path, kind, &stem, m),
            Err(e) => Unit {
                path,
                kind,
                stem,
                status: "error",
                issues: Vec::new(),
                sets: Vec::new(),
                also_closest: Vec::new(),
                extras: Vec::new(),
                error: Some(e),
                members: Vec::new(),
            },
        };
        if cli.deep && u.error.is_none() {
            deep_check(&ix, &mut u);
        }
        units.push(u);
    }
    find_split_sets(&mut units);

    let mut summary: BTreeMap<&'static str, usize> = BTreeMap::new();
    for u in &units {
        *summary.entry(u.status).or_default() += 1;
    }
    let mut groups: BTreeMap<String, SoundGroup> = BTreeMap::new();
    for u in &units {
        for s in &u.sets {
            let (Some(id), true) = (&s.sound.sound_rom_id, s.sound.sound_roms_good) else {
                continue;
            };
            if s.system {
                continue;
            }
            let g = groups.entry(id.clone()).or_insert_with(|| SoundGroup {
                sound_rom_id: id.clone(),
                board: s.sound.board.board.clone(),
                sets_found: Vec::new(),
                pinmame_sets: ix
                    .by_sound_id
                    .get(id)
                    .map(|v| v.iter().map(|&i| table[i].name.clone()).collect())
                    .unwrap_or_default(),
            });
            if !g.sets_found.contains(&s.set) {
                g.sets_found.push(s.set.clone());
            }
        }
    }
    let fixes = cli
        .fix_names
        .as_ref()
        .map(|o| fix_names(&ix, &units, o))
        .unwrap_or_default();

    if !to_stdout {
        for u in &units {
            print_unit(u, cli.quiet);
        }
        for i in &ignored {
            println!("ignored: {i}");
        }
        println!();
        let zips = units.iter().filter(|u| u.kind == "zip").count();
        println!(
            "{} unit(s) ({zips} zip(s), {} folder(s)), PinMAME has {} sets ({} of them \
             system ROM sets): {}",
            units.len(),
            units.len() - zips,
            table.len(),
            table.iter().filter(|d| d.system).count(),
            summary
                .iter()
                .map(|(k, v)| format!("{v} {k}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut fam: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for u in &units {
            for s in u
                .sets
                .iter()
                .filter(|s| s.sound.sound_roms_good && !s.system)
            {
                fam.entry(
                    s.sound
                        .board
                        .board
                        .clone()
                        .unwrap_or_else(|| "unknown".into()),
                )
                .or_default()
                .push(s.set.clone());
            }
        }
        println!("verified sound ROMs, by board:");
        for (b, sets) in &fam {
            println!("  {b}: {} set(s): {}", sets.len(), sets.join(", "));
        }
        println!("{} distinct sound ROM id(s) among them", groups.len());
        for f in &fixes {
            println!("fix-names: {f}");
        }
    }
    if let Some(p) = &cli.json {
        let report = Report {
            pinmame_sets: table.len(),
            units: &units,
            ignored: &ignored,
            summary,
            sound_groups: groups.into_values().collect(),
            fix_names: fixes,
        };
        let text = serde_json::to_string_pretty(&report).unwrap();
        if to_stdout {
            println!("{text}");
        } else if let Err(e) = std::fs::write(p, text) {
            eprintln!("error: {}: {e}", p.display());
            return 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::RomFile;

    fn rom(name: &str, data: &[u8], region: &str, sound: bool) -> RomFile {
        RomFile {
            name: name.into(),
            size: data.len() as u64,
            crc: Some(zipread::crc32(data)),
            sha1: Some(crate::sha1::hex(data)),
            region: region.into(),
            sound,
            optional: false,
            no_dump: false,
            bad_dump: false,
        }
    }

    fn driver(name: &str, parent: Option<&str>, roms: Vec<RomFile>) -> Driver {
        Driver {
            name: name.into(),
            parent: parent.map(Into::into),
            description: format!("{name} game"),
            year: "1990".into(),
            manufacturer: "Test".into(),
            source: "wpc/test.c".into(),
            roms,
            cpus: vec![],
            sound_chips: vec![],
            system: false,
            systems: vec![],
        }
    }

    /// A tiny table: game `tg_l1` (CPU + 2 sound ROMs), its clone `tg_l2` (another CPU
    /// ROM, the same sound ROMs) and an unrelated game.
    fn table() -> Vec<Driver> {
        vec![
            driver(
                "tg_l1",
                None,
                vec![
                    rom("cpu_l1.rom", b"cpu one", "cpu1", false),
                    rom("s2.rom", b"sound two", "sound1", true),
                    rom("s3.rom", b"sound three", "sound1", true),
                ],
            ),
            driver(
                "tg_l2",
                Some("tg_l1"),
                vec![
                    rom("cpu_l2.rom", b"cpu two", "cpu1", false),
                    rom("s2.rom", b"sound two", "sound1", true),
                    rom("s3.rom", b"sound three", "sound1", true),
                ],
            ),
            driver("other", None, vec![rom("o.rom", b"other", "cpu1", false)]),
        ]
    }

    fn zip(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let mut w = zipread::Writer::default();
        for (n, d) in files {
            w.add(n, d);
        }
        let p = dir.join(name);
        std::fs::write(&p, w.finish()).unwrap();
        p
    }

    fn check(ix: &Index, p: &Path) -> Unit {
        identify(
            ix,
            &p.display().to_string(),
            "zip",
            &stem_of(p),
            zip_members(p).unwrap(),
        )
    }

    #[test]
    fn identifies_zips_by_content() {
        let dir =
            std::env::temp_dir().join(format!("rom2altsound-romcheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = table();
        let ix = Index::new(&t, HashMap::new());

        // A good zip.
        let p = zip(
            &dir,
            "tg_l1.zip",
            &[
                ("cpu_l1.rom", b"cpu one"),
                ("s2.rom", b"sound two"),
                ("s3.rom", b"sound three"),
            ],
        );
        let u = check(&ix, &p);
        assert_eq!(u.status, "ok", "{:?}", u.issues);
        assert_eq!(u.sets.len(), 1);
        assert_eq!(u.sets[0].set, "tg_l1");
        assert!(u.sets[0].sound.sound_roms_good);
        assert_eq!(u.sets[0].sound.shared_with, vec!["tg_l2".to_owned()]);
        assert_eq!(
            u.sets[0].sound.sound_rom_id,
            t[1].sound_rom_id(),
            "a clone with the same sound ROMs has the same id"
        );

        // Misnamed, a file under another name, and a .vpx.
        let p = zip(
            &dir,
            "tgl2.zip",
            &[
                ("CPU.BIN", b"cpu two"),
                ("s2.rom", b"sound two"),
                ("s3.rom", b"sound three"),
                ("table.vpx", b"not a rom"),
            ],
        );
        let u = check(&ix, &p);
        assert_eq!(u.status, "misnamed");
        assert_eq!(u.sets[0].set, "tg_l2");
        assert_eq!(u.sets[0].renamed.len(), 1);
        assert_eq!(u.extras.len(), 1);
        assert_eq!(u.extras[0].kind, "not-a-rom");

        // A bad dump: the right names, one wrong CRC.
        let p = zip(
            &dir,
            "tg_l1b.zip",
            &[
                ("cpu_l1.rom", b"cpu oNe"),
                ("s2.rom", b"sound two"),
                ("s3.rom", b"sound three"),
            ],
        );
        let u = check(&ix, &p);
        assert_eq!(u.status, "bad-dump");
        assert_eq!(u.sets[0].set, "tg_l1");
        assert_eq!(u.sets[0].wrong.len(), 1);
        assert!(
            u.sets[0].sound.sound_roms_good,
            "the sound ROMs are still good"
        );

        // Incomplete: a sound ROM is missing.
        let p = zip(
            &dir,
            "tg_l1.zip",
            &[("cpu_l1.rom", b"cpu one"), ("s2.rom", b"sound two")],
        );
        let u = check(&ix, &p);
        assert_eq!(u.status, "incomplete");
        assert_eq!(u.sets[0].missing.len(), 1);
        assert!(u.sets[0].missing[0].sound);
        assert!(!u.sets[0].sound.sound_roms_good);

        // Merged: the clone in a subfolder.
        let p = zip(
            &dir,
            "tg_l1m.zip",
            &[
                ("cpu_l1.rom", b"cpu one"),
                ("s2.rom", b"sound two"),
                ("s3.rom", b"sound three"),
                ("tg_l2/cpu_l2.rom", b"cpu two"),
            ],
        );
        let u = check(&ix, &p);
        let names: Vec<&str> = u.sets.iter().map(|s| s.set.as_str()).collect();
        assert_eq!(names, ["tg_l1", "tg_l2"]);
        assert!(u.issues.iter().any(|i| i.starts_with("merged")));
        assert!(u.issues.iter().any(|i| i.starts_with("subfolder")));

        // Nothing of PinMAME.
        let p = zip(&dir, "junk.zip", &[("readme.txt", b"hello")]);
        assert_eq!(check(&ix, &p).status, "not-pinmame");

        // A split clone next to its parent; --fix-names writes a standalone clone zip.
        let split = dir.join("split");
        std::fs::create_dir_all(&split).unwrap();
        zip(
            &split,
            "tg_l1.zip",
            &[
                ("cpu_l1.rom", b"cpu one"),
                ("s2.rom", b"sound two"),
                ("s3.rom", b"sound three"),
            ],
        );
        zip(&split, "tg_l2.zip", &[("cpu_l2.rom", b"cpu two")]);
        let mut units: Vec<Unit> = ["tg_l1.zip", "tg_l2.zip"]
            .iter()
            .map(|n| check(&ix, &split.join(n)))
            .collect();
        find_split_sets(&mut units);
        assert_eq!(units[1].status, "split");
        assert_eq!(units[1].sets[0].status, SetStatus::Split);
        let out = dir.join("fixed");
        let log = fix_names(&ix, &units, &out);
        assert_eq!(log.len(), 2, "{log:?}");
        let fixed = zip_members(&out.join("tg_l2.zip")).unwrap();
        let mut names: Vec<&str> = fixed.iter().map(|m| m.path.as_str()).collect();
        names.sort();
        assert_eq!(names, ["cpu_l2.rom", "s2.rom", "s3.rom"]);
        let again = check(&ix, &out.join("tg_l2.zip"));
        assert_eq!(again.status, "ok");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A system ROM set (System 80's `gts80s`, `NOT_A_DRIVER` in PinMAME) and two games
    /// that load its two CPU board ROMs from it, one of them with no ROM of its own.
    fn system_table() -> Vec<Driver> {
        let bios = || {
            vec![
                rom("u2_80.bin", b"system u2", "cpu1", false),
                rom("u3_80.bin", b"system u3", "cpu1", false),
            ]
        };
        let mut sys = driver("gts80s", None, bios());
        sys.system = true;
        let mut game = bios();
        game.push(rom("653-1.cpu", b"game cpu", "cpu1", false));
        game.push(rom("653.snd", b"game sound", "sound1", true));
        let mut game = driver("spidermn", None, game);
        game.systems = vec!["gts80s".into()];
        let mut bare = driver("bare", None, bios());
        bare.systems = vec!["gts80s".into()];
        vec![sys, game, bare]
    }

    #[test]
    fn system_rom_sets_are_support() {
        let dir =
            std::env::temp_dir().join(format!("rom2altsound-romcheck-sys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = system_table();
        let ix = Index::new(&t, HashMap::new());
        let bios: [(&str, &[u8]); 2] = [("u2_80.bin", b"system u2"), ("u3_80.bin", b"system u3")];

        // The system zip: SUPPORT, not a misnamed or merged game.
        let sys = zip(&dir, "gts80s.zip", &bios);
        let u = check(&ix, &sys);
        assert_eq!(u.status, "support", "{:?}", u.issues);
        assert_eq!(u.sets.len(), 1);
        assert_eq!(u.sets[0].set, "gts80s");
        assert_eq!(u.sets[0].system_users, Some(2));
        assert!(u.issues.iter().all(|i| !i.starts_with("misnamed")));
        assert!(
            u.issues.iter().any(|i| i.contains("bare")),
            "{:?}",
            u.issues
        );

        // A game zip with the system ROMs in it: the game, the system set left out.
        let mut files = bios.to_vec();
        files.push(("653-1.cpu", b"game cpu"));
        files.push(("653.snd", b"game sound"));
        let p = zip(&dir, "spidermn.zip", &files);
        let u = check(&ix, &p);
        assert_eq!(u.status, "ok", "{:?}", u.issues);
        let names: Vec<&str> = u.sets.iter().map(|s| s.set.as_str()).collect();
        assert_eq!(names, ["spidermn"]);

        // A game zip without them: complete with the system zip of the same folder.
        let only = dir.join("only");
        std::fs::create_dir_all(&only).unwrap();
        zip(&only, "gts80s.zip", &bios);
        zip(
            &only,
            "spidermn.zip",
            &[("653-1.cpu", b"game cpu"), ("653.snd", b"game sound")],
        );
        let mut units: Vec<Unit> = ["gts80s.zip", "spidermn.zip"]
            .iter()
            .map(|n| check(&ix, &only.join(n)))
            .collect();
        assert_eq!(units[1].status, "incomplete");
        find_split_sets(&mut units);
        assert_eq!(units[0].status, "support");
        assert_eq!(units[1].status, "ok", "{:?}", units[1].issues);
        assert_eq!(units[1].sets[0].status, SetStatus::Good);
        assert!(units[1].sets[0].sound.sound_roms_good);
        // --fix-names: the system zip as is, the game standalone (system ROMs included).
        let out = dir.join("fixed");
        let log = fix_names(&ix, &units, &out);
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(check(&ix, &out.join("spidermn.zip")).status, "ok");
        assert_eq!(check(&ix, &out.join("gts80s.zip")).status, "support");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A Pinball 2000 version zip (its own update files only) and MAME's base zip of the
    /// game (`rfmpb`: the shared ROMs under other names): completable, and --fix-names
    /// writes the whole set.
    #[test]
    fn completes_a_set_from_another_zip_by_content() {
        let dir =
            std::env::temp_dir().join(format!("rom2altsound-romcheck-p2k-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = |d: &mut Vec<RomFile>| {
            d.push(rom("rfm_28f800.rom", b"sound flash", "sound1", true));
            d.push(rom("rfm_u100.rom", b"prism", "user1", false));
        };
        let mut parent = vec![rom("p_game.rom", b"game 1.60", "user2", false)];
        shared(&mut parent);
        let mut clone = vec![rom("c_game.rom", b"game 1.20", "user2", false)];
        shared(&mut clone);
        // rfm_010 has no file of its own: the base zip holds it whole.
        let mut base = Vec::new();
        shared(&mut base);
        let t = vec![
            driver("rfm_160", None, parent),
            driver("rfm_120", Some("rfm_160"), clone),
            driver("rfm_010", Some("rfm_160"), base),
        ];
        let ix = Index::new(&t, HashMap::new());
        zip(&dir, "rfm_120.zip", &[("c_game.rom", b"game 1.20")]);
        zip(
            &dir,
            "rfmpb.zip",
            &[
                ("28f800.bin", b"sound flash"),
                ("u100.rom", b"prism"),
                ("awdbios.bin", b"bios"),
            ],
        );
        let mut units: Vec<Unit> = ["rfm_120.zip", "rfmpb.zip"]
            .iter()
            .map(|n| check(&ix, &dir.join(n)))
            .collect();
        find_split_sets(&mut units);
        let s = &units[0].sets[0];
        assert_eq!(s.status, SetStatus::Incomplete);
        assert_eq!(s.complete_with.len(), 1, "{:?}", units[0].issues);
        assert!(units[0].issues.iter().any(|i| i.starts_with("completable")));
        let out = dir.join("fixed");
        let log = fix_names(&ix, &units, &out);
        assert!(log.iter().any(|l| l.starts_with("rfm_120:")), "{log:?}");
        assert!(log.iter().any(|l| l.starts_with("rfm_010:")), "{log:?}");
        let u = check(&ix, &out.join("rfm_120.zip"));
        assert_eq!(u.status, "ok", "{:?}", u.issues);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_dump_sets_by_name() {
        let dir = std::env::temp_dir().join(format!(
            "rom2altsound-romcheck-nodump-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // A Stern SAM colour mod: PinMAME declares its one file NO_DUMP.
        let mut r = rom("acd_168hc.bin", b"any colour mod", "cpu1", false);
        r.crc = None;
        r.sha1 = None;
        r.no_dump = true;
        let t = vec![driver("acd_168hc", None, vec![r])];
        let ix = Index::new(&t, HashMap::new());
        let p = zip(
            &dir,
            "acd_168hc.zip",
            &[("acd_168hc.bin", b"another dump..")],
        );
        let u = check(&ix, &p);
        assert_eq!(u.status, "ok", "{:?}", u.issues);
        assert_eq!(u.sets[0].no_dump, ["acd_168hc.bin"]);
        assert!(u.extras.is_empty());
        // Another length: still that file (PinMAME only warns), noted.
        let p = zip(&dir, "acd_168hc.zip", &[("ACD_168HC.BIN", b"short")]);
        let u = check(&ix, &p);
        assert_eq!(u.status, "ok", "{:?}", u.issues);
        assert!(u.sets[0].no_dump[0].contains("5 bytes"));
        let p = zip(&dir, "other.zip", &[("other.bin", b"short")]);
        assert_eq!(check(&ix, &p).status, "not-pinmame");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
