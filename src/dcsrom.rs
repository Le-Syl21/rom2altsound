//! Reads the track catalog of a DCS sound ROM, to know which commands are tracks.
//!
//! Layout (from mjrgh's DCSExplorer, `DCSDecoder::FindCatalog` and `AddROM`): the catalog
//! sits in ROM U2 at $3000, $4000 or $6000. It starts with U2's own catalog entry: its size
//! in 4 KiB units, its chip select (0) and its checksum (0). At catalog + $40 is a 24-bit
//! offset (into U2) of the track index, and at catalog + $46 the 16-bit track count. The
//! index holds one 24-bit ROM address per track; a high byte of $FF marks an empty slot.
//!
//! A DCS command below the track count plays that track; `55 AA ..` and the other
//! `55 xx` specials are far above it.

/// Offsets where the catalog may start in U2.
const CATALOG_OFFSETS: [usize; 3] = [0x3000, 0x4000, 0x6000];

/// The populated track numbers of a DCS ROM set, given U2's image (the start of PinMAME's
/// DCS sound region, where U2 is loaded at offset 0 in a slot of at most 1 MiB).
/// Returns `(track_count, populated)`, or None if no catalog is found.
pub fn tracks(u2: &[u8]) -> Option<(u16, Vec<u16>)> {
    let catalog = CATALOG_OFFSETS.into_iter().find(|&o| catalog_at(u2, o))?;
    let index = u24(u2, catalog + 0x40)? as usize;
    let count = u16be(u2, catalog + 0x46)?;
    let populated = (0..count)
        .filter(|&t| {
            u24(u2, index + 3 * t as usize).is_some_and(|addr| addr & 0x00FF_0000 != 0x00FF_0000)
        })
        .collect();
    Some((count, populated))
}

/// Output samples per DCS frame at 44.1 kHz: 240 samples at 31250 Hz (7.68 ms).
pub const SAMPLES_PER_FRAME_44K1: f64 = 240.0 * 44100.0 / 31250.0;

/// Where a track's program repeats, in DCS frames from the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackLoop {
    /// Frames before the board's state first reaches the state it comes back to.
    pub intro_frames: u32,
    pub period_frames: u32,
}

/// What a track's program does over time, played alone from a silent board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrackRun {
    /// The state of every channel (program position and counters, loop stack, audio stream
    /// position, mixing levels and fades, pending commands) comes back to an earlier one.
    Loops(TrackLoop),
    /// Every channel is idle after this many frames.
    Ends(u32),
    /// The program could not be followed (unknown opcode, a feature not modelled), or did
    /// not repeat within the frames simulated.
    Unknown(String),
}

const CHANNELS: usize = 8;

#[derive(Clone, Default, Hash, PartialEq, Eq)]
struct Stream {
    start: usize,
    frames: u16,
    left: u16,
    loops: u8,
}

#[derive(Clone, Copy, Default, Hash, PartialEq, Eq)]
struct Mixer {
    level: i32,
    target: i32,
    steps: u16,
    delta: i32,
}

#[derive(Clone, Default, Hash, PartialEq, Eq)]
struct Channel {
    pc: Option<usize>,
    counter: u16,
    loop_stack: Vec<(u8, usize)>,
    stream: Option<Stream>,
    /// The channel that loaded the stream.
    source: Option<usize>,
    /// Mixing level contributed by each program channel.
    mixer: [Mixer; CHANNELS],
    /// Deferred track (type 2): its command, started by opcode 05.
    deferred: Option<u16>,
}

/// Follows a DCS track program the way the board's firmware runs it (from mjrgh's
/// DCSExplorer, `DCSDecoderNative::MainLoop`, `ExecTrack`, `UpdateMixingLevels` and the
/// stream frame counters), without decoding any audio: one step per 7.68 ms frame, until
/// the whole state repeats. `region` is PinMAME's DCS sound region (U2 at 0, each further
/// chip 1 MiB on); ROM pointers carry the chip in bits 21-23. Opcodes 04 and 06 are read with
/// their 1994+ operands (the 1993 software differs), so a 1993 ROM may be misread: callers
/// check the result against the audio.
pub fn track_run(region: &[u8], track: u16, max_frames: u32) -> TrackRun {
    match Sim::new(region).and_then(|mut s| s.run(track, max_frames)) {
        Ok(r) => r,
        Err(e) => TrackRun::Unknown(e),
    }
}

struct Sim<'a> {
    rom: &'a [u8],
    index: usize,
    count: u16,
    ch: [Channel; CHANNELS],
    queue: Vec<u16>,
    /// Frames run so far, and what the programs did (`track_effects`).
    frame: u32,
    events: Vec<Event>,
}

/// One program instruction that acts on the board, for `track_effects`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub frame: u32,
    /// The channel whose program ran it.
    pub by: u8,
    pub what: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// A type 1 track started on this channel (the command itself, or queued).
    Start { track: u16, channel: u8 },
    /// Opcode 01: a stream loaded on this channel, played `loops` times (0: forever).
    Stream { channel: u8, loops: u8 },
    /// Opcode 02: this channel stopped (program and stream).
    Stop { channel: u8 },
    /// Opcode 03 (queue a track) or 05 (start the deferred one).
    Queue { track: u16 },
    /// Opcodes 07-0C on a channel: `mode` 0 set, 1 increase, 2 decrease; `param` in level
    /// units (the INT8 operand); `frames` > 0 for a fade.
    Mix { channel: u8, mode: u8, param: i8, frames: u16 },
}

impl<'a> Sim<'a> {
    fn new(rom: &'a [u8]) -> Result<Self, String> {
        let catalog = CATALOG_OFFSETS
            .into_iter()
            .find(|&o| catalog_at(rom, o))
            .ok_or("no DCS catalog")?;
        let r = |o| u24(rom, o).ok_or("catalog out of range");
        Ok(Self {
            rom,
            index: r(catalog + 0x40)? as usize,
            count: u16be(rom, catalog + 0x46).ok_or("catalog out of range")?,
            ch: Default::default(),
            queue: Vec::new(),
            frame: 0,
            events: Vec::new(),
        })
    }

    fn u8(&self, o: usize) -> Result<u8, String> {
        self.rom
            .get(o)
            .copied()
            .ok_or_else(|| format!("read past the ROM at {o:#x}"))
    }
    fn u16(&self, o: usize) -> Result<u16, String> {
        Ok(u16::from(self.u8(o)?) << 8 | u16::from(self.u8(o + 1)?))
    }
    fn ptr(&self, o: usize) -> Result<usize, String> {
        let a = u32::from(self.u8(o)?) << 16 | u32::from(self.u16(o + 1)?);
        Ok(rom_pointer(a))
    }

    fn reset_mixing(&mut self, source: usize) {
        for c in &mut self.ch {
            c.mixer[source] = Mixer::default();
        }
    }

    /// Starts the queued commands (the firmware does it at the start of a frame).
    fn start_commands(&mut self) -> Result<(), String> {
        for cmd in std::mem::take(&mut self.queue) {
            if cmd >= self.count {
                continue;
            }
            let a = u32::from(self.u8(self.index + 3 * cmd as usize)?) << 16
                | u32::from(self.u16(self.index + 3 * cmd as usize + 1)?);
            if a & 0xFF_0000 == 0xFF_0000 {
                continue;
            }
            let p = rom_pointer(a);
            let (kind, c) = (self.u8(p)?, self.u8(p + 1)? as usize);
            if c >= CHANNELS {
                return Err(format!("track {cmd:#06x} on channel {c}"));
            }
            match kind {
                1 => {
                    self.events.push(Event {
                        frame: self.frame,
                        by: c as u8,
                        what: EventKind::Start {
                            track: cmd,
                            channel: c as u8,
                        },
                    });
                    let ch = &mut self.ch[c];
                    ch.pc = Some(p + 2);
                    // LoadTrack also clears the channel's stream.
                    ch.stream = None;
                    ch.counter = 0;
                    ch.loop_stack.clear();
                    self.reset_mixing(c);
                }
                2 => self.ch[c].deferred = Some(self.u16(p + 2)?),
                k => return Err(format!("track {cmd:#06x} has type {k}")),
            }
        }
        Ok(())
    }

    /// Runs channel `c`'s program until it waits or ends.
    fn exec(&mut self, c: usize) -> Result<(), String> {
        while let Some(mut p) = self.ch[c].pc {
            let prefix = self.u16(p)?;
            if prefix == 0xFFFF || self.ch[c].counter != prefix {
                return Ok(());
            }
            self.ch[c].counter = 0;
            let op = self.u8(p + 2)?;
            p += 3;
            match op {
                0x00 => {
                    let ch = &mut self.ch[c];
                    ch.pc = None;
                    ch.stream = None;
                    ch.loop_stack.clear();
                    self.reset_mixing(c);
                    return Ok(());
                }
                0x01 => {
                    let target = self.u8(p)? as usize;
                    let start = self.ptr(p + 1)?;
                    let loops = self.u8(p + 4)?;
                    p += 5;
                    let frames = self.u16(start)?;
                    if target >= CHANNELS {
                        return Err(format!("stream on channel {target}"));
                    }
                    self.log(c, EventKind::Stream {
                        channel: target as u8,
                        loops,
                    });
                    if frames != 0 {
                        let old = self.ch[target].source;
                        if let Some(old) = old.filter(|&o| o != c) {
                            self.ch[target].mixer[old] = Mixer::default();
                        }
                        let t = &mut self.ch[target];
                        t.stream = Some(Stream {
                            start,
                            frames,
                            left: frames,
                            loops,
                        });
                        t.source = Some(c);
                    }
                }
                0x02 => {
                    let target = self.u8(p)? as usize;
                    p += 1;
                    if target >= CHANNELS {
                        return Err(format!("stop on channel {target}"));
                    }
                    self.log(c, EventKind::Stop {
                        channel: target as u8,
                    });
                    if self.ch[target].stream.take().is_some() {
                        self.reset_mixing(target);
                    }
                    self.ch[target].pc = None;
                    if target == c {
                        return Ok(());
                    }
                }
                0x03 => {
                    let track = self.u16(p)?;
                    self.log(c, EventKind::Queue { track });
                    self.queue.push(track);
                    p += 2;
                }
                // Data port write to the game CPU (halted here).
                0x04 => p += 1,
                0x05 => {
                    let target = self.u8(p)? as usize;
                    p += 1;
                    if let Some(cmd) = self.ch.get_mut(target).and_then(|t| t.deferred.take()) {
                        self.log(c, EventKind::Queue { track: cmd });
                        self.queue.push(cmd);
                    }
                }
                // Track variables: only read by type 3 tracks, which are not modelled.
                0x06 => p += 2,
                0x07..=0x0C => {
                    let fade = op >= 0x0A;
                    let target = self.u8(p)? as usize;
                    let param = i32::from(self.u8(p + 1)? as i8) << 6;
                    let steps = if fade { self.u16(p + 2)? } else { 0 };
                    p += if fade { 4 } else { 2 };
                    self.log(c, EventKind::Mix {
                        channel: target as u8,
                        mode: (op - 0x07) % 3,
                        param: self.u8(p - if fade { 3 } else { 1 })? as i8,
                        frames: steps,
                    });
                    let m = self
                        .ch
                        .get_mut(target)
                        .map(|t| &mut t.mixer[c])
                        .ok_or_else(|| format!("mixing level on channel {target}"))?;
                    let new = match (op - 0x07) % 3 {
                        0 => param,
                        1 => m.level + param,
                        _ => m.level - param,
                    };
                    let delta = new - m.level;
                    m.target = new.clamp(-8191, 8191);
                    m.steps = steps;
                    if steps != 0 {
                        m.delta = delta / i32::from(steps);
                    } else {
                        m.level = m.target;
                    }
                }
                0x0D => {}
                0x0E => {
                    let n = self.u8(p)?;
                    p += 1;
                    self.ch[c].loop_stack.push((n, p));
                }
                0x0F => {
                    let stack = &mut self.ch[c].loop_stack;
                    match stack.last_mut() {
                        Some((0, pos)) => p = *pos,
                        Some((1, _)) => {
                            stack.pop();
                        }
                        Some((n, pos)) => {
                            *n -= 1;
                            p = *pos;
                        }
                        None => {}
                    }
                }
                op => return Err(format!("opcode {op:#04x} at {:#x}", p - 3)),
            }
            self.ch[c].pc = Some(p);
        }
        Ok(())
    }

    fn log(&mut self, by: usize, what: EventKind) {
        self.events.push(Event {
            frame: self.frame,
            by: by as u8,
            what,
        });
    }

    fn end_frame(&mut self) {
        self.frame += 1;
        for c in &mut self.ch {
            for m in &mut c.mixer {
                if m.steps == 1 {
                    m.steps = 0;
                    m.level = m.target;
                } else if m.steps > 1 {
                    m.steps -= 1;
                    m.level = (m.level + m.delta).clamp(-8191, 8191);
                }
            }
            c.counter = c.counter.wrapping_add(1);
            if let Some(s) = &mut c.stream {
                s.left -= 1;
                if s.left == 0 {
                    s.left = s.frames;
                    if s.loops != 0 {
                        s.loops -= 1;
                        if s.loops == 0 {
                            c.stream = None;
                            c.source = None;
                        }
                    }
                }
            }
        }
    }

    /// The state that decides what the board plays from here on. A waiting counter only
    /// matters while a program waits on a count; past a 0xFFFF wait it counts nothing.
    fn key(&self) -> Result<Vec<Channel>, String> {
        let mut out = self.ch.to_vec();
        for c in &mut out {
            let waits = match c.pc {
                Some(p) => self.u16(p)? != 0xFFFF,
                None => false,
            };
            if !waits {
                c.counter = 0;
            }
        }
        Ok(out)
    }

    fn run(&mut self, track: u16, max_frames: u32) -> Result<TrackRun, String> {
        self.queue.push(track);
        // States are kept as 64-bit hashes (a full state is about 1.5 KB, and a long track
        // runs for 10^5 frames); a collision is unlikely, and the result is checked against
        // the audio anyway.
        let mut seen: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
        for frame in 0..max_frames {
            self.start_commands()?;
            // Programs only start from the command queue, at the start of a frame (opcodes
            // 03 and 05 queue for the next one), so one pass over the channels does it.
            for c in 0..CHANNELS {
                self.exec(c)?;
            }
            self.end_frame();
            let idle = self.ch.iter().all(|c| c.pc.is_none() && c.stream.is_none());
            if idle && self.queue.is_empty() {
                return Ok(TrackRun::Ends(frame + 1));
            }
            let hash = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (self.key()?, &self.queue).hash(&mut h);
                h.finish()
            };
            if let Some(first) = seen.insert(hash, frame) {
                return Ok(TrackRun::Loops(TrackLoop {
                    intro_frames: first + 1,
                    period_frames: frame - first,
                }));
            }
        }
        Ok(TrackRun::Unknown(format!(
            "no repeat within {max_frames} frames"
        )))
    }
}

/// What one command does to the board, played alone from a silent board: what its programs
/// did (`events`) and, frame by frame, each channel's mixing level (the sum of every
/// program's contribution, in 1/64 level units: one unit of the INT8 operand is 64, about
/// -0.235 dB, `pow(0.9733, n)`).
#[derive(Clone, Debug)]
pub struct Effects {
    /// The command's type (1 track, 2 deferred) and channel, from its header.
    pub kind: u8,
    pub channel: u8,
    pub events: Vec<Event>,
    pub levels: Vec<[i32; CHANNELS]>,
    /// Frames run: until every channel was idle, or `max_frames`.
    pub frames: u32,
    pub ended: bool,
    pub error: Option<String>,
}

/// dB per level unit of the mixing operands: -20 log10(0.9733) (a level is
/// `pow(0.9733, 127 - units)`).
pub const DB_PER_LEVEL: f64 = 0.235_2;

impl Effects {
    /// Channels this command loads a stream on (it plays there).
    pub fn stream_channels(&self) -> u8 {
        self.events.iter().fold(0, |m, e| match e.what {
            EventKind::Stream { channel, .. } => m | 1 << channel,
            _ => m,
        })
    }
}

/// Runs `track` from a silent board for at most `max_frames` and records what it does.
pub fn track_effects(region: &[u8], track: u16, max_frames: u32) -> Option<Effects> {
    let mut s = Sim::new(region).ok()?;
    let a = u24(region, s.index + 3 * track as usize)?;
    if track >= s.count || a & 0xFF_0000 == 0xFF_0000 {
        return None;
    }
    let p = rom_pointer(a);
    let (kind, channel) = (*region.get(p)?, *region.get(p + 1)?);
    let mut e = Effects {
        kind,
        channel,
        events: Vec::new(),
        levels: Vec::new(),
        frames: 0,
        ended: false,
        error: None,
    };
    s.queue.push(track);
    let step = |s: &mut Sim| -> Result<bool, String> {
        s.start_commands()?;
        for c in 0..CHANNELS {
            s.exec(c)?;
        }
        s.end_frame();
        Ok(s.ch.iter().all(|c| c.pc.is_none() && c.stream.is_none()) && s.queue.is_empty())
    };
    for _ in 0..max_frames {
        let r = step(&mut s);
        e.levels.push(std::array::from_fn(|t| {
            s.ch[t].mixer.iter().map(|m| m.level).sum::<i32>().clamp(-8191, 8191)
        }));
        e.frames += 1;
        match r {
            Ok(true) => {
                e.ended = true;
                break;
            }
            Ok(false) => {}
            Err(err) => {
                e.error = Some(err);
                break;
            }
        }
    }
    e.events = s.events;
    Some(e)
}

/// A 24-bit ROM pointer to an offset in PinMAME's DCS region: chip select in bits 21-23
/// (U2 = 0), offset in the low 20 bits (1 MiB chips; a 512 KiB chip is loaded twice).
fn rom_pointer(a: u32) -> usize {
    ((a >> 21) & 7) as usize * 0x10_0000 + (a & 0xF_FFFF) as usize
}

fn u16be(rom: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*rom.get(o)?, *rom.get(o + 1)?]))
}

fn u24(rom: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        0,
        *rom.get(o)?,
        *rom.get(o + 1)?,
        *rom.get(o + 2)?,
    ]))
}

fn catalog_at(rom: &[u8], o: usize) -> bool {
    let size = u16be(rom, o).map_or(0, |s| s as usize * 4096);
    let chip_sel = u16be(rom, o + 2).map_or(1, |c| c >> 8);
    let cksum = u16be(rom, o + 4).unwrap_or(1);
    // U2 is a 512 KiB or 1 MiB chip; the region slot may be larger than the chip.
    chip_sel == 0 && cksum == 0 && (0x80000..=0x100000).contains(&size) && size <= rom.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_catalog_and_skips_empty_slots() {
        let mut u2 = vec![0u8; 0x80000];
        let cat = 0x4000;
        u2[cat..cat + 2].copy_from_slice(&0x0080u16.to_be_bytes()); // 128 * 4 KiB
        u2[cat + 0x40..cat + 0x43].copy_from_slice(&[0x00, 0x50, 0x00]); // index at $5000
        u2[cat + 0x46..cat + 0x48].copy_from_slice(&3u16.to_be_bytes());
        u2[0x5000..0x5009].copy_from_slice(&[0x01, 0x23, 0x45, 0xFF, 0xFF, 0xFF, 0x20, 0, 0]);
        assert_eq!(tracks(&u2), Some((3, vec![0, 2])));
    }

    /// A ROM with a catalog at $4000 and track programs at $5100 + 0x100 * n.
    fn rom_with(tracks: &[&[u8]]) -> Vec<u8> {
        let mut rom = vec![0u8; 0x80000];
        let cat = 0x4000;
        rom[cat..cat + 2].copy_from_slice(&0x0080u16.to_be_bytes());
        rom[cat + 0x40..cat + 0x43].copy_from_slice(&[0x00, 0x50, 0x00]);
        rom[cat + 0x46..cat + 0x48].copy_from_slice(&(tracks.len() as u16).to_be_bytes());
        for (n, t) in tracks.iter().enumerate() {
            let at = 0x5100 + 0x100 * n;
            rom[0x5000 + 3 * n..0x5003 + 3 * n].copy_from_slice(&(at as u32).to_be_bytes()[1..]);
            rom[at..at + t.len()].copy_from_slice(t);
        }
        // Streams: 10 frames at $7000, 4 frames at $7100.
        rom[0x7000..0x7002].copy_from_slice(&10u16.to_be_bytes());
        rom[0x7100..0x7102].copy_from_slice(&4u16.to_be_bytes());
        rom
    }

    #[test]
    fn program_loop_period_is_the_sum_of_its_waits() {
        // type 1, channel 0: set level; loop forever { stream $7000 once; wait 10: stream
        // $7100 once; wait 4: jump back }.
        let t: &[u8] = &[
            1, 0, //
            0, 0, 0x07, 0, 0x64, //
            0, 0, 0x0E, 0, //
            0, 0, 0x01, 0, 0x00, 0x70, 0x00, 1, //
            0, 10, 0x01, 0, 0x00, 0x71, 0x00, 1, //
            0, 4, 0x0F,
        ];
        let rom = rom_with(&[&[1, 0, 0xFF, 0xFF], t]);
        assert_eq!(
            track_run(&rom, 1, 1000),
            TrackRun::Loops(TrackLoop {
                intro_frames: 1,
                period_frames: 14
            })
        );
    }

    #[test]
    fn a_fade_inside_the_loop_extends_the_intro() {
        // loop forever { decrease the level by 64 * 40; wait 5 }: the level reaches its
        // floor (-8191) after a few passes; only then does the state repeat.
        let t: &[u8] = &[
            1, 0, //
            0, 0, 0x01, 0, 0x00, 0x70, 0x00, 0, // stream forever (10 frames)
            0, 0, 0x0E, 0, //
            0, 0, 0x09, 0, 40, //
            0, 5, 0x0F,
        ];
        let rom = rom_with(&[t]);
        let TrackRun::Loops(l) = track_run(&rom, 0, 1000) else {
            panic!("no loop")
        };
        // lcm(5, 10) = 10; the floor is reached on the 4th pass (4 * 2560 > 8191).
        assert_eq!(l.period_frames, 10);
        assert!(l.intro_frames >= 15, "{l:?}");
    }

    #[test]
    fn a_track_that_stops_ends() {
        let t: &[u8] = &[1, 0, 0, 0, 0x01, 0, 0x00, 0x71, 0x00, 1, 0, 2, 0x00];
        let rom = rom_with(&[t]);
        assert_eq!(track_run(&rom, 0, 1000), TrackRun::Ends(3));
    }

    #[test]
    fn rom_pointers() {
        assert_eq!(rom_pointer(0x238ec4), 0x138ec4);
        assert_eq!(rom_pointer(0x4537c0), 0x2537c0);
        assert_eq!(rom_pointer(0x0a013e), 0x0a013e);
    }

    #[test]
    fn no_catalog() {
        assert_eq!(tracks(&[0xFFu8; 0x8000]), None);
    }
}
