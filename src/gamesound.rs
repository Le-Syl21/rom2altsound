//! Game-driven sound: the boards that take no sound command.
//!
//! On Stern's SB-300 (`ST300`) and its Astro board tester (`ASTRO`), the game CPU makes
//! every sound itself: it writes the sound chip's timers over time. There is no command byte between the game and its board, so
//! the sweep of the other boards has nothing to send. The game's own program has a sound
//! layer, though: a routine that starts a sound and a request the rest of the game uses to
//! ask for one (a pointer to a sound script, a sound number in a RAM byte...). This module
//! reads that layer in the game's program image and gives, for each sound the game can
//! play, the RAM writes that ask for it the way the game's own code does, and the writes
//! that silence it the way the game stops its sounds. The extraction then leaves the game
//! running (it idles in its attract mode) and records each one.
//!
//! The ids are the game's own internal sound ids (a script address, a sound number), not
//! sound commands: no command reaches AltSound on these machines, so the packs cannot play
//! in VPinball. They are a recording of the game's sounds, for measurement and archive.

use std::collections::BTreeMap;

/// One byte the game CPU's memory map gets (RAM: the game's sound request).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Poke {
    pub addr: u32,
    pub val: u8,
}

/// The most bytes one request writes (they all go in before the CPU runs again).
pub const MAX_POKES: usize = 24;

/// How the game asks for a sound (or stops them).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Bytes written into the game's RAM (its sound request).
    Pokes(Vec<Poke>),
}

/// A sound of the game: its id and how the game asks for it.
#[derive(Clone, Debug)]
pub struct GameSound {
    /// The game's own id for it (a script address, a sound number).
    pub id: u32,
    /// Where the game's program refers to it, for the manifest.
    pub refs: String,
    pub start: Request,
}

/// A game's sound layer, read in its program.
#[derive(Clone, Debug)]
pub struct Layer {
    /// How the layer was found: the routines, the RAM request, the catalog.
    pub what: String,
    /// What an id is, for the manifest and the docs.
    pub id_is: String,
    /// How many hex digits an id has.
    pub id_digits: usize,
    pub sounds: Vec<GameSound>,
    /// Silences what is playing, as the game does.
    pub stop: Request,
}

/// The families handled here, by PinMAME's board type string and sub-type: every one
/// whose sound the game CPU makes itself, with no command (the ST300V's speech is a
/// command, swept as such; its effects are not done here).
pub fn game_driven(typestr: &str, subtype: i32) -> bool {
    match typestr {
        "ST300" => subtype == 0,
        "ASTRO" => true,
        _ => false,
    }
}

/// Reads the sound layer of the running game, from its program image (the game CPU's
/// memory region, as the driver loaded it).
pub fn find(typestr: &str, image: &[u8]) -> Result<Layer, String> {
    match typestr {
        "ST300" | "ASTRO" => st300::find(image),
        _ => Err(format!("no reader for the {typestr} game's sound layer")),
    }
}

/// `pat` in `hay` from `from`, `None` in `pat` matching any byte.
fn find_pat(hay: &[u8], pat: &[Option<u8>], from: usize, to: usize) -> Option<usize> {
    let to = to.min(hay.len());
    (from..to.saturating_sub(pat.len() - 1)).find(|&i| {
        pat.iter()
            .zip(&hay[i..])
            .all(|(p, &b)| p.is_none_or(|p| p == b))
    })
}

/// A byte pattern written as hex pairs, `??` for any byte.
fn pat(s: &str) -> Vec<Option<u8>> {
    s.split_whitespace()
        .map(|t| (t != "??").then(|| u8::from_str_radix(t, 16).expect("hex pattern")))
        .collect()
}

fn be16(m: &[u8], a: usize) -> u16 {
    u16::from_be_bytes([m[a], m[a + 1]])
}

/// Stern MPU-200 (`ST300`, 1979-1982): sound scripts run by the game's interrupt.
///
/// Every Stern game of the SB-300 era has, in its interrupt handler, a small interpreter of
/// sound scripts: a 16-bit pointer in RAM (`$6D` on the first games, `$74` after) to the
/// next script byte, and a delay byte (`$4C`, then `$53`). When the delay has run out, the
/// handler reads the script: register writes to the SB-300's timers and control (`A0`..`A7`,
/// `C0`), delays, counted loops, jumps, pitch steps, and an end that silences the board and
/// clears the pointer. A sound is a script; the game starts one by storing its address in
/// the pointer and clearing the delay (Meteor's `5808`: `JSR` to the routine that reads the
/// word after the call, `STX $6D`, `CLR $4C`). The game's thread code asks for a sound with
/// a thread instruction (`57` or `58`) followed by the script's address; some sounds come
/// from tables (a sound number turned into a table entry) or from a direct `LDX #script`.
///
/// The script format changed from game to game (six interpreters among the 15 games of the
/// family, told apart by their code): the opcodes' lengths are read for each, so that a
/// candidate address is only taken when the bytes there are a script of that interpreter.
mod st300 {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Op {
        Bad,
        /// Sets a pitch (a timer latch `A2`..`A7`, both timers, a pitch step): `n` bytes.
        Pitch(u8),
        /// Sets up the rest of the sound (the timers' control `A0`/`A1`, the control bits,
        /// `C0`): `n` bytes.
        Setup(u8),
        /// Loads a loop counter: `n` bytes (a script may start with it).
        Counter(u8),
        /// Any other op that goes on to the next one (delay, no-op...): `n` bytes.
        Step(u8),
        /// Counted loop: 3 bytes, the target word last.
        Loop,
        Jump,
        /// Script subroutine (`E`): 3 bytes, the target word last.
        Call,
        /// Calls native code, which may move the pointer itself (`B`'s `1A`): the static
        /// reading stops there.
        Native,
        /// Returns from a script subroutine.
        Ret,
        /// Ends the script.
        End,
        /// Silences the board and ends the script.
        Silence,
    }

    /// One interpreter: how its entry looks, and its opcodes.
    struct Variant {
        name: &'static str,
        /// Its entry: `LDX ptr`, the test, then the first reads of the script (branch
        /// offsets left out: the MOD sets of 2010-2020 moved the code around, as Dragonfist
        /// MOD 14 and Nine Ball's ball-handling MODs, whose interpreters are the same).
        sig: &'static str,
        ops: fn(u8) -> Op,
        /// The opcode that silences and ends (the stop).
        silence: u8,
        games: &'static str,
    }

    /// The common core of the `DE` interpreters (A, A', B', C, E): `00` delay, `01` end,
    /// `02` counter, `03` loop, `04` jump, `05` C0, `06` both timers' step, `07` one step
    /// each, `08` silence, `09`..`0F` the control bits.
    fn core(op: u8) -> Op {
        match op {
            0x00 => Op::Step(2),
            0x01 => Op::End,
            0x02 => Op::Counter(2),
            0x03 => Op::Loop,
            0x04 => Op::Jump,
            0x05 => Op::Setup(2),
            0x06 => Op::Pitch(3),
            0x07 => Op::Pitch(5),
            0x08 => Op::Silence,
            0x09..=0x0F => Op::Setup(1),
            _ => Op::Bad,
        }
    }
    /// A register write, `op` being the register (`A0`..`A7`).
    fn reg(op: u8) -> Op {
        match op {
            0xA0 | 0xA1 => Op::Setup(3),
            0xA2..=0xA7 => Op::Pitch(3),
            _ => Op::Bad,
        }
    }
    /// Meteor, Galaxy, Ali.
    fn ops_a(op: u8) -> Op {
        match op {
            0x00..=0x0F => core(op),
            _ => reg(op),
        }
    }
    /// Big Game: `10` loads both timers (4 bytes), `11` and up do not read the script.
    fn ops_a2(op: u8) -> Op {
        match op {
            0x00..=0x0F => core(op),
            0x10 => Op::Pitch(5),
            0x11..=0x7F => Op::Step(1),
            _ => reg(op),
        }
    }
    /// Seawitch and Nine Ball: as Big Game, with a second counter (`11`, `12`); Nine
    /// Ball's `13` reloads the delay (one byte either way).
    fn ops_b2(op: u8) -> Op {
        match op {
            0x00..=0x0F => core(op),
            0x10 => Op::Pitch(5),
            0x11 => Op::Counter(2),
            0x12 => Op::Loop,
            0x13..=0x7F => Op::Step(1),
            _ => reg(op),
        }
    }
    /// Viper, Dragonfist, Iron Maiden, Lazer Lord, Cue: the registers are `20`..`27`,
    /// `16`/`17` call and return, `40` and up are one-byte ops (priority, delay in the
    /// opcode).
    fn ops_e(op: u8) -> Op {
        match op {
            0x00..=0x0F => core(op),
            0x10 => Op::Pitch(5),
            0x11 => Op::Counter(2),
            0x12 => Op::Loop,
            0x13..=0x15 | 0x18..=0x1F => Op::Step(1),
            0x16 => Op::Call,
            0x17 => Op::Ret,
            0x20..=0x27 => reg(op | 0x80),
            0x40..=0xFF => Op::Step(1),
            _ => Op::Bad,
        }
    }
    /// Cheetah, Star Gazer, Quicksilver: `00` silences, `01`..`07` the control bits, then
    /// a table of handlers from `08` to `1C` (checked by `table_ok` with `B_HANDLERS`).
    fn ops_b(op: u8) -> Op {
        match op {
            0x00 => Op::Silence,
            0x01..=0x07 | 0x13 | 0x18 | 0x19 | 0x1B => Op::Setup(1),
            0x08 => Op::Step(2),
            0x0A | 0x11 => Op::Counter(2),
            0x09 => Op::End,
            0x0B | 0x12 => Op::Loop,
            0x0C => Op::Jump,
            0x0D => Op::Setup(2),
            0x0E => Op::Pitch(3),
            0x0F | 0x10 => Op::Pitch(5),
            0x14..=0x17 => Op::Setup(3),
            0x1A => Op::Native,
            0x1C => Op::Step(1),
            _ => reg(op),
        }
    }

    /// Gamatron (Flight 2000's system): `B`'s opcodes `00`..`19` with `E`'s registers
    /// (`20`..`27`) and one-byte ops (`40` up); `1C`/`1D` scale a timer (one byte), `1E`/`1F`
    /// call and return (checked by `table_ok` with `F_HANDLERS`).
    fn ops_f(op: u8) -> Op {
        match op {
            0x00 => Op::Silence,
            0x01..=0x07 | 0x13 | 0x16 | 0x17 | 0x19 => Op::Setup(1),
            0x08 => Op::Step(2),
            0x09 => Op::End,
            0x0A | 0x11 => Op::Counter(2),
            0x0B | 0x12 => Op::Loop,
            0x0C => Op::Jump,
            0x0D => Op::Setup(2),
            0x0E => Op::Pitch(3),
            0x0F | 0x10 => Op::Pitch(5),
            0x14 | 0x15 => Op::Setup(3),
            0x18 => Op::Native,
            0x1A | 0x1B => Op::Step(1),
            0x1C | 0x1D => Op::Pitch(2),
            0x1E => Op::Call,
            0x1F => Op::Ret,
            0x20..=0x27 => reg(op | 0x80),
            0x40..=0xFF => Op::Step(1),
            _ => Op::Bad,
        }
    }

    const VARIANTS: &[Variant] = &[
        Variant {
            name: "A0",
            sig: "DE ?? 27 ?? A6 00 08 97 ?? 2A",
            ops: ops_a,
            silence: 0x08,
            games: "S.A.M. III board tester",
        },
        Variant {
            name: "A",
            sig: "DE ?? 27 ?? A6 00 08 B7 02 ?? 2A",
            ops: ops_a,
            silence: 0x08,
            games: "Meteor, Galaxy, Ali",
        },
        Variant {
            name: "A'",
            sig: "DE ?? 26 ?? FE 02 ?? DF A2 FE 02 ?? DF A4 3B A6 00 08 B7 02",
            ops: ops_a2,
            silence: 0x08,
            games: "Big Game",
        },
        Variant {
            name: "B'",
            sig: "DE ?? 26 ?? FE 02 ?? DF A2 FE 02 ?? DF A4 3B A6 00 08 E6 00 4D 2A 2A",
            ops: ops_b2,
            silence: 0x08,
            games: "Seawitch",
        },
        Variant {
            name: "C",
            sig: "DE ?? 26 ?? B6 02 ?? 88 00",
            ops: ops_b2,
            silence: 0x08,
            games: "Nine Ball",
        },
        Variant {
            name: "B",
            sig: "DE ?? 26 ?? B6 02 ?? BA 02",
            ops: ops_b,
            silence: 0x00,
            games: "Cheetah, Star Gazer, Quicksilver",
        },
        Variant {
            name: "F",
            sig: "DE ?? 26 ?? 7F 02 ?? CE 02 00 A6 FA",
            ops: ops_f,
            silence: 0x00,
            games: "Gamatron",
        },
        Variant {
            name: "E",
            sig: "DE ?? 26 ?? 7F 02 ?? B6 02 ?? 88 00 97 A2",
            ops: ops_e,
            silence: 0x08,
            games: "Viper, Dragonfist, Iron Maiden, Lazer Lord, Cue",
        },
    ];

    /// The first three bytes of each of `B`'s handlers `08`..`1C`, as Cheetah, Star Gazer
    /// and Quicksilver have them (the RAM bytes they use differ, masked).
    const B_HANDLERS: &[&str] = &[
        "08 D7 53", "CE 00 00", "08 F7 02", "7A 02 F3", "EE 00 20", "08 D7 C0", "A6 01 BB",
        "A6 01 08", "F7 02 FA", "08 D7 ??", "7A 00 ??", "CE 02 00", "F7 02 00", "F7 02 ??",
        "F7 02 ??", "F7 02 06", "FE 02 F6", "FE 02 FA", "08 08 DF", "86 82 8D", "7E ?? ??",
    ];

    /// The same for `F`'s handlers `08`..`1F`.
    const F_HANDLERS: &[&str] = &[
        "08 D7 53", "CE 00 00", "F7 02 F3", "7A 02 F3", "EE 00 20", "D7 C0 20", "A6 01 BB",
        "A6 01 08", "F7 02 FA", "F7 02 FE", "7A 02 FE", "CE 00 00", "F7 02 00", "F7 02 02",
        "FE 02 F6", "FE 02 FA", "08 08 DF", "86 82 8D", "96 28 44", "96 28 44", "20 55 20",
        "20 57 7E", "7E ?? ??", "7E ?? ??",
    ];

    /// `B`'s and `F`'s dispatch (`SUBA #8; ASLA; ADDA #lo; PSHA; LDAA #hi; ADCA #0; PSHA;
    /// RTS`) and its table of `BRA`s: every handler where the opcode lengths above expect it.
    fn table_ok(m: &[u8], from: usize, handlers: &[&str]) -> bool {
        let Some(d) = find_pat(
            m,
            &pat("80 08 48 8B ?? 36 86 ?? 89 00 36 39"),
            from,
            from + 0x200,
        ) else {
            return false;
        };
        let table = usize::from(m[d + 7]) << 8 | usize::from(m[d + 4]);
        handlers.iter().enumerate().all(|(i, h)| {
            let at = table + 2 * i;
            if at + 2 > m.len() || m[at] != 0x20 {
                return false;
            }
            let target = (at + 2).wrapping_add_signed(isize::from(m[at + 1] as i8));
            find_pat(m, &pat(h), target, target + 3) == Some(target)
        })
    }

    /// Program ROM of the MPU-200 board (U1/U5 at 1000, U2/U6 at 5000).
    fn rom(a: usize) -> bool {
        (0x1000..0x2000).contains(&a) || (0x5000..0x6000).contains(&a)
    }

    /// A script read statically: the addresses of its ops along the path read, whether one
    /// of them sets a pitch, and the address of the op that silences it, if it ends so.
    struct Script {
        path: Vec<usize>,
        pitch: bool,
        silence_at: Option<usize>,
    }

    /// Reads the script at `start` along its path (jumps followed, loops and calls read
    /// through); `None` when a byte is not an opcode of this interpreter, a target is out
    /// of the program, or the path never ends.
    fn read(m: &[u8], ops: fn(u8) -> Op, start: usize) -> Option<Script> {
        let mut s = Script {
            path: Vec::new(),
            pitch: false,
            silence_at: None,
        };
        let mut a = start;
        for _ in 0..300 {
            if !rom(a) || !rom(a + 2) {
                return None;
            }
            if s.path.contains(&a) {
                return Some(s);
            }
            s.path.push(a);
            let len = match ops(m[a]) {
                Op::Bad => return None,
                Op::Pitch(n) => {
                    s.pitch = true;
                    usize::from(n)
                }
                Op::Setup(n) | Op::Counter(n) | Op::Step(n) => usize::from(n),
                Op::End | Op::Ret => return Some(s),
                Op::Silence => {
                    s.silence_at = Some(a);
                    return Some(s);
                }
                Op::Native => return rom(usize::from(be16(m, a + 1))).then_some(s),
                Op::Jump => {
                    let t = usize::from(be16(m, a + 1));
                    if !rom(t) {
                        return None;
                    }
                    a = t;
                    continue;
                }
                Op::Loop | Op::Call => {
                    if !rom(usize::from(be16(m, a + 1))) {
                        return None;
                    }
                    3
                }
            };
            a += len;
        }
        None
    }

    /// `STX ptr` (direct or extended) at `a`.
    fn stores_ptr(m: &[u8], a: usize, ptr: u8) -> bool {
        a + 3 <= m.len()
            && (m[a] == 0xDF && m[a + 1] == ptr || m[a] == 0xFF && m[a + 1] == 0 && m[a + 2] == ptr)
    }

    /// After an `LDX #script` at `from - 3`: `STX ptr` within 12 bytes (no `LDX 0,X`
    /// before it), here or after one `BRA`/`JMP` (the tester's `1505`: `LDX #55B7; BRA
    /// 1512`, the routine that silences the board and stores the pointer).
    fn loads_then_stores(m: &[u8], from: usize, ptr: u8) -> bool {
        let stores = |a: usize| {
            (a..a + 12)
                .take_while(|&k| !(m[k] == 0xEE && m[k + 1] == 0))
                .any(|k| stores_ptr(m, k, ptr))
        };
        if stores(from) {
            return true;
        }
        let target = match m[from] {
            0x20 => (from + 2).wrapping_add_signed(isize::from(m[from + 1] as i8)),
            0x7E => usize::from(be16(m, from + 1)),
            _ => return false,
        };
        rom(target) && stores(target)
    }

    /// The thread engine's dispatch table (one word per thread opcode) and its sound
    /// opcodes: those whose routine, after at most two `BRA`, calls the routine that reads
    /// the word after the thread instruction (`JSR`) and stores the script pointer within
    /// 40 bytes. Opcodes 57 and 58 are sounds on every program of the family.
    fn sound_ops(m: &[u8], ptr: u8) -> Option<(usize, Vec<u8>)> {
        // A routine that calls the word reader and stores the pointer, reached from `a`.
        let stores_after = |mut a: usize| {
            for _ in 0..2 {
                if m[a] == 0x20 {
                    a = (a + 2).wrapping_add_signed(isize::from(m[a + 1] as i8));
                }
            }
            (a..a + 12)
                .find(|&k| m[k] == 0xBD)
                .is_some_and(|j| (j + 3..j + 40).any(|k| stores_ptr(m, k, ptr)))
        };
        // The table is at 1000 or 5000 on Stern's own programs; a MOD that moved it (Nine
        // Ball's ball-handling MODs) still points its entry 58 at such a routine.
        let moved = (0x1000..0x6000 - 1)
            .filter(|&p| rom(p) && rom(usize::from(be16(m, p))) && p >= 2 * 0x58)
            .filter(|&p| {
                m[usize::from(be16(m, p))] == 0xBD && stores_after(usize::from(be16(m, p)))
            })
            .map(|p| p - 2 * 0x58);
        for base in [0x1000usize, 0x5000].into_iter().chain(moved) {
            let ops: Vec<u8> = (0..0x80u8)
                .filter(|&op| {
                    let a = usize::from(be16(m, base + 2 * usize::from(op)));
                    rom(a) && rom(a + 12) && stores_after(a)
                })
                .collect();
            if ops.contains(&0x57) && ops.contains(&0x58) {
                return Some((base, ops));
            }
        }
        None
    }

    pub fn find(m: &[u8]) -> Result<Layer, String> {
        if m.len() < 0x10000 {
            return Err("program image smaller than the MPU-200 map".into());
        }
        // The program that runs is the one the reset vector points into (the S.A.M. IV
        // tester boots its own program at 3000, which has no sound code, and only carries
        // the S.A.M. III's).
        let reset = usize::from(be16(m, 0xFFFE));
        if !rom(reset) {
            return Err(format!(
                "the program starts at {reset:04X}, outside the MPU-200's program ROMs (1000, 5000): its sound code, if any, is not the scripts read here"
            ));
        }
        let found = VARIANTS.iter().find_map(|v| {
            let at = find_pat(m, &pat(v.sig), 0x1000, 0x6000)?;
            let ok = match v.name {
                "B" => table_ok(m, at, B_HANDLERS),
                "F" => table_ok(m, at, F_HANDLERS),
                _ => true,
            };
            ok.then_some((v, at))
        });
        let Some((v, at)) = found else {
            return Err(
                "no sound script interpreter of a known kind found in the game program".into(),
            );
        };
        let ptr = m[at + 1];
        // The delay byte: the first `STAA/STAB delay` (direct or extended) right before a
        // `STX ptr` after the entry (the delay op).
        let delay = (at..at + 0x200).find_map(|k| match m[k..k + 5] {
            [0x97 | 0xD7, d, 0xDF, p, _] | [0x97 | 0xD7, d, 0xFF, 0x00, p] if p == ptr => Some(d),
            [0xF7, 0x00, d, 0xDF, p] if p == ptr => Some(d),
            _ => None,
        });
        let Some(delay) = delay else {
            return Err(format!(
                "sound interpreter at {at:04X}: its delay byte was not found"
            ));
        };
        // No thread engine on the board tester: its sounds are direct loads.
        let (base, thread_ops) = sound_ops(m, ptr).unwrap_or_default();
        // Every place the program refers to a script from: thread instructions, direct
        // loads, tables.
        let mut refs: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        let mut tables: BTreeMap<usize, usize> = BTreeMap::new();
        for i in 0x1000..0x6000 - 3 {
            if !rom(i) || !rom(i + 2) {
                continue;
            }
            let w = usize::from(be16(m, i + 1));
            // (Not inside the dispatch table itself: its words are routine addresses.)
            let in_table = !thread_ops.is_empty() && (base..base + 0x100).contains(&(i + 1));
            if thread_ops.contains(&m[i]) && rom(w) && !in_table {
                refs.entry(w)
                    .or_default()
                    .push(format!("thread {:02X} at {i:04X}", m[i]));
            }
            if m[i] == 0xCE && rom(w) && loads_then_stores(m, i + 3, ptr) {
                refs.entry(w).or_default().push(format!("LDX at {i:04X}"));
            }
            // `LDX 0,X; STX ptr`: a table entry. Its base is the last `LDX #base`, or the
            // `LDAA #hi; ADCA #0` with an `ADDA #lo` before it, within 30 bytes back.
            if m[i] == 0xEE && m[i + 1] == 0 && stores_ptr(m, i + 2, ptr) {
                let base = (i.saturating_sub(30)..i).rev().find_map(|k| {
                    if m[k] == 0xCE && rom(usize::from(be16(m, k + 1))) {
                        return Some(usize::from(be16(m, k + 1)));
                    }
                    if m[k] == 0x86 && m[k + 2] == 0x89 && m[k + 3] == 0 {
                        let lo = (k.saturating_sub(12)..k).rev().find(|&q| m[q] == 0x8B)?;
                        return Some(usize::from(m[k + 1]) << 8 | usize::from(m[lo + 1]));
                    }
                    None
                });
                if let Some(b) = base {
                    tables.insert(b, i);
                }
            }
        }
        for (&t, &user) in &tables {
            let mut e = t;
            while rom(e) && rom(e + 1) {
                let w = usize::from(be16(m, e));
                if !rom(w) || read(m, v.ops, w).is_none() {
                    break;
                }
                refs.entry(w).or_default().push(format!(
                    "table {t:04X} entry {} (read at {user:04X})",
                    (e - t) / 2
                ));
                e += 2;
            }
        }
        // Taken: the referred addresses where a script of this interpreter starts, sets a
        // pitch somewhere, and starts as a sound does: with a pitch or set-up op, or, when
        // the address is not inside another script, with a loop counter. Anything else is a
        // coincidence of bytes (a delay, an end), the inside of another script, or a script
        // that only changes the state of what plays (the self-test's `C0` levels).
        let scripts: BTreeMap<usize, Script> = refs
            .keys()
            .filter_map(|&w| read(m, v.ops, w).map(|s| (w, s)))
            .collect();
        let inside = |w: usize| scripts.iter().any(|(&o, s)| o != w && s.path.contains(&w));
        let mut silence_at = None;
        let mut sounds = Vec::new();
        let mut rejected = 0;
        for (&w, why) in &refs {
            let starts = match (v.ops)(m[w]) {
                Op::Pitch(_) | Op::Setup(_) => true,
                Op::Counter(_) => !inside(w),
                _ => false,
            };
            match scripts.get(&w) {
                Some(s) if starts && s.pitch => {
                    silence_at = silence_at.or(s.silence_at);
                    let [hi, lo] = (w as u16).to_be_bytes();
                    sounds.push(GameSound {
                        id: w as u32,
                        refs: why.join(", "),
                        start: Request::Pokes(vec![
                            Poke {
                                addr: u32::from(delay),
                                val: 0,
                            },
                            Poke {
                                addr: u32::from(ptr),
                                val: hi,
                            },
                            Poke {
                                addr: u32::from(ptr) + 1,
                                val: lo,
                            },
                        ]),
                    });
                }
                _ => rejected += 1,
            }
        }
        if sounds.is_empty() {
            return Err(format!(
                "sound interpreter at {at:04X}: no script found among {} references",
                refs.len()
            ));
        }
        // The stop: the pointer on the game's own silencing op (the one that ends a script),
        // which writes the board's registers as the game silences it and clears the pointer.
        let silence_at = silence_at
            .or_else(|| (0x1000..0x6000).find(|&a| rom(a) && m[a] == v.silence))
            .expect("a silence opcode byte in the program");
        let [hi, lo] = (silence_at as u16).to_be_bytes();
        let stop = vec![
            Poke {
                addr: u32::from(delay),
                val: 0,
            },
            Poke {
                addr: u32::from(ptr),
                val: hi,
            },
            Poke {
                addr: u32::from(ptr) + 1,
                val: lo,
            },
        ];
        Ok(Layer {
            what: format!(
                "Stern MPU-200 sound scripts: interpreter {} ({}) at {at:04X}, script pointer ${ptr:02X}, delay ${delay:02X}; {}; {} scripts the program starts ({} other references are not scripts); stop: the pointer on the silencing op {:02X} at {silence_at:04X}",
                v.name,
                v.games,
                if thread_ops.is_empty() {
                    "no thread engine".to_string()
                } else {
                    format!(
                        "thread sound instructions {} (dispatch table at {base:04X})",
                        thread_ops
                            .iter()
                            .map(|o| format!("{o:02X}"))
                            .collect::<Vec<_>>()
                            .join("/")
                    )
                },
                sounds.len(),
                rejected,
                v.silence
            ),
            id_is: "the address of the sound script in the game's program (what the game stores in its script pointer)".into(),
            id_digits: 4,
            sounds,
            stop: Request::Pokes(stop),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_with_wildcards() {
        let hay = [0x00, 0xDE, 0x74, 0x26, 0x2D, 0xB6];
        assert_eq!(find_pat(&hay, &pat("DE ?? 26 2D"), 0, hay.len()), Some(1));
        assert_eq!(find_pat(&hay, &pat("DE ?? 27"), 0, hay.len()), None);
    }

    /// A made-up Meteor-like program: the interpreter's entry, a thread dispatch table whose
    /// opcode 58 calls a routine storing the pointer, one thread instruction, one script.
    #[test]
    fn st300_finds_a_thread_script() {
        let mut m = vec![0xFFu8; 0x10000];
        let entry = 0x5E82;
        let code = pat("DE 6D 27 F9 A6 00 08 B7 02 F6 2A 11 26 09 A6 00 08 97 4C DF 6D");
        for (i, b) in code.iter().enumerate() {
            m[entry + i] = b.unwrap();
        }
        // The reset vector, in the program.
        m[0xFFFE..].copy_from_slice(&[0x5C, 0x3B]);
        // Thread opcodes 57 and 58 -> 5808: JSR 5373; STX $6D; CLR $4C.
        for op in [0x57, 0x58] {
            m[0x5000 + 2 * op..0x5000 + 2 * op + 2].copy_from_slice(&[0x58, 0x08]);
        }
        m[0x5808..0x5810].copy_from_slice(&[0xBD, 0x53, 0x73, 0xDF, 0x6D, 0x7F, 0x00, 0x4C]);
        // Thread code: 58 5883.
        m[0x12E2..0x12E5].copy_from_slice(&[0x58, 0x58, 0x83]);
        // Script 5883: A2 0040, A4 0200, 05 02, 00 1E, 08.
        m[0x5883..0x588F].copy_from_slice(&[
            0xA2, 0x00, 0x40, 0xA4, 0x02, 0x00, 0x05, 0x02, 0x00, 0x1E, 0x08, 0x01,
        ]);
        let l = st300::find(&m).unwrap();
        assert_eq!(l.sounds.len(), 1);
        assert_eq!(l.sounds[0].id, 0x5883);
        assert_eq!(
            l.sounds[0].start,
            Request::Pokes(vec![
                Poke { addr: 0x4C, val: 0 },
                Poke {
                    addr: 0x6D,
                    val: 0x58
                },
                Poke {
                    addr: 0x6E,
                    val: 0x83
                },
            ])
        );
        // The stop points at the script's own silencing op.
        let Request::Pokes(stop) = &l.stop;
        assert_eq!(
            stop[1..],
            [
                Poke {
                    addr: 0x6D,
                    val: 0x58
                },
                Poke {
                    addr: 0x6E,
                    val: 0x8D
                }
            ]
        );
    }
}
