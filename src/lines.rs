//! Line edits: a sound can have several lines in altsound.csv (AltSound then picks one of
//! them at random each time), and a line can be turned off.
//!
//! `lines.csv` is the record, written by the listening page:
//!
//! ```text
//! ID,KIND,STATE,FNAME,GAIN,SOURCE
//! 0x000A,base,off,,,
//! 0x000A,extra,on,replace/0x000A-other jackpot.wav,80,other jackpot.wav
//! ```
//!
//! `base` is the sound's own line (the ROM's file, or the replacement of `replacements.csv`);
//! it can only be turned off, never deleted: the line that carries the id. An `extra` is
//! another line with the same id, with its own file and GAIN. A line that is off is not in
//! altsound.csv (nor in g-sound.csv), so the ROM's own sound plays when every line is off.
//!
//! The CSVs follow the record like they follow `replacements.csv`, and a new extraction
//! applies it again. `lines-base.csv` (`FILE,ID,ROW`) is ours: the base row of each edited
//! sound as the extraction wrote it, because a line that is off leaves nothing in the CSV to
//! copy the other columns from.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::altsound::{ALTSOUND_CSV, GSOUND_CSV, parse_id};
use crate::replace::{self, Entry};

/// The record, in the pack folder.
pub const FILE: &str = "lines.csv";
/// The base rows, in the pack folder.
pub const BASE_FILE: &str = "lines-base.csv";
const HEADER: &str = "ID,KIND,STATE,FNAME,GAIN,SOURCE";

/// Another line of a sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Extra {
    pub fname: String,
    pub gain: u32,
    pub source: String,
    pub on: bool,
}

/// The lines of one sound, besides the ROM's.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sound {
    /// The sound's own line is off.
    pub base_off: bool,
    pub extras: Vec<Extra>,
}

/// Sound id -> its line edits (only sounds with edits).
pub type Spec = BTreeMap<u32, Sound>;

fn id_str(id: u32) -> String {
    format!("0x{id:04X}")
}

/// Parses `lines.csv`.
pub fn parse(text: &str) -> Result<Spec, String> {
    let mut rows = text
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'));
    let Some((_, header)) = rows.next() else {
        return Ok(Spec::new());
    };
    let cols: Vec<String> = header
        .split(',')
        .map(|c| c.trim().to_ascii_uppercase())
        .collect();
    let col = |h: &str| cols.iter().position(|c| c == h);
    let (Some(ic), Some(kc), Some(sc)) = (col("ID"), col("KIND"), col("STATE")) else {
        return Err(format!("{FILE}: the header is not {HEADER}"));
    };
    let (fc, gc, oc) = (col("FNAME"), col("GAIN"), col("SOURCE"));
    let mut out = Spec::new();
    for (n, l) in rows {
        let f: Vec<&str> = l.split(',').map(str::trim).collect();
        let at = |i: Option<usize>| i.and_then(|i| f.get(i).copied()).unwrap_or("");
        let line = n + 1;
        let id = parse_id(at(Some(ic))).ok_or_else(|| format!("{FILE} line {line}: no ID"))?;
        let on = match at(Some(sc)) {
            "on" => true,
            "off" => false,
            s => return Err(format!("{FILE} line {line}: STATE is on or off, not {s:?}")),
        };
        let s = out.entry(id).or_default();
        match at(Some(kc)) {
            "base" => s.base_off = !on,
            "extra" => {
                let fname = at(fc).replace('\\', "/");
                // Your file in replace/, or the ROM's file, at the pack's root.
                let plain = !fname.is_empty() && replace::clean_file_name(&fname) == fname;
                if !plain && !replace::safe_member(&fname) {
                    return Err(format!(
                        "{FILE} line {line}: {fname:?} is not a file of the {}/ folder",
                        replace::DIR
                    ));
                }
                let gain: f64 = at(gc)
                    .parse()
                    .map_err(|_| format!("{FILE} line {line}: GAIN is not a number"))?;
                s.extras.push(Extra {
                    fname,
                    gain: gain.round().clamp(0.0, 100.0) as u32,
                    source: at(oc).to_owned(),
                    on,
                });
            }
            k => {
                return Err(format!(
                    "{FILE} line {line}: KIND is base or extra, not {k:?}"
                ));
            }
        }
    }
    out.retain(|_, s| s.base_off || !s.extras.is_empty());
    Ok(out)
}

/// The `lines.csv` text.
pub fn format(spec: &Spec) -> String {
    let mut s = format!("{HEADER}\r\n");
    let st = |on: bool| if on { "on" } else { "off" };
    for (id, snd) in spec {
        if snd.base_off {
            s.push_str(&format!("{},base,off,,,\r\n", id_str(*id)));
        }
        for e in &snd.extras {
            s.push_str(&format!(
                "{},extra,{},{},{},{}\r\n",
                id_str(*id),
                st(e.on),
                e.fname,
                e.gain,
                replace::clean_file_name(&e.source)
            ));
        }
    }
    s
}

/// The record of a pack folder (empty without one).
pub fn load(dir: &Path) -> Result<Spec, String> {
    let p = dir.join(FILE);
    match std::fs::read_to_string(&p) {
        Ok(t) => parse(&t).map_err(|e| format!("{}: {e}", dir.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Spec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// CSV file name and row id -> the base row as the extraction wrote it.
type Bases = BTreeMap<(String, u32), String>;

fn quote(v: &str) -> String {
    format!("\"{}\"", v.replace('"', "\"\""))
}

fn load_bases(dir: &Path) -> Bases {
    let mut out = Bases::new();
    let Ok(text) = std::fs::read_to_string(dir.join(BASE_FILE)) else {
        return out;
    };
    for l in text.lines().skip(1) {
        let mut it = l.splitn(3, ',');
        let (Some(file), Some(id), Some(row)) = (it.next(), it.next(), it.next()) else {
            continue;
        };
        let Some(id) = parse_id(id) else { continue };
        let row = row
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .map_or_else(|| row.to_owned(), |r| r.replace("\"\"", "\""));
        out.insert((file.to_owned(), id), row);
    }
    out
}

fn save_bases(dir: &Path, bases: &Bases) -> Result<(), String> {
    let p = dir.join(BASE_FILE);
    if bases.is_empty() {
        return match std::fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", p.display())),
        };
    }
    let mut s = String::from("FILE,ID,ROW\r\n");
    for ((file, id), row) in bases {
        s.push_str(&format!("{file},{},{}\r\n", id_str(*id), quote(row)));
    }
    std::fs::write(&p, s).map_err(|e| format!("{}: {e}", p.display()))
}

/// The ids of the CSV rows that have a base row saved, for the sounds in `sounds`.
pub fn saved_rows(dir: &Path, sounds: &BTreeMap<u32, String>) -> BTreeMap<u32, Vec<u32>> {
    let mut out: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (_, row) in load_bases(dir).keys() {
        if let Some(o) = replace::owner(*row, sounds) {
            let v = out.entry(o).or_default();
            if !v.contains(row) {
                v.push(*row);
            }
        }
    }
    out
}

/// What a CSV is made of, split for rewriting.
struct Csv<'a> {
    eol: &'a str,
    header: &'a str,
    idc: usize,
    fc: usize,
    gc: usize,
    width: usize,
    rows: Vec<&'a str>,
}

fn split_csv<'a>(text: &'a str, name: &str) -> Result<Csv<'a>, String> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = text.lines();
    let header = lines.next().ok_or(format!("{name} is empty"))?;
    let cols: Vec<&str> = header.split(',').collect();
    let col = |h: &str| cols.iter().position(|c| c.trim().eq_ignore_ascii_case(h));
    let (Some(idc), Some(fc), Some(gc)) = (col("ID"), col("FNAME"), col("GAIN")) else {
        return Err(format!("{name}: no ID, FNAME or GAIN column in its header"));
    };
    Ok(Csv {
        eol,
        header,
        idc,
        fc,
        gc,
        width: cols.len(),
        rows: lines.filter(|l| !l.trim().is_empty()).collect(),
    })
}

fn with_file(c: &Csv, row: &str, fname: &str, gain: u32) -> String {
    let mut f: Vec<String> = row.split(',').map(str::to_owned).collect();
    f.resize(c.width, String::new());
    f[c.fc] = fname.to_owned();
    f[c.gc] = gain.to_string();
    f.join(",")
}

/// Applies the record to altsound.csv and g-sound.csv, after `replace` set the base rows:
/// the rows of each sound with line edits are made again from its base row (the first one
/// the CSV has, saved in `lines-base.csv`), a new extraction's included (`fresh`).
/// Returns the number of extra lines now in the CSVs.
pub fn apply(
    dir: &Path,
    sounds: &BTreeMap<u32, String>,
    entries: &BTreeMap<u32, &Entry>,
    fresh: bool,
) -> Result<usize, String> {
    let spec = load(dir)?;
    let mut bases = load_bases(dir);
    let names = [ALTSOUND_CSV, GSOUND_CSV];
    let mut texts: Vec<Option<String>> = Vec::new();
    for n in names {
        let p = dir.join(n);
        texts.push(match std::fs::read_to_string(&p) {
            Ok(t) => Some(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("{}: {e}", p.display())),
        });
    }
    // The base rows: those of the sounds that have edits (kept as they were, unless this is
    // a new extraction), and nothing for a sound that has none any more.
    let managed = |o: u32| spec.contains_key(&o);
    for (n, t) in names.iter().zip(&texts) {
        let Some(t) = t else { continue };
        let c = split_csv(t, n)?;
        let mut seen = BTreeSet::new();
        for l in &c.rows {
            let f: Vec<&str> = l.split(',').collect();
            let Some(r) = f.get(c.idc).and_then(|v| parse_id(v.trim())) else {
                continue;
            };
            let saved = bases.contains_key(&(n.to_string(), r));
            if (saved || replace::owner(r, sounds).is_some_and(managed)) && seen.insert(r) {
                let k = (n.to_string(), r);
                if fresh || !bases.contains_key(&k) {
                    bases.insert(k, (*l).to_owned());
                }
            }
        }
    }
    let mut extras = 0;
    for (n, t) in names.iter().zip(&texts) {
        let Some(t) = t else { continue };
        let c = split_csv(t, n)?;
        let mut out = String::from(c.header);
        out.push_str(c.eol);
        let mut done = BTreeSet::new();
        let emit = |r: u32, out: &mut String, extras: &mut usize| {
            let (Some(o), Some(base)) = (replace::owner(r, sounds), bases.get(&(n.to_string(), r)))
            else {
                return;
            };
            let s = spec.get(&o).cloned().unwrap_or_default();
            if !s.base_off {
                let e = entries.get(&o);
                let fname = match e {
                    Some(e) if !e.fname.is_empty() => e.fname.clone(),
                    _ => sounds[&o].clone(),
                };
                out.push_str(&with_file(&c, base, &fname, e.map_or(100, |e| e.gain)));
                out.push_str(c.eol);
            }
            for x in s.extras.iter().filter(|x| x.on) {
                out.push_str(&with_file(&c, base, &x.fname, x.gain));
                out.push_str(c.eol);
                *extras += 1;
            }
        };
        for l in &c.rows {
            let r = l
                .split(',')
                .nth(c.idc)
                .and_then(|v| parse_id(v.trim()))
                .filter(|r| bases.contains_key(&(n.to_string(), *r)));
            match r {
                Some(r) => {
                    if done.insert(r) {
                        emit(r, &mut out, &mut extras);
                    }
                }
                None => {
                    out.push_str(l);
                    out.push_str(c.eol);
                }
            }
        }
        // Rows with nothing left in the CSV (every line off).
        let rest: Vec<u32> = bases
            .keys()
            .filter(|(f, r)| f == n && !done.contains(r))
            .map(|(_, r)| *r)
            .collect();
        for r in rest {
            emit(r, &mut out, &mut extras);
        }
        if out != **t {
            let p = dir.join(n);
            std::fs::write(&p, out).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    // A sound that has no edits any more gets its row back above and needs no base row.
    bases.retain(|(_, r), _| replace::owner(*r, sounds).is_some_and(managed));
    save_bases(dir, &bases)?;
    Ok(extras)
}

/// The files of `replace/` that the extras list (the ROM's files are not ours to remove).
pub fn files(spec: &Spec) -> BTreeSet<String> {
    spec.values()
        .flat_map(|s| s.extras.iter().map(|e| e.fname.clone()))
        .filter(|f| f.starts_with(replace::DIR))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trip() {
        let mut s = Spec::new();
        s.insert(
            0x0A,
            Sound {
                base_off: true,
                extras: vec![
                    Extra {
                        fname: "replace/0x000A-b.wav".into(),
                        gain: 80,
                        source: "b.wav".into(),
                        on: true,
                    },
                    Extra {
                        fname: "replace/0x000A-c.wav".into(),
                        gain: 100,
                        source: String::new(),
                        on: false,
                    },
                ],
            },
        );
        assert_eq!(parse(&format(&s)).unwrap(), s);
    }

    #[test]
    fn rejects_foreign_files() {
        let t = format!("{HEADER}\r\n0x000A,extra,on,../x.wav,100,\r\n");
        assert!(parse(&t).is_err());
        let t = format!("{HEADER}\r\n0x000A,extra,on,0x000A-x.wav,100,\r\n");
        assert!(parse(&t).is_ok());
        assert!(parse(&format!("{HEADER}\r\n0x000A,what,on,,,\r\n")).is_err());
    }
}
