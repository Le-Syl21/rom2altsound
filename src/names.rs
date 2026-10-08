//! Sound names from a `names.csv`: the names someone typed on the listening page (its
//! "Export names" button), applied to a pack by `rom2altsound names <folder> <names.csv>`
//! or at extraction with `--names <names.csv>`.
//!
//! The file: `#` comment lines, of which `# rom: <set>`, `# sound_rom_id: <id>` and
//! `# rom2altsound: <version>` are read, then `ID,NAME` rows (RFC 4180 quoting; the ID as
//! in altsound.csv, `0xNNNN`). An empty NAME clears the name. Names belong to a sound ROM,
//! not to a game version: every revision that shares the sound ROMs (same sound ROM id,
//! see `rom2altsound roms`) takes the same file, and a file made for another sound ROM is
//! refused unless forced.
//!
//! Applying names rewrites the NAME column of altsound.csv (cleaned like every NAME, see
//! [`crate::altsound::csv_name`]) and the names in manifest.json; nothing else changes: the
//! file names are id-based, and the channels, ducking and types stay as extracted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::{Value, json};

use crate::altsound::{ALTSOUND_CSV, csv_name, parse_id};

/// A parsed `names.csv`.
#[derive(Debug, Default, PartialEq)]
pub struct Names {
    /// `# rom:` of the header, when given.
    pub rom: Option<String>,
    /// `# sound_rom_id:` of the header, when given.
    pub sound_rom_id: Option<String>,
    pub rows: Vec<Entry>,
}

/// One `ID,NAME` row.
#[derive(Debug, PartialEq)]
pub struct Entry {
    /// The line it starts on (1-based), for the messages.
    pub line: usize,
    pub id: String,
    pub name: String,
}

/// `0x392`, `0x0392` and `0X0392` are one id: `0x0392`, like altsound.csv writes it. Ids
/// that are not a plain hex number are kept as written.
pub fn norm_id(id: &str) -> String {
    let id = id.trim();
    match parse_id(id) {
        Some(n) => format!("0x{n:04X}"),
        None => id.to_owned(),
    }
}

/// A name as manifest.json and the page keep it: control characters (newlines, tabs) become
/// spaces, runs of spaces one space, no space at either end.
pub fn clean_label(name: &str) -> String {
    name.split(|c: char| c.is_control() || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads a `names.csv`: comment lines (a record that starts with `#`), the `ID,NAME` header
/// (its columns may come in any order, with others), then the rows.
pub fn parse(text: &str) -> Result<Names, String> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut out = Names::default();
    let mut cols: Option<(usize, usize)> = None;
    for (line, rec) in records(text)? {
        let rec = match rec {
            Record::Comment(c) => {
                if let Some((k, v)) = c.trim_start_matches('#').split_once(':') {
                    let v = v.trim();
                    match k.trim().to_ascii_lowercase().as_str() {
                        "rom" if !v.is_empty() => out.rom = Some(v.to_owned()),
                        "sound_rom_id" if !v.is_empty() => {
                            out.sound_rom_id = Some(v.to_ascii_lowercase())
                        }
                        _ => {}
                    }
                }
                continue;
            }
            Record::Fields(f) => f,
        };
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let Some((id, name)) = cols else {
            let find = |h: &str| rec.iter().position(|f| f.trim().eq_ignore_ascii_case(h));
            match (find("ID"), find("NAME")) {
                (Some(i), Some(n)) => {
                    cols = Some((i, n));
                    continue;
                }
                _ => return Err(format!("line {line}: expected the header ID,NAME")),
            }
        };
        let (Some(i), n) = (rec.get(id), rec.get(name)) else {
            return Err(format!("line {line}: no ID"));
        };
        out.rows.push(Entry {
            line,
            id: i.trim().to_owned(),
            name: n.cloned().unwrap_or_default(),
        });
    }
    if cols.is_none() {
        return Err("no ID,NAME header".into());
    }
    Ok(out)
}

enum Record {
    Comment(String),
    Fields(Vec<String>),
}

/// RFC 4180 records (quoted fields may hold commas, `""` and line breaks), each with the
/// line it starts on; a record that starts with `#` is a comment up to the end of its line.
fn records(text: &str) -> Result<Vec<(usize, Record)>, String> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    let mut line = 1;
    while it.peek().is_some() {
        let start = line;
        if it.peek() == Some(&'#') {
            let mut c = String::new();
            for ch in it.by_ref() {
                if ch == '\n' {
                    line += 1;
                    break;
                }
                c.push(ch);
            }
            out.push((start, Record::Comment(c.trim_end_matches('\r').to_owned())));
            continue;
        }
        let mut fields = Vec::new();
        let mut f = String::new();
        let mut quoted = false;
        loop {
            let Some(ch) = it.next() else {
                if quoted {
                    return Err(format!("line {start}: a quoted field is not closed"));
                }
                fields.push(std::mem::take(&mut f));
                break;
            };
            if quoted {
                match ch {
                    '"' if it.peek() == Some(&'"') => {
                        it.next();
                        f.push('"');
                    }
                    '"' => quoted = false,
                    '\n' => {
                        line += 1;
                        f.push(ch);
                    }
                    _ => f.push(ch),
                }
                continue;
            }
            match ch {
                '"' if f.is_empty() => quoted = true,
                ',' => fields.push(std::mem::take(&mut f)),
                '\r' if it.peek() == Some(&'\n') => {}
                '\n' => {
                    line += 1;
                    fields.push(std::mem::take(&mut f));
                    break;
                }
                _ => f.push(ch),
            }
        }
        out.push((start, Record::Fields(fields)));
    }
    Ok(out)
}

/// A CSV field, quoted when it has to be (a comma, a quote, a line break, or a space at an
/// end that a reader could trim).
pub fn csv_field(v: &str) -> String {
    let needs = v.contains([',', '"', '\n', '\r'])
        || v.starts_with(char::is_whitespace)
        || v.ends_with(char::is_whitespace);
    if needs {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_owned()
    }
}

/// The names of a pack as a `names.csv` (what the page exports, edits aside): every sound
/// with a name.
pub fn export(m: &Value) -> String {
    let mut s = String::from("# rom2altsound names\r\n");
    if let Some(r) = m["rom"].as_str() {
        s.push_str(&format!("# rom: {r}\r\n"));
    }
    if let Some(id) = m["sound_rom_id"].as_str() {
        s.push_str(&format!("# sound_rom_id: {id}\r\n"));
    }
    s.push_str(&format!(
        "# rom2altsound: {}\r\n",
        env!("CARGO_PKG_VERSION")
    ));
    s.push_str("ID,NAME\r\n");
    for e in m["sounds"].as_array().into_iter().flatten() {
        let (Some(id), Some(name)) = (e["id"].as_str(), e["name"].as_str()) else {
            continue;
        };
        if !name.is_empty() {
            s.push_str(&format!(
                "{},{}\r\n",
                csv_field(&norm_id(id)),
                csv_field(name)
            ));
        }
    }
    s
}

/// Whether a names file may go on a pack: refused (an error) when both carry a sound ROM id
/// and they differ, unless forced with `flag` (then a warning). Returns the warnings.
pub fn check_rom(names: &Names, m: &Value, force: bool, flag: &str) -> Result<Vec<String>, String> {
    let mut warn = Vec::new();
    let pack_rom = m["rom"].as_str().unwrap_or("?");
    let pack_id = m["sound_rom_id"].as_str().map(str::to_ascii_lowercase);
    match (&names.sound_rom_id, &pack_id) {
        (Some(a), Some(b)) if a != b => {
            let msg = format!(
                "these names are for another sound ROM (names: {}, sound ROM id {a}; pack {pack_rom}: {b}); names belong to a sound ROM, not to a game version",
                names.rom.as_deref().unwrap_or("?")
            );
            if !force {
                return Err(format!("{msg} ({flag} applies them anyway)"));
            }
            warn.push(format!("{msg}: applied anyway ({flag})"));
        }
        (Some(_), Some(_)) => {}
        _ => {
            if let Some(r) = &names.rom
                && r != pack_rom
            {
                warn.push(format!(
                    "these names were written for {r}, the pack is {pack_rom}, and the sound ROM ids cannot be compared (one is missing)"
                ));
            }
        }
    }
    Ok(warn)
}

/// What applying names did.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Rows of the file that matched a sound of the pack.
    pub matched: usize,
    /// Sounds whose name changed.
    pub changed: usize,
    /// Ids of the file that are not in the pack (left out).
    pub unknown: Vec<String>,
    /// Ids given more than once with the same name (harmless).
    pub duplicates: Vec<String>,
}

/// The names by id, checked: an id given twice with two different names is refused.
fn by_id(names: &Names) -> Result<(HashMap<String, String>, Vec<String>), String> {
    let mut map: HashMap<String, (usize, String)> = HashMap::new();
    let mut dup = Vec::new();
    for e in &names.rows {
        let id = norm_id(&e.id);
        if id.is_empty() {
            return Err(format!("line {}: empty ID", e.line));
        }
        let name = clean_label(&e.name);
        if let Some((line, prev)) = map.get(&id) {
            if *prev != name {
                return Err(format!(
                    "{id} is given twice with different names (lines {line} and {}): {prev:?} and {name:?}",
                    e.line
                ));
            }
            dup.push(id);
            continue;
        }
        map.insert(id, (e.line, name));
    }
    Ok((map.into_iter().map(|(k, (_, n))| (k, n)).collect(), dup))
}

/// Applies names to a manifest (in memory) and to the text of its altsound.csv; returns the
/// report and the new altsound.csv.
pub fn apply(
    m: &mut Value,
    altsound: Option<&str>,
    names: &Names,
) -> Result<(Report, Option<String>), String> {
    let (map, duplicates) = by_id(names)?;
    let mut report = Report {
        duplicates,
        ..Report::default()
    };
    let mut seen = std::collections::HashSet::new();
    for e in m["sounds"].as_array_mut().into_iter().flatten() {
        let Some(id) = e["id"].as_str().map(norm_id) else {
            continue;
        };
        let Some(name) = map.get(&id) else { continue };
        seen.insert(id);
        report.matched += 1;
        let old = e["name"].as_str().unwrap_or("").to_owned();
        if old != *name {
            // The first name it had (sounds.dat's) stays on record.
            if e.get("sounds_dat_name").is_none() {
                e["sounds_dat_name"] = json!(old);
            }
            e["name"] = json!(name);
            report.changed += 1;
        }
    }
    let mut unknown: Vec<&String> = map.keys().filter(|k| !seen.contains(*k)).collect();
    unknown.sort();
    report.unknown = unknown.into_iter().cloned().collect();
    let csv = altsound.map(|t| rename_rows(t, &map)).transpose()?;
    Ok((report, csv))
}

/// altsound.csv with the NAME of the listed ids replaced; the other columns and rows as
/// they were (found by the header, so an edited file with its columns moved works too).
fn rename_rows(text: &str, map: &HashMap<String, String>) -> Result<String, String> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = text.lines();
    let header = lines.next().ok_or("altsound.csv is empty")?;
    let cols: Vec<&str> = header.split(',').collect();
    let col = |h: &str| cols.iter().position(|c| c.trim().eq_ignore_ascii_case(h));
    let (Some(idc), Some(namec)) = (col("ID"), col("NAME")) else {
        return Err("altsound.csv: no ID or NAME column in its header".into());
    };
    let mut out = String::from(header);
    out.push_str(eol);
    for (n, l) in lines.enumerate() {
        if l.trim().is_empty() {
            continue;
        }
        let mut f: Vec<String> = l.split(',').map(str::to_owned).collect();
        if f.len() != cols.len() {
            return Err(format!(
                "altsound.csv line {}: {} fields, the header has {}",
                n + 2,
                f.len(),
                cols.len()
            ));
        }
        if let Some(name) = map.get(&norm_id(&f[idc])) {
            f[namec] = csv_name(name, f[idc].trim());
        }
        out.push_str(&f.join(","));
        out.push_str(eol);
    }
    Ok(out)
}

/// Applies a names file to a pack folder: manifest.json (and its `names` record), then
/// altsound.csv when there is one. `from` is where the names came from, for the record.
pub fn apply_dir(
    dir: &Path,
    names: &Names,
    from: &Path,
    force: bool,
    flag: &str,
) -> Result<(Report, Vec<String>), String> {
    let mpath = dir.join("manifest.json");
    let text = std::fs::read_to_string(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let mut m: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    if m["sound_rom_id"].is_null()
        && let Some(id) = m["rom"].as_str().and_then(sound_rom_id_of)
    {
        // An older pack: its id from PinMAME's table (the ROM set is known by name).
        m["sound_rom_id"] = json!(id);
    }
    let warnings = check_rom(names, &m, force, flag)?;
    let cpath = dir.join(ALTSOUND_CSV);
    let csv = match std::fs::read_to_string(&cpath) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", cpath.display())),
    };
    let (report, csv) = apply(&mut m, csv.as_deref(), names)?;
    m["names"] = json!({
        "from": from.file_name().map(|f| f.to_string_lossy().into_owned()),
        "rom": names.rom,
        "sound_rom_id": names.sound_rom_id,
        "rows": names.rows.len(),
        "matched": report.matched,
        "unknown": report.unknown,
    });
    std::fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap())
        .map_err(|e| format!("{}: {e}", mpath.display()))?;
    if let Some(csv) = csv {
        std::fs::write(&cpath, csv).map_err(|e| format!("{}: {e}", cpath.display()))?;
    }
    Ok((report, warnings))
}

/// Reads and parses a names file.
pub fn load(path: &Path) -> Result<Names, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The sound ROM id of a ROM set, from PinMAME's table (as `rom2altsound roms` computes
/// it); None for a set it does not know or one without sound ROMs (Stern SAM).
pub fn sound_rom_id_of(rom: &str) -> Option<String> {
    crate::drivers::load()
        .into_iter()
        .find(|d| d.name.eq_ignore_ascii_case(rom))
        .and_then(|d| d.sound_rom_id())
}

/// Writes the sound ROM id into a freshly written manifest.json (nothing when None).
pub fn stamp_sound_rom_id(dir: &Path, id: Option<&str>) -> Result<(), String> {
    let Some(id) = id else { return Ok(()) };
    let mpath = dir.join("manifest.json");
    let text = std::fs::read_to_string(&mpath).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let mut m: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", mpath.display()))?;
    m["sound_rom_id"] = json!(id);
    std::fs::write(&mpath, serde_json::to_string_pretty(&m).unwrap())
        .map_err(|e| format!("{}: {e}", mpath.display()))
}

/// One line about what applying names did.
pub fn summary(r: &Report) -> String {
    let mut s = format!("names: {} matched, {} changed", r.matched, r.changed);
    if !r.unknown.is_empty() {
        s.push_str(&format!(
            ", {} id(s) not in the pack, left out: {}",
            r.unknown.len(),
            r.unknown.join(" ")
        ));
    }
    if !r.duplicates.is_empty() {
        s.push_str(&format!(
            ", given twice (same name): {}",
            r.duplicates.join(" ")
        ));
    }
    s
}

/// `rom2altsound names`.
#[derive(Parser)]
#[command(
    name = "rom2altsound names",
    about = "Apply the names of a names.csv (the listening page's \"Export names\") to a pack",
    long_about = "Apply the names of a names.csv (the listening page's \"Export names\") to a pack.

Rewrites the NAME column of altsound.csv, the names in manifest.json and the folder's
index.html. The WAV files keep their names (they are id-based), and the channels,
ducking and types do not change. Ids of the file that are not in the pack are reported
and left out; an id given twice with two names is refused. A names.csv whose sound ROM
id differs from the pack's is refused (names belong to a sound ROM, shared by the game's
revisions), unless --force.

Without a names.csv, prints the pack's names as a names.csv."
)]
struct NamesCli {
    /// The ROM folder (with manifest.json)
    folder: PathBuf,
    /// The names file
    names: Option<PathBuf>,
    /// Apply names made for another sound ROM
    #[arg(long)]
    force: bool,
    /// Do not write index.html again
    #[arg(long)]
    no_html: bool,
}

/// Entry point of `rom2altsound names`; returns the exit code.
pub fn cli(args: Vec<String>) -> i32 {
    let cli = NamesCli::parse_from(std::iter::once("rom2altsound names".to_owned()).chain(args));
    let Some(file) = &cli.names else {
        let mpath = cli.folder.join("manifest.json");
        let m: Result<Value, String> = std::fs::read_to_string(&mpath)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
        return match m {
            Ok(mut m) => {
                if m["sound_rom_id"].is_null()
                    && let Some(id) = m["rom"].as_str().and_then(sound_rom_id_of)
                {
                    m["sound_rom_id"] = json!(id);
                }
                print!("{}", export(&m));
                0
            }
            Err(e) => {
                eprintln!("error: {}: {e}", mpath.display());
                1
            }
        };
    };
    let result = load(file).and_then(|n| apply_dir(&cli.folder, &n, file, cli.force, "--force"));
    match result {
        Ok((report, warnings)) => {
            for w in warnings {
                eprintln!("warning: {w}");
            }
            println!("{}", summary(&report));
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

    fn manifest() -> Value {
        json!({
            "rom": "test_l1",
            "sound_rom_id": "abc123",
            "sounds": [
                { "id": "0x0001", "name": "Music: Main" },
                { "id": "0x0002", "name": "" },
                { "id": "0x0010", "name": "\"Jackpot\"" },
            ]
        })
    }

    const CSV: &str = "ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME\r\n\
        0x0001,0,100,100,100,0,Music: Main,0x0001-test_l1-loop.wav\r\n\
        0x0002,,100,100,0,0,sound 0x0002,0x0002-test_l1.wav\r\n\
        0x0010,1,60,100,0,0,Jackpot,0x0010-test_l1.wav\r\n";

    #[test]
    fn parses_header_comments_and_quoting() {
        let n = parse(
            "\u{FEFF}# rom2altsound names\r\n# rom: test_l2\r\n# sound_rom_id: ABC123\r\n\
             # rom2altsound: 0.2.2\r\nID,NAME\r\n0x2,\"Ramp, left\"\r\n\
             0x10,\"Say \"\"Jackpot\"\"\nnow\"\r\n0x0001,\r\n\r\n",
        )
        .unwrap();
        assert_eq!(n.rom.as_deref(), Some("test_l2"));
        assert_eq!(n.sound_rom_id.as_deref(), Some("abc123"));
        assert_eq!(n.rows.len(), 3);
        assert_eq!(n.rows[0].name, "Ramp, left");
        assert_eq!(n.rows[1].name, "Say \"Jackpot\"\nnow");
        assert_eq!(n.rows[1].line, 7);
        assert_eq!(n.rows[2].line, 9);
        assert_eq!(n.rows[2].name, "");
        // Columns in another order, with others.
        let n = parse("NAME,ID,NOTE\nBumper,0x0003,x\n").unwrap();
        assert_eq!(
            (n.rows[0].id.as_str(), n.rows[0].name.as_str()),
            ("0x0003", "Bumper")
        );
        assert!(parse("0x0001,Name\n").is_err());
        assert!(parse("ID,NAME\n0x1,\"open\n").is_err());
    }

    #[test]
    fn csv_quoting_round_trips() {
        for v in [
            "plain",
            "a, b",
            "say \"hi\"",
            "two\nlines",
            " lead",
            "",
            "trail ",
        ] {
            let line = format!("ID,NAME\n0x1,{}\n", csv_field(v));
            assert_eq!(parse(&line).unwrap().rows[0].name, v, "{v:?}");
        }
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("plain"), "plain");
        let m = manifest();
        let out = export(&m);
        assert!(out.contains("# rom: test_l1\r\n# sound_rom_id: abc123\r\n"));
        // Only named sounds; quotes doubled.
        assert!(out.ends_with("ID,NAME\r\n0x0001,Music: Main\r\n0x0010,\"\"\"Jackpot\"\"\"\r\n"));
        let back = parse(&out).unwrap();
        assert_eq!(back.rows[1].name, "\"Jackpot\"");
        assert_eq!(back.sound_rom_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn applies_to_manifest_and_csv() {
        let mut m = manifest();
        let names = parse(
            "ID,NAME\n0x2,\"Ramp, left\"\n0x0010,\"Say \"\"Jackpot\"\"\nnow\"\n0x0099,Ghost\n0x0002,\"Ramp, left\"\n",
        )
        .unwrap();
        let (r, csv) = apply(&mut m, Some(CSV), &names).unwrap();
        assert_eq!(r.matched, 2);
        assert_eq!(r.changed, 2);
        assert_eq!(r.unknown, ["0x0099"]);
        assert_eq!(r.duplicates, ["0x0002"]);
        // The manifest keeps the name readable, and sounds.dat's.
        assert_eq!(m["sounds"][1]["name"], "Ramp, left");
        assert_eq!(m["sounds"][1]["sounds_dat_name"], "");
        assert_eq!(m["sounds"][2]["name"], "Say \"Jackpot\" now");
        assert_eq!(m["sounds"][2]["sounds_dat_name"], "\"Jackpot\"");
        assert_eq!(m["sounds"][0]["name"], "Music: Main");
        // altsound.csv: no comma, quote or line break in a NAME; the rest untouched.
        let csv = csv.unwrap();
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], "ID,CHANNEL,DUCK,GAIN,LOOP,STOP,NAME,FNAME");
        assert_eq!(
            lines[1],
            "0x0001,0,100,100,100,0,Music: Main,0x0001-test_l1-loop.wav"
        );
        assert_eq!(lines[2], "0x0002,,100,100,0,0,Ramp left,0x0002-test_l1.wav");
        assert_eq!(
            lines[3],
            "0x0010,1,60,100,0,0,Say Jackpot now,0x0010-test_l1.wav"
        );
        assert!(csv.lines().all(|l| l.split(',').count() == 8));
        // An empty name clears it (altsound.csv then says "sound <id>").
        let mut m2 = manifest();
        let (_, csv) = apply(&mut m2, Some(CSV), &parse("ID,NAME\n0x0001,\n").unwrap()).unwrap();
        assert_eq!(m2["sounds"][0]["name"], "");
        assert!(
            csv.unwrap()
                .contains("0x0001,0,100,100,100,0,sound 0x0001,")
        );
    }

    #[test]
    fn conflicting_duplicates_are_refused() {
        let names = parse("ID,NAME\n0x0001,A\n0x1,B\n").unwrap();
        let e = apply(&mut manifest(), Some(CSV), &names).unwrap_err();
        assert!(e.contains("0x0001") && e.contains("lines 2 and 3"), "{e}");
    }

    #[test]
    fn another_sound_rom_is_refused_unless_forced() {
        let m = manifest();
        let other = parse("# rom: other_l1\n# sound_rom_id: fff\nID,NAME\n").unwrap();
        assert!(
            check_rom(&other, &m, false, "--force")
                .unwrap_err()
                .contains("another sound ROM")
        );
        let w = check_rom(&other, &m, true, "--force").unwrap();
        assert!(w[0].contains("--force"));
        // Same sound ROM, another revision: fine, silently.
        let same = parse("# rom: test_l2\n# sound_rom_id: ABC123\nID,NAME\n").unwrap();
        assert!(check_rom(&same, &m, false, "--force").unwrap().is_empty());
        // No id in the file: only a note when the ROM differs.
        let bare = parse("# rom: other_l1\nID,NAME\n").unwrap();
        assert_eq!(check_rom(&bare, &m, false, "--force").unwrap().len(), 1);
        assert!(
            check_rom(&parse("ID,NAME\n").unwrap(), &m, false, "--force")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn ids_normalize_like_altsound_csv() {
        assert_eq!(norm_id("0x2"), "0x0002");
        assert_eq!(norm_id(" 0X01ab "), "0x01AB");
        assert_eq!(norm_id("0x12345"), "0x12345");
        assert_eq!(norm_id("seq 1"), "seq 1");
    }

    #[test]
    fn cli_arguments() {
        let c = NamesCli::try_parse_from(["names", "afm_113b", "names.csv", "--force"]).unwrap();
        assert_eq!(c.folder, Path::new("afm_113b"));
        assert_eq!(c.names.as_deref(), Some(Path::new("names.csv")));
        assert!(c.force && !c.no_html);
        let c = NamesCli::try_parse_from(["names", "afm_113b"]).unwrap();
        assert!(c.names.is_none());
        assert!(NamesCli::try_parse_from(["names"]).is_err());
    }

    #[test]
    fn applies_in_a_folder() {
        let dir = std::env::temp_dir().join(format!("r2a-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), manifest().to_string()).unwrap();
        std::fs::write(dir.join(ALTSOUND_CSV), CSV).unwrap();
        let names = parse("# sound_rom_id: abc123\nID,NAME\n0x0002,Ramp\n").unwrap();
        let (r, w) = apply_dir(&dir, &names, Path::new("x/names.csv"), false, "--force").unwrap();
        assert_eq!((r.changed, w.len()), (1, 0));
        let m: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(m["sounds"][1]["name"], "Ramp");
        assert_eq!(m["names"]["from"], "names.csv");
        assert_eq!(m["names"]["matched"], 1);
        let csv = std::fs::read_to_string(dir.join(ALTSOUND_CSV)).unwrap();
        assert!(csv.contains("0x0002,,100,100,0,0,Ramp,0x0002-test_l1.wav\r\n"));
        // Another sound ROM: nothing written.
        let other = parse("# sound_rom_id: fff\nID,NAME\n0x0002,Other\n").unwrap();
        assert!(apply_dir(&dir, &other, Path::new("n.csv"), false, "--force").is_err());
        assert!(
            std::fs::read_to_string(dir.join(ALTSOUND_CSV))
                .unwrap()
                .contains(",Ramp,")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
