//! Replacement sounds: the user's own files played instead of the ROM's, one sound at a
//! time, without touching the files extracted from the ROM.
//!
//! A pack folder keeps the ROM's files as they were extracted. The user's files go in its
//! `replace/` folder, and `replacements.csv` lists what replaces what:
//!
//! ```text
//! ID,FNAME,GAIN,SOURCE
//! 0x000A,replace/0x000A-my jackpot.ogg,63,my jackpot.ogg
//! 0x0012,,50,
//! ```
//!
//! FNAME is the file AltSound plays for that sound (empty: the ROM's own file), GAIN its
//! volume (0 to 100, as in the CSVs: AltSound can only lower a sound), SOURCE the name of the
//! file the user picked, for the record. A row with an empty FNAME only changes the volume.
//!
//! `replacements.csv` is the record; altsound.csv and g-sound.csv follow it: on the rows of
//! a listed sound, and on the rows that play the same command under another id (the
//! Whitestar `0xFDxx` rows, the System 11 board 1 rows `0x00xx`), FNAME and GAIN are
//! rewritten; every other column and every other row stays as it is. Removing a sound from
//! the list puts its rows back to the ROM's file at GAIN 100.
//!
//! An extraction into a folder that has a `replacements.csv` applies it again to the new
//! CSVs (the ROM's files may have changed; the user's files have not), and says which
//! listed sounds the new pack no longer has.
//!
//! libaltsound decodes WAV, MP3, FLAC and Ogg Vorbis (miniaudio with stb_vorbis); Ogg Opus
//! does not play.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::Value;

use crate::altsound::{ALTSOUND_CSV, GSOUND_CSV, parse_id};

/// The record of the replacements, in the pack folder.
pub const FILE: &str = "replacements.csv";
/// The folder of the user's files, in the pack folder.
pub const DIR: &str = "replace";
/// The extensions AltSound plays.
pub const EXTENSIONS: [&str; 4] = ["wav", "ogg", "mp3", "flac"];
const HEADER: &str = "ID,FNAME,GAIN,SOURCE";

/// One row of `replacements.csv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: u32,
    /// The file AltSound plays (`replace/...`), or empty for the ROM's own file.
    pub fname: String,
    /// 0 to 100.
    pub gain: u32,
    /// The name of the file the user picked.
    pub source: String,
}

/// What applying the record did.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Sounds playing a file of the user's.
    pub replaced: usize,
    /// Sounds with only their volume changed.
    pub gain_only: usize,
    /// CSV rows rewritten (both files).
    pub rows: usize,
    /// Listed sounds this pack has no row for (`0x000A`).
    pub missing: Vec<String>,
    /// Listed files not found in `replace/`: those sounds play the ROM's file.
    pub missing_files: Vec<String>,
}

fn id_str(id: u32) -> String {
    format!("0x{id:04X}")
}

/// A file name as the CSVs can hold it: no comma, no quote, no path, nothing a file system
/// would refuse; at most 80 characters.
pub fn clean_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let s: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " _.()+-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim().trim_start_matches('.').trim();
    s.chars().take(80).collect()
}

/// The name a picked file gets in `replace/`: `<ID>-<its name>`.
pub fn stored_name(id: u32, source: &str) -> Result<String, String> {
    let clean = clean_file_name(source);
    let ext = Path::new(&clean)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "{source}: AltSound plays .wav, .ogg (Vorbis), .mp3 and .flac files, not this one"
        ));
    }
    Ok(format!("{DIR}/{}-{clean}", id_str(id)))
}

/// Parses `replacements.csv`.
pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'));
    let Some((_, header)) = lines.next() else {
        return Ok(Vec::new());
    };
    let cols: Vec<String> = header
        .split(',')
        .map(|c| c.trim().to_ascii_uppercase())
        .collect();
    let col = |h: &str| cols.iter().position(|c| c == h);
    let (Some(idc), Some(fc), Some(gc)) = (col("ID"), col("FNAME"), col("GAIN")) else {
        return Err(format!("{FILE}: the header is not {HEADER}"));
    };
    let sc = col("SOURCE");
    let mut out: Vec<Entry> = Vec::new();
    for (n, l) in lines {
        let f: Vec<&str> = l.split(',').map(str::trim).collect();
        let at = |i: usize| f.get(i).copied().unwrap_or("");
        let line = n + 1;
        let id = parse_id(at(idc)).ok_or_else(|| format!("{FILE} line {line}: no ID"))?;
        let gain: f64 = at(gc)
            .parse()
            .map_err(|_| format!("{FILE} line {line}: GAIN is not a number"))?;
        let fname = at(fc).replace('\\', "/");
        if !fname.is_empty() && !safe_member(&fname) {
            return Err(format!(
                "{FILE} line {line}: {fname} is not a file of the {DIR}/ folder"
            ));
        }
        let e = Entry {
            id,
            fname,
            gain: gain.round().clamp(0.0, 100.0) as u32,
            source: sc.map(at).unwrap_or("").to_owned(),
        };
        // The last row of an id wins.
        out.retain(|x| x.id != id);
        out.push(e);
    }
    out.sort_by_key(|e| e.id);
    Ok(out)
}

/// `replace/<name>` with a plain file name (no other folder, no `..`).
fn safe_member(path: &str) -> bool {
    path.strip_prefix(DIR)
        .and_then(|r| r.strip_prefix('/'))
        .is_some_and(|n| !n.is_empty() && clean_file_name(n) == n)
}

/// The `replacements.csv` text.
pub fn format(entries: &[Entry]) -> String {
    let mut s = format!("{HEADER}\r\n");
    for e in entries {
        s.push_str(&format!(
            "{},{},{},{}\r\n",
            id_str(e.id),
            e.fname,
            e.gain,
            clean_file_name(&e.source)
        ));
    }
    s
}

/// The record of a pack folder (empty without one).
pub fn load(dir: &Path) -> Result<Vec<Entry>, String> {
    let p = dir.join(FILE);
    match std::fs::read_to_string(&p) {
        Ok(t) => parse(&t).map_err(|e| format!("{}: {e}", dir.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// The pack's sounds that have rows: id -> the ROM's file their rows play.
pub fn factory_files(m: &Value) -> BTreeMap<u32, String> {
    m["sounds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let id = parse_id(s["id"].as_str()?)?;
            Some((id, s["pack"]["file"].as_str()?.to_owned()))
        })
        .collect()
}

/// The sound a CSV row plays: its own id, or for a row of the same command under another id
/// (see `altsound::whitestar_aliases` and `altsound::system11_board1_aliases`), the id of
/// the sound it was copied from.
pub fn owner(row: u32, sounds: &BTreeMap<u32, String>) -> Option<u32> {
    if sounds.contains_key(&row) {
        return Some(row);
    }
    let from = if row & 0xFF00 == 0xFD00 {
        row & 0xFF
    } else if row <= 0xFF {
        0x0100 | row
    } else {
        return None;
    };
    sounds.contains_key(&from).then_some(from)
}

/// The ids of the CSV rows of each sound (its own and its copies), from altsound.csv or
/// g-sound.csv text.
pub fn rows_of(csv: &str, sounds: &BTreeMap<u32, String>) -> BTreeMap<u32, Vec<u32>> {
    let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for l in csv.lines().skip(1) {
        let Some(id) = l.split(',').next().and_then(|f| parse_id(f.trim())) else {
            continue;
        };
        if let Some(o) = owner(id, sounds) {
            let v = out.entry(o).or_default();
            if !v.contains(&id) {
                v.push(id);
            }
        }
    }
    out
}

/// One CSV with FNAME and GAIN set on the rows of the `touched` sounds: the entry's file
/// and gain, or the ROM's file at 100 for a sound no longer listed. Returns the text and
/// how many rows changed.
fn rewrite_csv(
    text: &str,
    name: &str,
    sounds: &BTreeMap<u32, String>,
    touched: &BTreeSet<u32>,
    entries: &BTreeMap<u32, &Entry>,
) -> Result<(String, usize), String> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = text.lines();
    let header = lines.next().ok_or(format!("{name} is empty"))?;
    let cols: Vec<&str> = header.split(',').collect();
    let col = |h: &str| cols.iter().position(|c| c.trim().eq_ignore_ascii_case(h));
    let (Some(idc), Some(fc), Some(gc)) = (col("ID"), col("FNAME"), col("GAIN")) else {
        return Err(format!("{name}: no ID, FNAME or GAIN column in its header"));
    };
    let mut out = String::from(header);
    out.push_str(eol);
    let mut changed = 0;
    for (n, l) in lines.enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        let mut f: Vec<String> = l.split(',').map(str::to_owned).collect();
        if f.len() != cols.len() {
            return Err(format!(
                "{name} line {}: {} fields, the header has {}",
                n + 2,
                f.len(),
                cols.len()
            ));
        }
        let o = parse_id(f[idc].trim()).and_then(|id| owner(id, sounds));
        if let Some(o) = o.filter(|o| touched.contains(o)) {
            let e = entries.get(&o);
            let fname = match e {
                Some(e) if !e.fname.is_empty() => e.fname.clone(),
                _ => sounds[&o].clone(),
            };
            let gain = e.map_or(100, |e| e.gain).to_string();
            if f[fc] != fname || f[gc].trim() != gain {
                changed += 1;
            }
            f[fc] = fname;
            f[gc] = gain;
        }
        out.push_str(&f.join(","));
        out.push_str(eol);
    }
    Ok((out, changed))
}

/// Rewrites the pack's CSVs for its `replacements.csv`; `before` is the record they were
/// written for (its sounds that are no longer listed go back to the ROM's file).
pub fn apply(dir: &Path, before: &[Entry]) -> Result<Report, String> {
    let entries = load(dir)?;
    let mpath = dir.join("manifest.json");
    let m: Value = std::fs::read_to_string(&mpath)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
        .map_err(|e| format!("{}: {e}", mpath.display()))?;
    let sounds = factory_files(&m);
    let mut report = Report::default();
    let by_id: BTreeMap<u32, &Entry> = entries
        .iter()
        .filter(|e| {
            let known = sounds.contains_key(&e.id);
            if !known {
                report.missing.push(id_str(e.id));
            }
            let found = e.fname.is_empty() || dir.join(&e.fname).is_file();
            if !found {
                report.missing_files.push(e.fname.clone());
            }
            known && found
        })
        .map(|e| (e.id, e))
        .collect();
    for e in by_id.values() {
        if e.fname.is_empty() {
            report.gain_only += 1;
        } else {
            report.replaced += 1;
        }
    }
    let touched: BTreeSet<u32> = entries
        .iter()
        .chain(before)
        .map(|e| e.id)
        .filter(|i| sounds.contains_key(i))
        .collect();
    for name in [ALTSOUND_CSV, GSOUND_CSV] {
        let p = dir.join(name);
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{}: {e}", p.display())),
        };
        let (new, changed) = rewrite_csv(&text, name, &sounds, &touched, &by_id)?;
        report.rows += changed;
        if new != text {
            std::fs::write(&p, new).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    Ok(report)
}

/// After an extraction: the folder's `replacements.csv` (if any) applied to the new CSVs.
pub fn reapply(dir: &Path) -> Result<Option<Report>, String> {
    if !dir.join(FILE).is_file() {
        return Ok(None);
    }
    apply(dir, &[]).map(Some)
}

/// One line for the log.
pub fn summary(r: &Report) -> String {
    let mut s = format!(
        "replacements: {} sound{} replaced, {} with only the volume changed ({} CSV row{} rewritten)",
        r.replaced,
        if r.replaced == 1 { "" } else { "s" },
        r.gain_only,
        r.rows,
        if r.rows == 1 { "" } else { "s" },
    );
    if !r.missing.is_empty() {
        s.push_str(&format!(
            "; not in this pack (left in {FILE}): {}",
            r.missing.join(" ")
        ));
    }
    if !r.missing_files.is_empty() {
        s.push_str(&format!(
            "; files not found (the ROM's file plays): {}",
            r.missing_files.join(" ")
        ));
    }
    s
}

/// Writes the record and applies it; then removes the files of `replace/` that the old
/// record listed and the new one does not.
fn commit(dir: &Path, before: &[Entry], after: &[Entry]) -> Result<Report, String> {
    let p = dir.join(FILE);
    if after.is_empty() {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("{}: {e}", p.display())),
        }
    } else {
        std::fs::write(&p, format(after)).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let report = apply(dir, before)?;
    let kept: BTreeSet<&str> = after.iter().map(|e| e.fname.as_str()).collect();
    for e in before {
        if !e.fname.is_empty() && !kept.contains(e.fname.as_str()) {
            let _ = std::fs::remove_file(dir.join(&e.fname));
        }
    }
    Ok(report)
}

/// Applies the zip that the listening page saves when it cannot write to the folder
/// (`<rom>-replacements.zip`: `replacements.csv` and the new files of `replace/`).
pub fn apply_zip(dir: &Path, zip: &Path) -> Result<Report, String> {
    let before = load(dir)?;
    let members = crate::zipread::list(zip)?;
    let mut record = None;
    let mut files = Vec::new();
    for e in &members {
        let name = e.name.replace('\\', "/");
        if name.ends_with('/') {
            continue;
        }
        if name == FILE {
            let b = crate::zipread::read(zip, e)?;
            record = Some(String::from_utf8(b).map_err(|_| format!("{FILE}: not UTF-8"))?);
        } else if safe_member(&name) {
            files.push((name, e));
        } else {
            return Err(format!(
                "{}: {name} is neither {FILE} nor a file of {DIR}/",
                zip.display()
            ));
        }
    }
    let record = record.ok_or_else(|| format!("{}: no {FILE} in it", zip.display()))?;
    let after = parse(&record)?;
    std::fs::create_dir_all(dir.join(DIR)).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (name, e) in files {
        let b = crate::zipread::read(zip, e)?;
        let p = dir.join(&name);
        std::fs::write(&p, b).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    commit(dir, &before, &after)
}

/// The ROM a zip of the page is for: its `# rom:` line, else its name
/// (`<rom>-replacements.zip`).
pub fn zip_rom(zip: &Path) -> Option<String> {
    let from_csv = crate::zipread::list(zip).ok().and_then(|m| {
        let e = m.iter().find(|e| e.name == FILE)?;
        let t = String::from_utf8(crate::zipread::read(zip, e).ok()?).ok()?;
        t.lines()
            .find_map(|l| l.strip_prefix("# rom:").map(|r| r.trim().to_owned()))
            .filter(|r| !r.is_empty())
    });
    from_csv.or_else(|| {
        let n = zip.file_stem()?.to_string_lossy().into_owned();
        // "taf_l5-replacements (1)" when the browser kept an earlier download.
        let rom = n.split("-replacements").next()?.trim();
        (!rom.is_empty() && rom != n).then(|| rom.to_owned())
    })
}

/// [`apply_zip`], then the listening page written again; returns the summary line.
pub fn apply_zip_and_page(dir: &Path, zip: &Path) -> Result<String, String> {
    let r = apply_zip(dir, zip)?;
    crate::listen::write_page(dir)?;
    Ok(summary(&r))
}

#[derive(Parser, Debug)]
#[command(
    name = "rom2altsound replace",
    about = "Plays your own sound files instead of the ROM's, one sound at a time (the pack's replace/ folder and replacements.csv)",
    after_help = "Without an option: applies replacements.csv again (after editing it by hand) and lists it.\n\
                  AltSound plays .wav, .ogg (Vorbis, not Opus), .mp3 and .flac files. GAIN can only lower a sound:\n\
                  the listening page (index.html) measures a file and raises a quiet one into a WAV copy."
)]
struct ReplaceCli {
    /// The pack folder (altsound/<rom>).
    folder: PathBuf,
    /// The sound to replace or to set the volume of (0x000A).
    #[arg(long, value_name = "ID")]
    id: Option<String>,
    /// The file to play for --id (copied into replace/).
    #[arg(long, value_name = "FILE", requires = "id")]
    file: Option<PathBuf>,
    /// The volume of --id, 0 to 100 (100: as the file is).
    #[arg(long, value_name = "0-100", requires = "id", value_parser = clap::value_parser!(u32).range(0..=100))]
    gain: Option<u32>,
    /// Plays the ROM's file again for this sound, at full volume (repeatable).
    #[arg(long, value_name = "ID")]
    revert: Vec<String>,
    /// Plays every ROM file again.
    #[arg(long)]
    revert_all: bool,
    /// Applies a <rom>-replacements.zip saved by the listening page.
    #[arg(long, value_name = "ZIP", conflicts_with_all = ["id", "revert", "revert_all"])]
    apply: Option<PathBuf>,
    /// Leaves index.html as it is.
    #[arg(long)]
    no_html: bool,
}

fn parse_cli_id(s: &str) -> Result<u32, String> {
    parse_id(s.trim()).ok_or_else(|| format!("{s}: not an id (0x000A)"))
}

fn run_cli(cli: &ReplaceCli) -> Result<Report, String> {
    let dir = &cli.folder;
    if !dir.join("manifest.json").is_file() {
        return Err(format!(
            "{}: not a pack folder (no manifest.json)",
            dir.display()
        ));
    }
    if let Some(zip) = &cli.apply {
        return apply_zip(dir, zip);
    }
    let before = load(dir)?;
    let mut after = before.clone();
    if cli.revert_all {
        after.clear();
    }
    for r in &cli.revert {
        let id = parse_cli_id(r)?;
        after.retain(|e| e.id != id);
    }
    if let Some(id) = &cli.id {
        let id = parse_cli_id(id)?;
        let m: Value = std::fs::read_to_string(dir.join("manifest.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        if !factory_files(&m).contains_key(&id) {
            return Err(format!("{}: this pack has no row for it", id_str(id)));
        }
        let old = after.iter().find(|e| e.id == id).cloned();
        let mut e = old.unwrap_or(Entry {
            id,
            fname: String::new(),
            gain: 100,
            source: String::new(),
        });
        if let Some(f) = &cli.file {
            let source = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let name = stored_name(id, &source)?;
            let to = dir.join(&name);
            std::fs::create_dir_all(dir.join(DIR))
                .and_then(|()| std::fs::copy(f, &to).map(|_| ()))
                .map_err(|x| format!("{} -> {}: {x}", f.display(), to.display()))?;
            e.fname = name;
            e.source = source;
            if cli.gain.is_none() {
                e.gain = 100;
            }
        }
        if let Some(g) = cli.gain {
            e.gain = g;
        }
        after.retain(|x| x.id != id);
        if !e.fname.is_empty() || e.gain != 100 {
            after.push(e);
        }
        after.sort_by_key(|e| e.id);
    }
    commit(dir, &before, &after)
}

/// `rom2altsound replace <folder> [...]`.
pub fn cli(args: Vec<String>) -> i32 {
    let cli =
        ReplaceCli::parse_from(std::iter::once("rom2altsound replace".to_owned()).chain(args));
    match run_cli(&cli) {
        Ok(r) => {
            println!("{}", summary(&r));
            for e in load(&cli.folder).unwrap_or_default() {
                let what = if e.fname.is_empty() {
                    "the ROM's file".to_owned()
                } else {
                    e.fname.clone()
                };
                println!("  {}  {what}  GAIN {}", id_str(e.id), e.gain);
            }
            if !cli.no_html {
                match crate::listen::write_page(&cli.folder) {
                    Ok(p) => println!("page: {}", p.display()),
                    Err(e) => {
                        eprintln!("error: cannot write the listening page: {e}");
                        return 1;
                    }
                }
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ALT: &str = "ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME\r\n\
        0x0001,0,100,100,100,0,Music,0x0001-x_loop.wav\r\n\
        0x000A,,100,100,0,0,Jackpot,0x000A-x.wav\r\n\
        0x000B,,100,100,0,0,Twin,0x000A-x.wav\r\n\
        0xFD0A,,100,100,0,0,Jackpot,0x000A-x.wav\r\n";
    const GS: &str = "ID,TYPE,GAIN,DUCKING_PROFILE,FNAME\r\n\
        0x0001,music,100,0,0x0001-x_loop.wav\r\n\
        0x000A,sfx,100,0,0x000A-x.wav\r\n\
        0x000B,sfx,100,0,0x000A-x.wav\r\n\
        0xFD0A,sfx,100,0,0x000A-x.wav\r\n";

    fn pack(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("r2a-replace-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let m = json!({ "rom": "x", "sounds": [
            { "id": "0x01", "pack": { "file": "0x0001-x_loop.wav" } },
            { "id": "0x0A", "pack": { "file": "0x000A-x.wav" } },
            { "id": "0x0B", "pack": { "file": "0x000A-x.wav" }, "twin_of": "0x0A" },
            { "id": "0x0C" },
        ]});
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
        std::fs::write(dir.join(ALTSOUND_CSV), ALT).unwrap();
        std::fs::write(dir.join(GSOUND_CSV), GS).unwrap();
        dir
    }

    fn cli(dir: &Path, args: &[&str]) -> Result<Report, String> {
        let mut a = vec!["rom2altsound replace".to_owned(), dir.display().to_string()];
        a.extend(args.iter().map(|s| s.to_string()));
        run_cli(&ReplaceCli::try_parse_from(a).map_err(|e| e.to_string())?)
    }

    #[test]
    fn record_round_trip() {
        let e = vec![
            Entry {
                id: 0x0A,
                fname: "replace/0x000A-my jackpot.ogg".into(),
                gain: 63,
                source: "my jackpot.ogg".into(),
            },
            Entry {
                id: 0x12,
                fname: String::new(),
                gain: 50,
                source: String::new(),
            },
        ];
        assert_eq!(parse(&format(&e)).unwrap(), e);
        assert!(parse("ID,FNAME,GAIN\n0x1,../evil.wav,10\n").is_err());
        assert!(parse("ID,FNAME,GAIN\n0x1,replace/a/b.wav,10\n").is_err());
        assert_eq!(parse("ID,FNAME,GAIN\n0x1,,250\n").unwrap()[0].gain, 100);
    }

    #[test]
    fn file_names() {
        assert_eq!(
            stored_name(0x0A, "C:\\music\\Jack,pot \"1\".OGG").unwrap(),
            "replace/0x000A-Jack_pot _1_.OGG"
        );
        assert!(stored_name(0x0A, "a.opus").is_err());
        assert_eq!(clean_file_name("../.hidden.wav"), "hidden.wav");
    }

    #[test]
    fn owners_cover_the_copies() {
        let s: BTreeMap<u32, String> = [(0x0A, String::new()), (0x0105, String::new())].into();
        assert_eq!(owner(0x0A, &s), Some(0x0A));
        assert_eq!(owner(0xFD0A, &s), Some(0x0A));
        assert_eq!(owner(0x05, &s), Some(0x0105));
        assert_eq!(owner(0x06, &s), None);
        assert_eq!(
            rows_of(
                ALT,
                &factory_files(&json!({ "sounds": [
                    { "id": "0x0A", "pack": { "file": "f" } }
                ]}))
            )[&0x0A],
            vec![0x0A, 0xFD0A]
        );
    }

    #[test]
    fn replace_gain_and_revert() {
        let dir = pack("cli");
        let src = dir.join("mine.ogg");
        std::fs::write(&src, b"OggS").unwrap();
        let r = cli(
            &dir,
            &[
                "--id",
                "0x0A",
                "--file",
                src.to_str().unwrap(),
                "--gain",
                "63",
            ],
        )
        .unwrap();
        assert_eq!((r.replaced, r.gain_only, r.rows), (1, 0, 4));
        assert!(dir.join("replace/0x000A-mine.ogg").is_file());
        let alt = std::fs::read_to_string(dir.join(ALTSOUND_CSV)).unwrap();
        assert!(alt.contains("0x000A,,100,63,0,0,Jackpot,replace/0x000A-mine.ogg\r\n"));
        assert!(alt.contains("0xFD0A,,100,63,0,0,Jackpot,replace/0x000A-mine.ogg\r\n"));
        // The twin plays the same ROM file but is its own sound: untouched.
        assert!(alt.contains("0x000B,,100,100,0,0,Twin,0x000A-x.wav\r\n"));
        let gs = std::fs::read_to_string(dir.join(GSOUND_CSV)).unwrap();
        assert!(gs.contains("0x000A,sfx,63,0,replace/0x000A-mine.ogg\r\n"));

        // Volume only, on a ROM file.
        let r = cli(&dir, &["--id", "0x1", "--gain", "40"]).unwrap();
        assert_eq!((r.replaced, r.gain_only), (1, 1));
        let gs = std::fs::read_to_string(dir.join(GSOUND_CSV)).unwrap();
        assert!(gs.contains("0x0001,music,40,0,0x0001-x_loop.wav\r\n"));

        assert!(cli(&dir, &["--id", "0x0C", "--gain", "40"]).is_err());

        cli(&dir, &["--revert", "0x000A"]).unwrap();
        assert!(!dir.join("replace/0x000A-mine.ogg").exists());
        let gs = std::fs::read_to_string(dir.join(GSOUND_CSV)).unwrap();
        assert!(gs.contains("0x000A,sfx,100,0,0x000A-x.wav\r\n"));
        assert!(gs.contains("0xFD0A,sfx,100,0,0x000A-x.wav\r\n"));
        assert!(gs.contains("0x0001,music,40,0,0x0001-x_loop.wav\r\n"));

        cli(&dir, &["--revert-all"]).unwrap();
        assert!(!dir.join(FILE).exists());
        assert_eq!(std::fs::read_to_string(dir.join(GSOUND_CSV)).unwrap(), GS);
        assert_eq!(
            std::fs::read_to_string(dir.join(ALTSOUND_CSV)).unwrap(),
            ALT
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_new_extraction_gets_the_replacements_back() {
        let dir = pack("reapply");
        std::fs::create_dir_all(dir.join(DIR)).unwrap();
        std::fs::write(dir.join("replace/0x000A-a.wav"), b"RIFF").unwrap();
        std::fs::write(
            dir.join(FILE),
            "ID,FNAME,GAIN,SOURCE\n0x000A,replace/0x000A-a.wav,80,a.wav\n0x0099,,50,\n",
        )
        .unwrap();
        // The CSVs as a new extraction writes them: the ROM's files.
        let r = reapply(&dir).unwrap().unwrap();
        assert_eq!(r.missing, vec!["0x0099"]);
        assert_eq!(r.rows, 4);
        let alt = std::fs::read_to_string(dir.join(ALTSOUND_CSV)).unwrap();
        assert!(alt.contains("0x000A,,100,80,0,0,Jackpot,replace/0x000A-a.wav\r\n"));
        // The record keeps the id the pack no longer has.
        assert_eq!(load(&dir).unwrap().len(), 2);
        // A file that is gone: the ROM's file plays again, the record stays.
        std::fs::remove_file(dir.join("replace/0x000A-a.wav")).unwrap();
        let r = reapply(&dir).unwrap().unwrap();
        assert_eq!(r.missing_files, vec!["replace/0x000A-a.wav"]);
        assert!(
            std::fs::read_to_string(dir.join(ALTSOUND_CSV))
                .unwrap()
                .contains("0x000A,,100,100,0,0,Jackpot,0x000A-x.wav\r\n")
        );
        assert!(reapply(&pack("none")).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(pack("none")).unwrap();
    }

    #[test]
    fn zip_from_the_page() {
        let dir = pack("zip");
        let mut w = crate::zipread::Writer::default();
        let rec = b"ID,FNAME,GAIN,SOURCE\r\n0x000A,replace/0x000A-b.flac,70,b.flac\r\n";
        let flac = b"fLaC";
        w.add_raw(FILE, 0, crate::zipread::crc32(rec), rec.len() as u64, rec);
        w.add_raw(
            "replace/0x000A-b.flac",
            0,
            crate::zipread::crc32(flac),
            4,
            flac,
        );
        let zip = dir.join("x-replacements.zip");
        std::fs::write(&zip, w.finish()).unwrap();
        let r = apply_zip(&dir, &zip).unwrap();
        assert_eq!(r.replaced, 1);
        assert_eq!(
            std::fs::read(dir.join("replace/0x000A-b.flac")).unwrap(),
            flac
        );
        assert!(
            std::fs::read_to_string(dir.join(GSOUND_CSV))
                .unwrap()
                .contains("0x000A,sfx,70,0,replace/0x000A-b.flac\r\n")
        );

        let mut w = crate::zipread::Writer::default();
        w.add_raw("../manifest.json", 0, crate::zipread::crc32(b"x"), 1, b"x");
        w.add_raw(FILE, 0, crate::zipread::crc32(rec), rec.len() as u64, rec);
        std::fs::write(&zip, w.finish()).unwrap();
        assert!(apply_zip(&dir, &zip).is_err());
        // No "# rom:" line: the zip's name.
        assert_eq!(zip_rom(&zip).as_deref(), Some("x"));
        let named = dir.join("taf_l5-replacements (2).zip");
        std::fs::write(&named, b"").unwrap();
        assert_eq!(zip_rom(&named).as_deref(), Some("taf_l5"));
        let mut w = crate::zipread::Writer::default();
        let rec = b"# rom: afm_113b\r\nID,FNAME,GAIN,SOURCE\r\n";
        w.add_raw(FILE, 0, crate::zipread::crc32(rec), rec.len() as u64, rec);
        std::fs::write(&named, w.finish()).unwrap();
        assert_eq!(zip_rom(&named).as_deref(), Some("afm_113b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
