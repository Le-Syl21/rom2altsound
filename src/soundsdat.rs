//! Parser for PinMAME's `release/sounds.dat`, the list of named sound commands used by
//! its sound commander (format documented at the top of the file, reader in
//! src/wpc/snd_cmd.c `readCmds`).

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
