//! `rom2altsound sounds-dat --manifests <dir>... --out sounds.dat`: a PinMAME `sounds.dat`
//! (the list of named commands of its sound command mode, snd_cmd.c) made from the
//! per-set reports (`manifest.json`) of extraction runs: one entry per command that
//! produced a sound, with the bytes PinMAME's commander must send for it.
//!
//! How the commander reads the file (snd_cmd.c `readCmds`, run once per board): it takes
//! the entries under every heading that starts with the game's name or its parent's
//! (`clone_of`, unless that is a `NOT_A_DRIVER` system set), in file order, a command seen
//! twice keeping its first place and its last name. A clone thus sees its parent's entries
//! and its own: a clone gets a section only for what its parent's does not give it (a
//! command the parent's list lacks, or another real name), placed after the parent's so
//! that its names win. The prefix rule has a cost: a heading is also read by every game
//! whose name (or whose parent's) starts it (`afm_113b:` by `afm_113` and its clones,
//! `cueball:` by `cue`), so an entry goes in only if each of those games lists it already
//! with the same name (games that take no sound command aside); the others are held back
//! and counted. It sends an entry one byte every 4 frames through
//! `sndbrd_manCmd(board, byte)` (`playCmd`); on a machine with two sound boards, a game
//! section's entry is (board, byte) pairs.
//!
//! The bytes are those rom2altsound itself sends through `sndbrd_manCmd` (extract.rs
//! `board_sends`), framing included (idle and release bytes, strobes), so the commander
//! plays what the extraction recorded. WPCS is the exception the commander forces:
//! `sndbrd_manCmd` buffers its first byte for that board (`SNDBRD_DOUBLECMD`) and
//! `wpcs_manCmd_w` writes a pair, so a single byte `xx` goes out as `00 xx` (`00` stops
//! every sound, the commander's own "Stop playing" is `00 00`), and the second bank as
//! `7A xx`. A command the manual path cannot send (the data port, a control line, 16-bit
//! words, more than 6 bytes) is left out and counted.
//!
//! The output is PinMAME's own file unchanged (every line, comments included), then the
//! generated sections, sorted, each entry left out when the game already gets the same
//! bytes from PinMAME's sections as the commander reads them (`soundsdat::PinmameDat`):
//! an upstream line it cannot read (uppercase hex, a line cut by its 100-byte buffer, an
//! entry after an empty line) is written again in a form it reads.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::c_int;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::Value;

use crate::extract;
use crate::names::{self, Names};
use crate::soundsdat::{self, MAX_LINE_CHARS, MAX_NAME_CHARS, PinmameDat, SoundsDat};

#[derive(Parser)]
#[command(
    name = "rom2altsound sounds-dat",
    about = "Write a PinMAME sounds.dat from the reports of extraction runs",
    long_about = "Write a PinMAME sounds.dat from the reports of extraction runs.

PinMAME's sound command mode lists the commands sounds.dat names for the game. This
writes PinMAME's own sounds.dat unchanged, then one section per game measured (its
parent set's name; a clone only for what its parent's section does not give it): one
entry per command that produced a sound, with the bytes the commander must send for it
on that board (framing included). The names: sounds.dat's own (also the one the
extraction took from it), else the given names files', else the one a names file gave
the extraction, else the command and the sound's length (`Sound 2A (3.2 s)`,
`Sound 2A (loop)`). PinMAME reads every heading that starts with the game's name or its
parent's: an entry that another game would read too, without listing it, is held back.",
    after_help = "Example:
  rom2altsound sounds-dat --manifests runs/full --names names/ --out sounds.dat"
)]
struct SoundsDatCli {
    /// Folders of extraction runs (`rom2altsound <sets> --out DIR`, one folder per set,
    /// each with its manifest.json). Repeatable; a set found in several takes the last
    /// folder's
    #[arg(long, value_name = "DIR", required = true)]
    manifests: Vec<PathBuf>,
    /// Names files (the listening page's "Export names", as `--names` of an extraction),
    /// or folders of them (their *.csv files); each applies to the sets of its sound ROM
    /// id. Repeatable; a later file's name wins
    #[arg(long, value_name = "FILE|DIR")]
    names: Vec<PathBuf>,
    /// The sounds.dat to start from [default: the one of the PinMAME version this program
    /// is built with]
    #[arg(long, value_name = "FILE")]
    sounds_dat: Option<PathBuf>,
    /// The sounds.dat to write
    #[arg(long, value_name = "FILE", default_value = "sounds.dat")]
    out: PathBuf,
}

/// A game set as PinMAME's driver table gives it.
#[derive(Clone, Debug, Default)]
pub struct Game {
    pub name: String,
    /// `clone_of`, unless a `NOT_A_DRIVER` system set (`drivers::Driver::parent`).
    pub parent: Option<String>,
    pub description: String,
    pub year: String,
    pub manufacturer: String,
    pub sound_rom_id: Option<String>,
}

/// Where an entry's name comes from, in priority order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// The starting sounds.dat: the game's sections, its family's (`dcs:`, `wpcs:`), or
    /// the name the extraction took from it (the report's, or its `sounds_dat_name`).
    Dat,
    /// A names file (`--names`).
    Names,
    /// The report's own name, from a names file applied at extraction.
    Manifest,
    /// The command and the sound's length.
    Fallback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Named {
    pub name: String,
    pub source: Source,
}

/// One written sound of a report.
#[derive(Clone, Debug)]
struct Sound {
    /// As the report gives it (`0x7A05`).
    id: String,
    /// The id's bytes as a sounds.dat entry of the game names them (WPCS `7A xx` without
    /// the filler, System 3-7 without a trailing idle byte).
    key: Vec<u8>,
    /// What the commander sends, or why it cannot.
    bytes: Result<Vec<u8>, &'static str>,
    duration: f64,
    ended_by: String,
    dat_name: Option<String>,
    name: Option<String>,
}

/// One set's report, read.
#[derive(Clone, Debug)]
struct Measured {
    set: String,
    sound_rom_id: Option<String>,
    families: Vec<String>,
    two_boards: bool,
    sounds: Vec<Sound>,
}

/// What a report says about how its board takes commands, beyond its family name (the
/// sub-type, which only the running machine gives).
#[derive(Clone, Copy, Debug, Default)]
struct Hints {
    /// Game Plan MSU-1 program with a byte table (`gpsm_byte_table`): sent as the MSU-3's.
    gpsm_table: bool,
    /// Jac Van Ham, not Formula 1's sub-type: each command followed by the idle `3F`.
    jvh_idle: bool,
    /// System 3 World Cup / Disco Fever: `80` is the control line.
    s3_ctrl: bool,
    /// System 3-7 sub-type bits that set the idle byte (`s67s_idle`).
    s67s_sub: c_int,
    /// Baby Pac-Man's Cheap Squeak: data port and strobe.
    by45bp: bool,
}

impl Hints {
    fn of(m: &Value) -> Self {
        let notes: String = m["commands_from"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            gpsm_table: notes.contains("command table"),
            jvh_idle: notes.contains("idle 3F"),
            s3_ctrl: notes.contains("the control line"),
            s67s_sub: if notes.contains("7F = idle") {
                1
            } else if notes.contains("5F = idle") {
                4
            } else {
                0
            },
            by45bp: m["stop"].as_str().is_some_and(|s| s.contains("ctrl_w")),
        }
    }
}

/// The bytes PinMAME's commander sends for a command of `family` (`data`, the board's
/// bytes), as rom2altsound's `board_sends` frames them through `sndbrd_manCmd`.
fn framed(family: &str, data: &[u8], h: Hints) -> Result<Vec<u8>, &'static str> {
    Ok(match family {
        "DCS" if !data.len().is_multiple_of(2) => return Err("DCS: an odd number of bytes"),
        "DCSP2K" => return Err("Pinball 2000: the board takes 16-bit words"),
        // `sndbrd_manCmd` keeps a WPCS board's first byte and `wpcs_manCmd_w` writes the
        // pair: a lone byte would wait for the next command's.
        "WPCS" => match data {
            [x] => vec![0x00, *x],
            d if d.len().is_multiple_of(2) => d.to_vec(),
            _ => return Err("WPCS: an odd number of bytes after the first"),
        },
        "ZAC1370" => extract::zac_strobed(data),
        "SPINB" | "INDER" => extract::spinb_released(data),
        "GPSM" if h.gpsm_table => extract::gpsm3_framed(data),
        "GPSM" => extract::gpsm_framed(data),
        "GPSM3" => extract::gpsm3_framed(data),
        "JVH" if h.jvh_idle => extract::jvh_framed(data),
        "GTS80" | "GTS80SS" | "ZAC1346" => extract::gts80_released(data),
        "PLAYZ" => extract::playz_framed(data),
        "WMSS67" if h.s3_ctrl && data.contains(&extract::S3_CTRL) => {
            return Err("System 3: a pulse of the control line");
        }
        "WMSS67" => extract::s67s_framed(h.s67s_sub, data),
        "BYSNT" if data == [extract::BYSNT_BACKGROUND_ON] => {
            vec![extract::BYSNT_BACKGROUND_ON, extract::BYSNT_NOOP]
        }
        "TABART" => return Err("Tabart: the commands go through the data port"),
        "BY45" if h.by45bp => return Err("Baby Pac-Man's board: nibbles with a strobe"),
        "BY56" | "BY51N" => return Err("two nibbles on the same lines"),
        _ => data.to_vec(),
    })
}

/// Bytes a sounds.dat entry holds (snd_cmd.c `MAX_CMD_LENGTH`).
const MAX_CMD_BYTES: usize = 6;

/// The commander's bytes for a report's id: on two boards, the id is the board then its
/// bytes (a sweep) or already (board, byte) pairs (a sounds.dat entry), and each framed
/// byte goes out with its board.
fn commander_bytes(
    family: &str,
    id: &[u8],
    two_boards: bool,
    h: Hints,
) -> Result<Vec<u8>, &'static str> {
    let out = if two_boards {
        let (board, data): (u8, Vec<u8>) = match id {
            [b, rest @ ..] if id.len() >= 4 && id.len().is_multiple_of(2) && is_pairs(id) => {
                (*b, rest.iter().step_by(2).copied().collect())
            }
            [b, rest @ ..] if !rest.is_empty() => (*b, rest.to_vec()),
            _ => return Err("an id without its board"),
        };
        framed(family, &data, h)?
            .into_iter()
            .flat_map(|x| [board, x])
            .collect()
    } else {
        framed(family, id, h)?
    };
    if out.len() > MAX_CMD_BYTES {
        return Err("more than the 6 bytes of an entry");
    }
    Ok(out)
}

/// `bb xx bb yy ...`: every other byte the same board number.
fn is_pairs(id: &[u8]) -> bool {
    id[0] <= 1 && id.iter().step_by(2).all(|&b| b == id[0])
}

/// The key a sounds.dat entry's bytes stand for (as the extraction matches them,
/// `Extractor::list_key`).
fn key_of(bytes: &[u8], families: &[String], two_boards: bool, h: Hints) -> Vec<u8> {
    if two_boards {
        return bytes.to_vec();
    }
    let family = families.first().map_or("", String::as_str);
    let mut b = extract::wpcs_bank_of(family, bytes);
    if family == "WMSS67" {
        while b.len() > 1 && b.last().is_some_and(|&x| extract::s67s_idle(h.s67s_sub, x)) {
            b.pop();
        }
    }
    b
}

fn parse_hex(id: &str) -> Option<Vec<u8>> {
    let h = id.strip_prefix("0x").or_else(|| id.strip_prefix("0X"))?;
    if h.is_empty() || !h.len().is_multiple_of(2) || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// Why a report gives no entry at all, or the set read.
fn measured(m: &Value) -> Result<Measured, &'static str> {
    if !m["not_applicable"].is_null() {
        return Err("not applicable");
    }
    if !m["game_sound"].is_null() {
        return Err("game-driven: the board takes no sound command");
    }
    let Some(sounds) = m["sounds"].as_array() else {
        return Err("no sounds in the report");
    };
    let families: Vec<String> = m["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|b| b.as_str().map(str::to_owned))
        .collect();
    if !m["sam"].is_null() || families.iter().any(|f| f.starts_with("SAM")) {
        return Err("Stern SAM: no sound command");
    }
    let two_boards = families.len() == 2;
    let h = Hints::of(m);
    let mut out = Vec::new();
    for s in sounds.iter().filter(|s| !s["file"].is_null()) {
        let id = s["id"].as_str().unwrap_or_default().to_owned();
        let family = s["board"].as_str().unwrap_or_default();
        let (key, bytes) = match parse_hex(&id) {
            Some(raw) => {
                let key = key_of(&raw, &families, two_boards, h);
                let bytes = commander_bytes(family, &key, two_boards, h);
                (key, bytes)
            }
            None => (Vec::new(), Err("an id that is no command")),
        };
        let text = |k: &str| {
            s[k].as_str()
                .map(names::clean_label)
                .filter(|n| !n.is_empty())
        };
        // `sounds_dat_name` is there once names were applied (`names.rs`): `name` is then
        // the names file's. Without it, `name` is the one sounds.dat gave the extraction.
        let (dat_name, name) = if s.get("sounds_dat_name").is_some() {
            (text("sounds_dat_name"), text("name"))
        } else {
            (text("name"), None)
        };
        out.push(Sound {
            id,
            key,
            bytes,
            duration: s["duration"].as_f64().unwrap_or(0.0),
            ended_by: s["ended_by"].as_str().unwrap_or_default().to_owned(),
            dat_name,
            name,
        });
    }
    Ok(Measured {
        set: m["rom"].as_str().unwrap_or_default().to_owned(),
        sound_rom_id: m["sound_rom_id"].as_str().map(str::to_ascii_lowercase),
        families,
        two_boards,
        sounds: out,
    })
}

/// `Sound 2A (3.2 s)`, `Sound 2A (loop)`, `Sound 2A (120+ s)` (cut at the cap).
fn fallback_name(s: &Sound) -> String {
    let id = s.id.trim_start_matches("0x").trim_start_matches("0X");
    let len = match s.ended_by.as_str() {
        "loop" => "loop".to_owned(),
        "max" => format!("{:.0}+ s", s.duration),
        _ if s.duration < 10.0 => format!("{:.1} s", s.duration),
        _ => format!("{:.0} s", s.duration),
    };
    format!("Sound {} ({len})", id.to_ascii_uppercase())
}

/// Whether a name read from the file (`read`, cut to 29 characters by the commander) is
/// `name`, spacing aside.
fn same_name(read: &str, name: &str) -> bool {
    let shown = |n: &str| -> String {
        names::clean_label(&n.chars().take(MAX_NAME_CHARS).collect::<String>())
    };
    let (a, b) = (shown(read), shown(name));
    a == b || (!a.is_empty() && names::clean_label(name).starts_with(&a))
}

/// Lowercase hex, the only case `readCmds` reads.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A line cut to what `mame_fgets` reads whole (a longer one would go on as a line of its
/// own, read as a heading).
fn fit(mut line: String) -> String {
    if line.len() > MAX_LINE_CHARS {
        let mut cut = MAX_LINE_CHARS;
        while !line.is_char_boundary(cut) {
            cut -= 1;
        }
        line.truncate(cut);
        line.truncate(line.trim_end().len());
    }
    line
}

/// `:hex:name`, the name cut so that the line fits.
fn entry_line(bytes: &[u8], name: &str) -> String {
    fit(format!(":{}:{name}", hex(bytes)))
}

/// Everything the generation reads.
pub struct Inputs<'a> {
    /// The sounds.dat to start from.
    pub base: &'a str,
    /// PinMAME's games (driver table).
    pub games: &'a [Game],
    /// manifest.json per set name.
    pub manifests: &'a HashMap<String, Value>,
    /// Names files, a later one's names winning.
    pub names: &'a [Names],
    /// For the top comment.
    pub version: &'a str,
}

/// What the generation did.
#[derive(Debug, Default)]
pub struct Stats {
    pub reports: usize,
    /// Reports left out entirely, by reason.
    pub sets_left_out: BTreeMap<String, usize>,
    /// Written sounds of the reports read that can get no entry, by reason.
    pub sounds_left_out: BTreeMap<String, usize>,
    pub sections: usize,
    pub clone_sections: usize,
    pub entries: usize,
    pub by_source: BTreeMap<String, usize>,
    /// Games given an entry list (their own report, or one of their sound ROM id).
    pub games: usize,
    /// Commands a game should list that the commander would not find, once the output is
    /// read back as PinMAME reads it (the entries held back, see `held_back`).
    pub missing: usize,
    /// Commands whose real name differs, read back (another section's name won).
    pub misnamed: usize,
    /// A few of them: game, command, the name wanted, the name read.
    pub misnamed_examples: Vec<String>,
    /// Games that also read a generated section of another game (a heading that starts
    /// with their name): the game, the sections, and how many commands they add that the
    /// game's own list does not have.
    pub foreign: Vec<(String, Vec<String>, usize)>,
    /// Entries a game needs that are left out because another game would read them too
    /// (its name, or its parent's, is the start of this game's name).
    pub held_back: usize,
    /// The games concerned: name, entries held back, the other games.
    pub held_back_sections: Vec<(String, usize, Vec<String>)>,
    /// Lines of the starting file not found in the output (should be 0).
    pub base_lines_lost: usize,
}

pub struct Output {
    pub text: String,
    pub stats: Stats,
}

/// What a game should list: entry bytes and names.
type Wanted = BTreeMap<Vec<u8>, Named>;

/// A generated section: its heading (a game's name) and its entries.
type Section = (String, Vec<(Vec<u8>, Named)>);

/// A game's entry list from a set's report (its own, or one of the same sound ROMs).
fn wanted(g: &Game, m: &Measured, dat: &SoundsDat, names: &[&Names]) -> Wanted {
    let h = Hints::default();
    let mut from_dat: HashMap<Vec<u8>, String> = HashMap::new();
    if !m.two_boards {
        for f in &m.families {
            for e in dat.family_entries(f) {
                from_dat.insert(key_of(&e.bytes, &m.families, false, h), e.name);
            }
        }
    }
    for e in dat.game_entries(&g.name, g.parent.as_deref()) {
        from_dat.insert(key_of(&e.bytes, &m.families, m.two_boards, h), e.name);
    }
    let mut from_names: HashMap<String, String> = HashMap::new();
    for n in names {
        for r in &n.rows {
            let name = names::clean_label(&r.name);
            if !name.is_empty() {
                from_names.insert(names::norm_id(&r.id), name);
            }
        }
    }
    let mut out = Wanted::new();
    for s in &m.sounds {
        let Ok(bytes) = &s.bytes else { continue };
        let named = if let Some(n) = from_dat
            .get(&s.key)
            .map(|n| names::clean_label(n))
            .filter(|n| !n.is_empty())
            .or_else(|| s.dat_name.clone())
        {
            Named {
                name: n,
                source: Source::Dat,
            }
        } else if let Some(n) = from_names.get(&names::norm_id(&s.id)) {
            Named {
                name: n.clone(),
                source: Source::Names,
            }
        } else if let Some(n) = &s.name {
            Named {
                name: n.clone(),
                source: Source::Manifest,
            }
        } else {
            Named {
                name: fallback_name(s),
                source: Source::Fallback,
            }
        };
        out.entry(bytes.clone()).or_insert(named);
    }
    out
}

/// Whether a names file is for a game: same sound ROM id, else (no id in the file) its
/// `# rom:` is the game or the set measured.
fn names_for<'a>(all: &'a [Names], g: &Game, m: &Measured) -> Vec<&'a Names> {
    let id = g.sound_rom_id.as_ref().or(m.sound_rom_id.as_ref());
    all.iter()
        .filter(|n| match (&n.sound_rom_id, id) {
            (Some(a), Some(b)) => a == b,
            (Some(_), None) => false,
            (None, _) => n.rom.as_ref().is_some_and(|r| *r == g.name || *r == m.set),
        })
        .collect()
}

pub fn generate(inp: &Inputs) -> Output {
    let mut stats = Stats {
        reports: inp.manifests.len(),
        ..Stats::default()
    };
    let dat = SoundsDat::parse(inp.base);
    let base = PinmameDat::new(inp.base);
    // The reports, by set and by sound ROM id.
    let mut by_set: BTreeMap<String, Measured> = BTreeMap::new();
    // Sets that take no sound command (game-driven, Stern SAM, nothing to extract), and
    // their sound ROM ids: no list of theirs to protect.
    let mut silent: BTreeSet<String> = BTreeSet::new();
    let mut silent_ids: BTreeSet<String> = BTreeSet::new();
    for (set, v) in inp.manifests {
        match measured(v) {
            Ok(mut m) => {
                if m.set.is_empty() {
                    m.set = set.clone();
                }
                by_set.insert(set.clone(), m);
            }
            Err(why) => {
                *stats.sets_left_out.entry(why.to_owned()).or_default() += 1;
                silent.insert(set.clone());
                if let Some(id) = v["sound_rom_id"].as_str() {
                    silent_ids.insert(id.to_ascii_lowercase());
                }
            }
        }
    }
    let mut by_id: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for m in by_set.values() {
        if let Some(id) = &m.sound_rom_id {
            by_id.entry(id).or_default().push(&m.set);
        }
    }
    // Every game: the driver table's, and any set reported that it does not know.
    let mut games: BTreeMap<String, Game> = inp
        .games
        .iter()
        .map(|g| (g.name.clone(), g.clone()))
        .collect();
    for (set, v) in inp.manifests {
        games.entry(set.clone()).or_insert_with(|| Game {
            name: set.clone(),
            parent: v["parent"].as_str().map(str::to_owned),
            sound_rom_id: v["sound_rom_id"].as_str().map(str::to_ascii_lowercase),
            ..Game::default()
        });
    }
    // The set whose report a game takes: its own, else its parent's or the first of its
    // sound ROM id.
    let rep = |g: &Game| -> Option<&Measured> {
        if let Some(m) = by_set.get(&g.name) {
            return Some(m);
        }
        let sets = by_id.get(g.sound_rom_id.as_deref()?)?;
        let pick = g
            .parent
            .as_deref()
            .filter(|p| sets.contains(p))
            .unwrap_or(sets[0]);
        by_set.get(pick)
    };
    let mut want: BTreeMap<String, (Wanted, &Measured)> = BTreeMap::new();
    let mut used: BTreeSet<&str> = BTreeSet::new();
    for g in games.values() {
        let Some(m) = rep(g) else { continue };
        let w = wanted(g, m, &dat, &names_for(inp.names, g, m));
        if !w.is_empty() {
            used.insert(&m.set);
            want.insert(g.name.clone(), (w, m));
        }
    }
    for m in by_set.values() {
        for s in &m.sounds {
            if let Err(why) = s.bytes {
                *stats.sounds_left_out.entry(why.to_owned()).or_default() += 1;
            }
        }
    }
    stats.games = want.len();
    // Heads first, each followed by its clones.
    let head_of = |name: &str| -> String {
        games
            .get(name)
            .and_then(|g| g.parent.clone())
            .unwrap_or_else(|| name.to_owned())
    };
    let mut order: Vec<&String> = want.keys().collect();
    order.sort_by_key(|n| (head_of(n), games[*n].parent.is_some(), (*n).clone()));
    let mut sections: Vec<Section> = Vec::new();
    let mut by_heading: HashMap<String, usize> = HashMap::new();
    for name in order {
        let g = &games[name];
        let (w, _) = &want[name];
        let parent = g.parent.as_deref();
        // What the commander already lists for the game: PinMAME's sections (whose names
        // come first anyway), then the parent's generated one (a clone's own real name
        // wins over it).
        let upstream: BTreeSet<Vec<u8>> = base
            .entries(name, parent)
            .into_iter()
            .map(|(e, _)| e.bytes)
            .collect();
        let from_parent: HashMap<&Vec<u8>, &Named> = parent
            .and_then(|p| by_heading.get(p))
            .map(|&i| sections[i].1.iter().map(|(b, n)| (b, n)).collect())
            .unwrap_or_default();
        let needed: Vec<(&Vec<u8>, &Named)> = w
            .iter()
            .filter(|(b, n)| {
                !upstream.contains(*b)
                    && from_parent.get(b).is_none_or(|p| {
                        n.source != Source::Fallback && !same_name(&p.name, &n.name)
                    })
            })
            .collect();
        // The other games that read this heading too (`readCmds` takes every heading that
        // starts with the game's name or its parent's): an entry goes in only if each of
        // them lists it already, with the same name. The game's own clones are meant to,
        // and a game that takes no sound command has nothing to lose.
        let others: Vec<&str> = games
            .values()
            .filter(|x| {
                x.name != *name
                    && x.parent.as_deref() != Some(name.as_str())
                    && !silent.contains(&x.name)
                    && !x
                        .sound_rom_id
                        .as_ref()
                        .is_some_and(|i| silent_ids.contains(i))
                    && (name.starts_with(&x.name)
                        || x.parent.as_deref().is_some_and(|p| name.starts_with(p)))
            })
            .map(|x| x.name.as_str())
            .collect();
        let (list, held): (Vec<_>, Vec<_>) = needed.into_iter().partition(|(b, n)| {
            others.iter().all(|x| {
                want.get(*x)
                    .and_then(|(xw, _)| xw.get(*b))
                    .is_some_and(|xn| {
                        same_name(&xn.name, &n.name)
                            || (xn.source == Source::Fallback && n.source == Source::Fallback)
                    })
            })
        });
        if !held.is_empty() {
            stats.held_back += held.len();
            stats.held_back_sections.push((
                name.clone(),
                held.len(),
                others.iter().map(|x| x.to_string()).collect(),
            ));
        }
        let list: Vec<(Vec<u8>, Named)> = list
            .into_iter()
            .map(|(b, n)| (b.clone(), n.clone()))
            .collect();
        if list.is_empty() {
            continue;
        }
        by_heading.insert(name.clone(), sections.len());
        sections.push((name.clone(), list));
    }
    // The text: PinMAME's file, then the sections.
    let mut text = format!(
        "; Generated by rom2altsound {} (rom2altsound sounds-dat) from the reports of {} sets.\n\
         ; PinMAME's sounds.dat follows unchanged, then one section per game measured: every\n\
         ; command that produced a sound, with the bytes the commander sends on that board.\n\
         ;-----------------------------------------------------------------------\n",
        inp.version,
        used.len()
    );
    text.push_str(inp.base);
    if !inp.base.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(
        "\n;-----------------------------------------------------------------------\n\
         ; rom2altsound sections: a game's commands that the sections above do not give it\n\
         ; (a clone: only what its parent's section does not), sorted by command.\n\
         ;-----------------------------------------------------------------------\n",
    );
    for (name, list) in &sections {
        let g = &games[name];
        let (_, m) = &want[name];
        text.push('\n');
        text.push_str(&section_comment(g, m, list.len()));
        text.push_str(name);
        text.push_str(":\n");
        for (b, n) in list {
            text.push_str(&entry_line(b, &n.name));
            text.push('\n');
            *stats
                .by_source
                .entry(format!("{:?}", n.source))
                .or_default() += 1;
        }
        stats.entries += list.len();
        stats.sections += 1;
        if g.parent.is_some() {
            stats.clone_sections += 1;
        }
    }
    // Read back as PinMAME reads it.
    let back = PinmameDat::new(&text);
    let generated: BTreeSet<&str> = sections.iter().map(|(n, _)| n.as_str()).collect();
    for (name, (w, _)) in &want {
        let parent = games[name].parent.as_deref();
        let got: HashMap<Vec<u8>, (String, String)> = back
            .entries(name, parent)
            .into_iter()
            .map(|(e, h)| (e.bytes, (e.name, h)))
            .collect();
        let mut foreign: BTreeSet<String> = BTreeSet::new();
        for (b, n) in w {
            match got.get(b) {
                None => stats.missing += 1,
                Some((got_name, _)) => {
                    if n.source != Source::Fallback && !same_name(got_name, &n.name) {
                        stats.misnamed += 1;
                        if stats.misnamed_examples.len() < 12 {
                            stats.misnamed_examples.push(format!(
                                "{name} {}: {:?} read as {:?}",
                                hex(b),
                                n.name,
                                got_name
                            ));
                        }
                    }
                }
            }
        }
        let mut extra = 0;
        for (b, (_, h)) in &got {
            let h = heading_name(h);
            if generated.contains(h) && h != name && Some(h) != parent {
                foreign.insert(h.to_owned());
                extra += usize::from(!w.contains_key(b));
            }
        }
        if !foreign.is_empty() {
            stats
                .foreign
                .push((name.clone(), foreign.into_iter().collect(), extra));
        }
    }
    let mut lines: HashMap<&str, usize> = HashMap::new();
    for l in text.lines() {
        *lines.entry(l).or_default() += 1;
    }
    for l in inp.base.lines() {
        match lines.get_mut(l) {
            Some(c) if *c > 0 => *c -= 1,
            _ => stats.base_lines_lost += 1,
        }
    }
    Output { text, stats }
}

/// `afm_113:` -> `afm_113`.
fn heading_name(h: &str) -> &str {
    h.trim_end().trim_end_matches(':')
}

/// `; Attack From Mars (1.13 / S1.1) (1999, Bally): DCS, 1 entry (measured on afm_113b)`.
fn section_comment(g: &Game, m: &Measured, n: usize) -> String {
    let mut about = String::new();
    if !g.description.is_empty() {
        about.push_str(&g.description);
        let made: Vec<&str> = [g.year.as_str(), g.manufacturer.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        if !made.is_empty() {
            about.push_str(&format!(" ({})", made.join(", ")));
        }
        about.push_str(": ");
    }
    let from = if m.set == g.name {
        String::new()
    } else {
        format!(" (measured on {})", m.set)
    };
    let mut c = fit(format!(
        "; {about}{}, {n} entr{}{from}",
        m.families.join(" + "),
        if n == 1 { "y" } else { "ies" }
    ));
    c.push('\n');
    let left: usize = m.sounds.iter().filter(|s| s.bytes.is_err()).count();
    if left > 0 {
        let why: BTreeSet<&str> = m
            .sounds
            .iter()
            .filter_map(|s| s.bytes.as_ref().err().copied())
            .collect();
        c.push_str(&fit(format!(
            "; {left} sound(s) left out: {}",
            why.into_iter().collect::<Vec<_>>().join("; ")
        )));
        c.push('\n');
    }
    c
}

/// Names files from the paths given: files, and the *.csv files of folders.
fn load_names(paths: &[PathBuf]) -> (Vec<Names>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for p in paths {
        let files: Vec<PathBuf> = if p.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(p)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|f| {
                            f.is_file()
                                && f.extension().is_some_and(|x| x.eq_ignore_ascii_case("csv"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        } else {
            vec![p.clone()]
        };
        for f in files {
            match names::load(&f) {
                Ok(n) if n.sound_rom_id.is_none() && n.rom.is_none() => errors.push(format!(
                    "{}: no `# rom:` nor `# sound_rom_id:` line, left out",
                    f.display()
                )),
                Ok(n) => out.push(n),
                Err(e) => errors.push(e),
            }
        }
    }
    (out, errors)
}

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

pub fn cli(args: Vec<String>) -> i32 {
    let cli =
        SoundsDatCli::parse_from(std::iter::once("rom2altsound sounds-dat".to_owned()).chain(args));
    let base = match &cli.sounds_dat {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("error: {}: {e}", p.display());
                return 1;
            }
        },
        None => soundsdat::BUILT_IN.to_owned(),
    };
    let games: Vec<Game> = crate::drivers::load()
        .into_iter()
        .filter(|d| !d.system)
        .map(|d| Game {
            sound_rom_id: d.sound_rom_id(),
            name: d.name,
            parent: d.parent,
            description: d.description,
            year: d.year,
            manufacturer: d.manufacturer,
        })
        .collect();
    let (runs, errors) = crate::catalog::read_runs(&cli.manifests);
    for e in &errors {
        eprintln!("warning: {e}");
    }
    let manifests: HashMap<String, Value> = runs
        .into_iter()
        .filter_map(|(k, r)| Some((k, r.manifest?)))
        .collect();
    let (names, errors) = load_names(&cli.names);
    for e in &errors {
        eprintln!("warning: {e}");
    }
    let out = generate(&Inputs {
        base: &base,
        games: &games,
        manifests: &manifests,
        names: &names,
        version: env!("CARGO_PKG_VERSION"),
    });
    if let Err(e) = write(&cli.out, &out.text) {
        eprintln!("error: {}: {e}", cli.out.display());
        return 1;
    }
    let s = &out.stats;
    eprintln!(
        "{}: {} sections ({} for clones), {} entries, for {} games, from {} reports ({} names files)",
        cli.out.display(),
        s.sections,
        s.clone_sections,
        s.entries,
        s.games,
        s.reports,
        names.len()
    );
    for (k, v) in &s.by_source {
        eprintln!("  names from {k}: {v}");
    }
    for (k, v) in &s.sets_left_out {
        eprintln!("  reports left out ({k}): {v}");
    }
    for (k, v) in &s.sounds_left_out {
        eprintln!("  sounds left out ({k}): {v}");
    }
    eprintln!(
        "  read back as PinMAME reads it: {} missing, {} with another name, {} games reading another game's section{}",
        s.missing,
        s.misnamed,
        s.foreign.len(),
        if s.foreign.is_empty() {
            String::new()
        } else {
            format!(
                " ({})",
                s.foreign
                    .iter()
                    .map(|(g, h, n)| format!("{g} < {}: {n} extra", h.join(" ")))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    );
    for e in &s.misnamed_examples {
        eprintln!("    {e}");
    }
    eprintln!(
        "  held back (another game would read them too): {} entries of {} games",
        s.held_back,
        s.held_back_sections.len()
    );
    for (g, n, o) in &s.held_back_sections {
        eprintln!("    {g}: {n} (also read by {})", o.join(" "));
    }
    eprintln!(
        "  lines of the starting sounds.dat not in the output: {}",
        s.base_lines_lost
    );
    i32::from(s.missing > s.held_back || s.base_lines_lost > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sound(id: &str, board: &str, secs: f64, ended_by: &str, name: &str) -> Value {
        json!({"id": id, "board": board, "file": format!("{id}.wav"), "duration": secs,
               "ended_by": ended_by, "name": name})
    }

    fn report(rom: &str, id: &str, boards: &[&str], sounds: Vec<Value>) -> Value {
        json!({"rom": rom, "sound_rom_id": id, "boards": boards, "commands_from": [],
               "sounds": sounds})
    }

    fn game(name: &str, parent: Option<&str>, id: &str) -> Game {
        Game {
            name: name.into(),
            parent: parent.map(Into::into),
            description: format!("{name} game"),
            year: "1995".into(),
            manufacturer: "Maker".into(),
            sound_rom_id: Some(id.into()),
        }
    }

    fn run(base: &str, games: &[Game], reports: Vec<Value>, names: &[Names]) -> Output {
        let manifests: HashMap<String, Value> = reports
            .into_iter()
            .map(|r| (r["rom"].as_str().unwrap().to_owned(), r))
            .collect();
        generate(&Inputs {
            base,
            games,
            manifests: &manifests,
            names,
            version: "9.9.9",
        })
    }

    /// The generated part: from the first heading after the banner.
    fn generated(text: &str) -> &str {
        &text[text.find("; rom2altsound sections").unwrap()..]
    }

    #[test]
    fn bytes_as_the_commander_sends_them() {
        let h = Hints::default();
        // DCS: two bytes per command, never an odd count.
        assert_eq!(
            commander_bytes("DCS", &[0x01, 0x86], false, h),
            Ok(vec![0x01, 0x86])
        );
        assert!(commander_bytes("DCS", &[0x01], false, h).is_err());
        // WPCS: the manual command takes pairs.
        assert_eq!(
            commander_bytes("WPCS", &[0x2A], false, h),
            Ok(vec![0x00, 0x2A])
        );
        assert_eq!(
            commander_bytes("WPCS", &[0x7A, 0x05], false, h),
            Ok(vec![0x7A, 0x05])
        );
        // Two boards: (board, byte) pairs, from a sweep id or a sounds.dat one.
        assert_eq!(
            commander_bytes("WMSS11C", &[0x01, 0x22], true, h),
            Ok(vec![0x01, 0x22])
        );
        assert_eq!(
            commander_bytes("WMSS11", &[0x00, 0x7A, 0x05], true, h),
            Ok(vec![0x00, 0x7A, 0x00, 0x05])
        );
        assert_eq!(
            commander_bytes("WMSS11", &[0x01, 0x7A, 0x01, 0x05], true, h),
            Ok(vec![0x01, 0x7A, 0x01, 0x05])
        );
        // Framing: release, strobe, idle bytes.
        assert_eq!(
            commander_bytes("GTS80", &[0x03], false, h),
            Ok(vec![0x03, 0x00])
        );
        assert_eq!(
            commander_bytes("ZAC1370", &[0xFE], false, h),
            Ok(vec![0x7E, 0xFE, 0x7E])
        );
        assert_eq!(
            commander_bytes("WMSS67", &[0x05], false, h),
            Ok(vec![0xFF, 0x05, 0xFF])
        );
        // What the manual command cannot send.
        let capcom = [0xDA, 0x04, 0x07, 0x0F, 0x00, 0x01];
        assert_eq!(
            commander_bytes("CAPCOMS", &capcom, false, h),
            Ok(capcom.to_vec())
        );
        assert!(commander_bytes("CAPCOMS", &[&[0][..], &capcom].concat(), true, h).is_err());
        assert!(commander_bytes("TABART", &[0x01], false, h).is_err());
        assert!(commander_bytes("DCSP2K", &[0x00, 0x01], false, h).is_err());
        assert_eq!(entry_line(&[0x7A, 0xB0], "x"), ":7ab0:x");
    }

    #[test]
    fn a_clone_gets_only_what_its_parent_lacks() {
        let games = [
            game("pg_10", None, "aa"),
            game("pg_09", Some("pg_10"), "aa"),
            game("pg_fr", Some("pg_10"), "bb"),
        ];
        let reports = vec![
            report(
                "pg_10",
                "aa",
                &["DCS"],
                vec![
                    sound("0x0001", "DCS", 1.25, "silence", ""),
                    sound("0x0002", "DCS", 30.0, "loop", ""),
                ],
            ),
            // The French sound ROM: one more command.
            report(
                "pg_fr",
                "bb",
                &["DCS"],
                vec![
                    sound("0x0001", "DCS", 1.5, "silence", ""),
                    sound("0x0003", "DCS", 2.0, "end", ""),
                ],
            ),
        ];
        let out = run("", &games, reports, &[]);
        let g = generated(&out.text);
        // The parent's section, then the clone's: only 0003 (0001's name is a fallback
        // either way); pg_09 shares the parent's sound ROMs: no section.
        let parent = g.find("\npg_10:\n:0001:Sound 0001 (1.2 s)\n:0002:Sound 0002 (loop)\n");
        let clone = g.find("\npg_fr:\n:0003:Sound 0003 (2.0 s)\n");
        assert!(parent.is_some() && clone.is_some() && parent < clone, "{g}");
        assert!(!g.contains("pg_09:"));
        assert_eq!(out.stats.sections, 2);
        assert_eq!(out.stats.clone_sections, 1);
        assert_eq!(out.stats.missing, 0);
        assert!(g.contains("; pg_10 game (1995, Maker): DCS, 2 entries\n"));
    }

    #[test]
    fn names_in_priority_order() {
        let base = "dcs:\n:03d4:Menu Select A\n\npg_10:\n:0001:Music: Theme\n";
        let games = [
            game("pg_10", None, "aa"),
            game("pg_11", Some("pg_10"), "aa"),
        ];
        let reports = vec![report(
            "pg_11",
            "aa",
            &["DCS"],
            vec![
                sound("0x0001", "DCS", 3.0, "end", "Music: Theme"),
                sound("0x0002", "DCS", 1.0, "end", ""),
                sound("0x0003", "DCS", 1.0, "end", "From the run's sounds.dat"),
                sound("0x0004", "DCS", 120.0, "max", ""),
                sound("0x03D4", "DCS", 0.5, "end", ""),
                sound("0x0005", "DCS", 12.0, "silence", ""),
            ],
        )];
        let names = names::parse(
            "# rom: pg_11\n# sound_rom_id: aa\nID,NAME\n0x0001,Typed theme\n0x0002,Typed two\n",
        )
        .unwrap();
        let out = run(base, &games, reports, &[names]);
        let g = generated(&out.text);
        // 0001 is in PinMAME's section already: not written again, its name kept.
        assert!(!g.contains(":0001:"), "{g}");
        assert!(g.contains(":0002:Typed two\n"));
        assert!(g.contains(":0003:From the run's sounds.dat\n"));
        assert!(g.contains(":0004:Sound 0004 (120+ s)\n"));
        assert!(g.contains(":03d4:Menu Select A\n"));
        assert!(g.contains(":0005:Sound 0005 (12 s)\n"));
        // The game measured is a clone: the parent's section holds the list.
        assert!(g.contains("\npg_10:\n"));
        assert!(!g.contains("\npg_11:\n"));
        assert_eq!(out.stats.by_source.get("Names"), Some(&1));
        assert_eq!(out.stats.by_source.get("Dat"), Some(&2));
        assert_eq!(out.stats.by_source.get("Fallback"), Some(&2));
    }

    #[test]
    fn every_line_of_the_starting_file_is_kept() {
        // Uppercase hex and an entry after an empty line: PinMAME reads neither.
        let base = "; header\n;---\nlotr:\n:fc01:One\n:fcA0:Duck\n\n:fc02:Two\n";
        let games = [game("lotr", None, "aa")];
        let reports = vec![report(
            "lotr",
            "aa",
            &["AT91"],
            vec![
                sound("0xFC01", "AT91", 1.0, "end", ""),
                sound("0xFCA0", "AT91", 1.0, "end", ""),
                sound("0xFC02", "AT91", 1.0, "end", ""),
            ],
        )];
        let out = run(base, &games, reports, &[]);
        assert!(out.text.contains(base));
        assert_eq!(out.stats.base_lines_lost, 0);
        assert!(out.text.starts_with("; Generated by rom2altsound 9.9.9"));
        let g = generated(&out.text);
        assert!(!g.contains(":fc01:"));
        assert!(g.contains("\nlotr:\n:fc02:Two\n:fca0:Duck\n"), "{g}");
        assert_eq!(out.stats.missing, 0);
    }

    #[test]
    fn a_heading_another_game_reads_too_is_held_back() {
        // PinMAME's `cue` reads every heading that starts with "cue".
        let games = [game("cue", None, "aa"), game("cueball", None, "bb")];
        let reports = vec![
            report(
                "cue",
                "aa",
                &["BY45"],
                vec![sound("0x01", "BY45", 1.0, "end", "")],
            ),
            report(
                "cueball",
                "bb",
                &["GTS80B"],
                vec![
                    sound("0x01", "GTS80B", 1.0, "end", ""),
                    sound("0x02", "GTS80B", 1.0, "end", ""),
                ],
            ),
        ];
        let out = run("", &games, reports, &[]);
        let g = generated(&out.text);
        // 01 is in cue's list too (a fallback name either way); 02 is not.
        assert!(g.contains("\ncueball:\n:01:Sound 01 (1.0 s)\n"), "{g}");
        assert!(!g.contains(":02:"));
        assert_eq!(out.stats.held_back, 1);
        assert_eq!(out.stats.missing, 1);
    }

    #[test]
    fn read_as_pinmame_reads_it() {
        let long = format!(":0003:{}", "x".repeat(120));
        let text = format!(
            "afm_113:\n:0001:One\n#c\n:0002:Two\n{long}\n:0004:Four\n\n:0005:Five\nafm_113b:\n:0001:Uno\n"
        );
        let d = PinmameDat::new(&text);
        let e = d.entries("afm_113", None);
        let got: Vec<(Vec<u8>, &str)> = e
            .iter()
            .map(|(e, _)| (e.bytes.clone(), e.name.as_str()))
            .collect();
        // The cut line's rest is a heading: 0004 is lost, so is 0005 after the empty line.
        // afm_113b's heading starts with afm_113: its name wins for 0001.
        assert_eq!(
            got,
            vec![
                (vec![0x00, 0x01], "Uno"),
                (vec![0x00, 0x02], "Two"),
                (vec![0x00, 0x03], "xxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
            ]
        );
    }
}
