//! PinMAME's driver table, read from the library linked in: every game, its ROM files as
//! its ROM_START block declares them, its CPUs and sound chips, and its sound board. The
//! ROM verifier (`rom2altsound roms`) identifies ROM zips against it, so what it says always
//! matches the emulator that extracts the sounds.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_uint};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;

use crate::ffi;

include!(concat!(env!("OUT_DIR"), "/sndbrd_names.rs"));

/// One ROM file of a game.
#[derive(Debug, Clone, Serialize)]
pub struct RomFile {
    pub name: String,
    pub size: u64,
    /// CRC32 (None: not dumped, or no CRC listed).
    pub crc: Option<u32>,
    /// SHA-1, lowercase hex.
    pub sha1: Option<String>,
    /// The memory region it is loaded into ("cpu1", "sound1"...; "cpu1+sound1" for a file
    /// loaded into two).
    pub region: String,
    /// Sound ROM: its region is marked sound-only (`SOUNDREGION`, loaded only with sound
    /// on), is a REGION_SOUNDn, or is the program region of an audio CPU.
    pub sound: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// No dump exists (`NO_DUMP`): never required.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_dump: bool,
    /// PinMAME only knows a bad dump (`BAD_DUMP`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bad_dump: bool,
}

/// One game (driver) of PinMAME.
#[derive(Debug, Clone, Serialize)]
pub struct Driver {
    pub name: String,
    pub parent: Option<String>,
    pub description: String,
    pub year: String,
    pub manufacturer: String,
    pub source: String,
    pub roms: Vec<RomFile>,
    pub cpus: Vec<String>,
    pub sound_chips: Vec<String>,
}

impl Driver {
    /// The files a complete set must hold (every ROM with a known dump).
    pub fn required(&self) -> impl Iterator<Item = (usize, &RomFile)> {
        self.roms
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.no_dump && r.crc.is_some())
    }

    /// The sound ROM id: the SHA-1 of the sound ROMs' SHA-1s, see [`sound_rom_id`].
    pub fn sound_rom_id(&self) -> Option<String> {
        sound_rom_id(self.roms.iter())
    }
}

/// The sound ROM id of a game, the key under which games that share their sound ROMs (all
/// the revisions of a game, usually) are grouped: the SHA-1 of the text made of the
/// distinct SHA-1s (lowercase hex) of its sound ROMs (see [`RomFile::sound`]; ROMs with no
/// dump left out), sorted, each followed by a line feed (`\n`). None when the game has no
/// sound ROM (Stern SAM: the sound data is in the main image) or when a sound ROM has no
/// SHA-1 in PinMAME's table.
pub fn sound_rom_id<'a>(roms: impl Iterator<Item = &'a RomFile>) -> Option<String> {
    let mut sums: Vec<&str> = Vec::new();
    for r in roms.filter(|r| r.sound && !r.no_dump) {
        sums.push(r.sha1.as_deref()?);
    }
    if sums.is_empty() {
        return None;
    }
    sums.sort_unstable();
    sums.dedup();
    let mut s = crate::sha1::Sha1::default();
    for h in sums {
        s.update(h.as_bytes());
        s.update(b"\n");
    }
    Some(crate::sha1::to_hex(&s.finish()))
}

/// What a game's init sets up about its sound board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    /// `core_gameData->hw.soundBoard` (`SNDBRD_TYPE(main, sub)`; 0 is SNDBRD_NONE).
    pub hw_board: u32,
    /// `core_gameData->gen` (GEN_* in wpc/gen.h).
    pub generation: u64,
    /// The CPU family's machine init, when it is one that picks the board itself (see
    /// [`Board::sound_boards`]).
    pub core_init: Option<String>,
    /// The game's init function crashed or exited after setting its game data (it needs a
    /// machine); the game data is still the one it set.
    pub init_crashed: bool,
}

/// src/wpc/gen.h
mod generation {
    pub const WPCALPHA_1: u64 = 0x1;
    pub const WPCALPHA_2: u64 = 0x2;
    pub const WPCDMD: u64 = 0x4;
    pub const WPCFLIPTRON: u64 = 0x8;
    pub const WPCDCS: u64 = 0x10;
    pub const WPCSECURITY: u64 = 0x20;
    pub const WPC95DCS: u64 = 0x40;
    pub const WPC95: u64 = 0x80;
    pub const S11X: u64 = 0x100;
    pub const S11B2: u64 = 0x200;
    pub const S11C: u64 = 0x400;
    pub const S9: u64 = 0x800;
    pub const DE: u64 = 0x1000;
    pub const S3C: u64 = 0x80000;
    pub const S11: u64 = 0x8000000;
}

fn board(name: &str) -> u32 {
    SNDBRD_NAMES
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(v, _)| *v)
        .unwrap_or_else(|| panic!("{name} is not in sndbrd.h"))
}

impl Board {
    /// The sound boards the game's machine init starts, in board order, as PinMAME's own
    /// machine inits choose them: most pass `hw.soundBoard` on, these pick by themselves
    /// (the `switch` statements of wpc.c `MACHINE_INIT(wpc)`, s11.c `MACHINE_INIT(s11)` and
    /// `(s9pf)`, s7.c `(s7)`/`(s7S6)`/`(s7nd)`/`(rr)`, s6.c, s4.c, se.c `(se)`/`(se3)`, p2k.c).
    /// Display boards (the Data East DMD boards, which PinMAME runs as a sound board 0) are
    /// not listed. Empty: no sound board (Stern SAM: the sound is in the main image).
    pub fn sound_boards(&self) -> Vec<u32> {
        use generation as g;
        let hw = self.hw_board;
        let generation = self.generation;
        let v: Vec<&str> = match self.core_init.as_deref() {
            Some("wpc") => match generation {
                g::WPCALPHA_1 => vec!["SNDBRD_S11CS"],
                g::WPCALPHA_2 | g::WPCDMD | g::WPCFLIPTRON => vec!["SNDBRD_WPCS"],
                g::WPCDCS | g::WPCSECURITY | g::WPC95DCS => vec!["SNDBRD_DCS"],
                g::WPC95 => vec!["SNDBRD_DCS95"],
                _ => vec![],
            },
            Some("s11") => match generation {
                g::S9 => vec!["SNDBRD_S9S"],
                g::S11 => vec!["SNDBRD_S11S"],
                g::S11X => vec!["SNDBRD_S11XS", "SNDBRD_S11CS"],
                g::S11B2 => vec!["SNDBRD_S11BS", "SNDBRD_S11JS"],
                g::S11C => vec!["SNDBRD_S11CS"],
                g::DE => vec!["SNDBRD_DE1S"],
                // GEN_DEDMD16/32/64: board 0 is the DMD (hw.display), board 1 hw.soundBoard.
                _ => return if hw != 0 { vec![hw] } else { vec![] },
            },
            Some("s9pf") | Some("rr") => vec!["SNDBRD_S9S"],
            Some("s7") | Some("s7S6") | Some("s6") => vec!["SNDBRD_S67S"],
            Some("s7nd") => vec!["SNDBRD_S7S_ND"],
            Some("s4") if generation & g::S3C != 0 => vec![],
            Some("s4") => return vec![if hw != 0 { hw } else { board("SNDBRD_S67S") }],
            Some("se") => vec!["SNDBRD_DE2S"],
            Some("se3") => vec!["SNDBRD_DE3S"],
            Some("p2k") => vec!["SNDBRD_DCSP2K"],
            _ => return if hw != 0 { vec![hw] } else { vec![] },
        };
        v.into_iter().map(board).collect()
    }

    /// The family name: the SNDBRD_* names of [`Board::sound_boards`] joined by `+`
    /// ("SNDBRD_S11XS+SNDBRD_S11CS"), "SNDBRD_NONE" when there is none.
    pub fn family(&self) -> String {
        let b = self.sound_boards();
        if b.is_empty() {
            return "SNDBRD_NONE".into();
        }
        b.iter()
            .map(|&b| board_name(b))
            .collect::<Vec<_>>()
            .join("+")
    }
}

/// The SNDBRD_* name of a board type ("SNDBRD_DCS"), or its hex value when unnamed.
pub fn board_name(board: u32) -> String {
    match SNDBRD_NAMES.iter().find(|(v, _)| *v == board) {
        Some((_, n)) => (*n).to_owned(),
        None => format!("SNDBRD_TYPE({},{})", board >> 8, board & 0xFF),
    }
}

/// The sound board interface's own name (PinMAME's `typestr`, e.g. "WMSDCS"), if any.
pub fn board_interface(board: u32) -> Option<String> {
    ffi::cstr(unsafe { ffi::shim_sndbrd_name(board as c_uint) })
}

fn text(i: c_int, field: c_int) -> String {
    ffi::cstr(unsafe { ffi::shim_driver_text(i, field) }).unwrap_or_default()
}

fn cbuf(b: &[c_char]) -> String {
    let bytes: Vec<u8> = b
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Every game of the PinMAME linked in, in its driver list order.
pub fn load() -> Vec<Driver> {
    let n = unsafe { ffi::shim_driver_count() };
    let mut out = Vec::with_capacity(n.max(0) as usize);
    for i in 0..n {
        let flags = unsafe { ffi::shim_driver_flags(i) };
        // NOT_A_DRIVER: the root and the "containers", which are no game.
        if flags & 0x4000 != 0 {
            continue;
        }
        let mut audio_mask: c_uint = 0;
        unsafe { ffi::shim_driver_machine(i, &mut audio_mask) };
        let cpus = (0..8)
            .filter_map(|k| ffi::cstr(unsafe { ffi::shim_machine_cpu(k) }))
            .collect::<Vec<_>>();
        let sound_chips = (0..5)
            .filter_map(|k| ffi::cstr(unsafe { ffi::shim_machine_sound(k) }))
            .collect();
        let mut roms = Vec::new();
        let mut j = 0;
        loop {
            // SAFETY: plain data, filled by the shim; `name` points into the static table.
            let mut r: ffi::ShimRom = unsafe { std::mem::zeroed() };
            if unsafe { ffi::shim_driver_rom(i, j, &mut r) } == 0 {
                break;
            }
            j += 1;
            let region = ffi::cstr(unsafe { ffi::shim_region_name(r.region) })
                .unwrap_or_else(|| format!("0x{:x}", r.region));
            let audio_cpu_region = region
                .strip_prefix("cpu")
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n >= 1 && audio_mask & (1 << (n - 1)) != 0);
            let crc = cbuf(&r.crc);
            let sha1 = cbuf(&r.sha1);
            let name = ffi::cstr(r.name).unwrap_or_default();
            let crc = u32::from_str_radix(&crc, 16).ok();
            let sound = r.sound_only != 0 || region.starts_with("sound") || audio_cpu_region;
            // The same file loaded into two regions (Pinball 2000's boot ROM, in the CPU's
            // and the sound board's): one file, in both regions.
            if let Some(prev) = roms.iter_mut().find(|p: &&mut RomFile| {
                p.name.eq_ignore_ascii_case(&name) && p.crc == crc && crc.is_some()
            }) {
                prev.region = format!("{}+{region}", prev.region);
                prev.sound |= sound;
                continue;
            }
            roms.push(RomFile {
                name,
                size: r.length as u64,
                crc,
                sha1: (sha1.len() == 40).then(|| sha1.to_ascii_lowercase()),
                sound,
                region,
                optional: r.optional != 0,
                no_dump: r.no_dump != 0,
                bad_dump: r.bad_dump != 0,
            });
        }
        let parent = text(i, 1);
        // "/home/.../src/wpc/wpcgames.c" -> "wpc/wpcgames.c"
        let source = text(i, 5).replace('\\', "/");
        let source = source
            .rsplit_once("/src/")
            .map_or(source.as_str(), |(_, s)| s)
            .to_owned();
        out.push(Driver {
            name: text(i, 0),
            parent: (!parent.is_empty()).then_some(parent),
            description: text(i, 2),
            year: text(i, 3),
            manufacturer: text(i, 4),
            source,
            roms,
            cpus,
            sound_chips,
        });
    }
    out
}

/// Internal subcommand (`__driver-boards <start>`): prints the game data of every driver
/// from `start` on (shim/shim.c `shim_print_driver_boards`).
pub fn print_boards(args: Vec<String>) {
    let start: c_int = args.first().and_then(|a| a.parse().ok()).unwrap_or(0);
    unsafe { ffi::shim_print_driver_boards(start) };
}

/// The sound board of every driver, by name: each driver's init function is run in child
/// processes of this program (`__driver-boards`), as it may need a running machine and crash
/// (the child then reports the game data it set, and the next child starts after it). A
/// driver with no game data has no entry.
pub fn boards(exe: &Path) -> Result<HashMap<String, Board>, String> {
    let n = unsafe { ffi::shim_driver_count() };
    let mut by_index: HashMap<c_int, Board> = HashMap::new();
    let mut start: c_int = 0;
    let mut restarts = 0;
    while start < n {
        let mut child = Command::new(exe)
            .arg("__driver-boards")
            .arg(start.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for l in BufReader::new(stdout).lines() {
                let Ok(l) = l else { break };
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
        // (index, crashed) of the last line.
        let mut last: Option<(c_int, bool)> = None;
        loop {
            match rx.recv_timeout(Duration::from_secs(20)) {
                Ok(l) => {
                    if let Some((i, b)) = parse_board_line(&l) {
                        last = Some((i, b.init_crashed));
                        if let Some(b) = b.has_data.then_some(b.board) {
                            by_index.insert(i, b);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    break;
                }
            }
        }
        let _ = child.wait();
        let _ = reader.join();
        start = match last {
            // Done, or it crashed on a driver and said so: go on after it.
            Some((i, _)) => i + 1,
            // Nothing printed at all: the driver at `start` is skipped.
            None => start + 1,
        };
        if last.is_some_and(|(i, crashed)| !crashed && i + 1 < n) {
            // It stopped after a driver that went well: the next one hung or died silently.
            start += 1;
        }
        restarts += 1;
        if restarts > 2000 {
            return Err("too many driver init functions crashed".into());
        }
    }
    let mut out = HashMap::new();
    for i in 0..n {
        if let Some(b) = by_index.remove(&i) {
            out.insert(text(i, 0), b);
        }
    }
    Ok(out)
}

struct BoardLine {
    has_data: bool,
    board: Board,
}

impl std::ops::Deref for BoardLine {
    type Target = Board;
    fn deref(&self) -> &Board {
        &self.board
    }
}

/// "R2A <index> <ok> <hw.soundBoard> <gen> <core init or -> <ok|crash>"
fn parse_board_line(l: &str) -> Option<(c_int, BoardLine)> {
    let f: Vec<&str> = l.trim().split('\t').collect();
    let ["R2A", i, ok, hw, generation, init, how] = f.as_slice() else {
        return None;
    };
    Some((
        i.parse().ok()?,
        BoardLine {
            has_data: ok.parse::<c_int>().ok()? != 0,
            board: Board {
                hw_board: hw.parse().ok()?,
                generation: generation.parse().ok()?,
                core_init: (*init != "-").then(|| (*init).to_owned()),
                init_crashed: *how == "crash",
            },
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom(sha1: &str, sound: bool) -> RomFile {
        RomFile {
            name: "x".into(),
            size: 1,
            crc: Some(1),
            sha1: Some(sha1.into()),
            region: "sound1".into(),
            sound,
            optional: false,
            no_dump: false,
            bad_dump: false,
        }
    }

    #[test]
    fn sound_rom_id_is_order_free_and_documented() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let roms = [rom(&b, true), rom(&a, true), rom(&"c".repeat(40), false)];
        let id = sound_rom_id(roms.iter()).unwrap();
        // SHA-1 of "aaa...\nbbb...\n": sorted, one per line, the non-sound ROM left out.
        assert_eq!(id, crate::sha1::hex(format!("{a}\n{b}\n").as_bytes()));
        let swapped = [rom(&a, true), rom(&b, true), rom(&b, true)];
        assert_eq!(sound_rom_id(swapped.iter()).unwrap(), id);
        assert_eq!(sound_rom_id([rom(&a, false)].iter()), None);
    }

    #[test]
    fn boards_chosen_like_the_machine_inits() {
        let b = |hw: &str, generation: u64, init: Option<&str>| Board {
            hw_board: if hw.is_empty() { 0 } else { board(hw) },
            generation,
            core_init: init.map(Into::into),
            init_crashed: false,
        };
        // Twilight Zone: WPC Fliptronic, hw.soundBoard unset.
        assert_eq!(b("", 0x8, Some("wpc")).family(), "SNDBRD_WPCS");
        assert_eq!(b("", 0x80, Some("wpc")).family(), "SNDBRD_DCS95");
        assert_eq!(
            b("", 0x100, Some("s11")).family(),
            "SNDBRD_S11XS+SNDBRD_S11CS"
        );
        // Data East 128x32: board 0 is the DMD; the sound board is hw.soundBoard.
        assert_eq!(
            b("SNDBRD_DE2S", 0x4000, Some("s11")).family(),
            "SNDBRD_DE2S"
        );
        assert_eq!(b("", 0x4000000000, Some("se")).family(), "SNDBRD_DE2S");
        assert_eq!(b("SNDBRD_BY61", 0x400000, None).family(), "SNDBRD_BY61");
        assert_eq!(b("", 0x100000000000, None).family(), "SNDBRD_NONE");
        let l = parse_board_line("R2A\t7\t1\t512\t8\twpc\tcrash").unwrap();
        assert_eq!(l.0, 7);
        assert!(l.1.init_crashed && l.1.has_data);
        assert_eq!(l.1.family(), "SNDBRD_WPCS");
    }

    #[test]
    fn board_names_come_from_the_header() {
        assert_eq!(board_name(0x0300), "SNDBRD_DCS");
        assert_eq!(board_name(0x0200), "SNDBRD_WPCS");
        assert_eq!(board_name(0x0400), "SNDBRD_BY32");
    }
}
