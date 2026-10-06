//! Stern SAM (2006-2014): the sound data of the flash image, read statically.
//!
//! A SAM machine has no sound board and no sound CPU: its one ARM7 mixes every voice in
//! software and writes the mix to a DAC. The game never sends a sound command anywhere,
//! so the method of the other boards (send a command, record the board) cannot work, and
//! PinMAME has no AltSound hook for SAM. The sounds are read from the image instead.
//!
//! The format was reverse engineered on Tron LE 1.74 by Ashram56
//! (<https://github.com/Ashram56/Tron-Legacy-LE-ROM-Decryption>) and confirmed on AC/DC
//! LE 1.68 (`docs/how-it-works.md`, "Stern SAM"):
//!
//! - a banked pointer `p` is the file offset `(p >> 24) * 8 MB + (p & 0xFFFFFF)`;
//! - the sample directory (first 8 MB) holds, per sample id, one pointer per language to
//!   a stream script (0 = none);
//! - a script is `05 <voice mask> <voices> <len32>` (length in 4000 Hz ticks), then
//!   opcodes (`SCRIPT_ARGS`): `0a <voice> <ptr32>` starts an ADPCM stream, `10` waits for
//!   it, `0f` waits N ticks, `07`/`03` loop, `0d` tells the game the song position...;
//! - a stream is an 8-byte header (`u32 samples, u16 1, u8 divisor, u8 divisor`) then
//!   4-bit IMA ADPCM, low nibble first, at 24000 / divisor Hz, decoder state reset at the
//!   start of each stream;
//! - the sound call table: 20-byte records whose `+8` points to the 0-terminated u16 list
//!   of sample ids the call picks from. The call is what the game code asks for.
//!
//! Music is sequenced: a song script chains 1-2.5 s chunks, one `0a` each.

use std::collections::{BTreeMap, HashMap};

include!(concat!(env!("OUT_DIR"), "/sam_sets.rs"));

pub const BANK: usize = 0x80_0000;
/// The DAC's sample rate: every stream is 24000 / divisor Hz.
pub const BASE_RATE: u32 = 24000;
/// The FIQ that runs the scripts (`0f` waits and `len32` count its ticks).
pub const FIQ_HZ: u32 = 4000;
/// Output frames per FIQ tick.
const FRAMES_PER_TICK: u64 = (BASE_RATE / FIQ_HZ) as u64;

/// The SAM set of PinMAME's driver with this name: (image file name, CRC32, length).
pub fn sam_set(rom: &str) -> Option<(&'static str, u32, u32)> {
    SAM_SETS
        .iter()
        .find(|s| s.0.eq_ignore_ascii_case(rom))
        .map(|s| (s.1, s.2, s.3))
}

// ------------------------------------------------------------------------------- ADPCM

const STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IDX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Standard IMA ADPCM, low nibble first, predictor and step index starting at 0:
/// `count` samples from `data`.
pub fn ima_decode(data: &[u8], count: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(count);
    let (mut pred, mut idx) = (0i32, 0i32);
    'bytes: for &b in data {
        for nib in [b & 15, b >> 4] {
            if out.len() >= count {
                break 'bytes;
            }
            let st = STEP[idx as usize];
            let mut d = st >> 3;
            if nib & 1 != 0 {
                d += st >> 2;
            }
            if nib & 2 != 0 {
                d += st >> 1;
            }
            if nib & 4 != 0 {
                d += st;
            }
            pred = if nib & 8 != 0 { pred - d } else { pred + d };
            pred = pred.clamp(-32768, 32767);
            idx = (idx + IDX[nib as usize]).clamp(0, 88);
            out.push(pred as i16);
        }
    }
    out
}

/// The matching encoder (tests build synthetic images with it).
#[cfg(test)]
pub fn ima_encode(pcm: &[i16]) -> Vec<u8> {
    let (mut pred, mut idx) = (0i32, 0i32);
    let mut nibs = Vec::with_capacity(pcm.len());
    for &x in pcm {
        let st = STEP[idx as usize];
        let mut diff = x as i32 - pred;
        let mut nib = 0u8;
        if diff < 0 {
            nib = 8;
            diff = -diff;
        }
        let mut d = st >> 3;
        if diff >= st {
            nib |= 4;
            diff -= st;
            d += st;
        }
        if diff >= st >> 1 {
            nib |= 2;
            diff -= st >> 1;
            d += st >> 1;
        }
        if diff >= st >> 2 {
            nib |= 1;
            d += st >> 2;
        }
        pred = if nib & 8 != 0 { pred - d } else { pred + d }.clamp(-32768, 32767);
        idx = (idx + IDX[nib as usize]).clamp(0, 88);
        nibs.push(nib);
    }
    nibs.chunks(2)
        .map(|c| c[0] | (c.get(1).copied().unwrap_or(0) << 4))
        .collect()
}

// ------------------------------------------------------------------------------- parsing

fn u32_at(rom: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(rom[o..o + 4].try_into().unwrap())
}

fn u16_at(rom: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([rom[o], rom[o + 1]])
}

/// The file offset of a banked pointer, or None if it cannot be one.
pub fn banked(p: u32, size: usize) -> Option<usize> {
    let (bank, off) = ((p >> 24) as usize, (p & 0xFF_FFFF) as usize);
    if bank > 15 || off >= BANK {
        return None;
    }
    let o = bank * BANK + off;
    (o < size).then_some(o)
}

/// An ADPCM stream: its 8-byte header at `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    pub off: usize,
    /// Samples at the stream's own rate.
    pub count: usize,
    /// The header's u16 (1 on every real stream).
    pub field4: u16,
    pub div: u32,
    /// End of the ADPCM data.
    pub end: usize,
}

impl Stream {
    pub fn rate(&self) -> u32 {
        BASE_RATE / self.div
    }
    /// Length in 24 kHz output frames.
    pub fn frames24(&self) -> u64 {
        (self.count as u64) * self.div as u64
    }
    pub fn secs(&self) -> f64 {
        self.frames24() as f64 / BASE_RATE as f64
    }
    /// The stream decoded at its own rate.
    pub fn decode(&self, rom: &[u8]) -> Vec<i16> {
        ima_decode(&rom[self.off + 8..self.end], self.count)
    }
}

/// The stream header at `o`; `strict` also wants the u16 field to be 1.
pub fn stream_header(rom: &[u8], o: usize, strict: bool) -> Option<Stream> {
    if o + 8 > rom.len() {
        return None;
    }
    let count = u32_at(rom, o) as usize;
    let field4 = u16_at(rom, o + 4);
    let (div, div2) = (rom[o + 6], rom[o + 7]);
    if div != div2 || !(1..=4).contains(&div) || (strict && field4 != 1) {
        return None;
    }
    if !(2..=BASE_RATE as usize * 1200).contains(&count) {
        return None;
    }
    let end = o + 8 + count.div_ceil(2);
    (end <= rom.len()).then_some(Stream {
        off: o,
        count,
        field4,
        div: div as u32,
        end,
    })
}

/// A stream header, strict first.
fn stream_any(rom: &[u8], o: usize) -> Option<Stream> {
    stream_header(rom, o, true).or_else(|| stream_header(rom, o, false))
}

/// A script header `05 <mask> <voices> <len32>`.
#[derive(Debug, Clone, Copy)]
pub struct ScriptHead {
    pub off: usize,
    pub mask: u8,
    pub voices: u8,
    pub len32: u32,
}

impl ScriptHead {
    pub fn secs(&self) -> f64 {
        self.len32 as f64 / FIQ_HZ as f64
    }
}

pub fn script_at(rom: &[u8], o: usize) -> Option<ScriptHead> {
    if o + 7 > rom.len() || rom[o] != 0x05 {
        return None;
    }
    let (mask, voices) = (rom[o + 1], rom[o + 2]);
    let len32 = u32_at(rom, o + 3);
    if mask == 0 || !(1..=8).contains(&voices) || len32 > FIQ_HZ * 1800 {
        return None;
    }
    Some(ScriptHead {
        off: o,
        mask,
        voices,
        len32,
    })
}

/// The sample directory: `n` u32 words from file offset `start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Directory {
    pub start: usize,
    pub n: usize,
}

/// The longest run (first 8 MB, 4-byte aligned) of u32 words that are 0 or banked
/// pointers to a valid script, without its leading and trailing zeros.
pub fn find_directory(rom: &[u8]) -> Option<Directory> {
    let lim = rom.len().min(BANK) & !3;
    let mut cache: HashMap<u32, bool> = HashMap::new();
    // (start, words, pointers, zeros at the end)
    let mut best: Option<(usize, usize, usize, usize)> = None;
    let mut cur: Option<(usize, usize, usize, usize)> = None;
    let close = |cur: &mut Option<(usize, usize, usize, usize)>,
                 best: &mut Option<(usize, usize, usize, usize)>| {
        if let Some(c) = cur.take()
            && best.is_none_or(|b| c.2 > b.2)
        {
            *best = Some(c);
        }
    };
    for i in 0..lim / 4 {
        let w = u32_at(rom, i * 4);
        let ok = if w == 0 {
            Some(false)
        } else if (w >> 24) <= 15 {
            let p = *cache.entry(w).or_insert_with(|| {
                banked(w, rom.len()).is_some_and(|o| o >= 0x1000 && script_at(rom, o).is_some())
            });
            p.then_some(true)
        } else {
            None
        };
        match ok {
            Some(is_ptr) => {
                let c = cur.get_or_insert((i * 4, 0, 0, 0));
                c.1 += 1;
                if is_ptr {
                    c.2 += 1;
                    c.3 = 0;
                } else {
                    c.3 += 1;
                }
            }
            None => close(&mut cur, &mut best),
        }
    }
    close(&mut cur, &mut best);
    let (mut start, mut n, ptrs, zero_tail) = best?;
    if ptrs < 4 {
        return None;
    }
    n -= zero_tail;
    while n > 0 && u32_at(rom, start) == 0 {
        start += 4;
        n -= 1;
    }
    Some(Directory { start, n })
}

/// The number of languages: the largest stride k (2..8) for which at least half of the
/// groups of k consecutive entries hold one same pointer (sounds that are not speech
/// point every language at the same script); 1 if none.
pub fn guess_nlang(words: &[u32]) -> usize {
    let mut best = 1;
    for k in 2..=8 {
        if words.len() < 2 * k {
            continue;
        }
        let groups: Vec<&[u32]> = (0..=words.len() - k)
            .step_by(k)
            .map(|i| &words[i..i + k])
            .collect();
        let same = groups
            .iter()
            .filter(|g| g.iter().all(|&w| w == g[0]))
            .count();
        if same as f64 / groups.len() as f64 >= 0.5 {
            best = k;
        }
    }
    best
}

/// One record of the sound call table.
#[derive(Debug, Clone)]
pub struct Call {
    /// The call id: the record's index from the table's start.
    pub id: u32,
    pub off: usize,
    /// The sample ids the call picks from.
    pub samples: Vec<u16>,
    pub raw: [u8; 20],
}

/// The sample list of the call record at `rec`, if its `+8` word points (CPU address
/// 0x04xxxxxx, the fixed first 8 MB) to a 0-terminated list of ids below `nsamples`.
fn call_list(rom: &[u8], rec: usize, nsamples: usize, allow_empty: bool) -> Option<Vec<u16>> {
    let lim = rom.len().min(BANK);
    if rec + 20 > lim {
        return None;
    }
    let p = u32_at(rom, rec + 8) as usize;
    if !(0x0400_0000..0x0400_0000 + lim).contains(&p) {
        return None;
    }
    let o = p - 0x0400_0000;
    let mut lst = Vec::new();
    for j in 0..64 {
        if o + 2 * j + 2 > lim {
            return None;
        }
        let v = u16_at(rom, o + 2 * j);
        if v == 0 {
            return (allow_empty || !lst.is_empty()).then_some(lst);
        }
        if v as usize >= nsamples {
            return None;
        }
        lst.push(v);
    }
    None
}

/// The sound call table: the longest run of 20-byte records with a valid sample list
/// (first 8 MB), then grown over neighbouring records with an empty list (a call that
/// plays nothing, such as call 0) whose first word, a pointer to the call's own state,
/// continues the run's sequence.
pub fn find_call_table(rom: &[u8], nsamples: usize) -> Option<Vec<Call>> {
    let lim = rom.len().min(BANK);
    let mut best: Option<(usize, usize)> = None; // (start, records)
    for phase in (0..20).step_by(4) {
        let mut i = phase;
        while i + 20 <= lim {
            if call_list(rom, i, nsamples, false).is_none() {
                i += 20;
                continue;
            }
            let start = i;
            while call_list(rom, i, nsamples, false).is_some() {
                i += 20;
            }
            let n = (i - start) / 20;
            if n >= 8 && best.is_none_or(|b| n > b.1) {
                best = Some((start, n));
            }
        }
    }
    let (mut start, mut n) = best?;
    let state = |rec: usize| u32_at(rom, rec);
    let follows = |a: usize, b: usize| {
        // b's state pointer comes right after a's (a few bytes further, same region).
        let (pa, pb) = (state(a), state(b));
        pb > pa && pb - pa <= 64 && pa >> 24 == pb >> 24
    };
    while start >= 20
        && call_list(rom, start - 20, nsamples, true).is_some()
        && follows(start - 20, start)
    {
        start -= 20;
        n += 1;
    }
    loop {
        let next = start + 20 * n;
        if call_list(rom, next, nsamples, true).is_some() && follows(next - 20, next) {
            n += 1;
        } else {
            break;
        }
    }
    Some(
        (0..n)
            .map(|k| {
                let off = start + 20 * k;
                Call {
                    id: k as u32,
                    off,
                    samples: call_list(rom, off, nsamples, true).unwrap_or_default(),
                    raw: rom[off..off + 20].try_into().unwrap(),
                }
            })
            .collect(),
    )
}

// ------------------------------------------------------------------------------- scripts

/// Operand bytes of each script opcode (decoded on acd_168h: every one of its 1054
/// scripts parses to its end with this table). Bytes above 0x10 are skipped (0xFF pads).
pub const SCRIPT_ARGS: [Option<usize>; 17] = [
    Some(0),  // 00 end: wait for the ramps / len32, free the voices
    Some(2),  // 01 ? (01 00 00 / 01 00 01)
    Some(2),  // 02 voice/bus routing: 02 <voice> <bus> (music = bus 3)
    Some(1),  // 03 loop: jump back to the 07 mark
    Some(2),  // 04 bus-to-bus link
    Some(6),  // 05 script header (05 <mask> <voices> <len32>)
    Some(0),  // 06 ?
    Some(1),  // 07 loop start mark
    Some(4),  // 08 u32 marker the game polls
    Some(15), // 09 volume ramp: <bus> <period u32> <steps u32> <start> <delta> <final>
    Some(5),  // 0a start stream: 0a <voice> <banked ptr32>
    Some(2),  // 0b channel stop stub
    Some(0),  // 0c unused
    Some(4),  // 0d u32 song position the game polls: song << 24 | chunk index
    Some(1),  // 0e stop the voices in mask
    Some(4),  // 0f wait u32 FIQ ticks (4000 Hz)
    Some(1),  // 10 wait until the voices in mask have finished
];

/// One opcode: the opcode (None: unknown, the parse stopped) and operands.
#[derive(Debug, Clone)]
pub struct Op {
    pub op: Option<u8>,
    pub args: Vec<u8>,
}

/// The opcodes of the script at `o` (after its 7-byte header) up to `end`; stops after
/// 00 (end), 03 (loop), 0b (stop stub) or an unknown opcode.
pub fn parse_script(rom: &[u8], o: usize, end: usize) -> Vec<Op> {
    let end = end.min(rom.len());
    let mut p = o + 7;
    let mut ops = Vec::new();
    while p < end {
        let op = rom[p];
        if op > 0x10 {
            p += 1;
            continue;
        }
        let n = SCRIPT_ARGS[op as usize];
        match n {
            Some(n) if p + 1 + n <= end => {
                ops.push(Op {
                    op: Some(op),
                    args: rom[p + 1..p + 1 + n].to_vec(),
                });
                p += 1 + n;
                if matches!(op, 0x00 | 0x03 | 0x0b) {
                    break;
                }
            }
            _ => {
                ops.push(Op {
                    op: None,
                    args: rom[p..(p + 8).min(rom.len())].to_vec(),
                });
                break;
            }
        }
    }
    ops
}

/// A script with several streams chained, or a loop: music (songs and loop beds).
pub fn is_sequenced(ops: &[Op]) -> bool {
    let has = |c: u8| ops.iter().any(|o| o.op == Some(c));
    ops.iter().filter(|o| o.op == Some(0x0a)).count() >= 2 || (has(0x07) && has(0x03))
}

/// The streams a script starts, in order.
pub fn script_streams(rom: &[u8], ops: &[Op]) -> Vec<Stream> {
    ops.iter()
        .filter(|o| o.op == Some(0x0a))
        .filter_map(|o| banked(u32_at(&o.args, 1), rom.len()).and_then(|p| stream_any(rom, p)))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marker {
    pub at: u64,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub at: u64,
    pub song: u32,
    pub index: u32,
}

/// A script run once, in 24 kHz output frames.
#[derive(Debug, Clone)]
pub struct Timeline {
    /// (start frame, stream, voice)
    pub events: Vec<(u64, Stream, u8)>,
    /// The `07` mark, and the loop's end (the script's end) when it ends with `03`.
    pub loop_start: Option<u64>,
    pub loop_end: Option<u64>,
    pub markers: Vec<Marker>,
    pub positions: Vec<Position>,
    pub end: u64,
    /// Event index -> frames played: a new stream on a busy voice replaces the old one.
    pub cut: BTreeMap<usize, u64>,
}

impl Timeline {
    /// Frames each event plays.
    pub fn played(&self, i: usize) -> u64 {
        self.cut
            .get(&i)
            .copied()
            .unwrap_or_else(|| self.events[i].1.frames24())
    }
}

/// Runs a script once. The FIQ runs it at 4000 Hz (6 output frames per tick); streams
/// chained with `10` are joined gaplessly.
pub fn timeline(rom: &[u8], ops: &[Op]) -> Timeline {
    let mut t = 0u64;
    let mut voices: BTreeMap<u8, u64> = BTreeMap::new();
    let mut tl = Timeline {
        events: Vec::new(),
        loop_start: None,
        loop_end: None,
        markers: Vec::new(),
        positions: Vec::new(),
        end: 0,
        cut: BTreeMap::new(),
    };
    for o in ops {
        let a = &o.args;
        match o.op {
            Some(0x0a) => {
                let Some(h) = banked(u32_at(a, 1), rom.len()).and_then(|p| stream_any(rom, p))
                else {
                    continue;
                };
                voices.insert(a[0], t + h.frames24());
                tl.events.push((t, h, a[0]));
            }
            Some(0x10) => {
                let mask = a[0] as u32;
                if let Some(e) = voices
                    .iter()
                    .filter(|(v, _)| **v < 32 && (mask >> **v) & 1 != 0)
                    .map(|(_, e)| *e)
                    .max()
                {
                    t = t.max(e);
                }
            }
            Some(0x0f) => t += FRAMES_PER_TICK * u32_at(a, 0) as u64,
            Some(0x07) => tl.loop_start = Some(t),
            Some(0x08) => tl.markers.push(Marker {
                at: t,
                value: u32_at(a, 0),
            }),
            Some(0x0d) => {
                let v = u32_at(a, 0);
                tl.positions.push(Position {
                    at: t,
                    song: v >> 24,
                    index: v & 0xFF_FFFF,
                });
            }
            Some(0x00) => break,
            Some(0x03) => {
                tl.loop_end = Some(t);
                break;
            }
            _ => {}
        }
    }
    tl.end = voices.values().copied().fold(t, u64::max);
    for (i, &(t0, h, v)) in tl.events.iter().enumerate() {
        if let Some(&(nxt, _, _)) = tl.events[i + 1..].iter().find(|e| e.2 == v)
            && nxt < t0 + h.frames24()
        {
            tl.cut.insert(i, nxt - t0);
        }
    }
    if tl.loop_end.is_some() {
        tl.loop_end = Some(tl.end);
    }
    tl
}

/// A stream brought to 24 kHz the way the FIQ handlers do it (linear interpolation for
/// the 12 / 8 / 6 kHz streams).
pub fn stream_pcm24(rom: &[u8], h: &Stream) -> Vec<i16> {
    let pcm = h.decode(rom);
    if h.div == 1 {
        return pcm;
    }
    let d = h.div as i32;
    let mut out = Vec::with_capacity(pcm.len() * d as usize);
    for (i, &x) in pcm.iter().enumerate() {
        let y = pcm.get(i + 1).copied().unwrap_or(x) as i32;
        let x = x as i32;
        for k in 0..d {
            out.push((x + ((y - x) * k).div_euclid(d)) as i16);
        }
    }
    out
}

/// Leading samples of a stream that only ramp up from the decoder's reset state: the
/// encoder saturates the first nibbles (x7 / xF), at most 16 of them.
pub fn reset_ramp(rom: &[u8], h: &Stream) -> usize {
    let mut k = 0;
    'bytes: for &b in &rom[(h.off + 8).min(rom.len())..(h.off + 16).min(rom.len())] {
        for nib in [b & 15, b >> 4] {
            if nib & 7 != 7 || k >= 16 {
                break 'bytes;
            }
            k += 1;
        }
    }
    k
}

/// Every chunk restarts the ADPCM decoder at predictor 0, so its first samples ramp up from
/// 0 and the join clicks (on the machine too). Replaces that ramp (`k` samples) by a
/// straight line from the previous sample `prev`.
pub fn declick(pcm: &mut [i16], k: usize, prev: i16) {
    if k == 0 || k + 1 >= pcm.len() {
        return;
    }
    let (end, prev) = (pcm[k] as i32, prev as i32);
    for (i, v) in pcm[..k].iter_mut().enumerate() {
        *v = (prev + ((end - prev) * (i as i32 + 1)).div_euclid(k as i32 + 1)) as i16;
    }
}

/// A sequenced script rendered to one 24 kHz mono signal.
pub struct Rendered {
    pub pcm: Vec<i16>,
    /// The ramp length of the stream starting at the loop's start, when the join there
    /// was declicked (the extended files redo it at each cycle).
    pub loop_ramp: usize,
}

/// Renders a script's timeline: streams joined back to back (tick padding holds the last
/// value, longer gaps are silence) with declicked joins, or mixed when streams overlap
/// (short loop beds).
pub fn render(rom: &[u8], tl: &Timeline, declick_joins: bool) -> Rendered {
    let n = tl.end as usize;
    let ev = &tl.events;
    let overlap = (0..ev.len().saturating_sub(1)).any(|i| ev[i + 1].0 < ev[i].0 + tl.played(i));
    if overlap {
        let mut mix = vec![0i32; n];
        for (j, (t0, h, _)) in ev.iter().enumerate() {
            let pcm = stream_pcm24(rom, h);
            let len = (tl.played(j) as usize).min(pcm.len());
            let t0 = *t0 as usize;
            for (i, &x) in pcm[..len].iter().take(n.saturating_sub(t0)).enumerate() {
                mix[t0 + i] += x as i32;
            }
        }
        return Rendered {
            pcm: mix
                .into_iter()
                .map(|x| x.clamp(-32768, 32767) as i16)
                .collect(),
            loop_ramp: 0,
        };
    }
    let mut out: Vec<i16> = Vec::with_capacity(n);
    let (mut cur, mut last) = (0u64, 0i16);
    let mut loop_ramp = 0;
    for (j, (t0, h, _)) in ev.iter().enumerate() {
        let t0 = *t0;
        let joined = (t0 > cur && t0 - cur < FRAMES_PER_TICK) || (t0 == cur && cur > 0);
        if t0 > cur {
            let gap = (t0 - cur) as usize;
            let v = if gap < FRAMES_PER_TICK as usize {
                last
            } else {
                0
            };
            out.extend(std::iter::repeat_n(v, gap));
        }
        let mut pcm = stream_pcm24(rom, h);
        pcm.truncate(tl.played(j) as usize);
        if declick_joins && joined && h.div == 1 {
            let k = reset_ramp(rom, h);
            declick(&mut pcm, k, last);
            if tl.loop_start == Some(t0) && k > 0 && k + 1 < pcm.len() {
                loop_ramp = k;
            }
        }
        cur = t0 + pcm.len() as u64;
        last = pcm.last().copied().unwrap_or(last);
        out.extend_from_slice(&pcm);
    }
    if tl.end > cur {
        let rest = (tl.end - cur) as usize;
        let v = if rest < FRAMES_PER_TICK as usize {
            last
        } else {
            0
        };
        out.extend(std::iter::repeat_n(v, rest));
    }
    Rendered {
        pcm: out,
        loop_ramp,
    }
}

/// The role of a music script.
pub fn music_role(tl: &Timeline) -> &'static str {
    let first = tl.positions.first().map(|p| p.index);
    if tl.markers.iter().any(|m| m.value == 1) && tl.loop_start.is_some() {
        "teaser" // song select: one chunk then a short looped section, markers 0 then 1
    } else if !tl.positions.is_empty() && first == Some(1) && tl.loop_end.is_none() {
        "full" // chunk 1 to the last chunk, once
    } else if tl.loop_end.is_some() {
        if tl.positions.is_empty() {
            "bed"
        } else {
            "main"
        }
    } else {
        "resume" // from a later chunk to the end, once
    }
}

// ------------------------------------------------------------------------------- catalog

/// One directory entry that points to a script.
#[derive(Debug, Clone, Copy)]
pub struct Entry {
    pub sample: u16,
    pub lang: u8,
    pub script: usize,
}

/// A distinct stream of the (non-sequenced) sound scripts.
#[derive(Debug, Clone)]
pub struct SoundStream {
    pub stream: Stream,
    /// (sample, language) of every entry playing it, in directory order.
    pub owners: Vec<(u16, u8)>,
    pub script: ScriptHead,
}

/// A sequenced script (a song version or a loop bed).
#[derive(Debug, Clone)]
pub struct Music {
    pub owners: Vec<(u16, u8)>,
    pub script: ScriptHead,
    pub timeline: Timeline,
    pub role: &'static str,
}

impl Music {
    pub fn sample(&self) -> u16 {
        self.owners[0].0
    }
    pub fn song(&self) -> Option<u32> {
        self.timeline.positions.first().map(|p| p.song)
    }
    pub fn first_index(&self) -> Option<u32> {
        self.timeline.positions.first().map(|p| p.index)
    }
    pub fn last_index(&self) -> Option<u32> {
        self.timeline.positions.last().map(|p| p.index)
    }
}

/// Everything read from a SAM image.
pub struct Catalog {
    pub directory: Directory,
    pub nlang: usize,
    pub entries: Vec<Entry>,
    pub calls: Vec<Call>,
    pub sounds: Vec<SoundStream>,
    pub music: Vec<Music>,
    /// Distinct scripts, and those that play nothing (channel-stop stubs).
    pub scripts: usize,
    pub stubs: usize,
    pub unknown_opcodes: usize,
}

impl Catalog {
    /// The calls each sample is in.
    pub fn calls_of(&self) -> HashMap<u16, Vec<u32>> {
        let mut m: HashMap<u16, Vec<u32>> = HashMap::new();
        for c in &self.calls {
            for &s in &c.samples {
                m.entry(s).or_default().push(c.id);
            }
        }
        m
    }

    /// Samples whose languages point at different scripts: speech.
    pub fn localized(&self) -> std::collections::HashSet<u16> {
        let mut per: BTreeMap<u16, Vec<Option<usize>>> = BTreeMap::new();
        let k = self.nlang.max(1);
        for e in &self.entries {
            per.entry(e.sample).or_insert_with(|| vec![None; k])[e.lang as usize] = Some(e.script);
        }
        per.into_iter()
            .filter(|(_, v)| v.iter().any(|x| *x != v[0]))
            .map(|(s, _)| s)
            .collect()
    }
}

/// Reads the directory, the scripts and the call table of a SAM image.
pub fn catalog(rom: &[u8]) -> Result<Catalog, String> {
    let directory = find_directory(rom)
        .ok_or("no SAM sample directory found in the image (not a SAM sound layout?)")?;
    let words: Vec<u32> = (0..directory.n)
        .map(|i| u32_at(rom, directory.start + 4 * i))
        .collect();
    let nlang = guess_nlang(&words);
    let entries: Vec<Entry> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| **w != 0)
        .filter_map(|(i, &w)| {
            let script = banked(w, rom.len())?;
            Some(Entry {
                sample: (i / nlang) as u16,
                lang: (i % nlang) as u8,
                script,
            })
        })
        .collect();
    let mut offs: Vec<usize> = entries.iter().map(|e| e.script).collect();
    offs.sort_unstable();
    offs.dedup();
    let next: HashMap<usize, usize> = offs
        .iter()
        .enumerate()
        .map(|(i, &o)| (o, offs.get(i + 1).copied().unwrap_or(rom.len())))
        .collect();
    // Scripts in directory order, with their owners.
    let mut order: Vec<usize> = Vec::new();
    let mut owners: HashMap<usize, Vec<(u16, u8)>> = HashMap::new();
    for e in &entries {
        let o = owners.entry(e.script).or_default();
        if o.is_empty() {
            order.push(e.script);
        }
        o.push((e.sample, e.lang));
    }
    let mut sounds: Vec<SoundStream> = Vec::new();
    let mut by_stream: HashMap<usize, usize> = HashMap::new();
    let mut music = Vec::new();
    let (mut stubs, mut unknown_opcodes) = (0, 0);
    // A stream's owners in directory order: walk the entries, not the scripts.
    let mut script_ops: HashMap<usize, Vec<Op>> = HashMap::new();
    for &so in &order {
        let ops = parse_script(rom, so, next[&so]);
        unknown_opcodes += ops.iter().filter(|o| o.op.is_none()).count();
        let head = script_at(rom, so).expect("directory pointers are valid scripts");
        if is_sequenced(&ops) {
            let timeline = timeline(rom, &ops);
            let role = music_role(&timeline);
            music.push(Music {
                owners: owners[&so].clone(),
                script: head,
                timeline,
                role,
            });
        } else if script_streams(rom, &ops).is_empty() {
            stubs += 1;
        }
        script_ops.insert(so, ops);
    }
    for e in &entries {
        let ops = &script_ops[&e.script];
        if is_sequenced(ops) {
            continue;
        }
        for st in script_streams(rom, ops) {
            match by_stream.get(&st.off) {
                Some(&i) => sounds[i].owners.push((e.sample, e.lang)),
                None => {
                    by_stream.insert(st.off, sounds.len());
                    sounds.push(SoundStream {
                        stream: st,
                        owners: vec![(e.sample, e.lang)],
                        script: script_at(rom, e.script).unwrap(),
                    });
                }
            }
        }
    }
    // The script that picks up right after a song's teaser is the in-game one ("main");
    // the other partial versions are "resume".
    let teaser_last: BTreeMap<u32, u32> = music
        .iter()
        .filter(|m| m.role == "teaser")
        .filter_map(|m| Some((m.song()?, m.last_index()?)))
        .collect();
    for (&song, &last) in &teaser_last {
        let cands: Vec<usize> = (0..music.len())
            .filter(|&i| {
                music[i].song() == Some(song) && matches!(music[i].role, "main" | "resume")
            })
            .collect();
        let mut first: Option<usize> = None;
        for &i in &cands {
            let fi = music[i].first_index().unwrap_or(0);
            if fi > last && first.is_none_or(|f| fi < music[f].first_index().unwrap_or(0)) {
                first = Some(i);
            }
        }
        for &i in &cands {
            music[i].role = if Some(i) == first { "main" } else { "resume" };
        }
    }
    let nsamples = words.len() / nlang + 1;
    let calls = find_call_table(rom, nsamples).unwrap_or_default();
    Ok(Catalog {
        directory,
        nlang,
        entries,
        calls,
        sounds,
        music,
        scripts: order.len(),
        stubs,
        unknown_opcodes,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn tone(n: usize, rate: u32, f: f64, amp: f64) -> Vec<i16> {
        (0..n)
            .map(|i| {
                (amp * (2.0 * std::f64::consts::PI * f * i as f64 / rate as f64).sin()
                    * (1.0 - i as f64 / n as f64)) as i16
            })
            .collect()
    }

    fn ptr(o: usize) -> u32 {
        (((o / BANK) as u32) << 24) | (o % BANK) as u32
    }

    /// A synthetic SAM-like image (in the spirit of sam_study.py's selftest): a directory
    /// of 12 samples x 5 languages, one-stream scripts in bank 1, streams in bank 2 (some
    /// at 12 kHz), one speech sample per 4 with its own script per language and one
    /// language missing, a 3-chunk song that loops after its first chunk, and a call
    /// table (call 0 empty).
    pub struct Synthetic {
        pub rom: Vec<u8>,
        pub originals: Vec<(usize, Vec<i16>)>,
        pub dir_off: usize,
        pub song_chunks: Vec<usize>,
        pub song_sample: u16,
    }

    pub fn synthetic() -> Synthetic {
        let mut rom = vec![0xFFu8; 3 * BANK];
        let (nlang, nsamp) = (5usize, 12usize);
        let dir_off = 0x12_0048;
        let divs = [1u32, 2, 1, 1, 2, 1, 1, 1, 2, 1, 1, 1];
        let mut sp = BANK + 0x100;
        let mut dp = 2 * BANK + 0x40;
        let mut originals = Vec::new();
        let put_stream = |rom: &mut Vec<u8>, dp: &mut usize, pcm: &[i16], div: u32| {
            let enc = ima_encode(pcm);
            let o = *dp;
            rom[o..o + 4].copy_from_slice(&(pcm.len() as u32).to_le_bytes());
            rom[o + 4..o + 6].copy_from_slice(&1u16.to_le_bytes());
            rom[o + 6] = div as u8;
            rom[o + 7] = div as u8;
            rom[o + 8..o + 8 + enc.len()].copy_from_slice(&enc);
            *dp += 8 + enc.len() + 5;
            o
        };
        let put_script = |rom: &mut Vec<u8>, sp: &mut usize, body: &[u8], len32: u32, mask: u8| {
            let o = *sp;
            let mut s = vec![5, mask, 1];
            s.extend_from_slice(&len32.to_le_bytes());
            s.extend_from_slice(body);
            rom[o..o + s.len()].copy_from_slice(&s);
            *sp += s.len() + 3;
            o
        };
        let mut words = vec![0u32; (nsamp + 1) * nlang];
        for s in 0..nsamp {
            let div = divs[s];
            let rate = BASE_RATE / div;
            let n = (rate as f64 * (0.3 + 0.1 * s as f64)) as usize;
            let pcm = tone(n, rate, 220.0 * (1 + s % 5) as f64, 12000.0);
            let o = put_stream(&mut rom, &mut dp, &pcm, div);
            originals.push((o, pcm));
            let mut body = vec![0x0a, 0x00];
            body.extend_from_slice(&ptr(o).to_le_bytes());
            body.extend_from_slice(&[0x10, 0x01, 0x0f, 0x04, 0, 0, 0, 0x00]);
            let len32 = (n as f64 * div as f64 / BASE_RATE as f64 * FIQ_HZ as f64).round() as u32;
            let so = put_script(&mut rom, &mut sp, &body, len32, 1 << (s % 3));
            let speech = s % 4 == 0;
            for lang in 0..nlang {
                let w = if speech && lang > 0 {
                    if lang == 4 {
                        0
                    } else {
                        // A localized line: the same audio, its own script.
                        ptr(put_script(&mut rom, &mut sp, &body, len32, 1))
                    }
                } else {
                    ptr(so)
                };
                words[s * nlang + lang] = w;
            }
        }
        // The song: three 24 kHz chunks, loop mark after the first one.
        let mut chunks = Vec::new();
        let mut body = vec![0x01, 0, 0, 0x02, 0, 3];
        for (i, f) in [330.0, 440.0, 550.0].iter().enumerate() {
            let pcm = tone(6000 + 1000 * i, BASE_RATE, *f, 9000.0);
            let o = put_stream(&mut rom, &mut dp, &pcm, 1);
            originals.push((o, pcm));
            chunks.push(o);
            if i == 1 {
                body.extend_from_slice(&[0x07, 0x00]);
            }
            body.push(0x0d);
            body.extend_from_slice(&((1u32 << 24) | (i as u32 + 1)).to_le_bytes());
            body.extend_from_slice(&[0x0a, 0x00]);
            body.extend_from_slice(&ptr(o).to_le_bytes());
            body.extend_from_slice(&[0x10, 0x01]);
        }
        body.extend_from_slice(&[0x03, 0x00]);
        let so = put_script(&mut rom, &mut sp, &body, 3000, 1);
        let song_sample = nsamp as u16;
        for lang in 0..nlang {
            words[nsamp * nlang + lang] = ptr(so);
        }
        for (i, w) in words.iter().enumerate() {
            rom[dir_off + 4 * i..dir_off + 4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
        // Call table at 0x10_0000: call 0 empty, then call c plays sample c (1..=12, the
        // song is 12; sample 0 cannot be listed, 0 ends a list), call 13 picks among
        // samples 1, 2, 3. Lists at 0x11_0000.
        let (tab, lists) = (0x10_0000usize, 0x11_0000usize);
        let mut lp = lists;
        let ncalls = nsamp + 2;
        for c in 0..ncalls {
            let list: Vec<u16> = match c {
                0 => vec![],
                c if c <= nsamp => vec![c as u16],
                _ => vec![1, 2, 3],
            };
            let r = tab + 20 * c;
            let state = 0x0211_10ac + 4 * c as u32;
            rom[r..r + 4].copy_from_slice(&state.to_le_bytes());
            rom[r + 4..r + 8].copy_from_slice(&(state + 2).to_le_bytes());
            rom[r + 8..r + 12].copy_from_slice(&(0x0400_0000 + lp as u32).to_le_bytes());
            rom[r + 12..r + 20].copy_from_slice(&[0, 0, 0, 0, 0xFF, 1, 0, 0]);
            for (j, v) in list.iter().chain(std::iter::once(&0)).enumerate() {
                rom[lp + 2 * j..lp + 2 * j + 2].copy_from_slice(&v.to_le_bytes());
            }
            lp += 2 * (list.len() + 1);
            lp = lp.next_multiple_of(4);
        }
        // Not a record: breaks the run on both sides.
        rom[tab - 20..tab].fill(0);
        rom[tab + 20 * ncalls..tab + 20 * ncalls + 20].fill(0);
        Synthetic {
            rom,
            originals,
            dir_off,
            song_chunks: chunks,
            song_sample,
        }
    }

    #[test]
    fn adpcm_round_trip_and_known_nibbles() {
        // Nibble 7 from reset: +7>>3 + 7>>2 + 7>>1 + 7 = 0+1+3+7 = 11.
        assert_eq!(ima_decode(&[0x07], 1), vec![11]);
        // Low nibble first: 0x70 is nibble 0 (+0) then 7.
        assert_eq!(ima_decode(&[0x70], 2), vec![0, 11]);
        let pcm = tone(4000, 24000, 440.0, 12000.0);
        let dec = ima_decode(&ima_encode(&pcm), pcm.len());
        assert_eq!(dec.len(), pcm.len());
        let err = (pcm
            .iter()
            .zip(&dec)
            .map(|(a, b)| ((*a as f64) - (*b as f64)).powi(2))
            .sum::<f64>()
            / pcm.len() as f64)
            .sqrt();
        assert!(err < 400.0, "{err}");
    }

    #[test]
    fn banked_pointers() {
        assert_eq!(banked(0x0e1f_6cb0, 15 * BANK), Some(14 * BANK + 0x1f_6cb0));
        assert_eq!(banked(0x0090_0000, 15 * BANK), None); // offset beyond a bank
        assert_eq!(banked(0x1000_0000, usize::MAX), None); // bank 16
        assert_eq!(banked(0x0100_0000, BANK), None); // past the image
    }

    #[test]
    fn synthetic_image_catalog() {
        let s = synthetic();
        let c = catalog(&s.rom).unwrap();
        assert_eq!(c.directory.start, s.dir_off);
        assert_eq!(c.nlang, 5);
        // 12 sound streams (the localized scripts play the same stream), one song.
        assert_eq!(c.sounds.len(), 12);
        for (snd, (o, pcm)) in c.sounds.iter().zip(&s.originals) {
            assert_eq!(snd.stream.off, *o);
            assert_eq!(snd.stream.count, pcm.len());
        }
        // Sample 0 is speech: its own script in languages 1-3, missing in 4.
        let speech = c.localized();
        assert!(speech.contains(&0) && speech.contains(&4) && !speech.contains(&1));
        assert_eq!(c.sounds[0].owners, vec![(0, 0), (0, 1), (0, 2), (0, 3)]);
        assert_eq!(c.music.len(), 1);
        let m = &c.music[0];
        assert_eq!(m.sample(), s.song_sample);
        assert_eq!(m.song(), Some(1));
        let offs: Vec<usize> = m.timeline.events.iter().map(|e| e.1.off).collect();
        assert_eq!(offs, s.song_chunks);
        // Loops from the second chunk, to the end.
        assert_eq!(m.timeline.loop_start, Some(6000));
        assert_eq!(m.timeline.loop_end, Some(6000 + 7000 + 8000));
        assert_eq!(m.role, "main");
        // Calls: call 0 is empty and still numbered 0.
        assert_eq!(c.calls.len(), 14);
        assert!(c.calls[0].samples.is_empty());
        assert_eq!(c.calls[1].samples, vec![1]);
        assert_eq!(c.calls[12].samples, vec![s.song_sample]);
        assert_eq!(c.calls[13].samples, vec![1, 2, 3]);
        assert_eq!(c.calls_of()[&2], vec![2, 13]);
    }

    #[test]
    fn song_render_is_gapless_and_declicked() {
        let s = synthetic();
        let c = catalog(&s.rom).unwrap();
        let m = &c.music[0];
        let raw = render(&s.rom, &m.timeline, false);
        let fixed = render(&s.rom, &m.timeline, true);
        assert_eq!(raw.pcm.len(), 21000);
        assert_eq!(fixed.pcm.len(), 21000);
        // Each chunk is its own decode, back to back.
        for (i, &o) in s.song_chunks.iter().enumerate() {
            let h = stream_header(&s.rom, o, true).unwrap();
            let at = [0, 6000, 13000][i];
            assert_eq!(&raw.pcm[at..at + h.count], &h.decode(&s.rom)[..]);
        }
        // Declicking only touches the ramp at each join (after the first chunk).
        for (at, chunk) in [(6000usize, 1usize), (13000, 2)] {
            let k = reset_ramp(
                &s.rom,
                &stream_header(&s.rom, s.song_chunks[chunk], true).unwrap(),
            );
            assert_eq!(fixed.pcm[at + k..at + 50], raw.pcm[at + k..at + 50]);
            let prev = fixed.pcm[at - 1] as i32;
            // The ramp is a line from the previous sample: no step above the target's.
            for i in 0..k {
                let v = fixed.pcm[at + i] as i32;
                let lo = prev.min(raw.pcm[at + k] as i32);
                let hi = prev.max(raw.pcm[at + k] as i32);
                assert!(v >= lo && v <= hi, "{at} {i}");
            }
        }
        assert_eq!(fixed.pcm[..6000], raw.pcm[..6000]);
    }

    #[test]
    fn declick_draws_a_line_over_the_ramp() {
        let mut pcm = vec![0i16, 10, 20, 1000, 1001, 1002];
        declick(&mut pcm, 3, 500);
        // From 500 to pcm[3] = 1000 in 4 steps: 625, 750, 875, then 1000 untouched.
        assert_eq!(pcm, vec![625, 750, 875, 1000, 1001, 1002]);
        let mut short = vec![1i16, 2];
        declick(&mut short, 3, 0);
        assert_eq!(short, vec![1, 2]);
        // Floor division, as the reference: from 0 down to -7 in 2 steps.
        let mut neg = vec![0i16, -7, -7];
        declick(&mut neg, 1, 0);
        assert_eq!(neg, vec![-4, -7, -7]);
        // The ramp: leading nibbles x7 / xF, low nibble first.
        let mut rom = vec![0u8; 24];
        rom[8..11].copy_from_slice(&[0xF7, 0x7F, 0x37]);
        let h = Stream {
            off: 0,
            count: 30,
            field4: 1,
            div: 1,
            end: 23,
        };
        assert_eq!(reset_ramp(&rom, &h), 5);
    }

    #[test]
    fn upsampling_matches_linear_floor_interpolation() {
        // A 12 kHz stream: each sample then the floor of the midpoint to the next.
        let mut rom = vec![0u8; 64];
        let pcm = vec![0i16, 100, -101, 7];
        let enc = ima_encode(&pcm);
        rom[0..4].copy_from_slice(&4u32.to_le_bytes());
        rom[4..6].copy_from_slice(&1u16.to_le_bytes());
        rom[6] = 2;
        rom[7] = 2;
        rom[8..8 + enc.len()].copy_from_slice(&enc);
        let h = stream_header(&rom, 0, true).unwrap();
        let d = h.decode(&rom);
        let up = stream_pcm24(&rom, &h);
        assert_eq!(up.len(), 8);
        for i in 0..4 {
            let x = d[i] as i32;
            let y = d.get(i + 1).copied().unwrap_or(d[i]) as i32;
            assert_eq!(up[2 * i] as i32, x);
            assert_eq!(up[2 * i + 1] as i32, x + (y - x).div_euclid(2));
        }
    }

    #[test]
    fn script_opcodes_and_roles() {
        // 05 hdr, 09 ramp (15 operands), FF pad, 08 marker 1, 07, 0a, 10, 03.
        let mut rom = vec![0u8; 0x400];
        let st = 0x200;
        rom[st..st + 4].copy_from_slice(&100u32.to_le_bytes());
        rom[st + 4] = 1;
        rom[st + 6] = 1;
        rom[st + 7] = 1;
        let mut s = vec![5u8, 1, 1, 10, 0, 0, 0, 0x09];
        s.extend_from_slice(&[0; 15]);
        s.push(0xFF);
        s.extend_from_slice(&[0x08, 1, 0, 0, 0, 0x07, 0x00, 0x0a, 0x00]);
        s.extend_from_slice(&(st as u32).to_le_bytes());
        s.extend_from_slice(&[0x10, 0x01, 0x03, 0x00, 0x0a]);
        rom[..s.len()].copy_from_slice(&s);
        let ops = parse_script(&rom, 0, 0x100);
        let codes: Vec<Option<u8>> = ops.iter().map(|o| o.op).collect();
        assert_eq!(
            codes,
            vec![
                Some(0x09),
                Some(0x08),
                Some(0x07),
                Some(0x0a),
                Some(0x10),
                Some(0x03)
            ]
        );
        assert!(is_sequenced(&ops));
        let tl = timeline(&rom, &ops);
        assert_eq!(tl.loop_start, Some(0));
        assert_eq!(tl.loop_end, Some(100));
        assert_eq!(music_role(&tl), "teaser");
        // An unknown opcode stops the parse.
        rom[7] = 0x0c;
        rom[8] = 0x11;
        let ops = parse_script(&rom, 0, 9);
        assert_eq!(ops.last().unwrap().op, Some(0x0c));
    }

    #[test]
    fn sam_sets_come_from_the_driver() {
        let (file, crc, len) = sam_set("acd_168h").unwrap();
        assert_eq!(file, "acd_168h.bin");
        assert_eq!(crc, 0x5a42_46a1);
        assert_eq!(len, 0x0722_3FA0);
        assert!(sam_set("afm_113b").is_none());
        assert!(SAM_SETS.len() > 100);
    }
}
