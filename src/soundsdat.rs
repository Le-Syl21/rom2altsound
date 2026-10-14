//! Parser for PinMAME's `release/sounds.dat`, the list of named sound commands used by
//! its sound commander (format documented at the top of the file, reader in
//! src/wpc/snd_cmd.c `readCmds`).

use std::collections::HashMap;

/// The sounds.dat of the PinMAME version this program is built with.
pub const BUILT_IN: &str = include_str!("../vendor/pinmame/release/sounds.dat");

/// One named command: the raw bytes as written in the file.
#[derive(Clone, Debug)]
pub struct Entry {
    pub bytes: Vec<u8>,
    pub name: String,
}

struct Section {
    header: String,
    entries: Vec<Entry>,
}

pub struct SoundsDat {
    sections: Vec<Section>,
}

impl SoundsDat {
    pub fn parse(text: &str) -> Self {
        let mut sections: Vec<Section> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            match line.chars().next() {
                None | Some(';' | '#' | ' ') => {}
                Some(':') => {
                    if let (Some(s), Some(e)) = (sections.last_mut(), parse_entry(&line[1..])) {
                        s.entries.push(e);
                    }
                }
                Some(_) => sections.push(Section {
                    header: line.to_string(),
                    entries: Vec::new(),
                }),
            }
        }
        Self { sections }
    }

    /// Entries of the sections whose header starts with the game or the parent name
    /// (the same prefix rule as `readCmds`). A duplicated command keeps its first
    /// position and takes the last name, as in PinMAME.
    pub fn game_entries(&self, game: &str, parent: Option<&str>) -> Vec<Entry> {
        self.collect(|h| h.starts_with(game) || parent.is_some_and(|p| h.starts_with(p)))
    }

    /// Entries of the generic section of a board family (`dcs:`, `wpcs:`), matched on the
    /// board's type string. Note: PinMAME compares case-sensitively against the upper-case
    /// type strings ("DCS", "WPCS"), so its own commander never loads these sections.
    pub fn family_entries(&self, typestr: &str) -> Vec<Entry> {
        let t = typestr.to_ascii_lowercase();
        self.collect(|h| h.to_ascii_lowercase().starts_with(&t))
    }

    fn collect(&self, mut wanted: impl FnMut(&str) -> bool) -> Vec<Entry> {
        let mut out: Vec<Entry> = Vec::new();
        for s in self.sections.iter().filter(|s| wanted(&s.header)) {
            for e in &s.entries {
                match out.iter_mut().find(|o| o.bytes == e.bytes) {
                    Some(o) => o.name = e.name.clone(),
                    None => out.push(e.clone()),
                }
            }
        }
        out
    }
}

/// sounds.dat as PinMAME's commander reads it (snd_cmd.c `readCmds`, game and parent
/// headings only): `mame_fgets` lines of at most 100 bytes (a longer line goes on as a
/// line of its own), an LF read as CR, so that an empty line is a heading that matches
/// nothing and ends the section, the hex digits lowercase only, the name from the second
/// character after them (one separator), cut to 29 characters.
pub struct PinmameDat<'a> {
    lines: Vec<&'a str>,
    /// Index of each heading line (anything but an entry, a comment or a line starting
    /// with a space).
    headings: Vec<usize>,
}

impl<'a> PinmameDat<'a> {
    pub fn new(text: &'a str) -> Self {
        let lines = fgets_lines(text);
        let headings = (0..lines.len())
            .filter(|&i| !matches!(lines[i].as_bytes().first(), Some(b':' | b';' | b'#' | b' ')))
            .collect();
        Self { lines, headings }
    }

    /// What the commander lists for a game: the entries under every heading that starts
    /// with the game's name or the parent's, each with its heading; a duplicated command
    /// keeps its first position and takes the last name.
    pub fn entries(&self, game: &str, parent: Option<&str>) -> Vec<(Entry, String)> {
        let mut out: Vec<(Entry, String)> = Vec::new();
        let mut at: HashMap<Vec<u8>, usize> = HashMap::new();
        for (k, &i) in self.headings.iter().enumerate() {
            let h = self.lines[i];
            if !(h.starts_with(game) || parent.is_some_and(|p| h.starts_with(p))) {
                continue;
            }
            let end = self
                .headings
                .get(k + 1)
                .copied()
                .unwrap_or(self.lines.len());
            for line in &self.lines[i + 1..end] {
                let Some(rest) = line.strip_prefix(':') else {
                    continue;
                };
                let digits = rest
                    .bytes()
                    .take_while(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
                    .count();
                let bytes: Vec<u8> = (0..digits / 2)
                    .map(|j| u8::from_str_radix(&rest[2 * j..2 * j + 2], 16).unwrap())
                    .collect();
                let name: String = rest[digits..]
                    .chars()
                    .skip(1)
                    .take(MAX_NAME_CHARS)
                    .collect();
                match at.get(&bytes) {
                    Some(&j) => out[j] = (Entry { bytes, name }, h.to_string()),
                    None => {
                        at.insert(bytes.clone(), out.len());
                        out.push((Entry { bytes, name }, h.to_string()));
                    }
                }
            }
        }
        out
    }
}

/// The commander shows this many characters of a name (`MAX_NAME_LENGTH` 30, with its NUL).
pub const MAX_NAME_CHARS: usize = 29;

/// The longest line, without its line feed, that `mame_fgets` (buffer `MAX_LINE_LENGTH`
/// 100) reads whole and terminated.
pub const MAX_LINE_CHARS: usize = 98;

/// The file as `readCmds` reads it, line by line, without the line ends: an empty line
/// stays a line (`"\r"` for PinMAME, a heading), and a line of 100 bytes or more is cut
/// after its 100th byte, the rest read as the next line.
fn fgets_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let body = text.strip_suffix('\n').unwrap_or(text);
    for line in body.split('\n') {
        let mut l = line.strip_suffix('\r').unwrap_or(line);
        while l.len() >= 100 {
            let mut cut = 100;
            while !l.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push(&l[..cut]);
            l = &l[cut..];
        }
        out.push(l);
    }
    out
}

/// Parses `0186:Name` (the leading ':' already removed): hex digit pairs, then a
/// separator, then the name. A trailing odd digit is dropped, as in PinMAME.
fn parse_entry(s: &str) -> Option<Entry> {
    let hex_len = s.find(|c: char| !c.is_ascii_hexdigit()).unwrap_or(s.len());
    let bytes: Vec<u8> = (0..hex_len / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    if bytes.is_empty() {
        return None;
    }
    let name = s[hex_len..].get(1..).unwrap_or("").trim().to_string();
    Some(Entry { bytes, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "; comment\n\ndcs:\n:0000:All sound off\n:55aaff00:Set volume (31)\n\
        afm_113:\n# music\n:0001:Music: Prelaunch Loop\n:0186:Martian\n:0001:Music: Renamed\n";

    #[test]
    fn game_and_parent() {
        let d = SoundsDat::parse(SAMPLE);
        assert!(d.game_entries("afm_113b", None).is_empty());
        let e = d.game_entries("afm_113b", Some("afm_113"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].bytes, vec![0x00, 0x01]);
        assert_eq!(e[0].name, "Music: Renamed");
        assert_eq!(e[1].bytes, vec![0x01, 0x86]);
    }

    #[test]
    fn family() {
        let d = SoundsDat::parse(SAMPLE);
        let e = d.family_entries("DCS");
        assert_eq!(e[1].bytes, vec![0x55, 0xaa, 0xff, 0x00]);
        assert_eq!(e[0].name, "All sound off");
    }
}
