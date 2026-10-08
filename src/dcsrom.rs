//! Reads the track catalog of a DCS sound ROM, to know which commands are tracks, and
//! follows their programs: where they loop (`track_run`), and what they do to the board's
//! channels (`command_effects`: home channel, ducks, stops).
//!
//! Layout (from mjrgh's DCSExplorer, `DCSDecoder::FindCatalog` and `AddROM`): the catalog
//! sits in ROM U2 at $3000, $4000 or $6000. It starts with U2's own catalog entry: its size
//! in 4 KiB units, its chip select (0) and its checksum (0). At catalog + $40 is a 24-bit
//! offset (into U2) of the track index, and at catalog + $46 the 16-bit track count. The
//! index holds one 24-bit ROM address per track; a high byte of $FF marks an empty slot.
//!
//! A DCS command below the track count plays that track; `55 AA ..` and the other
//! `55 xx` specials are far above it.
//!
//! Pinball 2000 (DCS2, `SNDBRD_DCSP2K`) keeps the same catalog and the same track programs
//! in 16-bit words: its sound flash, U109 and U110 are word-wide chips (PinMAME's region:
//! the flash at 0, U109 at $400000, U110 at $800000, little-endian words), and each word
//! holds two bytes of the DCS layout, high byte first. `p2k_image` turns the region into
//! that byte stream; the catalog is then at $10000 (the flash's own entry: $100 * 4 KiB,
//! chip 0, checksum 0; then U109 and U110 with their chip selects 04 and 08 and their
//! checksums), and a ROM pointer is a plain 24-bit offset into the image.

use serde::Serialize;

/// Offsets where the catalog may start in U2.
const CATALOG_OFFSETS: [usize; 3] = [0x3000, 0x4000, 0x6000];
/// Where it starts in a Pinball 2000 image (`p2k_image`).
const P2K_CATALOG: usize = 0x10000;

/// A Pinball 2000 sound region as the byte stream the DCS layout reads: each 16-bit word
/// (little-endian in PinMAME's region) as its high byte, then its low byte.
pub fn p2k_image(region: &[u8]) -> Vec<u8> {
    region
        .chunks(2)
        .flat_map(|w| match *w {
            [lo, hi] => [hi, lo],
            [b] => [b, 0xFF],
            _ => unreachable!(),
        })
        .collect()
}

/// The catalog's offset, and whether the ROM is a Pinball 2000 image (plain 24-bit
/// pointers) rather than a WPC DCS region (chip select in the pointer's bits 21-23).
fn find_catalog(rom: &[u8]) -> Option<(usize, bool)> {
    if let Some(o) = CATALOG_OFFSETS.into_iter().find(|&o| catalog_at(rom, o)) {
        return Some((o, false));
    }
    p2k_catalog_at(rom).then_some((P2K_CATALOG, true))
}

/// A ROM pointer to an offset in the region (`rom_pointer`), or in a Pinball 2000 image.
fn pointer(p2k: bool, a: u32) -> usize {
    if p2k { a as usize } else { rom_pointer(a) }
}

/// The populated track numbers of a DCS ROM set, given U2's image (the start of PinMAME's
/// DCS sound region, where U2 is loaded at offset 0 in a slot of at most 1 MiB).
/// Returns `(track_count, populated)`, or None if no catalog is found.
pub fn tracks(u2: &[u8]) -> Option<(u16, Vec<u16>)> {
    let (catalog, _) = find_catalog(u2)?;
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
    /// A Pinball 2000 image: plain 24-bit pointers.
    p2k: bool,
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
    Mix {
        channel: u8,
        mode: u8,
        param: i8,
        frames: u16,
    },
}

impl<'a> Sim<'a> {
    fn new(rom: &'a [u8]) -> Result<Self, String> {
        let (catalog, p2k) = find_catalog(rom).ok_or("no DCS catalog")?;
        let r = |o| u24(rom, o).ok_or("catalog out of range");
        Ok(Self {
            rom,
            p2k,
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
        Ok(pointer(self.p2k, a))
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
            let p = pointer(self.p2k, a);
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
                    self.log(
                        c,
                        EventKind::Stream {
                            channel: target as u8,
                            loops,
                        },
                    );
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
                    self.log(
                        c,
                        EventKind::Stop {
                            channel: target as u8,
                        },
                    );
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
                    self.log(
                        c,
                        EventKind::Mix {
                            channel: target as u8,
                            mode: (op - 0x07) % 3,
                            param: self.u8(p - if fade { 3 } else { 1 })? as i8,
                            frames: steps,
                        },
                    );
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
                // Pinball 2000 (DCS2) only: a level and a fade of the program's own channel,
                // without a channel operand (`13 ll`, `14 ll nnnn`): the board channel is
                // the host's choice there. They change no timing, and are not modelled.
                0x13 if self.p2k => p += 1,
                0x14 if self.p2k => p += 3,
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
    let p = pointer(s.p2k, a);
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
            s.ch[t]
                .mixer
                .iter()
                .map(|m| m.level)
                .sum::<i32>()
                .clamp(-8191, 8191)
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

/// One DCS frame, in seconds (240 samples at 31250 Hz).
pub const FRAME_SECS: f64 = 240.0 / 31250.0;

/// The gain of one mixing level unit: a channel's level is `pow(0.9733, 127 - units)`.
pub const LEVEL_FACTOR: f64 = 0.9733;

/// How long a command is followed for `command_effects`: a music track loops forever, and
/// what it does to the other channels is all in its first seconds.
pub const EFFECTS_MAX_SECS: f64 = 60.0;

/// What a command does to the board, as stored in `manifest.json` (`dcs`): its home channel,
/// the streams it plays, what it does to the other channels.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommandEffects {
    /// 1: a track that runs at once; 2: it only sets the deferred track of `channel`.
    pub track_type: u8,
    /// The home channel. A type 1 track replaces the program on this channel and clears its
    /// stream: a new command on a channel cuts the previous one there, and nothing else.
    pub channel: u8,
    /// Channels it plays a stream on.
    pub streams: Vec<u8>,
    /// Channels whose sound it stops without playing anything there: opcode 02, or its home
    /// channel when it plays nothing there (0x03E3 on AFM only clears channel 0, the music).
    pub stops: Vec<u8>,
    /// Level changes on the channels it does not play on.
    pub ducks: Vec<Duck>,
    /// Type 2: the track it leaves for its channel, started by the music at a phrase
    /// boundary (opcode 05).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred: Option<String>,
    /// Tracks it starts itself (opcode 03, or 05 for a deferred one).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub queues: Vec<String>,
    /// Its own level on its home channel (opcode 07 on that channel), in level units.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub own_level: Option<i8>,
    /// Played alone, every channel was idle after this long (null: still running after
    /// `EFFECTS_MAX_SECS`, a loop).
    pub length_s: Option<f64>,
    /// The program could not be followed (then the other fields stop there).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A level change one command applies to a channel it does not play on (a duck when
/// negative). The board adds every program's contribution to a channel's level, and gives
/// it back when the program that set it ends.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Duck {
    pub channel: u8,
    /// The deepest (or highest) contribution reached, in level units (negative: lower) and
    /// in dB (`DB_PER_LEVEL` per unit).
    pub units: f64,
    pub db: f64,
    /// When it starts and when it reaches that depth, in seconds from the command.
    pub start_s: f64,
    pub full_s: f64,
    /// When the channel is back at 0 (null: still applied when the simulation stopped).
    pub end_s: Option<f64>,
    /// How it ends: "fade" (a mixing fade back, over `release_s`), "step" (a mixing
    /// opcode puts it back at once), "program end" (the contribution is dropped when the
    /// program ends), or "held".
    pub restore: &'static str,
    pub release_s: f64,
}

impl CommandEffects {
    /// The change applied to `channel`, if any.
    pub fn duck_on(&self, channel: u8) -> Option<&Duck> {
        self.ducks.iter().find(|d| d.channel == channel)
    }

    /// Plays a stream on its home channel 0: a music track.
    pub fn is_music(&self) -> bool {
        self.track_type == 1 && self.channel == 0 && self.streams.contains(&0)
    }
}

/// The volume factor (0..1) of a duck of `units` level units (negative: lower).
pub fn duck_factor(units: f64) -> f64 {
    LEVEL_FACTOR.powf(-units.min(0.0))
}

/// Follows `track` from a silent board for at most `max_secs` and sums up what it does.
/// None for an empty catalog slot or a track number out of the catalog.
pub fn command_effects(region: &[u8], track: u16, max_secs: f64) -> Option<CommandEffects> {
    let e = track_effects(region, track, (max_secs / FRAME_SECS) as u32)?;
    let streams_mask = e.stream_channels();
    let streams: Vec<u8> = (0..CHANNELS as u8)
        .filter(|c| streams_mask & (1 << c) != 0)
        .collect();
    let mut stops: Vec<u8> = e
        .events
        .iter()
        .filter_map(|ev| match ev.what {
            EventKind::Stop { channel } => Some(channel),
            _ => None,
        })
        .chain((e.kind == 1).then_some(e.channel))
        .filter(|c| streams_mask & (1 << c) == 0)
        .collect();
    stops.sort_unstable();
    stops.dedup();
    let home_of = |track: u16| {
        let s = Sim::new(region).ok()?;
        let a = u24(region, s.index + 3 * track as usize)?;
        (track < s.count && a & 0xFF_0000 != 0xFF_0000).then(|| pointer(s.p2k, a))
    };
    let deferred = (e.kind == 2)
        .then(|| home_of(track).and_then(|p| u16be(region, p + 2)))
        .flatten()
        .map(|t| format!("0x{t:04X}"));
    let mut queues: Vec<String> = e
        .events
        .iter()
        .filter_map(|ev| match ev.what {
            EventKind::Queue { track } => Some(format!("0x{track:04X}")),
            _ => None,
        })
        .collect();
    queues.dedup();
    let own_level = e.events.iter().find_map(|ev| match ev.what {
        EventKind::Mix {
            channel,
            mode: 0,
            param,
            ..
        } if channel == e.channel => Some(param),
        _ => None,
    });
    let mut ducks = Vec::new();
    for ch in 0..CHANNELS as u8 {
        if streams_mask & (1 << ch) != 0 || ch == e.channel {
            continue;
        }
        let lv: Vec<i32> = e.levels.iter().map(|l| l[ch as usize]).collect();
        let Some(first) = lv.iter().position(|&v| v != 0) else {
            continue;
        };
        // The deepest point (or the highest, for a raise), its first frame.
        let (peak_i, peak) =
            lv.iter()
                .copied()
                .enumerate()
                .fold((0usize, 0i32), |(bi, bv), (i, v)| {
                    if v.abs() > bv.abs() { (i, v) } else { (bi, bv) }
                });
        let end = lv[peak_i..]
            .iter()
            .position(|&v| v == 0)
            .map(|k| k + peak_i);
        let release_start = lv[peak_i..]
            .iter()
            .position(|&v| v != peak)
            .map(|k| k + peak_i);
        let (restore, release) = match (end, release_start) {
            (None, _) => ("held", 0),
            // The fade's first step is already in `levels[rs]`.
            (Some(end), Some(rs)) if end > rs => ("fade", end + 1 - rs),
            (Some(end), _) => {
                // Dropped at once: by a mixing opcode, or by the end of the program that set
                // it (which resets its contributions).
                let by_op = e.events.iter().any(|ev| {
                    ev.frame + 1 == end as u32
                        && matches!(ev.what, EventKind::Mix { channel, .. } if channel == ch)
                });
                (if by_op { "step" } else { "program end" }, 0)
            }
        };
        let units = f64::from(peak) / 64.0;
        // Level `levels[i]` is the state after frame i ran: a change made in frame i is
        // heard from i on, so frame indices are times from the command.
        ducks.push(Duck {
            channel: ch,
            units: round2(units),
            db: round2(units * DB_PER_LEVEL),
            start_s: round3(first as f64 * FRAME_SECS),
            full_s: round3(peak_i as f64 * FRAME_SECS),
            end_s: end.map(|x| round3(x as f64 * FRAME_SECS)),
            restore,
            release_s: round3(release as f64 * FRAME_SECS),
        });
    }
    Some(CommandEffects {
        track_type: e.kind,
        channel: e.channel,
        streams,
        stops,
        ducks,
        deferred,
        queues,
        own_level,
        length_s: e.ended.then(|| round3(e.frames as f64 * FRAME_SECS)),
        error: e.error,
    })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
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

/// The Pinball 2000 catalog: the sound flash's own entry ($100 * 4 KiB = 1 MiB, chip 0,
/// checksum 0), then U109's with chip select 04.
fn p2k_catalog_at(rom: &[u8]) -> bool {
    let o = P2K_CATALOG;
    u16be(rom, o) == Some(0x0100)
        && u16be(rom, o + 2) == Some(0)
        && u16be(rom, o + 4) == Some(0)
        && u16be(rom, o + 8).is_some_and(|c| c >> 8 == 0x04)
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
    fn a_callout_ducks_the_music_and_fades_it_back() {
        // "Jackpot!" (afm 0x01B6) in small: channel 3 lowers channel 0 by 10 units, plays
        // its stream, fades channel 0 back over 4 frames, ends.
        let t: &[u8] = &[
            1, 3, //
            0, 0, 0x09, 0, 10, //
            0, 0, 0x07, 3, 0x7D, //
            0, 0, 0x01, 3, 0x00, 0x70, 0x00, 1, //
            0, 5, 0x0B, 0, 10, 0, 4, //
            0, 4, 0x00,
        ];
        let rom = rom_with(&[t]);
        let e = command_effects(&rom, 0, 10.0).unwrap();
        assert_eq!((e.track_type, e.channel), (1, 3));
        assert_eq!(e.streams, vec![3]);
        assert!(e.stops.is_empty());
        assert_eq!(e.own_level, Some(0x7D));
        let d = e.duck_on(0).unwrap();
        assert_eq!(d.units, -10.0);
        assert!((d.db + 2.35).abs() < 0.01, "{d:?}");
        assert_eq!((d.start_s, d.full_s), (0.0, 0.0));
        assert_eq!(d.restore, "fade");
        assert!((d.release_s - 4.0 * FRAME_SECS).abs() < 0.001, "{d:?}");
        assert!(e.length_s.is_some());
    }

    #[test]
    fn a_duck_ends_with_its_program() {
        let t: &[u8] = &[
            1, 1, //
            0, 0, 0x09, 0, 20, //
            0, 0, 0x01, 1, 0x00, 0x70, 0x00, 1, //
            0, 10, 0x00,
        ];
        let rom = rom_with(&[t]);
        let d = command_effects(&rom, 0, 10.0).unwrap().ducks[0].clone();
        assert_eq!((d.channel, d.units, d.restore), (0, -20.0, "program end"));
        assert_eq!(d.release_s, 0.0);
        assert!(
            (d.end_s.unwrap() - 10.0 * FRAME_SECS).abs() < 0.001,
            "{d:?}"
        );
    }

    #[test]
    fn stops_and_deferred_tracks() {
        // 0: clears channel 0 (plays nothing there); 1: stops channels 1 and 2; 2: leaves
        // track 0x0003 for channel 0.
        let rom = rom_with(&[
            &[1, 0, 0, 0, 0x00],
            &[1, 0, 0, 0, 0x02, 1, 0, 0, 0x02, 2, 0, 0, 0x00],
            &[2, 0, 0x00, 0x03],
        ]);
        let e = command_effects(&rom, 0, 1.0).unwrap();
        assert!(e.streams.is_empty());
        assert_eq!(e.stops, vec![0]);
        assert!(!e.is_music());
        assert_eq!(command_effects(&rom, 1, 1.0).unwrap().stops, vec![0, 1, 2]);
        let e = command_effects(&rom, 2, 1.0).unwrap();
        assert_eq!((e.track_type, e.channel), (2, 0));
        assert_eq!(e.deferred.as_deref(), Some("0x0003"));
        assert!(e.stops.is_empty());
    }

    /// The Pinball 2000 layout: little-endian words, the catalog at $10000 of the byte
    /// image, plain 24-bit pointers (here into "U109", at $400000).
    #[test]
    fn p2k_catalog_and_pointers() {
        let mut img = vec![0xFFu8; 0x40_0100];
        let cat = 0x10000;
        img[cat..cat + 12].copy_from_slice(&[1, 0, 0, 0, 0, 0, 4, 0, 4, 0, 0x25, 0xDB]);
        img[cat + 0x40..cat + 0x43].copy_from_slice(&[0x01, 0x00, 0x48]);
        img[cat + 0x46..cat + 0x48].copy_from_slice(&2u16.to_be_bytes());
        // Track 0: FFFFFF (empty); track 1 at $011000.
        img[0x10048..0x1004E].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x01, 0x10, 0x00]);
        // Type 1, channel 1: 13 69 (own level), a 4-frame stream at $400000 played once,
        // then 14 69 0002 (a fade) a frame later and the end 3 frames after that.
        let prog = [
            1, 1, 0, 0, 0x13, 0x69, 0, 0, 0x01, 0, 0x40, 0x00, 0x00, 1, 0, 1, 0x14, 0x69, 0, 2, 0,
            3, 0,
        ];
        img[0x11000..0x11000 + prog.len()].copy_from_slice(&prog);
        img[0x40_0000..0x40_0002].copy_from_slice(&4u16.to_be_bytes());
        // As PinMAME's region holds it: each pair of bytes swapped.
        let region = p2k_image(&img);
        assert_eq!(p2k_image(&region), img);
        assert_eq!(tracks(&img), Some((2, vec![1])));
        // Frame 0: 13, the stream; frame 1: 14; frame 4: the end.
        assert_eq!(track_run(&img, 1, 100), TrackRun::Ends(5));
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
