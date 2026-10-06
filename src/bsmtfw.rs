//! The BSMT2000's own program (Data East, Sega, Stern Whitestar and Alvin G. sound boards).
//!
//! The BSMT2000 is a TMS320C15 DSP whose mask ROM holds the sound program. With that
//! program, `bsmt2000.bin` (MAME's device ROM, 8 KiB, CRC c2a265af), the PinMAME built in
//! runs the real chip (low level emulation, LLE); without it, it falls back to its older
//! high level emulation (HLE). PinMAME looks the file up like a ROM: in `bsmt2000.zip` or a
//! `bsmt2000/` folder of its ROM path, else inside the game's own zip (then its parent's).
//!
//! rom2altsound never ships the file: the user supplies it. This module brings it into the
//! private PinMAME directory, next to the game's zip, and tells which emulation PinMAME
//! will pick, by doing the same lookup as `src/sound/bsmt2000.c` (`lle_load_firmware`).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The firmware's set name (zip or folder) and file name, as PinMAME looks them up.
pub const SET: &str = "bsmt2000";
pub const FILE: &str = "bsmt2000.bin";
/// The only program PinMAME accepts (any other CRC is ignored, with the HLE used).
pub const CRC: u32 = 0xc2a2_65af;
pub const SIZE: u64 = 0x2000;
/// Set (to anything but `0`) in the environment, PinMAME uses the HLE even with the firmware.
pub const HLE_ENV: &str = "PINMAME_BSMT2000_HLE";

/// Which BSMT2000 emulation ran, for the manifest and the summary.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// `lle` (the chip's own program) or `hle` (PinMAME's older emulation).
    pub emulation: &'static str,
    /// LLE: the program's CRC (always c2a265af).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_crc: Option<String>,
    /// LLE: where PinMAME found it, relative to its ROM directory (e.g. `bsmt2000.zip`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    /// LLE: the user's file it was taken from, when rom2altsound brought it in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    pub note: String,
}

impl Report {
    pub fn label(&self) -> String {
        match self.emulation {
            "lle" => format!(
                "LLE, the chip's own program ({FILE} CRC {} in {})",
                self.firmware_crc.as_deref().unwrap_or("?"),
                self.firmware.as_deref().unwrap_or("?")
            ),
            _ => format!("HLE ({})", self.note),
        }
    }
}

/// What PinMAME will find, decided before it runs: the HLE when forced, else the LLE when a
/// valid firmware is in its ROM directory.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// Why the HLE is forced (`--bsmt-hle`, or the environment variable).
    pub forced_hle: Option<String>,
    /// The firmware PinMAME will load: where, relative to its ROM directory.
    pub found: Option<String>,
    /// The user's file it was brought in from.
    pub source: Option<PathBuf>,
    /// Candidates that were seen but cannot be used (wrong CRC or size, unreadable).
    pub rejected: Vec<String>,
}

impl Status {
    pub fn report(&self) -> Report {
        if let Some(why) = &self.forced_hle {
            return Report {
                emulation: "hle",
                firmware_crc: None,
                firmware: None,
                source: None,
                note: format!("forced by {why}"),
            };
        }
        match &self.found {
            Some(at) => Report {
                emulation: "lle",
                firmware_crc: Some(format!("{CRC:08x}")),
                firmware: Some(at.clone()),
                source: self.source.clone(),
                note: "the BSMT2000's own program ran on an emulated TMS320C15".into(),
            },
            None => {
                let mut note = format!(
                    "no {FILE} with CRC {CRC:08x}: put {SET}.zip next to the ROM zip, in --roms or in ./roms for the chip's own program"
                );
                if !self.rejected.is_empty() {
                    note.push_str(&format!("; ignored: {}", self.rejected.join(", ")));
                }
                Report {
                    emulation: "hle",
                    firmware_crc: None,
                    firmware: None,
                    source: None,
                    note,
                }
            }
        }
    }
}

/// Why the HLE is forced, if it is: the flag, else the environment variable.
pub fn forced_hle(flag: bool) -> Option<String> {
    if flag {
        return Some("--bsmt-hle".into());
    }
    std::env::var(HLE_ENV)
        .ok()
        .filter(|v| !v.is_empty() && !v.starts_with('0'))
        .map(|_| HLE_ENV.to_string())
}

/// Brings the user's firmware into `roms` (the private PinMAME ROM directory): the first
/// valid `bsmt2000.zip` or `bsmt2000/bsmt2000.bin` of `search`, linked (Unix) or copied
/// (Windows) like the game's zip. Then does PinMAME's own lookup in `roms`, for the game
/// `sets` (the ROM, then its parent). Without a valid one in `search`, `roms` is left as it
/// is (a firmware put there by hand is still used).
pub fn stage(search: &[PathBuf], roms: &Path, sets: &[&str], forced_hle: Option<String>) -> Status {
    let mut status = Status {
        forced_hle,
        ..Status::default()
    };
    let mut chosen = None;
    for dir in search {
        let zip = dir.join(format!("{SET}.zip"));
        let folder = dir.join(SET).join(FILE);
        for (path, is_zip) in [(folder, false), (zip, true)] {
            if !path.is_file() {
                continue;
            }
            let check = if is_zip {
                zip_entry(&path, FILE).and_then(|e| e.ok_or_else(|| format!("no {FILE} inside")))
            } else {
                file_crc(&path)
            };
            match check {
                Ok((crc, size)) if crc == CRC && size == SIZE => {
                    chosen = Some((path, is_zip));
                    break;
                }
                Ok((crc, size)) => status
                    .rejected
                    .push(format!("{} (CRC {crc:08x}, {size} bytes)", path.display())),
                Err(e) => status.rejected.push(format!("{} ({e})", path.display())),
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    if let Some((src, is_zip)) = chosen {
        match bring_in(&src, is_zip, roms) {
            Ok(()) => status.source = std::path::absolute(&src).ok(),
            Err(e) => status.rejected.push(format!("{} ({e})", src.display())),
        }
    }
    status.found = lookup(roms, sets);
    if status.found.is_none() {
        status.source = None;
    }
    status
}

/// Replaces both forms of the firmware in `roms` by the chosen one: a folder there would
/// win over a zip (PinMAME tries the folder first).
fn bring_in(src: &Path, is_zip: bool, roms: &Path) -> Result<(), String> {
    let src = std::path::absolute(src).map_err(|e| e.to_string())?;
    let zip = roms.join(format!("{SET}.zip"));
    let folder = roms.join(SET);
    let _ = std::fs::remove_file(&zip);
    // A link to a folder goes like a file; a real folder keeps everything but our file.
    if folder.is_symlink() {
        let _ = std::fs::remove_file(&folder);
    } else {
        let _ = std::fs::remove_file(folder.join(FILE));
    }
    let dst = if is_zip {
        zip
    } else {
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        folder.join(FILE)
    };
    #[cfg(unix)]
    let r = std::os::unix::fs::symlink(&src, &dst);
    #[cfg(not(unix))]
    let r = std::fs::copy(&src, &dst).map(|_| ());
    r.map_err(|e| format!("{}: {e}", dst.display()))
}

/// PinMAME's lookup (`lle_load_firmware`): the `bsmt2000` set, then each game set; within a
/// set, the folder before the zip, and the first file found decides for that set (a wrong
/// one makes PinMAME go on to the next set). Returns where the valid firmware is.
pub fn lookup(roms: &Path, sets: &[&str]) -> Option<String> {
    for set in std::iter::once(SET).chain(sets.iter().copied()) {
        let in_folder = roms.join(set).join(FILE);
        if in_folder.is_file() {
            if file_crc(&in_folder).is_ok_and(|(crc, size)| crc == CRC && size == SIZE) {
                return Some(format!("{set}/{FILE}"));
            }
            continue;
        }
        let zip = roms.join(format!("{set}.zip"));
        if let Ok(Some((crc, size))) = zip_entry(&zip, FILE)
            && crc == CRC
            && size == SIZE
        {
            return Some(format!("{set}.zip"));
        }
    }
    None
}

fn file_crc(path: &Path) -> Result<(u32, u64), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok((crc32(&data), data.len() as u64))
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// The CRC and uncompressed size of the first entry of a zip whose base name is `name`
/// (case-insensitive, as PinMAME's `equal_filename`), read from the central directory.
/// `Ok(None)`: the zip has no such entry. No Zip64 (ROM zips are small).
pub fn zip_entry(path: &Path, name: &str) -> Result<Option<(u32, u64)>, String> {
    let mut f = File::open(path).map_err(|e| e.to_string())?;
    let len = f.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
    // End of central directory: 22 bytes, plus a comment of up to 65535.
    let tail_len = len.min(22 + 0xffff);
    f.seek(SeekFrom::Start(len - tail_len))
        .map_err(|e| e.to_string())?;
    let mut tail = vec![0; tail_len as usize];
    f.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let eocd = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("not a zip file")?;
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let entries = u16_at(&tail, eocd + 10);
    let cd_size = u32_at(&tail, eocd + 12) as u64;
    let cd_offset = u32_at(&tail, eocd + 16) as u64;
    if cd_offset + cd_size > len {
        return Err("damaged zip (central directory out of the file)".into());
    }
    f.seek(SeekFrom::Start(cd_offset))
        .map_err(|e| e.to_string())?;
    let mut cd = vec![0; cd_size as usize];
    f.read_exact(&mut cd).map_err(|e| e.to_string())?;
    let mut i = 0;
    for _ in 0..entries {
        if i + 46 > cd.len() || cd[i..i + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err("damaged zip (central directory)".into());
        }
        let crc = u32_at(&cd, i + 16);
        let size = u32_at(&cd, i + 24) as u64;
        let n = u16_at(&cd, i + 28) as usize;
        let extra = u16_at(&cd, i + 30) as usize;
        let comment = u16_at(&cd, i + 32) as usize;
        let Some(entry) = cd.get(i + 46..i + 46 + n) else {
            return Err("damaged zip (central directory)".into());
        };
        let entry = String::from_utf8_lossy(entry);
        let base = entry.rsplit('/').next().unwrap_or(&entry);
        if base.eq_ignore_ascii_case(name) {
            return Ok(Some((crc, size)));
        }
        i += 46 + n + extra + comment;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_check_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    /// A stored (uncompressed) zip with the given entries, as a zip tool writes it.
    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();
        for (name, data) in entries {
            let offset = out.len() as u32;
            let crc = crc32(data);
            let header = |sig: [u8; 4], central: bool| {
                let mut h = sig.to_vec();
                if central {
                    h.extend(20u16.to_le_bytes()); // made by
                }
                h.extend(20u16.to_le_bytes()); // needed
                h.extend(0u16.to_le_bytes()); // flags
                h.extend(0u16.to_le_bytes()); // stored
                h.extend(0u32.to_le_bytes()); // time, date
                h.extend(crc.to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((name.len() as u16).to_le_bytes());
                h.extend(0u16.to_le_bytes()); // extra
                if central {
                    h.extend(0u16.to_le_bytes()); // comment
                    h.extend(0u16.to_le_bytes()); // disk
                    h.extend(0u16.to_le_bytes()); // internal attributes
                    h.extend(0u32.to_le_bytes()); // external attributes
                    h.extend(offset.to_le_bytes());
                }
                h.extend(name.as_bytes());
                h
            };
            out.extend(header([0x50, 0x4b, 0x03, 0x04], false));
            out.extend(*data);
            cd.extend(header([0x50, 0x4b, 0x01, 0x02], true));
        }
        let cd_offset = out.len() as u32;
        out.extend(&cd);
        out.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((entries.len() as u16).to_le_bytes());
        out.extend((cd.len() as u32).to_le_bytes());
        out.extend(cd_offset.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    fn scratch(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("rom2altsound-bsmtfw-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn zip_entries_by_base_name() {
        let d = scratch("zip");
        let p = d.join("a.zip");
        std::fs::write(
            &p,
            zip(&[("x.rom", b"abc"), ("dir/BSMT2000.BIN", b"123456789")]),
        )
        .unwrap();
        assert_eq!(zip_entry(&p, FILE).unwrap(), Some((0xcbf4_3926, 9)));
        assert_eq!(zip_entry(&p, "y.rom").unwrap(), None);
        std::fs::write(&p, b"not a zip").unwrap();
        assert!(zip_entry(&p, FILE).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn lookup_follows_pinmame() {
        let d = scratch("lookup");
        // A wrong firmware is no firmware: the game's zip is not even looked at for it.
        std::fs::write(d.join("game.zip"), zip(&[(FILE, &[1u8; SIZE as usize])])).unwrap();
        assert_eq!(lookup(&d, &["game"]), None);
        // Neither stage nor lookup ever need the real program: CRC and size are all they read.
        let mut fake = vec![0u8; SIZE as usize];
        // Patch four bytes so that the CRC comes out right (CRC32 is linear: solve for them).
        fix_crc(&mut fake, CRC);
        assert_eq!(crc32(&fake), CRC);
        std::fs::write(d.join("parent.zip"), zip(&[(FILE, &fake)])).unwrap();
        assert_eq!(
            lookup(&d, &["game", "parent"]).as_deref(),
            Some("parent.zip")
        );
        // The bsmt2000 set comes first, and a folder before a zip.
        let src = scratch("lookup-src");
        std::fs::create_dir_all(src.join(SET)).unwrap();
        std::fs::write(src.join(SET).join(FILE), &fake).unwrap();
        let s = stage(std::slice::from_ref(&src), &d, &["game"], None);
        assert_eq!(s.found.as_deref(), Some("bsmt2000/bsmt2000.bin"));
        assert_eq!(s.report().emulation, "lle");
        assert_eq!(
            stage(&[], &d, &["game"], Some("--bsmt-hle".into()))
                .report()
                .emulation,
            "hle"
        );
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Sets the last four bytes so that the CRC32 of `data` is `want`.
    fn fix_crc(data: &mut [u8], want: u32) {
        let n = data.len();
        data[n - 4..].fill(0);
        // CRC32 of the data with a zero tail, then the four bytes that bring it to `want`:
        // running the register backwards from `want` over four bytes gives what they must be.
        let mut reg = !want;
        for _ in 0..32 {
            reg = if reg & 0x8000_0000 != 0 {
                ((reg ^ 0xedb8_8320) << 1) | 1
            } else {
                reg << 1
            };
        }
        let state = !crc32(&data[..n - 4]);
        let tail = (reg ^ state).to_le_bytes();
        data[n - 4..].copy_from_slice(&tail);
    }
}
