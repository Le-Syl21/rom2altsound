//! Game-driven sound: the boards that take no sound command.
//!
//! On Stern's SB-300 (`ST300`), Atari's generation 1 and 2 boards (`ATARI1`, `ATARI2`),
//! Stern's Astro board tester (`ASTRO`) and Romstar's Goofy Hoops (`ROMSTAR`), the game CPU
//! makes every sound itself: it writes the sound chip's registers over time (timers, tone
//! latches, QSound voices). There is no command byte between the game and its board, so
//! the sweep of the other boards has nothing to send. The game's own program has a sound
//! layer, though: a routine that starts a sound and a request the rest of the game uses to
//! ask for one (a pointer to a sound script, a sound number in a RAM byte...). This module
//! reads that layer in the game's program image and gives, for each sound the game can
//! play, the RAM writes that ask for it the way the game's own code does, and the writes
//! that silence it the way the game stops its sounds. The extraction then leaves the game
//! running (it idles in its attract mode) and records each one.
//!
//! Two more are driven the same way: the SB-300 with its speech board (`ST300V`), whose
//! scripts also say its words, and Tabart's L'Hexagone (`TABART`, sub-type 0), whose sound
//! CPU takes no command but plays on the playfield switches it reads itself: its layer is
//! read in the sound program, and a sound is a switch closed.
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

/// A call of the game's own code (68000 family): `code` is a routine that saves the
/// registers, calls the game's routine with its arguments, restores them and returns from
/// exception (`RTE`): the shim enters it as an exception would (`shim_m68k_call`), when the
/// CPU is in supervisor mode with its interrupts unmasked (not inside an interrupt), its
/// program counter is outside `busy_pc` (the game's own sound code) and the word at `lock`
/// (the sound system's lock) is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub code: [u8; MAX_CALL],
    pub len: u8,
    pub lock: u32,
    pub busy_pc: (u32, u32),
}

/// The longest routine a `Call` carries.
pub const MAX_CALL: usize = 48;

/// How the game asks for a sound (or stops them).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Bytes written into the game's RAM (its sound request).
    Pokes(Vec<Poke>),
    /// The game's own routine, called.
    Call(Call),
    /// A playfield switch (PinMAME's switch number) closed, then opened, with the game
    /// running: the sound board reads the switch matrix itself (Tabart's L'Hexagone).
    Switch(u16),
    /// Bytes through the sound board's manual command (`sndbrd_manCmd`, board 0), one per
    /// send: the ST300V's speech words, which have their own path from the game.
    Board(Vec<u8>),
    /// The sound board's reset (its CPU's reset line, `shim_reset_audio_cpus`): the stop
    /// of a board whose program has none.
    Reset,
    /// The sound lines as the game writes them (`sndbrd_data_w`, board 0).
    Lines(u8),
    /// A playfield switch closed (`true`) or opened, and left so.
    SwitchState(u16, bool),
    /// These, one after the other.
    Seq(Vec<Request>),
    /// The bits `mask` of the game's RAM byte `addr` set (`on`) or cleared, the other bits
    /// kept (the game's own copy of a DIP switch).
    RamBits { addr: u32, mask: u8, on: bool },
    /// Waits, frame by frame, while the 16-bit script pointer at `ptr` is on one of the
    /// address ranges `ops` (a script and its subroutines), then clears the bits `mask` of
    /// `addr`: the ST300V's speech switch, on for one speech script only.
    ClearAfter {
        ptr: u32,
        ops: [(u16, u16); MAX_RANGES],
        addr: u32,
        mask: u8,
    },
}

/// The most address ranges a `Request::ClearAfter` watches.
pub const MAX_RANGES: usize = 6;

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
    /// DIP switches set once before the first sound (`PinmameSetDIP`): the bits `mask` of
    /// bank `bank` set to `value`: the mode the board's program reads (L'Hexagone's sound
    /// DIPs), or the game's own switch for its speech (the ST300V games' S17).
    pub dips: Vec<Dip>,
    /// Sent once before the first sound, after `dips`.
    pub setup: Vec<Request>,
}

/// A DIP switch setting: in bank `bank`, the bits `mask` set to `value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dip {
    pub bank: i32,
    pub mask: i32,
    pub value: i32,
}

/// The families handled here, by PinMAME's board type string and sub-type: every one
/// whose sound the game CPU makes itself, with no command (the ST300V's effects are the
/// ST300's, and its speech is said by the same scripts), and Tabart's L'Hexagone, whose
/// board plays on the playfield switches it reads itself.
pub fn game_driven(typestr: &str, subtype: i32) -> bool {
    match typestr {
        "ST300" => subtype == 0 || subtype == ST300V_SUBTYPE,
        "ASTRO" | "ATARI1" | "ATARI2" => true,
        "TMS320AV120" => subtype == 1,
        "TABART" => subtype == 0,
        _ => false,
    }
}

/// `SNDBRD_ST300V` is `SNDBRD_TYPE(31, 1)`: the ST300 interface with the speech board.
pub const ST300V_SUBTYPE: i32 = 1;

/// Whether the layer is read in the sound CPU's program rather than the game CPU's.
pub fn in_sound_program(typestr: &str) -> bool {
    typestr == "TABART"
}

/// Reads the sound layer of the running game, from its program image (the game CPU's
/// memory region, as the driver loaded it; the sound CPU's when `in_sound_program`).
/// `speech` is the ST300V's speech ROM (empty elsewhere).
pub fn find(typestr: &str, subtype: i32, image: &[u8], speech: &[u8]) -> Result<Layer, String> {
    match typestr {
        "ST300" if subtype == ST300V_SUBTYPE => st300::find_speech(image, speech),
        "ST300" | "ASTRO" => st300::find(image),
        "ATARI1" => atari::gen1(image),
        "ATARI2" => atari::gen2(image),
        "TMS320AV120" => romstar::find(image),
        "TABART" => tabart::find(image),
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
        /// Says a word (`80`..`BF` on interpreters E and F: the game's speech request, which
        /// its interrupt sends to the ST300V's speech board, `40 | word`); one byte.
        Word,
        /// Sets the speech board's speed and volume (`40`..`7F` on E and F, sent as `80 |
        /// ...`); one byte.
        Voice,
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
            0x40..=0xFF => speech(op),
            _ => Op::Bad,
        }
    }
    /// The one-byte ops `40`..`FF` of E and F: the speech requests (handled the same way on
    /// the games without the speech board, where they say nothing) and the delays `C0` up.
    fn speech(op: u8) -> Op {
        match op {
            0x40..=0x7F => Op::Voice,
            0x80..=0xBF => Op::Word,
            _ => Op::Step(1),
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
            0x40..=0xFF => speech(op),
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
        /// The words it says (speech requests along the path read).
        words: Vec<u8>,
        /// The script subroutines it calls.
        calls: Vec<usize>,
        silence_at: Option<usize>,
    }

    /// Reads the script at `start` along its path (jumps followed, loops and calls read
    /// through); `None` when a byte is not an opcode of this interpreter, a target is out
    /// of the program, or the path never ends.
    fn read(m: &[u8], ops: fn(u8) -> Op, start: usize) -> Option<Script> {
        let mut s = Script {
            path: Vec::new(),
            pitch: false,
            words: Vec::new(),
            calls: Vec::new(),
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
                Op::Word => {
                    s.words.push(m[a] & 0x3F);
                    1
                }
                Op::Voice => 1,
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
                    let t = usize::from(be16(m, a + 1));
                    if !rom(t) {
                        return None;
                    }
                    if ops(m[a]) == Op::Call {
                        s.calls.push(t);
                    }
                    3
                }
            };
            a += len;
        }
        None
    }

    /// The address ranges a script's ops lie in, its subroutines' included (one level):
    /// consecutive ops (at most 5 bytes apart) make one range. `None` when there are more
    /// than `MAX_RANGES`.
    fn op_ranges(m: &[u8], ops: fn(u8) -> Op, s: &Script) -> Option<[(u16, u16); MAX_RANGES]> {
        let mut at: Vec<usize> = s.path.clone();
        for &c in &s.calls {
            if let Some(sub) = read(m, ops, c) {
                at.extend(sub.path);
            }
        }
        at.sort_unstable();
        let mut ranges: Vec<(u16, u16)> = Vec::new();
        for a in at {
            match ranges.last_mut() {
                Some(r) if a <= usize::from(r.1) + 5 => r.1 = a as u16,
                _ => ranges.push((a as u16, a as u16)),
            }
        }
        if ranges.len() > MAX_RANGES {
            return None;
        }
        let mut out = [(0, 0); MAX_RANGES];
        out[..ranges.len()].copy_from_slice(&ranges);
        Some(out)
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
    /// 40 bytes (`reach`; 41 on the ST300V games, whose speech opcode 59 stores it 40 bytes
    /// after its call: Flight 2000's `57BD`, the routine that says a speech script after
    /// setting the speech board's resting speed and volume). Opcodes 57 and 58 are sounds
    /// on every program of the family.
    fn sound_ops(m: &[u8], ptr: u8, reach: usize) -> Option<(usize, Vec<u8>)> {
        // A routine that calls the word reader and stores the pointer, reached from `a`.
        let stores_after = |mut a: usize| {
            for _ in 0..2 {
                if m[a] == 0x20 {
                    a = (a + 2).wrapping_add_signed(isize::from(m[a + 1] as i8));
                }
            }
            (a..a + 12)
                .find(|&k| m[k] == 0xBD)
                .is_some_and(|j| (j + 3..j + reach).any(|k| stores_ptr(m, k, ptr)))
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
        find_with(m, None)
    }

    /// The ST300V games (Flight 2000, Free Fall, Lightning, Split Second, Catacomb, Orbitor
    /// 1): the same interpreters (F, E), whose one-byte ops `80`..`BF` say a word and
    /// `40`..`7F` set the speech board's speed and volume: the game's speech request
    /// (`$02FF` on Flight 2000, `$02F7` on Free Fall), which its interrupt sends to the
    /// board inverted in bits 6-7 (Flight 2000's `5F43`: `EORA #C0`, port A, then CA2:
    /// by35.c `pia1ca2_w`, `sndbrd_0_ctrl_w`), when the speech enable bit (`$28` bit 0, set
    /// at boot) is on. A script that says a word is a sound, pitch or not. `words` is the
    /// speech ROM (`VSU100_ROMREGION`): the words no script says are sent alone.
    pub fn find_speech(m: &[u8], words: &[u8]) -> Result<Layer, String> {
        find_with(m, Some(words))
    }

    /// The words of an S14001A speech ROM: word `w`'s start (`CWAR`, 12 bits) is the two
    /// bytes at `2w` (s14001a.c `CWARMSB`, `CWARLSB`); the table ends where the first word
    /// starts, and the entries after it are bytes of the words. Words whose start is the
    /// same are one word: the first of each.
    pub fn speech_words(rom: &[u8]) -> Vec<u8> {
        let start = |w: usize| (usize::from(rom[2 * w]) << 4) | usize::from(rom[2 * w + 1] >> 4);
        if rom.len() < 128 {
            return Vec::new();
        }
        let n = (start(0) / 2).min(64);
        let mut seen = std::collections::BTreeSet::new();
        (0..n)
            .filter(|&w| start(w) >= 2 * n && start(w) < rom.len() && seen.insert(start(w)))
            .map(|w| w as u8)
            .collect()
    }

    fn find_with(m: &[u8], speech_rom: Option<&[u8]>) -> Result<Layer, String> {
        let speech = speech_rom.is_some();
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
        let (base, thread_ops) =
            sound_ops(m, ptr, if speech { 41 } else { 40 }).unwrap_or_default();
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
        let v_ops = v.ops;
        let mut scripts: BTreeMap<usize, Script> = refs
            .keys()
            .filter_map(|&w| read(m, v.ops, w).map(|s| (w, s)))
            .collect();
        // The words of the subroutines a speech script calls (Orbitor 1's `5931` calls
        // `5928`, which says three of its five words).
        if speech {
            for s in scripts.values_mut() {
                for &c in &s.calls {
                    if let Some(sub) = read(m, v.ops, c) {
                        s.words.extend(sub.words);
                    }
                }
            }
        }
        let inside = |w: usize| scripts.iter().any(|(&o, s)| o != w && s.path.contains(&w));
        // The speech gate: every speech op of the interpreter first tests bit 0 of a RAM
        // byte (`BITA #C0; BEQ; LDAB gate; LSRB; BCC`; Flight 2000's `1C92`, Free Fall's
        // `1B4B`: `$28`), the game's copy of DIP switches 17-24 (bank 2), read at boot:
        // S17 is the speech switch, off in PinMAME's default (`$28` = 18: S20 and S21), and
        // with it off the game says nothing. It is set for the sounds that speak, and
        // cleared by the stop: with it on all the time, Orbitor 1's attract mode speaks
        // without end (a phrase every 2.2 s, under every other sound). Lightning has no
        // gate (the test is NOPed out: it always speaks).
        let speech_gate = speech
            .then(|| find_pat(m, &pat("85 C0 27 ?? D6 ?? 54 24"), at, at + 0x100))
            .flatten()
            .map(|k| u32::from(m[k + 5]));
        let gate = |on: bool| {
            speech_gate.map(|addr| Request::RamBits {
                addr,
                mask: 0x01,
                on,
            })
        };
        let mut silence_at = None;
        let mut sounds = Vec::new();
        let mut rejected = 0;
        for (&w, why) in &refs {
            let starts = match (v.ops)(m[w]) {
                Op::Pitch(_) | Op::Setup(_) => true,
                Op::Counter(_) => !inside(w),
                _ => false,
            };
            // A speech script opens the game's speech frame (a one-byte op: F's `1A`, E's
            // `14`, which wait for the board and silence the timers), sets the speech
            // board's speed and volume, then says its words (Flight 2000's `1A27`: `1A 42
            // 80 1B`, then an effect). Self-tests and bytes that only look like words (a
            // word op first, a set-up op before them) are left out.
            let speaks =
                speech && matches!((v.ops)(m[w]), Op::Step(1)) && (v.ops)(m[w + 1]) == Op::Voice;
            match scripts.get(&w) {
                Some(s) if (starts && s.pitch) || (speaks && !s.words.is_empty()) => {
                    silence_at = silence_at.or(s.silence_at);
                    let [hi, lo] = (w as u16).to_be_bytes();
                    let request = Request::Pokes(vec![
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
                    ]);
                    let start = match (gate(true), speech_gate) {
                        (Some(on), Some(addr)) if !s.words.is_empty() => {
                            let mut v = vec![on, request];
                            if let Some(ops) = op_ranges(m, v_ops, s) {
                                v.push(Request::ClearAfter {
                                    ptr: u32::from(ptr),
                                    ops,
                                    addr,
                                    mask: 0x01,
                                });
                            }
                            Request::Seq(v)
                        }
                        _ => request,
                    };
                    sounds.push(GameSound {
                        id: w as u32,
                        refs: why.join(", "),
                        start,
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
        let scripted = sounds.len();
        let mut speech_note = String::new();
        if speech {
            // The words no script says, through the speech path the game's interrupt takes
            // (the manual command is it: stsnd.c `st300_man_w`), after the speed and volume
            // the game's scripts set (the most frequent `40`..`7F` op, sent as `op ^ C0`).
            let said: std::collections::BTreeSet<u8> = sounds
                .iter()
                .filter_map(|g| scripts.get(&(g.id as usize)))
                .flat_map(|s| s.words.iter().copied())
                .collect();
            let mut voices: BTreeMap<u8, usize> = BTreeMap::new();
            for g in &sounds {
                for &a in scripts
                    .get(&(g.id as usize))
                    .map_or(&[][..], |s| &s.path[..])
                {
                    if (v.ops)(m[a]) == Op::Voice {
                        *voices.entry(m[a]).or_default() += 1;
                    }
                }
            }
            let voice = voices
                .iter()
                .max_by_key(|&(&op, &n)| (n, std::cmp::Reverse(op)))
                .map(|(&op, _)| op ^ 0xC0);
            let words = speech_words(speech_rom.unwrap_or_default());
            for &w in words.iter().filter(|w| !said.contains(w)) {
                let mut bytes: Vec<u8> = voice.into_iter().collect();
                bytes.push(0x40 | w);
                sounds.push(GameSound {
                    id: u32::from(0x40 | w),
                    refs: format!(
                        "speech word {w:02X}, said by no script: sent as the game sends a word{}",
                        voice.map_or(String::new(), |b| format!(
                            ", after the scripts' speed and volume {b:02X}"
                        ))
                    ),
                    start: Request::Board(bytes),
                });
            }
            speech_note = format!(
                "; the scripts say {} of the speech ROM's {} words, the {} others are sent alone (ids 0040 + word){}",
                said.len(),
                words.len(),
                words.iter().filter(|w| !said.contains(w)).count(),
                voice.map_or(String::new(), |b| format!(
                    " after the scripts' speed and volume {b:02X}"
                ))
            );
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
                scripted,
                rejected,
                v.silence
            ) + &speech_note,
            id_is: "the address of the sound script in the game's program (what the game stores in its script pointer)".into(),
            id_digits: 4,
            sounds,
            setup: Vec::new(),
            dips: Vec::new(),
            stop: match gate(false) {
                Some(g) => Request::Seq(vec![g, Request::Pokes(stop)]),
                None => Request::Pokes(stop),
            },
        })
    }
}

/// Atari (1976-1979): the game program steps the tone itself, from slots in RAM.
///
/// No sound CPU: the game writes a waveform select, a frequency and a volume to latches
/// (generation 1: `1080`/`1084`/`1088`, enable `3000`, reset `6000`; generation 2: `1800`
/// and `1820`), one step of a sound at a time, from a routine its main loop or interrupt
/// calls. What the rest of the game sets to ask for a sound is a RAM byte per sound:
/// - generation 1, The Atarians, Time 2000, Airborne Avenger: one down-counter per sound
///   (the routine plays the highest one that is not zero, one table step per call, and
///   disables the tone when all are zero); the game starts a sound by storing its length
///   there (`LDAA #len; STAA counter`). Id: the counter's address and that length.
/// - generation 1, Space Riders: a table of slots (a step byte and a pending count each);
///   the game asks for slot `n` by adding to its pending count. Id: the slot number.
/// - generation 1, Middle Earth: a list of sound descriptors, each pointing at its own
///   step and pending bytes in RAM. Id: the descriptor's address.
/// - generation 2, Superman, Hercules, Road Runner: an array of pending counts, one per
///   sound number, the descriptor table at `A8B7` (6 bytes each); the game's rule code
///   asks for sound `n` through its sound instruction (`3214`: `INC count[n]`). Id: the
///   sound number.
mod atari {
    use super::*;

    fn poke(addr: usize, val: u8) -> Poke {
        Poke {
            addr: addr as u32,
            val,
        }
    }

    /// Every match of the pattern `p` in `from..to`.
    fn all(m: &[u8], p: &str, from: usize, to: usize) -> Vec<usize> {
        let p = pat(p);
        let mut v = Vec::new();
        let mut i = from;
        while let Some(at) = find_pat(m, &p, i, to) {
            v.push(at);
            i = at + 1;
        }
        v
    }

    pub fn gen1(m: &[u8]) -> Result<Layer, String> {
        if m.len() < 0x8000 {
            return Err("program image smaller than the generation 1 map".into());
        }
        let (lo, hi) = (0x7000, 0x8000);
        // Down-counters: `LDAA c; BEQ; LDX #table; JSR index; LDAB wave; DEC c`.
        let blocks: Vec<usize> = all(m, "96 ?? 27 ?? CE ?? ?? BD ?? ?? F6 ?? ?? 7A 00 ??", lo, hi)
            .into_iter()
            .filter(|&i| m[i + 1] == m[i + 15])
            .collect();
        if blocks.len() >= 2 {
            let counters: Vec<u8> = blocks.iter().map(|&i| m[i + 1]).collect();
            let mut sounds = Vec::new();
            for &c in &counters {
                // Every `STAA c` / `STAB c` with its `LDAA #v` / `LDAB #v` a few bytes before.
                let mut starts = std::collections::BTreeMap::<u8, Vec<String>>::new();
                for i in lo + 2..hi - 1 {
                    let reg = match m[i] {
                        0x97 => 0x86,
                        0xD7 => 0xC6,
                        _ => continue,
                    };
                    if m[i + 1] != c {
                        continue;
                    }
                    if let Some(k) = (i.saturating_sub(10)..i - 1).rev().find(|&k| m[k] == reg)
                        && m[k + 1] != 0
                    {
                        starts.entry(m[k + 1]).or_default().push(format!("{i:04X}"));
                    }
                }
                for (v, at) in starts {
                    sounds.push(GameSound {
                        id: u32::from(c) << 8 | u32::from(v),
                        refs: format!("counter ${c:02X} = {v:02X} at {}", at.join(", ")),
                        start: Request::Pokes(vec![poke(usize::from(c), v)]),
                    });
                }
            }
            if sounds.is_empty() {
                return Err("generation 1 sound counters found, but no code starting them".into());
            }
            return Ok(Layer {
                what: format!(
                    "Atari generation 1 sound counters {} (the routine at {:04X} plays the highest one that is not zero, one step per call, and turns the tone off when all are zero); {} sounds: the lengths the game stores in them; stop: every counter to 0",
                    counters.iter().map(|c| format!("${c:02X}")).collect::<Vec<_>>().join(" "),
                    blocks[0],
                    sounds.len()
                ),
                id_is: "the counter's RAM address (high byte) and the length the game stores in it to start the sound (low byte)".into(),
                id_digits: 4,
                sounds,
                setup: Vec::new(),
                dips: Vec::new(),
                stop: Request::Pokes(counters.iter().map(|&c| poke(usize::from(c), 0)).collect()),
            });
        }
        // Slots of a step and a pending count: `LDX #base ... LDAA 0,X; BNE; LDAA 1,X; BEQ;
        // DEC 1,X`, up to `CPX #end`.
        if let Some(i) = all(
            m,
            "CE 00 ?? DF ?? 7F 00 ?? DE ?? A6 00 26 ?? A6 01 27 ?? 6A 01",
            lo,
            hi,
        )
        .first()
        .copied()
        {
            let base = usize::from(m[i + 2]);
            let Some(end) = (i..i + 0x60)
                .find(|&k| m[k] == 0x8C && m[k + 1] == 0 && usize::from(m[k + 2]) > base)
                .map(|k| usize::from(m[k + 2]))
            else {
                return Err(format!(
                    "generation 1 sound slots at ${base:02X}: their end was not found"
                ));
            };
            let n = (end - base) / 2;
            let sounds = (0..n)
                .map(|s| GameSound {
                    id: s as u32,
                    refs: format!("slot {s}: pending count at ${:02X}", base + 2 * s + 1),
                    start: Request::Pokes(vec![poke(base + 2 * s + 1, 1)]),
                })
                .collect();
            return Ok(Layer {
                what: format!(
                    "Atari generation 1 sound slots ${base:02X}..${:02X} (a step byte and a pending count each, stepped by the routine at {i:04X}); {n} sounds; stop: every slot to 0",
                    end - 1
                ),
                id_is: "the sound's slot number (the game asks for slot n by adding to its pending count)".into(),
                id_digits: 2,
                sounds,
                setup: Vec::new(),
                dips: Vec::new(),
                stop: Request::Pokes((base..end).map(|a| poke(a, 0)).collect()),
            });
        }
        // Descriptors with a pointer to their own step and pending bytes: the routine
        // (`STX; STX; LDX 1,X; LDAA 0,X; BNE; LDAA 1,X; BNE`) and every `LDX #descriptor;
        // BSR routine` that calls it.
        if let Some(r) = all(m, "DF ?? DF ?? EE 01 A6 00 26 ?? A6 01 26", lo, hi)
            .first()
            .copied()
        {
            let mut descs: Vec<usize> = Vec::new();
            for i in all(m, "CE ?? ?? 8D ??", lo, hi) {
                let target = (i + 5).wrapping_add_signed(isize::from(m[i + 4] as i8));
                let d = usize::from(be16(m, i + 1));
                if target == r && (lo..hi).contains(&d) && !descs.contains(&d) {
                    descs.push(d);
                }
            }
            descs.sort_unstable();
            if descs.is_empty() {
                return Err(format!(
                    "generation 1 sound routine at {r:04X}: no descriptor passed to it"
                ));
            }
            let state = |d: usize| usize::from(be16(m, d + 1));
            return Ok(Layer {
                what: format!(
                    "Atari generation 1 sound descriptors {} (stepped by the routine at {r:04X}; each points at its step and pending bytes in RAM); {} sounds; stop: every step and pending byte to 0",
                    descs
                        .iter()
                        .map(|d| format!("{d:04X}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    descs.len()
                ),
                id_is: "the address of the sound's descriptor in the game's program".into(),
                id_digits: 4,
                sounds: descs
                    .iter()
                    .map(|&d| GameSound {
                        id: d as u32,
                        refs: format!("descriptor {d:04X}: pending count at ${:02X}", state(d) + 1),
                        start: Request::Pokes(vec![poke(state(d) + 1, 1)]),
                    })
                    .collect(),
                setup: Vec::new(),
                dips: Vec::new(),
                stop: Request::Pokes(
                    descs
                        .iter()
                        .flat_map(|&d| [poke(state(d), 0), poke(state(d) + 1, 0)])
                        .collect(),
                ),
            });
        }
        Err("no generation 1 sound routine of a known kind found".into())
    }

    /// The generation 2 system program (Superman, Hercules: `3214`; Road Runner: `31FE`).
    pub fn gen2(m: &[u8]) -> Result<Layer, String> {
        if m.len() < 0x4000 {
            return Err("program image smaller than the generation 2 map".into());
        }
        let (lo, hi) = (0x2800, 0x4000);
        // The sound instruction: `PSHA; LDX #count; ANDA #1F; JSR add; ...; INC 0,X`.
        let Some(req) = all(m, "36 CE 00 ?? 84 1F BD", lo, hi).first().copied() else {
            return Err("no generation 2 sound instruction found".into());
        };
        let base = usize::from(m[req + 3]);
        // The driver: `CMPA #n` before the `CLR 1800` that ends a pass over the counts,
        // then `LDAA #FF; STAA current` (no sound playing).
        let Some(clr) = all(m, "7F 18 00", lo, hi)
            .into_iter()
            .find(|&k| (k.saturating_sub(16)..k).any(|q| m[q] == 0x81 && m[q + 2] == 0x26))
        else {
            return Err("generation 2 sound driver not found".into());
        };
        let count = (clr - 16..clr)
            .rev()
            .find(|&q| m[q] == 0x81 && m[q + 2] == 0x26)
            .map(|q| usize::from(m[q + 1]))
            .unwrap_or(0);
        let current =
            find_pat(m, &pat("86 FF 97 ??"), clr, clr + 16).map(|k| usize::from(m[k + 3]));
        // The descriptor of sound `n`: `LDX #table + 6n` in the driver, after its pass end.
        let Some(table) = find_pat(m, &pat("48 16 48 1B CE ?? ?? BD"), clr, clr + 0x80)
            .map(|k| usize::from(be16(m, k + 5)))
        else {
            return Err("generation 2 sound descriptor table not found".into());
        };
        // The descriptors in use: 6 bytes, two lengths in order, small flags (the table is
        // followed by other data).
        let valid = |n: usize| {
            let d = &m[table + 6 * n..table + 6 * n + 6];
            d[2] <= d[3] && d[3] < 0x40 && d[4] < 0x10
        };
        let n = (0..count.min(0x20)).take_while(|&n| valid(n)).count();
        if n == 0 {
            return Err(format!(
                "generation 2 sound table at {table:04X}: no descriptor"
            ));
        }
        let mut stop: Vec<Poke> = (base..base + count).map(|a| poke(a, 0)).collect();
        if let Some(c) = current {
            stop.push(poke(c, 0xFF));
        }
        stop.push(poke(0x1800, 0));
        Ok(Layer {
            what: format!(
                "Atari generation 2 sound instruction at {req:04X}: one pending count per sound number at ${base:02X}..${:02X}, the driver's descriptors at {table:04X} (6 bytes each, {n} in use of the {count} counts); stop: the counts to 0, no sound current{}, the sound register 1800 to 0",
                base + count - 1,
                current
                    .map(|c| format!(" (${c:02X} = FF)"))
                    .unwrap_or_default()
            ),
            id_is: "the game's sound number (the index of its pending count and descriptor)".into(),
            id_digits: 2,
            sounds: (0..n)
                .map(|s| GameSound {
                    id: s as u32,
                    refs: format!(
                        "sound {s}: pending count at ${:02X}, descriptor {:04X}",
                        base + s,
                        table + 6 * s
                    ),
                    start: Request::Pokes(vec![poke(base + s, 1)]),
                })
                .collect(),
            setup: Vec::new(),
            dips: Vec::new(),
            stop: Request::Pokes(stop),
        })
    }
}

/// Christian Tabart's L'Hexagone (1986, Gottlieb System 1 hardware, `SNDBRD_TABART`): the
/// sound board takes no command, it watches the playfield.
///
/// Its Z80 program (`hexagone.bin`) reads the game's switch returns itself: the game's
/// switch strobe 1 raises its NMI (`0066`, gts1.c `port_w` → tabart.c `tabart_ctrl_w`),
/// which reads YM2203 port B (the returns of the strobed row) into `$400A` and arms four
/// reads more on the YM2203's timer interrupt (`01BD`, rows 2 to 5 as the game's strobe
/// moves on, `$400B`..`$400E`). Its main loop (`0381`) finds a newly closed switch
/// (`0209`: each row XOR its last state, AND the new one; `$4009` = row << 4 | bit) and,
/// with the outhole open (port A bit 4: tabart.c `core_getSw(66)`), plays the routine of
/// the table at `1083` (`044D`: row × `12`, bit × 2). Eleven of them (table `02E7`) play
/// only once until their bank of switches opens again (drop targets). The sound DIPs (the
/// game's sound lines bits 4 and 7, gts1.c `snd_w`: `core_getDip(3) & 0x90`, port A bits 5
/// and 6) choose the mode: a tune played over and over (`04CE`, both bits giving `00`), a
/// background after each sound (`0422`), a call every minute or so (`03E8`). The chime
/// lines (the game's 10, 100 and 1000 chimes) play one more sound (`04B6`), while the
/// outhole is closed (the bonus count): not reached here (they are the game's solenoids).
///
/// A sound is a switch: closed, then opened, with the game running (its strobe). The id
/// is PinMAME's switch number (`m2sw`: bit × 10 + row, the strobe being the row + 1), its
/// decimal digits as hex digits, one per routine of the table (the first switch that plays
/// it). The stop is the board's
/// reset: its program has none (a routine plays to its end).
mod tabart {
    use super::*;

    /// The rows the program reads: at the NMI, then four timer interrupts (`0084`: `LD
    /// A,4; LD ($4020),A`).
    const ROWS: usize = 5;

    /// The sound DIP bank and value: both sound DIPs off (port A bits 6-5 = `10`): the
    /// switch sounds, no tune, no background after them; the program's own call every
    /// minute or so (`03E8`) is reset with the board after each sound.
    const DIP_BANK: i32 = 3;
    const DIP_VALUE: i32 = 0x00;
    /// The sound lines in attract mode with those DIPs (gts1.c `snd_w`: game over, no
    /// tilt, no chime, `core_getDip(3) & 0x90`).
    const LINES: u8 = DIP_VALUE as u8 & 0x90;
    /// The 10's chime line (`snd_w`: bit 2, the game's solenoid 3).
    const CHIME_10: u8 = 0x04;
    /// The outhole (tabart.c `core_getSw(66)`).
    const OUTHOLE: u16 = 66;
    /// The chimes' id: no switch number (they are at most 79) reads as `C4`, the chime
    /// line's bit (`04`) after a `C`.
    const CHIME_ID: u32 = 0xC4;

    pub fn find(m: &[u8]) -> Result<Layer, String> {
        // `044D`: LD HL,table; AND F0; SRL A x4; AND 7; INC A; LD B,A; DJNZ; LD A,C; AND
        // 0F; CALL index ... `0479`: LD DE,stride; ADD HL,DE.
        let at = find_pat(
            m,
            &pat("21 ?? ?? E6 F0 CB 3F CB 3F CB 3F CB 3F E6 07 3C 47 10 ?? 79 E6 0F CD"),
            0,
            0x4000,
        )
        .ok_or("no switch-sound dispatch of L'Hexagone's kind in the sound program")?;
        let table = usize::from(u16::from_le_bytes([m[at + 1], m[at + 2]]));
        let stride = find_pat(m, &pat("11 ?? 00 19 18"), at, at + 0x40)
            .map(|k| usize::from(m[k + 1]))
            .ok_or("the row size of the switch-sound table was not found")?;
        let word = |a: usize| usize::from(u16::from_le_bytes([m[a], m[a + 1]]));
        // The routine that plays nothing: `JP` to the main loop's restart (`1080`: `JP
        // 107A`, which reloads the stack and jumps back), the most frequent entry.
        let mut count: BTreeMap<usize, usize> = BTreeMap::new();
        for r in 0..ROWS {
            for b in 0..8 {
                *count.entry(word(table + r * stride + 2 * b)).or_default() += 1;
            }
        }
        let nothing = count
            .iter()
            .max_by_key(|&(_, &n)| n)
            .map(|(&w, _)| w)
            .unwrap_or_default();
        let mut sounds: Vec<GameSound> = Vec::new();
        let mut seen: BTreeMap<usize, u32> = BTreeMap::new();
        for r in 0..ROWS {
            for b in 0..8 {
                let w = word(table + r * stride + 2 * b);
                if w == nothing || w >= 0x4000 {
                    continue;
                }
                // Row 0 bit 0 makes the code `00`, which the loop takes for "none".
                if r == 0 && b == 0 {
                    continue;
                }
                let sw = (b * 10 + r) as u16;
                // The id reads as the switch number: its decimal digits in hex (`0x46` is
                // switch 46).
                let id = u32::from(sw / 10) << 4 | u32::from(sw % 10);
                if let Some(first) = seen.get(&w) {
                    if let Some(g) = sounds.iter_mut().find(|g| g.id == *first) {
                        g.refs.push_str(&format!(", switch {sw}"));
                    }
                    continue;
                }
                seen.insert(w, id);
                sounds.push(GameSound {
                    id,
                    refs: format!(
                        "routine {w:04X} (table {table:04X} row {r} bit {b}): switch {sw}"
                    ),
                    start: Request::Switch(sw),
                });
            }
        }
        if sounds.is_empty() {
            return Err(format!(
                "the switch-sound table at {table:04X} has no routine"
            ));
        }
        // The chimes (`03D3`: BIT 0,(HL); CALL Z,chime; the same for bits 1 and 2), read
        // while the outhole is closed: the line of the game's 10's chime on, then off, the
        // outhole switch held closed around it.
        let switch_sounds = sounds.len();
        let chime = find_pat(
            m,
            &pat("CB 46 CC ?? ?? CB 4E CC ?? ?? CB 56 CC ?? ??"),
            0,
            0x4000,
        )
        .map(|k| usize::from(u16::from_le_bytes([m[k + 3], m[k + 4]])));
        if let Some(c) = chime {
            sounds.push(GameSound {
                id: CHIME_ID,
                refs: format!(
                    "routine {c:04X}: a chime line (the game's 10's chime) with the outhole (switch {OUTHOLE}) closed"
                ),
                start: Request::Seq(vec![
                    Request::SwitchState(OUTHOLE, true),
                    Request::Lines(LINES | CHIME_10),
                    Request::Lines(LINES),
                    Request::SwitchState(OUTHOLE, false),
                ]),
            });
        }
        Ok(Layer {
            what: format!(
                "Tabart switch sounds: the sound program's table at {table:04X} ({ROWS} rows of {} switches, `{nothing:04X}` = none): {switch_sounds} routines, each played by closing one of its switches with the game running{}; DIP bank {DIP_BANK} bits 4 and 7 off (both sound DIPs off: no tune, no background); stop: the board reset",
                stride / 2,
                chime.map_or(String::new(), |c| format!(", and the chimes' routine {c:04X} (id {CHIME_ID:02X})"))
            ),
            id_is: "PinMAME's number of the playfield switch that plays the sound, its decimal digits written as hex digits (0x46: switch 46; the board reads the switches itself)".into(),
            id_digits: 2,
            sounds,
            // The game writes its lines on a lamp change only (gts1.c `lamp_w` → `snd_w`):
            // the lines it would write in attract mode with these DIPs (the sound DIPs, no
            // game, no tilt, no chime) go out once, through its own path.
            setup: vec![Request::Lines(LINES)],
            dips: vec![Dip {
                bank: DIP_BANK,
                mask: 0x90,
                value: DIP_VALUE,
            }],
            stop: Request::Reset,
        })
    }
}

/// Romstar's Goofy Hoops (1994): a 68306 that drives a QSound chip itself.
///
/// The game's program (loaded at `10000000`, PinMAME's `REGION_USER1`) has a sound
/// sequencer: sound effects are byte sequences (a priority byte, then events: a delay and an
/// op, `FD` volume, `FE` pan, a sample number with its length, `F8` end), music a song
/// structure of channel sequences. The game plays them with two routines, as its own sound
/// test does (`SINGLE SOUND TEST`: an address in the song range goes to the song player,
/// any other to the effect player on channel `12`, pan `120`, the middle):
/// - `play_sfx(sequence, pan, channel)` (`B44FA`), `play_song(song)` (`B441A`), both
///   callee-cleaned stack arguments;
/// - the sound system reset (`B4FF0`, what the sound test calls when it is left), the stop.
///
/// The catalog is every sequence and song the program passes to these routines (or to the
/// wrappers that call them): the last immediate it loads before the call (`PEA`, `MOVE.L
/// #`, `MOVEA.L #`, `LEA`), or the entries of a table a loop before the call walks (the
/// "play all" list of effects at `BE718`), each checked as a sequence of the sequencer's
/// grammar (an effect) or as an address in the song range. The
/// requests are calls of the game's own routines (`Request::Call`): writing the sequencer's
/// handles directly could not stop a song, which needs the QSound voices keyed off.
mod romstar {
    use super::*;

    /// Where the program sits in the CPU's address space.
    const BASE: u32 = 0x1000_0000;
    /// The first program ROM pair: code, effects and songs.
    const LEN: usize = 0x10_0000;

    /// An effect sequence at `a` (an offset in the image): its priority byte, a first delay
    /// of 0, then events up to `F8` (end) or `FF` (loop) within 64 events.
    pub(super) fn effect(m: &[u8], a: usize) -> bool {
        if a + 0x200 > LEN || m.get(a + 1) != Some(&0) {
            return false;
        }
        let mut i = a + 1;
        for _ in 0..64 {
            let op = m[i + 1];
            i += 2;
            match op {
                0x00..=0xEF => {
                    // A sample: its length, two more bytes when 0.
                    i += if m[i] == 0 { 3 } else { 1 };
                }
                0xF8 | 0xFF => return true,
                0xFD | 0xFE => i += 1,
                0xFA => i += 2,
                0xF7 | 0xF9 => i += 4,
                0xF0..=0xF5 | 0xFB => {}
                _ => return false,
            }
        }
        false
    }

    fn be32(m: &[u8], a: usize) -> u32 {
        u32::from_be_bytes([m[a], m[a + 1], m[a + 2], m[a + 3]])
    }

    /// The opcode words that load a 32-bit immediate in the code: `PEA (xxx).L`,
    /// `MOVE.L #,-(A7)`, `MOVE.L #,Dn`, `MOVEA.L #,An`, `LEA (xxx).L,An`.
    fn loads_immediate(op: u16) -> bool {
        op == 0x4879
            || op == 0x2F3C
            || op & 0xF1FF == 0x203C
            || op & 0xF1FF == 0x207C
            || op & 0xF1FF == 0x41F9
    }

    /// The routine `JSR (abs).L`, the words to push before it, as a call with saved registers.
    pub(super) fn call(routine: u32, args: &[Arg], lock: u32, busy: (u32, u32)) -> Call {
        let mut code: Vec<u8> = vec![0x48, 0xE7, 0xFF, 0xFE]; // MOVEM.L D0-D7/A0-A6,-(A7)
        for a in args {
            match *a {
                Arg::Word(w) => {
                    code.extend([0x3F, 0x3C]); // MOVE.W #w,-(A7)
                    code.extend(w.to_be_bytes());
                }
                Arg::Long(l) => {
                    code.extend([0x2F, 0x3C]); // MOVE.L #l,-(A7)
                    code.extend(l.to_be_bytes());
                }
            }
        }
        code.extend([0x4E, 0xB9]); // JSR (abs).L
        code.extend(routine.to_be_bytes());
        code.extend([0x4C, 0xDF, 0x7F, 0xFF, 0x4E, 0x73]); // MOVEM.L (A7)+,D0-D7/A0-A6; RTE
        let mut c = Call {
            code: [0; MAX_CALL],
            len: code.len() as u8,
            lock,
            busy_pc: busy,
        };
        c.code[..code.len()].copy_from_slice(&code);
        c
    }

    pub(super) enum Arg {
        Word(u16),
        Long(u32),
    }

    /// PinMAME keeps a 16-bit big-endian region in the host's order: on a little-endian
    /// host each pair of bytes is swapped. The program is read in the CPU's order.
    pub fn find(region: &[u8]) -> Result<Layer, String> {
        if region.len() < LEN {
            return Err("program image smaller than its first ROM pair".into());
        }
        let mut m = region[..LEN].to_vec();
        if cfg!(target_endian = "little") {
            for w in m.as_chunks_mut::<2>().0 {
                w.swap(0, 1);
            }
        }
        let m = &m[..];
        let at = |p: &str| find_pat(m, &pat(p), 0, LEN).filter(|a| a % 2 == 0);
        // play_sfx: `MOVE.W $A(A7),-(A7); MOVE.L 6(A7),-(A7); JSR priority; TST.W D0;
        // BEQ; MOVE.W $A(A7),D0; JSR stop_channel`.
        let sfx = at("3F 2F 00 0A 2F 2F 00 06 4E B9 ?? ?? ?? ?? 4A 40 67 ?? 30 2F 00 0A 4E B9");
        // play_song: `JSR stop_music; JSR stop_voices; MOVE.W #1,tick; CLR.W`.
        let song = at("4E B9 ?? ?? ?? ?? 4E B9 ?? ?? ?? ?? 31 FC 00 01 ?? ?? 42 78");
        // The reset: `MOVEM.L; MOVE.W #1,lock; CLR.L`.
        let reset = at("48 E7 C0 80 31 FC 00 01 ?? ?? 42 B8");
        // The sound test's song range: `CMPI.L #lo,D0; BCS; CMPI.L #hi,D0; BHI`.
        let range = at("0C 80 ?? ?? ?? ?? 65 ?? 0C 80 ?? ?? ?? ?? 62");
        let (Some(sfx), Some(song), Some(reset), Some(range)) = (sfx, song, reset, range) else {
            return Err(format!(
                "Goofy Hoops' sound routines not all found (effects {sfx:?}, songs {song:?}, reset {reset:?}, song range {range:?})"
            ));
        };
        let lock = u32::from(be16(m, reset + 8));
        let (lo, hi) = (be32(m, range + 2), be32(m, range + 10));
        let lowest = sfx.min(song).min(reset) as u32;
        let highest = sfx.max(song).max(reset) as u32;
        let busy = (
            BASE + lowest.saturating_sub(0x1000),
            BASE + highest + 0x1000,
        );
        let addr = |off: usize| BASE + off as u32;
        let is_song = |v: u32| (lo..=hi).contains(&v);
        let in_image = |v: u32| (BASE..BASE + LEN as u32).contains(&v);
        let candidate = |v: u32| in_image(v) && (is_song(v) || effect(m, (v - BASE) as usize));
        // The calls: every `JSR (abs).L`, by target. The sound routines are the two players
        // and the wrappers that call one of them within their first 24 bytes (`B457A`: an
        // effect on channel 11).
        let mut calls: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        for i in (0..LEN - 6).step_by(2) {
            if be16(m, i) == 0x4EB9 {
                calls.entry(be32(m, i + 2)).or_default().push(i);
            }
        }
        let players = [addr(sfx), addr(song)];
        let calls_player = |t: u32| {
            in_image(t) && {
                let o = (t - BASE) as usize;
                (o..o + 24)
                    .step_by(2)
                    .any(|k| be16(m, k) == 0x4EB9 && players.contains(&be32(m, k + 2)))
            }
        };
        let api: Vec<u32> = calls
            .keys()
            .copied()
            .filter(|&t| players.contains(&t) || calls_player(t))
            .collect();
        // What the program passes them: the last immediate it loads before the call (back
        // to the previous call or return), and the tables a loop before the call walks
        // (`MOVEA.L #table,An` ... `MOVE.L (An)+,-(A7)`: the "play all" list at `BE718`).
        let mut refs: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for t in &api {
            for &c in &calls[t] {
                for k in (1..=24).filter_map(|n| c.checked_sub(2 * n)) {
                    let op = be16(m, k);
                    if op == 0x4EB9 || op == 0x4E75 || op & 0xFF00 == 0x6100 {
                        break;
                    }
                    if loads_immediate(op) && candidate(be32(m, k + 2)) {
                        refs.entry(be32(m, k + 2))
                            .or_default()
                            .push(format!("{:08X}", addr(k)));
                    }
                }
                for k in (c.saturating_sub(24)..c).step_by(2) {
                    if be16(m, k) & 0xF1FF != 0x207C {
                        continue;
                    }
                    let table = be32(m, k + 2);
                    let mut e = table;
                    while in_image(e) && (e - BASE) as usize + 4 <= LEN {
                        let v = be32(m, (e - BASE) as usize);
                        if !candidate(v) {
                            break;
                        }
                        refs.entry(v)
                            .or_default()
                            .push(format!("table {table:08X} entry {}", (e - table) / 4));
                        e += 4;
                    }
                }
            }
        }
        if refs.is_empty() {
            return Err("no sound sequence referred to in the program".into());
        }
        let busy_call = |routine: usize, args: &[Arg]| call(addr(routine), args, lock, busy);
        let sounds: Vec<GameSound> = refs
            .iter()
            .map(|(&v, why)| GameSound {
                id: v,
                refs: format!(
                    "{} {}",
                    if is_song(v) { "song" } else { "effect" },
                    why.join(", ")
                ),
                start: Request::Call(if is_song(v) {
                    busy_call(song, &[Arg::Long(v)])
                } else {
                    busy_call(sfx, &[Arg::Word(0x12), Arg::Word(0x120), Arg::Long(v)])
                }),
            })
            .collect();
        let songs = sounds.iter().filter(|s| is_song(s.id)).count();
        Ok(Layer {
            what: format!(
                "Goofy Hoops' sound sequencer: effects played with play_sfx at {:08X} (channel 12, pan 120, as the sound test does), songs ({lo:08X}..{hi:08X}) with play_song at {:08X}, each a call of the game's routine; {} effects and {songs} songs the program refers to; stop: the sound system reset at {:08X}; lock word {lock:04X}",
                addr(sfx),
                addr(song),
                sounds.len() - songs,
                addr(reset)
            ),
            id_is: "the address of the effect sequence or song in the game's program (what the game passes to its play routine)".into(),
            id_digits: 8,
            sounds,
            setup: Vec::new(),
            dips: Vec::new(),
            stop: Request::Call(busy_call(reset, &[])),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An S14001A word table: three words, the second a duplicate of the first, the
    /// entries after the table (bytes of the words) left out.
    #[test]
    fn speech_words_stop_at_the_first_word_and_skip_duplicates() {
        let mut rom = vec![0u8; 0x800];
        // Words 0, 1, 2 start at 006, 006, 00A: the table is 6 bytes long.
        rom[..6].copy_from_slice(&[0x00, 0x60, 0x00, 0x60, 0x00, 0xA0]);
        rom[6] = 0x12;
        assert_eq!(st300::speech_words(&rom), vec![0, 2]);
        assert!(st300::speech_words(&[]).is_empty());
    }

    /// A made-up L'Hexagone-like program: the switch-sound dispatch, a table of two rows
    /// (the no-sound routine everywhere but two entries) and the chime test.
    #[test]
    fn tabart_reads_the_switch_table_and_the_chimes() {
        let mut m = vec![0xFFu8; 0x4000];
        let put = |m: &mut Vec<u8>, at: usize, p: &str| {
            for (i, b) in pat(p).iter().enumerate() {
                m[at + i] = b.unwrap();
            }
        };
        put(
            &mut m,
            0x044D,
            "21 83 10 E6 F0 CB 3F CB 3F CB 3F CB 3F E6 07 3C 47 10 19 79 E6 0F CD 54 07",
        );
        put(&mut m, 0x0479, "11 12 00 19 18 DF");
        put(
            &mut m,
            0x03D3,
            "CB 46 CC B6 04 CB 4E CC B6 04 CB 56 CC B6 04",
        );
        for e in 0..5 * 9 {
            m[0x1083 + 2 * e..0x1085 + 2 * e].copy_from_slice(&[0x80, 0x10]);
        }
        // Row 0 bit 1 (switch 10) and row 1 bit 4 (switch 41).
        m[0x1083 + 2..0x1083 + 4].copy_from_slice(&[0x9D, 0x05]);
        m[0x1095 + 8..0x1095 + 10].copy_from_slice(&[0xDD, 0x08]);
        let l = tabart::find(&m).unwrap();
        let ids: Vec<u32> = l.sounds.iter().map(|g| g.id).collect();
        assert_eq!(ids, vec![0x10, 0x41, 0xC4]);
        assert_eq!(l.sounds[0].start, Request::Switch(10));
        assert_eq!(l.stop, Request::Reset);
        assert_eq!(l.setup, vec![Request::Lines(0)]);
    }

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
        let Request::Pokes(stop) = &l.stop else {
            panic!("pokes expected")
        };
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

    /// Goofy Hoops: an effect sequence as the game stores it (priority, volume, pan, the
    /// sample with its length, end), and a song channel (an instrument pointer first).
    #[test]
    fn romstar_effects_are_read_as_the_sequencer_reads_them() {
        let mut m = vec![0u8; 0x400];
        m[..12].copy_from_slice(&[
            0x10, 0x00, 0xFD, 0x20, 0x00, 0xFE, 0xF9, 0x00, 0x58, 0x55, 0x55, 0xF8,
        ]);
        assert!(romstar::effect(&m, 0));
        m[16..22].copy_from_slice(&[0x10, 0x0B, 0x5D, 0xE8, 0x00, 0xFD]);
        assert!(!romstar::effect(&m, 16));
    }

    /// The routine a `Call` carries: registers saved, the arguments pushed as the game
    /// pushes them, the call, registers restored, `RTE`.
    #[test]
    fn romstar_calls_push_the_arguments_as_the_game_does() {
        use romstar::Arg;
        let c = romstar::call(
            0x100B_44FA,
            &[Arg::Word(0x12), Arg::Word(0x120), Arg::Long(0x100B_5E42)],
            0x0AA8,
            (0x100B_34FA, 0x100B_5FF0),
        );
        assert_eq!(
            c.code[..c.len as usize],
            [
                0x48, 0xE7, 0xFF, 0xFE, 0x3F, 0x3C, 0x00, 0x12, 0x3F, 0x3C, 0x01, 0x20, 0x2F, 0x3C,
                0x10, 0x0B, 0x5E, 0x42, 0x4E, 0xB9, 0x10, 0x0B, 0x44, 0xFA, 0x4C, 0xDF, 0x7F, 0xFF,
                0x4E, 0x73
            ]
        );
    }

    /// Atari generation 1 (The Atarians): one block per down-counter in the sound routine,
    /// and the lengths the game stores in each.
    #[test]
    fn atari_gen1_counters_and_their_lengths() {
        let mut m = vec![0u8; 0x10000];
        let block = |c: u8| {
            [
                0x96, c, 0x27, 0x0E, 0xCE, 0x7F, 0x6B, 0xBD, 0x7B, 0xDA, 0xF6, 0x7F, 0x8B, 0x7A,
                0x00, c,
            ]
        };
        m[0x785D..0x786D].copy_from_slice(&block(0xC5));
        m[0x786F..0x787F].copy_from_slice(&block(0xA5));
        // LDAA #1F; STAA $A5 and LDAA #20; STAA $C5.
        m[0x7929..0x792D].copy_from_slice(&[0x86, 0x1F, 0x97, 0xA5]);
        m[0x7B6A..0x7B6E].copy_from_slice(&[0x86, 0x20, 0x97, 0xC5]);
        let l = atari::gen1(&m).unwrap();
        let ids: Vec<u32> = l.sounds.iter().map(|s| s.id).collect();
        assert_eq!(ids, [0xC520, 0xA51F]);
        assert_eq!(
            l.sounds[1].start,
            Request::Pokes(vec![Poke {
                addr: 0xA5,
                val: 0x1F
            }])
        );
    }
}
