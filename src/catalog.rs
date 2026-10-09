//! `rom2altsound catalog <dir|zip>... --out catalog.json`: the sound ROM catalog behind the
//! project's web site (docs/index.html). One entry per sound ROM id (see
//! [`drivers::sound_rom_id`]) of the PinMAME linked in, plus one per game set that has no
//! sound ROM id (Stern SAM, whose sounds are in the main image, and the games with no sound
//! ROM at all). Each entry lists the sets that share it, how they were found in the ROM
//! folders given (`rom2altsound roms`), their sound board family and how far rom2altsound
//! gets with it (docs/board-support.md), the sound ROM files (name, size, CRC32, SHA-1),
//! the number of tracks or calls read statically from the ROMs when the layout is known
//! (DCS track catalog, Pinball 2000's, Stern SAM's call table) and how many commands
//! PinMAME's sounds.dat names for them.
//!
//! Only metadata goes in the file: names, sizes, checksums and counts. Nothing of a ROM's
//! content, and only the number of sounds.dat names, not the names themselves.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use clap::Parser;
use serde::Serialize;

use crate::drivers::{self, Driver, RomFile};
use crate::romcheck::{self, BoardInfo, Index, Member, SetStatus, Unit};
use crate::soundsdat::SoundsDat;
use crate::{dcsrom, sam, sha1};

/// The board survey, read for the support level of each family.
const BOARD_SUPPORT: &str = include_str!("../docs/board-support.md");

#[derive(Parser)]
#[command(
    name = "rom2altsound catalog",
    about = "Write the sound ROM catalog (catalog.json) of the project's web site",
    long_about = "Write the sound ROM catalog (catalog.json) of the project's web site.

One entry per sound ROM id of the PinMAME built in (and one per set with no sound ROM:
Stern SAM, games with no sound board). Each lists the sets sharing it with how they were
found in the given ROM folders (as `rom2altsound roms` checks them), the sound board
family and rom2altsound's support level for it (docs/board-support.md), the sound ROM
files (names, sizes, CRC32 and SHA-1), the number of tracks or calls read from the ROMs
when their layout is known (DCS and Pinball 2000 track catalogs, Stern SAM call tables),
and the number of commands sounds.dat names. Metadata only: nothing of the ROMs' content.",
    after_help = "Example:
  rom2altsound catalog ~/vpinball/roms --out docs/catalog.json"
)]
struct CatalogCli {
    /// ROM zips, ROM folders, or folders holding them (as `rom2altsound roms`)
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<PathBuf>,
    /// The JSON file to write
    #[arg(long, value_name = "FILE", default_value = "catalog.json")]
    out: PathBuf,
    /// ROMs read at once for the track and call counts (Stern SAM images are large)
    #[arg(long, default_value_t = 2)]
    jobs: usize,
    /// The map of the per-family docs ({"SNDBRD_X": "file.md#anchor"}, relative to its
    /// folder); a family it does not list links to docs/board-support.md
    #[arg(
        long,
        value_name = "FILE",
        default_value = "docs/families/families.json"
    )]
    family_docs: PathBuf,
}

/// Where the docs are browsed.
const DOCS_URL: &str = "https://github.com/Le-Syl21/rom2altsound/blob/main/docs/";

/// The doc link of each SNDBRD_* family, from the per-family docs map (`text`, its entries
/// relative to docs/families/).
pub fn family_docs(text: Option<&str>) -> BTreeMap<String, String> {
    let Some(v) = text.and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok()) else {
        return BTreeMap::new();
    };
    let Some(m) = v.as_object() else {
        return BTreeMap::new();
    };
    m.iter()
        .filter_map(|(k, v)| {
            let rel = v.as_str()?.trim_start_matches("./");
            (k.starts_with("SNDBRD_") && !rel.contains("://") && !rel.contains(".."))
                .then(|| (k.clone(), format!("{DOCS_URL}families/{rel}")))
        })
        .collect()
}

/// How far rom2altsound gets with a family, from the status column of the survey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    /// ✅ sounds come out, distinct, each from silence.
    Works,
    /// ⚠️ few commands give a sound, or doubtful.
    Partial,
    /// ❌ no sound from the commands tried.
    None,
    /// — no sound board to drive.
    NoBoard,
    /// ❔ not tried, or the family is not in the survey.
    Untested,
}

/// One row of the survey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportRow {
    pub support: Support,
    pub test_rom: String,
}

/// The survey's table, by family label (its first column: "DCS95", "S11XS+S11CS",
/// "NONE (Stern SAM)").
pub fn parse_support(md: &str) -> HashMap<String, SupportRow> {
    let mut out = HashMap::new();
    for line in md.lines() {
        let cells: Vec<&str> = line.trim().split('|').map(str::trim).collect();
        // "| a | b | ... |" splits into an empty first and last cell.
        if cells.len() < 12 || !cells[0].is_empty() {
            continue;
        }
        let cells = &cells[1..cells.len() - 1];
        let label = cells[0];
        if label.is_empty() || label.starts_with("family") || label.starts_with("---") {
            continue;
        }
        let status = cells[9];
        let support = if status.contains('✅') {
            Support::Works
        } else if status.contains('⚠') {
            Support::Partial
        } else if status.contains('❌') {
            Support::None
        } else if status.contains('—') {
            Support::NoBoard
        } else {
            Support::Untested
        };
        let test_rom = cells[8].trim_matches('-').trim().to_owned();
        out.entry(label.to_owned())
            .or_insert(SupportRow { support, test_rom });
    }
    out
}

/// The survey's label of a family: the SNDBRD_* names without the prefix ("DCS95",
/// "S11XS+S11CS"); "NONE (Stern SAM)" or "NONE (other)" for no board.
pub fn family_label(board: Option<&str>, sam: bool) -> String {
    match board {
        None => "unknown".into(),
        Some("SNDBRD_NONE") if sam => "NONE (Stern SAM)".into(),
        Some("SNDBRD_NONE") => "NONE (other)".into(),
        Some(b) => b
            .split('+')
            .map(|p| p.strip_prefix("SNDBRD_").unwrap_or(p))
            .collect::<Vec<_>>()
            .join("+"),
    }
}

/// How a set was found in the folders read, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Found {
    /// Complete, every file with the right CRC.
    Good,
    /// A clone complete with its parent's zip.
    Split,
    /// Complete with files of other zips of the folder (Pinball 2000's base zips).
    Completable,
    /// Every file there, some with a wrong CRC.
    BadDump,
    /// Files missing.
    Incomplete,
    /// Not in the folders read.
    Absent,
}

/// The best state each set was found in, over every unit.
pub fn found_sets(units: &[Unit]) -> HashMap<String, Found> {
    let mut out: HashMap<String, Found> = HashMap::new();
    for s in units.iter().flat_map(|u| &u.sets) {
        let f = match s.status {
            SetStatus::Good => Found::Good,
            SetStatus::Split => Found::Split,
            SetStatus::BadDump => Found::BadDump,
            SetStatus::Incomplete if !s.complete_with.is_empty() => Found::Completable,
            SetStatus::Incomplete => Found::Incomplete,
        };
        out.entry(s.set.clone())
            .and_modify(|o| *o = (*o).min(f))
            .or_insert(f);
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct SetRow {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub description: String,
    pub manufacturer: String,
    pub year: String,
    pub found: Found,
    /// Commands sounds.dat names for the set (its section and its parent's).
    pub names: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RomRow {
    pub name: String,
    pub size: u64,
    pub crc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    /// A file with this CRC32 and size is in the folders read.
    pub found: bool,
}

/// What could be counted in the ROMs, without running them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sounds {
    /// "dcs-tracks" (DCS and Pinball 2000: populated slots of the track catalog) or
    /// "sam-calls" (Stern SAM: records of the sound call table).
    pub kind: &'static str,
    pub count: usize,
    /// DCS: the catalog's track slots, populated or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slots: Option<usize>,
    /// Stern SAM: the sample ids of the directory, and the music scripts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub samples: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    /// The sound ROM id; None for a set without sound ROMs (then `key` is `set:<name>`).
    pub id: Option<String>,
    pub key: String,
    /// The SNDBRD_* family ("SNDBRD_DCS95"; "unknown" when the game's init was not read).
    pub family: String,
    /// The survey's label for it ("DCS95", "NONE (Stern SAM)").
    pub label: String,
    pub support: Support,
    /// The game: its first set without a parent (else its first set).
    pub title: String,
    pub manufacturer: String,
    /// The years of its sets ("1993", "1993-2026").
    pub year: String,
    pub sets: Vec<SetRow>,
    /// The sound ROM files (Stern SAM: the image the sounds are read from).
    pub roms: Vec<RomRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sounds: Option<Sounds>,
    /// Distinct commands sounds.dat names, over all the sets.
    pub names: usize,
}

/// The Stern SAM image of a set: its file name, CRC32 and length ([`sam::sam_set`]).
pub type SamLookup = dyn Fn(&str) -> Option<(&'static str, u32, u32)>;

/// What the catalog's entries are built from, besides PinMAME's table.
pub struct Inputs<'a> {
    /// Family (`BoardInfo::board`) of each set, by name.
    pub families: &'a HashMap<String, String>,
    pub found: &'a HashMap<String, Found>,
    /// (CRC32, size) of every file in the folders read.
    pub files: &'a BTreeSet<(u32, u64)>,
    pub dat: &'a SoundsDat,
    pub support: &'a HashMap<String, SupportRow>,
    /// Whether a set is a Stern SAM set (its image file, CRC32 and length).
    pub sam: &'a SamLookup,
}

fn rom_row(r: &RomFile, files: &BTreeSet<(u32, u64)>) -> RomRow {
    RomRow {
        name: r.name.clone(),
        size: r.size,
        crc: r.crc.map_or_else(String::new, |c| format!("{c:08x}")),
        sha1: r.sha1.clone(),
        found: r.crc.is_some_and(|c| files.contains(&(c, r.size))),
    }
}

fn year_span(sets: &[SetRow]) -> String {
    let years: BTreeSet<&str> = sets
        .iter()
        .map(|s| s.year.as_str())
        .filter(|y| y.len() == 4 && y.bytes().all(|b| b.is_ascii_digit()))
        .collect();
    match (years.first(), years.last()) {
        (Some(a), Some(b)) if a == b => (*a).to_owned(),
        (Some(a), Some(b)) => format!("{a}-{b}"),
        _ => sets.first().map(|s| s.year.clone()).unwrap_or_default(),
    }
}

/// The catalog's entries, without the track and call counts (see [`count_sounds`]), sorted
/// by game title, then key.
pub fn entries(drivers: &[Driver], inp: &Inputs) -> Vec<Entry> {
    let mut groups: BTreeMap<String, Vec<&Driver>> = BTreeMap::new();
    for d in drivers.iter().filter(|d| !d.system) {
        let key = match d.sound_rom_id() {
            Some(id) => id,
            None => format!("set:{}", d.name),
        };
        groups.entry(key).or_default().push(d);
    }
    let mut out: Vec<Entry> = groups
        .into_iter()
        .map(|(key, ds)| {
            let id = (!key.starts_with("set:")).then(|| key.clone());
            let sets: Vec<SetRow> = ds
                .iter()
                .map(|d| SetRow {
                    name: d.name.clone(),
                    parent: d.parent.clone(),
                    description: d.description.clone(),
                    manufacturer: d.manufacturer.clone(),
                    year: d.year.clone(),
                    found: inp.found.get(&d.name).copied().unwrap_or(Found::Absent),
                    names: inp.dat.game_entries(&d.name, d.parent.as_deref()).len(),
                })
                .collect();
            let mut named: BTreeSet<Vec<u8>> = BTreeSet::new();
            for d in &ds {
                for e in inp.dat.game_entries(&d.name, d.parent.as_deref()) {
                    named.insert(e.bytes);
                }
            }
            // Families by number of sets: the most common one names the entry.
            let mut fams: BTreeMap<&str, usize> = BTreeMap::new();
            for d in &ds {
                let f = inp.families.get(&d.name).map_or("unknown", String::as_str);
                *fams.entry(f).or_default() += 1;
            }
            let family = fams
                .iter()
                .max_by_key(|(f, n)| (**n, std::cmp::Reverse(**f)))
                .map(|(f, _)| (*f).to_owned())
                .unwrap_or_else(|| "unknown".into());
            // PinMAME's SAM driver (its boot flash sets included), or a set with a SAM image.
            let is_sam = ds
                .iter()
                .any(|d| d.source.ends_with("sam.c") || (inp.sam)(&d.name).is_some());
            let label = family_label((family != "unknown").then_some(&family), is_sam);
            let support = inp
                .support
                .get(&label)
                .map_or(Support::Untested, |r| r.support);
            let head = ds.iter().find(|d| d.parent.is_none()).unwrap_or(&ds[0]);
            let roms: Vec<RomRow> = if id.is_some() {
                let mut seen = BTreeSet::new();
                ds.iter()
                    .flat_map(|d| d.roms.iter())
                    .filter(|r| r.sound && !r.no_dump)
                    .filter(|r| seen.insert(r.sha1.clone()))
                    .map(|r| rom_row(r, inp.files))
                    .collect()
            } else if let Some((file, crc, len)) = (inp.sam)(&ds[0].name) {
                ds[0]
                    .roms
                    .iter()
                    .filter(|r| {
                        r.name.eq_ignore_ascii_case(file)
                            || (r.crc == Some(crc) && r.size == len as u64)
                    })
                    .map(|r| rom_row(r, inp.files))
                    .collect()
            } else {
                Vec::new()
            };
            Entry {
                id,
                key,
                family,
                label,
                support,
                title: head.description.clone(),
                manufacturer: head.manufacturer.clone(),
                year: year_span(&sets),
                sets,
                roms,
                sounds: None,
                names: named.len(),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// What the ROMs of an entry tell without running them: the DCS (or Pinball 2000) track
/// catalog, or the Stern SAM call table. `fetch` gives a file's bytes from its CRC32 and
/// size (None when it is not at hand); a file whose SHA-1 is not PinMAME's is not used.
pub fn count_sounds(e: &Entry, fetch: &dyn Fn(u32, u64) -> Option<Vec<u8>>) -> Option<Sounds> {
    let get = |r: &RomRow| -> Option<Vec<u8>> {
        let crc = u32::from_str_radix(&r.crc, 16).ok()?;
        let data = fetch(crc, r.size)?;
        match &r.sha1 {
            Some(s) if *s != sha1::hex(&data) => None,
            _ => Some(data),
        }
    };
    match e.label.as_str() {
        "DCS" | "DCS95" => e.roms.iter().find_map(|r| {
            let (slots, populated) = dcsrom::tracks(&get(r)?)?;
            Some(Sounds {
                kind: "dcs-tracks",
                count: populated.len(),
                slots: Some(slots as usize),
                samples: None,
                music: None,
            })
        }),
        "DCSP2K" => {
            // PinMAME's region: the sound flash at 0, U109 at $400000, U110 at $800000.
            let mut region = vec![0xFFu8; 0xC0_0000];
            for (k, r) in e.roms.iter().take(3).enumerate() {
                let data = get(r)?;
                let at = k * 0x40_0000;
                let n = data.len().min(0x40_0000);
                region[at..at + n].copy_from_slice(&data[..n]);
            }
            let (slots, populated) = dcsrom::tracks(&dcsrom::p2k_image(&region))?;
            Some(Sounds {
                kind: "dcs-tracks",
                count: populated.len(),
                slots: Some(slots as usize),
                samples: None,
                music: None,
            })
        }
        "NONE (Stern SAM)" => {
            let image = get(e.roms.first()?)?;
            let cat = sam::catalog(&image).ok()?;
            let samples: BTreeSet<u16> = cat.entries.iter().map(|x| x.sample).collect();
            Some(Sounds {
                kind: "sam-calls",
                count: cat.calls.len(),
                slots: None,
                samples: Some(samples.len()),
                music: Some(cat.music.len()),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Default, Serialize)]
struct Tally {
    entries: usize,
    sound_rom_ids: usize,
    sets: usize,
    sets_found: usize,
}

impl Tally {
    fn add(&mut self, e: &Entry) {
        self.entries += 1;
        self.sound_rom_ids += e.id.is_some() as usize;
        self.sets += e.sets.len();
        self.sets_found += e
            .sets
            .iter()
            .filter(|s| s.found <= Found::Completable)
            .count();
    }
}

#[derive(Debug, Serialize)]
struct FamilyTally {
    label: String,
    support: Support,
    #[serde(skip_serializing_if = "String::is_empty")]
    test_rom: String,
    #[serde(flatten)]
    tally: Tally,
}

#[derive(Debug, Serialize)]
struct Stats {
    #[serde(flatten)]
    all: Tally,
    /// Sets of the folders read, by how they were found.
    found: BTreeMap<String, usize>,
    support: BTreeMap<String, Tally>,
    families: Vec<FamilyTally>,
    entries_with_counts: usize,
    entries_with_names: usize,
}

fn stats(entries: &[Entry], support: &HashMap<String, SupportRow>) -> Stats {
    let mut all = Tally::default();
    let mut found: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_support: BTreeMap<String, Tally> = BTreeMap::new();
    let mut fams: BTreeMap<String, (Support, Tally)> = BTreeMap::new();
    for e in entries {
        all.add(e);
        for s in &e.sets {
            let k = serde_json::to_value(s.found).unwrap();
            *found.entry(k.as_str().unwrap().to_owned()).or_default() += 1;
        }
        let k = serde_json::to_value(e.support).unwrap();
        by_support
            .entry(k.as_str().unwrap().to_owned())
            .or_default()
            .add(e);
        fams.entry(e.label.clone())
            .or_insert_with(|| (e.support, Tally::default()))
            .1
            .add(e);
    }
    let mut families: Vec<FamilyTally> = fams
        .into_iter()
        .map(|(label, (sup, tally))| FamilyTally {
            test_rom: support
                .get(&label)
                .map(|r| r.test_rom.clone())
                .unwrap_or_default(),
            label,
            support: sup,
            tally,
        })
        .collect();
    families.sort_by(|a, b| {
        a.support
            .cmp(&b.support)
            .then(b.tally.sets.cmp(&a.tally.sets))
            .then(a.label.cmp(&b.label))
    });
    Stats {
        all,
        found,
        support: by_support,
        families,
        entries_with_counts: entries.iter().filter(|e| e.sounds.is_some()).count(),
        entries_with_names: entries.iter().filter(|e| e.names > 0).count(),
    }
}

/// The file: a header, then one entry per line (small diffs when it is regenerated).
fn render(
    entries: &[Entry],
    support: &HashMap<String, SupportRow>,
    docs: &BTreeMap<String, String>,
    units: usize,
) -> String {
    let head = serde_json::json!({
        "format": 1,
        "generator": format!("rom2altsound {} catalog", env!("CARGO_PKG_VERSION")),
        "units_read": units,
        "family_docs": docs,
        "family_docs_default": format!("{DOCS_URL}board-support.md"),
        "stats": stats(entries, support),
    });
    let mut s = serde_json::to_string(&head).unwrap();
    s.pop(); // the closing brace
    s.push_str(",\"entries\":[\n");
    for (i, e) in entries.iter().enumerate() {
        s.push_str(&serde_json::to_string(e).unwrap());
        s.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
    }
    s.push_str("]}\n");
    s
}

/// Entry point of `rom2altsound catalog`; returns the exit code.
pub fn cli(args: Vec<String>) -> i32 {
    let cli =
        CatalogCli::parse_from(std::iter::once("rom2altsound catalog".to_owned()).chain(args));
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
            eprintln!("error: sound boards unknown: {e}");
            return 1;
        }
    };
    let ix = Index::new(&table, boards);
    let families: HashMap<String, String> = table
        .iter()
        .filter_map(|d| {
            let b = BoardInfo::of(ix.boards.get(&d.name)).board?;
            Some((d.name.clone(), b))
        })
        .collect();
    let (paths, ignored) = romcheck::collect_units(&cli.paths);
    for i in &ignored {
        eprintln!("ignored: {i}");
    }
    let units = romcheck::scan(&ix, &paths, false);
    eprintln!("{} unit(s) read", units.len());
    let found = found_sets(&units);
    let mut by_hash: HashMap<(u32, u64), &Member> = HashMap::new();
    for m in units.iter().flat_map(|u| &u.members) {
        by_hash.entry((m.crc, m.size)).or_insert(m);
    }
    let files: BTreeSet<(u32, u64)> = by_hash.keys().copied().collect();
    let dat = SoundsDat::parse(crate::soundsdat::BUILT_IN);
    let support = parse_support(BOARD_SUPPORT);
    let sam_of = |name: &str| sam::sam_set(name);
    let inp = Inputs {
        families: &families,
        found: &found,
        files: &files,
        dat: &dat,
        support: &support,
        sam: &sam_of,
    };
    let mut entries = entries(&table, &inp);
    let fetch = |crc: u32, size: u64| -> Option<Vec<u8>> {
        romcheck::member_bytes(by_hash.get(&(crc, size))?).ok()
    };
    // The track and call counts, `jobs` entries at a time.
    let todo: Vec<usize> = (0..entries.len())
        .filter(|&i| {
            matches!(
                entries[i].label.as_str(),
                "DCS" | "DCS95" | "DCSP2K" | "NONE (Stern SAM)"
            ) && entries[i].roms.iter().any(|r| r.found)
        })
        .collect();
    eprintln!("reading the track/call tables of {} entries", todo.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let counted: Vec<(usize, Option<Sounds>)> = std::thread::scope(|sc| {
        let workers: Vec<_> = (0..cli.jobs.max(1))
            .map(|_| {
                sc.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&i) = todo.get(k) else { break };
                        out.push((i, count_sounds(&entries[i], &fetch)));
                    }
                    out
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect()
    });
    for (i, s) in counted {
        entries[i].sounds = s;
    }
    let docs = family_docs(std::fs::read_to_string(&cli.family_docs).ok().as_deref());
    if docs.is_empty() {
        eprintln!(
            "note: no family docs map at {}: families link to board-support.md",
            cli.family_docs.display()
        );
    }
    let text = render(&entries, &support, &docs, units.len());
    if let Err(e) = std::fs::write(&cli.out, &text) {
        eprintln!("error: {}: {e}", cli.out.display());
        return 1;
    }
    let st = stats(&entries, &support);
    eprintln!(
        "{}: {} entries ({} sound ROM ids), {} sets ({} found), {} with track/call counts, {} bytes",
        cli.out.display(),
        st.all.entries,
        st.all.sound_rom_ids,
        st.all.sets,
        st.all.sets_found,
        st.entries_with_counts,
        text.len()
    );
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(name: &str, crc: u32, sha: &str, sound: bool) -> RomFile {
        RomFile {
            name: name.into(),
            size: 4,
            crc: Some(crc),
            sha1: Some(sha.into()),
            region: if sound { "sound1" } else { "cpu1" }.into(),
            sound,
            optional: false,
            no_dump: false,
            bad_dump: false,
        }
    }

    fn driver(name: &str, parent: Option<&str>, year: &str, roms: Vec<RomFile>) -> Driver {
        Driver {
            name: name.into(),
            parent: parent.map(Into::into),
            description: format!("Game {name}"),
            year: year.into(),
            manufacturer: "Williams".into(),
            source: String::new(),
            roms,
            cpus: Vec::new(),
            sound_chips: Vec::new(),
            system: false,
            systems: Vec::new(),
        }
    }

    const SURVEY: &str = "\
| family (`SNDBRD_*`) | PinMAME board | sets | games | ids | years | makers | full | test ROM | status | notes |
|---|---|---|---|---|---|---|---|---|---|---|
| NONE (Stern SAM) | - | 406 | 27 | 0 | 2006-2024 | Stern | 398 | acd_170h | ✅ | no board |
| WPCS | WPCS | 219 | 25 | 35 | 1990-2026 | Bally | 212 | tz_92 | ✅ | 37 of 40 |
| S67S | WMSS67 | 105 | 38 | 28 | 1978-2022 | Williams | 104 | bk_l4 | ⚠️ | 3 of 40 |
| ST300 | ST300 | 76 | 17 | 0 | 1979-2026 | Stern | 67 | meteor | ❌ | not a command board |
| NONE (other) | - | 257 | 172 | 17 | 1974-2025 | Bally | 218 | - | — | nothing to drive |
";

    #[test]
    fn reads_the_survey() {
        let s = parse_support(SURVEY);
        assert_eq!(s.len(), 5);
        assert_eq!(s["WPCS"].support, Support::Works);
        assert_eq!(s["WPCS"].test_rom, "tz_92");
        assert_eq!(s["S67S"].support, Support::Partial);
        assert_eq!(s["ST300"].support, Support::None);
        assert_eq!(s["NONE (other)"].support, Support::NoBoard);
        assert_eq!(s["NONE (other)"].test_rom, "");
        assert_eq!(s["NONE (Stern SAM)"].support, Support::Works);
        // The real survey: every family row is read.
        let real = parse_support(BOARD_SUPPORT);
        assert!(real.len() > 80, "{} rows", real.len());
        assert_eq!(real["DCS95"].support, Support::Works);
        assert_eq!(real["DE3S"].support, Support::Partial);
    }

    #[test]
    fn labels() {
        assert_eq!(family_label(Some("SNDBRD_DCS95"), false), "DCS95");
        assert_eq!(
            family_label(Some("SNDBRD_S11XS+SNDBRD_S11CS"), false),
            "S11XS+S11CS"
        );
        assert_eq!(family_label(Some("SNDBRD_NONE"), true), "NONE (Stern SAM)");
        assert_eq!(family_label(Some("SNDBRD_NONE"), false), "NONE (other)");
        assert_eq!(family_label(None, false), "unknown");
    }

    #[test]
    fn groups_by_sound_rom_id() {
        let s1 = "1".repeat(40);
        let s2 = "2".repeat(40);
        let table = vec![
            driver(
                "tz_92",
                None,
                "1993",
                vec![
                    rom("cpu.rom", 1, &"a".repeat(40), false),
                    rom("s1", 10, &s1, true),
                ],
            ),
            driver(
                "tz_94h",
                Some("tz_92"),
                "1994",
                vec![
                    rom("cpu2.rom", 2, &"b".repeat(40), false),
                    rom("s1", 10, &s1, true),
                ],
            ),
            driver("other", None, "1990", vec![rom("x", 20, &s2, true)]),
            driver(
                "samgame",
                None,
                "2008",
                vec![rom("sam.bin", 30, &"c".repeat(40), false)],
            ),
        ];
        let families: HashMap<String, String> = [
            ("tz_92", "SNDBRD_WPCS"),
            ("tz_94h", "SNDBRD_WPCS"),
            ("other", "SNDBRD_S67S"),
            ("samgame", "SNDBRD_NONE"),
        ]
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect();
        let found: HashMap<String, Found> = [("tz_94h".to_owned(), Found::Good)].into();
        let files: BTreeSet<(u32, u64)> = [(10, 4)].into();
        let dat = SoundsDat::parse("tz_92:\n:01:One\n:02:Two\ntz_94h:\n:02:Two again\n:03:Three\n");
        let support = parse_support(SURVEY);
        let sam = |n: &str| (n == "samgame").then_some(("sam.bin", 30u32, 4u32));
        let inp = Inputs {
            families: &families,
            found: &found,
            files: &files,
            dat: &dat,
            support: &support,
            sam: &sam,
        };
        let e = entries(&table, &inp);
        assert_eq!(e.len(), 3);
        let tz = e.iter().find(|x| x.title == "Game tz_92").unwrap();
        assert_eq!(tz.id, drivers::sound_rom_id(table[0].roms.iter()));
        assert_eq!(tz.sets.len(), 2);
        assert_eq!(tz.sets[0].found, Found::Absent);
        assert_eq!(tz.sets[1].found, Found::Good);
        assert_eq!(tz.year, "1993-1994");
        assert_eq!(tz.label, "WPCS");
        assert_eq!(tz.support, Support::Works);
        assert_eq!(tz.roms.len(), 1);
        assert!(tz.roms[0].found);
        // tz_94h reads its own section and its parent's ("tz_92" prefix): 01, 02, 03.
        assert_eq!(tz.sets[1].names, 3);
        assert_eq!(tz.names, 3);
        let other = e.iter().find(|x| x.title == "Game other").unwrap();
        assert_eq!(other.support, Support::Partial);
        assert!(!other.roms[0].found);
        let samg = e.iter().find(|x| x.key == "set:samgame").unwrap();
        assert_eq!(samg.id, None);
        assert_eq!(samg.label, "NONE (Stern SAM)");
        assert_eq!(samg.roms.len(), 1);
        assert_eq!(samg.roms[0].name, "sam.bin");

        // The file: a header line, then one entry per line; valid JSON.
        let docs = family_docs(Some(
            r#"{"SNDBRD_WPCS": "williams.md#sndbrd_wpcs", "x": "y"}"#,
        ));
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs["SNDBRD_WPCS"],
            "https://github.com/Le-Syl21/rom2altsound/blob/main/docs/families/williams.md#sndbrd_wpcs"
        );
        assert!(family_docs(None).is_empty());
        assert!(family_docs(Some("not json")).is_empty());
        let text = render(&e, &support, &docs, 1);
        assert_eq!(text.lines().count(), 2 + e.len());
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["stats"]["entries"], 3);
        assert_eq!(v["stats"]["sound_rom_ids"], 2);
        assert_eq!(v["stats"]["sets"], 4);
        assert_eq!(v["stats"]["sets_found"], 1);
        assert_eq!(v["stats"]["support"]["works"]["sets"], 3);
        assert_eq!(v["entries"][0]["sets"][0]["found"], "absent");
        // Only metadata: no ROM bytes anywhere, no sounds.dat name.
        assert!(!text.contains("Two again"));
    }

    #[test]
    fn counts_dcs_tracks_from_u2() {
        // A U2 image with a catalog at $4000 and 3 track slots, 2 populated.
        let mut u2 = vec![0u8; 0x80000];
        let cat = 0x4000;
        u2[cat..cat + 2].copy_from_slice(&0x0080u16.to_be_bytes());
        u2[cat + 0x40..cat + 0x43].copy_from_slice(&[0x00, 0x50, 0x00]);
        u2[cat + 0x46..cat + 0x48].copy_from_slice(&3u16.to_be_bytes());
        u2[0x5000..0x5009].copy_from_slice(&[0x01, 0x23, 0x45, 0xFF, 0xFF, 0xFF, 0x20, 0, 0]);
        let other = vec![0u8; 16];
        let crc_u2 = crate::zipread::crc32(&u2);
        let roms = vec![
            RomRow {
                name: "s3".into(),
                size: other.len() as u64,
                crc: format!("{:08x}", crate::zipread::crc32(&other)),
                sha1: Some(sha1::hex(&other)),
                found: true,
            },
            RomRow {
                name: "s2".into(),
                size: u2.len() as u64,
                crc: format!("{crc_u2:08x}"),
                sha1: Some(sha1::hex(&u2)),
                found: true,
            },
        ];
        let mut e = Entry {
            id: Some("x".into()),
            key: "x".into(),
            family: "SNDBRD_DCS".into(),
            label: "DCS".into(),
            support: Support::Works,
            title: String::new(),
            manufacturer: String::new(),
            year: String::new(),
            sets: Vec::new(),
            roms,
            sounds: None,
            names: 0,
        };
        let fetch = |crc: u32, _size: u64| {
            if crc == crc_u2 {
                Some(u2.clone())
            } else {
                Some(other.clone())
            }
        };
        let s = count_sounds(&e, &fetch).unwrap();
        assert_eq!((s.kind, s.count, s.slots), ("dcs-tracks", 2, Some(3)));
        // A file whose SHA-1 is not PinMAME's is not read.
        e.roms[1].sha1 = Some("0".repeat(40));
        assert_eq!(count_sounds(&e, &fetch), None);
        // Families with no static layout: nothing counted.
        e.label = "WPCS".into();
        assert_eq!(count_sounds(&e, &fetch), None);
    }

    #[test]
    fn best_found_state_wins() {
        assert!(Found::Good < Found::Split);
        assert!(Found::Completable < Found::BadDump);
        assert!(Found::Incomplete < Found::Absent);
    }
}
