//! Music loops found in the sound CPU's state (`seqstate`) instead of the audio.
//!
//! On every board but DCS the music is a program on the sound CPU: a sequencer that, at
//! each tick of a timer interrupt, walks its score and drives the sound chips. When the
//! score loops, the sequencer's state comes back, but the audio does not repeat
//! sample-exactly: the ticks are not locked to the chips' sample clocks, so each note of
//! the next cycle starts a fraction of a millisecond off (Twilight Zone `02`: the same
//! notes 55 to 91 samples later than one 47.2 s cycle before, -15 to -19 dB of residual),
//! and the FM oscillators are at other phases. The audio method (`looping`) then never
//! finds an exact cycle, and this one finds the musical one.
//!
//! The state is read at the end of every emulated frame. What carries the music's
//! position is what is left once these are taken out (`mask`):
//! - the registers and the stack (where the CPU happened to be when the frame ended);
//! - free-running counters, bytes that only ever count up (or down): they never come back;
//! - bytes that change in most frames (a tick countdown, a phase): at a frame boundary
//!   their value depends on where the frame fell between two ticks.
//!
//! A period holds when, from some frame on, nearly every remaining byte of frame `k`
//! equals the same byte one period later, give or take a frame (the period is rarely a
//! whole number of frames, and a tick falls on either side of a frame boundary). The audio
//! is then cut one period apart at the point where the two cycles differ least.

use crate::looping;

/// Where each snapshot byte comes from: (cpu, address), registers as `u32::MAX - reg`.
pub type Origin = [(i32, u32)];

/// A byte that changes in at least this share of the frames moves with the sequencer's
/// tick inside the frame: at a frame boundary its value depends on where the frame fell
/// between two ticks, not on where the music is.
const FAST_SHARE: f64 = 0.5;
/// Bytes this close to the stack pointer's range are left out: an interrupt pushes the
/// interrupted registers there.
const STACK_MARGIN: u32 = 16;
/// A state period holds when at least this share of the kept bytes, over every frame after
/// the start, equal the same byte one period later (+/- a frame). Measured on Twilight Zone
/// `02` (47.2 s): 0.9992 at the period, 0.979 at most at any other lag; the bytes that miss
/// are values held for a single frame (a working variable caught mid-update).
pub const BYTE_SHARE: f64 = 0.998;
/// A byte one period later may be this many snapshots early or late (with 4, the periods
/// came out a frame off and their cuts no longer matched the audio).
const FRAME_TOL: usize = 1;
/// Candidate periods come from this many interleaved groups of bytes.
const HASH_GROUPS: usize = 8;
/// The tail is first checked on every this many snapshots.
const TAIL_STEP: usize = 4;
/// At most this many bytes, and 5% of the kept ones, may keep a clock of their own
/// (whirl_l3 `0121`: 2 of 296; with 4, a bar of Twilight Zone `02` passed for its loop).
const OWN_CLOCK_MAX: usize = 2;
/// At most this many state periods are tried on the audio.
const MAX_STATE_LOOPS: usize = 6;
/// Audio cut: the window over which the two cycles are compared, and how far the best lag
/// is looked for around the state's period, beyond one frame.
const SEAM_WIN: usize = 512;
const SEAM_HOP: usize = 256;
const LAG_MARGIN: usize = 48;
/// At the cut, the lag is refined this far around its half second's.
const LAG_FINE: usize = 8;
/// The cut is looked for in this much of the cycle first, and taken there when the two
/// cycles differ by at most `SEAM_GOOD_DB` in its window.
const SEAM_SEARCH_SECS: usize = 10;
const SEAM_GOOD_DB: f64 = -30.0;

/// The sound CPU state of one recording, frame by frame, kept as the bytes that changed.
#[derive(Default)]
pub struct StateLog {
    /// The first snapshot, and the current one.
    first: Vec<u8>,
    last: Vec<u8>,
    /// Recording frame (sample index) of each snapshot.
    pub at: Vec<u64>,
    /// Per snapshot after the first: (byte, new value).
    changes: Vec<Vec<(u32, u8)>>,
    varying: Vec<bool>,
}

impl StateLog {
    /// Adds a snapshot taken at recording frame `at`.
    pub fn push(&mut self, snap: &[u8], at: u64) {
        if self.at.is_empty() {
            self.first = snap.to_vec();
            self.last = snap.to_vec();
            self.varying = vec![false; snap.len()];
        } else {
            let mut ch = Vec::new();
            for (i, (l, &s)) in self.last.iter_mut().zip(snap).enumerate() {
                if *l != s {
                    *l = s;
                    self.varying[i] = true;
                    ch.push((i as u32, s));
                }
            }
            self.changes.push(ch);
        }
        self.at.push(at);
    }

    /// The bytes that changed at least once, frame by frame.
    pub fn dense(&self) -> Snapshots {
        let cols: Vec<usize> = (0..self.varying.len())
            .filter(|&i| self.varying[i])
            .collect();
        let mut index = vec![usize::MAX; self.varying.len()];
        for (j, &c) in cols.iter().enumerate() {
            index[c] = j;
        }
        let mut row: Vec<u8> = cols.iter().map(|&c| self.first[c]).collect();
        let mut data = Vec::with_capacity(self.at.len() * cols.len());
        data.extend_from_slice(&row);
        for ch in &self.changes {
            for &(i, v) in ch {
                row[index[i as usize]] = v;
            }
            data.extend_from_slice(&row);
        }
        Snapshots {
            len: cols.len(),
            at: self.at.clone(),
            data,
            cols,
            first: self.first.clone(),
        }
    }
}

/// Snapshots, one row per frame.
pub struct Snapshots {
    pub len: usize,
    /// Recording frame (sample index) of each snapshot.
    pub at: Vec<u64>,
    pub data: Vec<u8>,
    /// The snapshot byte of each column.
    pub cols: Vec<usize>,
    /// The first snapshot, whole (the value of the bytes that never changed).
    pub first: Vec<u8>,
}

impl Snapshots {
    pub fn frame(&self, k: usize) -> &[u8] {
        &self.data[k * self.len..(k + 1) * self.len]
    }
}

/// How the state was masked.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct MaskReport {
    /// Bytes that changed during the recording, and those left out by kind.
    pub varying: usize,
    pub registers: usize,
    pub stack: usize,
    pub counters: usize,
    pub fast: usize,
    pub kept: usize,
}

/// The columns that carry the music's position, from snapshot `from` on (see the module
/// documentation).
pub fn mask(s: &Snapshots, origin: &Origin, from: usize) -> (Vec<usize>, MaskReport) {
    let n = s.at.len();
    let mut report = MaskReport::default();
    // The stack pointer's range per CPU (register 2 on the 6800 and 6809 families, two
    // little-endian bytes; a byte that never changed is read in the first snapshot).
    let mut stack: Vec<(i32, u32, u32)> = Vec::new();
    for (c, &(cpu, addr)) in origin.iter().enumerate() {
        if addr == u32::MAX - 2 && origin.get(c + 1) == Some(&(cpu, addr)) {
            let col = |b: usize| s.cols.iter().position(|&x| x == b);
            let (lo_col, hi_col) = (col(c), col(c + 1));
            let byte =
                |k: usize, b: usize, j: Option<usize>| j.map_or(s.first[b], |j| s.frame(k)[j]);
            let value =
                |k: usize| u32::from(byte(k, c, lo_col)) | u32::from(byte(k, c + 1, hi_col)) << 8;
            let (lo, hi) = (from..n)
                .map(value)
                .fold((u32::MAX, 0), |(a, b), v| (a.min(v), b.max(v)));
            if lo <= hi {
                stack.push((cpu, lo.saturating_sub(STACK_MARGIN), hi + STACK_MARGIN));
            }
        }
    }
    let frames = n.saturating_sub(from + 1).max(1);
    let mut keep = Vec::new();
    for (j, &c) in s.cols.iter().enumerate() {
        let (cpu, addr) = origin[c];
        let mut changes = 0usize;
        let (mut up, mut down) = (true, true);
        for k in from + 1..n {
            let (p, v) = (s.frame(k - 1)[j], s.frame(k)[j]);
            if p != v {
                changes += 1;
                up &= (1..=64).contains(&v.wrapping_sub(p));
                down &= (1..=64).contains(&p.wrapping_sub(v));
            }
        }
        if changes == 0 {
            continue;
        }
        report.varying += 1;
        if addr > u32::MAX - 64 {
            report.registers += 1;
        } else if stack
            .iter()
            .any(|&(sc, lo, hi)| sc == cpu && (lo..=hi).contains(&addr))
        {
            report.stack += 1;
        } else if (up || down) && changes >= 2 {
            report.counters += 1;
        } else if changes as f64 / frames as f64 >= FAST_SHARE {
            report.fast += 1;
        } else {
            keep.push(j);
        }
    }
    report.kept = keep.len();
    (keep, report)
}

/// A loop of the masked state.
#[derive(Clone, Debug)]
pub struct StateLoop {
    /// The first snapshot from which the state repeats, and the period, in snapshots.
    pub start: usize,
    pub period: usize,
    /// Share of the kept bytes, over the snapshots from `start`, equal one period later.
    pub byte_share: f64,
    /// Snapshots compared.
    pub matched: usize,
    /// Bytes left out because they keep a clock of their own (they miss in most frames).
    pub own_clock: usize,
    /// Snapshots whose state is exactly the one a period later (no frame off): of two
    /// neighbouring periods, the closer to the true one has more.
    pub exact: usize,
}

/// Whether byte `j` of snapshot `k` is not found one period later (see `misses`).
fn byte_misses(s: &Snapshots, j: usize, k: usize, p: usize) -> bool {
    let a = s.frame(k)[j];
    (p - FRAME_TOL..=p + FRAME_TOL).all(|q| a != s.frame(k + q)[j])
        && (k == 0 || a == s.frame(k - 1)[j] || a == s.frame(k + 1)[j])
}

/// Bytes of snapshot `k` that are not found one period later, give or take a snapshot. A
/// value held for a single snapshot is a working variable caught mid-update (apollo13 `06`:
/// `13 13 0E 13 13`, about one snapshot in 15 on some bytes) and is not compared.
fn misses(s: &Snapshots, keep: &[usize], k: usize, p: usize) -> usize {
    let t = FRAME_TOL;
    let a = s.frame(k);
    let later: Vec<&[u8]> = (p - t..=p + t).map(|q| s.frame(k + q)).collect();
    let (before, after) = (s.frame(k.saturating_sub(1)), s.frame(k + 1));
    keep.iter()
        .filter(|&&j| later.iter().all(|b| a[j] != b[j]))
        .filter(|&&j| k == 0 || a[j] == before[j] || a[j] == after[j])
        .count()
}

/// The periods (at least `min_period` snapshots) at which the masked state holds from some
/// snapshot to the end, over at least one period and `confirm` snapshots after it (two
/// cycles in the recording); of two neighbours, the better one. Shortest first, those that
/// hold on every byte before those that leave a few out (`own_clock`).
pub fn find(
    s: &Snapshots,
    keep: &[usize],
    from: usize,
    min_period: usize,
    confirm: usize,
) -> Vec<StateLoop> {
    use std::hash::{Hash, Hasher};
    let n = s.at.len();
    let min_period = min_period.max(2);
    if keep.is_empty() || n < from + 2 * min_period + 2 {
        return Vec::new();
    }
    // Candidates: how far back the state of a group of bytes was last entered, per group
    // (so that a byte with a clock of its own only hides the period from its group), with
    // their neighbours.
    let mut periods: Vec<usize> = Vec::new();
    let groups = HASH_GROUPS.min(keep.len());
    for g in 0..groups {
        let mut last: std::collections::HashMap<u64, usize> = Default::default();
        let mut seen: std::collections::BTreeMap<usize, usize> = Default::default();
        let mut prev = None;
        for k in from..n {
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            let f = s.frame(k);
            for &j in keep.iter().skip(g).step_by(groups) {
                f[j].hash(&mut hs);
            }
            let hk = hs.finish();
            if prev == Some(hk) {
                continue;
            }
            prev = Some(hk);
            if let Some(&p) = last.get(&hk)
                && k - p >= min_period
            {
                *seen.entry(k - p).or_default() += 1;
            }
            last.insert(hk, k);
        }
        periods.extend(
            seen.iter()
                .filter(|&(_, &c)| c >= 2)
                .flat_map(|(&p, _)| [p - 1, p, p + 1])
                .filter(|&p| p >= min_period),
        );
    }
    periods.sort_unstable();
    periods.dedup();
    let verify = |p: usize| -> Option<StateLoop> {
        let end = n.checked_sub(p + FRAME_TOL)?;
        if p <= FRAME_TOL {
            return None;
        }
        if end <= from {
            return None;
        }
        // On a sample of the tail: the bytes that miss most of the time follow a clock of
        // their own (whirl_l3 `0121`: two bytes counting down from 14 and 09 over several
        // cycles); a few of them are left out, then the tail must hold.
        let tail = p.max(confirm).min(end - from);
        let sample: Vec<usize> = (end - tail..end).step_by(TAIL_STEP).collect();
        let mut by_byte = vec![0usize; keep.len()];
        for &k in &sample {
            for (i, &j) in keep.iter().enumerate() {
                by_byte[i] += usize::from(byte_misses(s, j, k, p));
            }
        }
        let own_clock: Vec<usize> = (0..keep.len())
            .filter(|&i| by_byte[i] * 2 > sample.len())
            .collect();
        if own_clock.len() > OWN_CLOCK_MAX || own_clock.len() * 20 > keep.len() {
            return None;
        }
        let keep: Vec<usize> = (0..keep.len())
            .filter(|i| !own_clock.contains(i))
            .map(|i| keep[i])
            .collect();
        let sampled: usize = (0..by_byte.len())
            .filter(|i| !own_clock.contains(i))
            .map(|i| by_byte[i])
            .sum();
        if 1.0 - (sampled as f64 / (sample.len() * keep.len()).max(1) as f64) < BYTE_SHARE {
            return None;
        }
        let keep = &keep[..];
        let miss: Vec<usize> = (from..end).map(|k| misses(s, keep, k, p)).collect();
        // Walk back from the end: the earliest start whose share holds, on a frame that
        // matches fully.
        let mut best = None;
        let mut missed = 0usize;
        for (i, &m) in miss.iter().enumerate().rev() {
            missed += m;
            let len = miss.len() - i;
            let share = 1.0 - missed as f64 / (len * keep.len()) as f64;
            if m == 0 && share >= BYTE_SHARE && len >= p.max(confirm) {
                best = Some(StateLoop {
                    start: from + i,
                    period: p,
                    byte_share: share,
                    matched: len,
                    own_clock: own_clock.len(),
                    exact: 0,
                });
            }
        }
        best.map(|mut l: StateLoop| {
            l.exact = (l.start..end)
                .filter(|&k| keep.iter().all(|&j| s.frame(k)[j] == s.frame(k + p)[j]))
                .count();
            l
        })
    };
    let mut found: Vec<StateLoop> = Vec::new();
    for p in periods {
        let Some(l) = verify(p) else { continue };
        if let Some(f) = found.last_mut()
            && p <= f.period + 1
        {
            if (l.byte_share, l.exact) > (f.byte_share, f.exact) {
                *f = l;
            }
        } else {
            found.push(l);
        }
    }
    // The whole state first: leaving bytes out can turn a bar that repeats inside a longer
    // loop into a period (Twilight Zone `02`: 1.88 s with four bytes left out, 47.2 s with
    // none), and the audio of a bar is as close to the next bar as to the next cycle.
    found.sort_by_key(|l| l.own_clock > 0);
    found.truncate(MAX_STATE_LOOPS);
    found
}

/// The audio cut of a state loop: one cycle of `x` (mono, from the recording's first
/// sound), `lag` samples long, starting at `intro`.
#[derive(Clone, Debug)]
pub struct Cut {
    pub intro: usize,
    pub lag: usize,
    /// Residual of the two cycles against the signal (dB) in the window at the cut, and the
    /// median over the windows of the whole cycle (each at its own best lag).
    pub seam_residual_db: f64,
    pub cycle_residual_db: f64,
    /// Share of the half-second windows (with signal), over a cycle and at least the
    /// confirmation span, whose best lag is within
    /// `LAG_AGREE` samples of the median: the audio follows the state's period.
    pub lag_agreement: f64,
}

impl Cut {
    /// The audio repeats, musically, at the state's period: the same notes come back one
    /// cycle later, at a steady lag, even if not sample-exactly. On Twilight Zone the lags
    /// agree within 36 to 72 samples and the cycles differ by -8 to -17 dB; on the BSMT
    /// boards (xfiles `04`, apollo13 `06`), whose music lies in the BSMT2000's sample
    /// streams rather than in the 6809's RAM, the state repeated while the audio did not
    /// (residual about 0 dB, lags all over the place), and that is no loop.
    pub fn confirmed(&self) -> bool {
        self.lag_agreement >= LAG_AGREE_SHARE && self.cycle_residual_db <= CYCLE_MAX_DB
    }
}

/// The audio confirmation of a state loop (see `Cut::confirmed`).
const LAG_AGREE: usize = 100;
const LAG_AGREE_SHARE: f64 = 0.75;
const CYCLE_MAX_DB: f64 = -3.0;

/// Where to cut one cycle: in the cycle that follows `from` (samples), the window where the
/// audio and the audio one cycle later (`lag0` samples, give or take `spread`) differ least,
/// preferably at a quiet point.
pub fn cut(
    x: &[f32],
    rate: u32,
    from: usize,
    lag0: usize,
    spread: usize,
    confirm: usize,
) -> Option<Cut> {
    let radius = spread + LAG_MARGIN;
    let lo = lag0.checked_sub(radius)?;
    let hi = lag0 + radius;
    if x.len() < from + hi + lag0 + SEAM_WIN {
        return None;
    }
    let ssd = |a: usize, l: usize, w: usize, step: usize| -> f64 {
        (0..w)
            .step_by(step)
            .map(|i| ((x[a + i] - x[a + i + l]) as f64).powi(2))
            .sum::<f64>()
            / w.div_ceil(step) as f64
    };
    let energy = |a: usize, w: usize| {
        x[a..a + w].iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / w as f64
    };
    // The lag over the cycle, and over `confirm` samples for a short one (a bar that
    // repeats inside a longer loop must not pass): per half second, the best one, then the
    // median.
    let half = rate as usize / 2;
    let span = lag0.max(confirm).min(x.len() - hi - from);
    let mut lags = Vec::new();
    let mut cycle = Vec::new();
    // Per half second: its best lag when it carries signal.
    let mut local: Vec<Option<usize>> = Vec::new();
    let mut a = from;
    while a + half <= from + span {
        let (e, l) = (lo..=hi)
            .map(|l| (ssd(a, l, half, 8), l))
            .min_by(|p, q| p.0.total_cmp(&q.0))?;
        let sig = energy(a, half);
        if sig > 100.0 {
            lags.push(l);
            cycle.push(10.0 * (e / sig).log10());
        }
        local.push((sig > 100.0).then_some(l));
        a += half;
    }
    let mut sorted = lags.clone();
    sorted.sort_unstable();
    let mid = *sorted.get(sorted.len() / 2)?;
    let lag_agreement = lags
        .iter()
        .filter(|&&l| l.abs_diff(mid) <= LAG_AGREE)
        .count() as f64
        / lags.len() as f64;
    cycle.sort_by(f64::total_cmp);
    let cycle_residual_db = cycle.get(cycle.len() / 2).copied().unwrap_or(0.0);
    // The cut: the window with the least squared difference (absolute, so that a quiet
    // point wins) at the lag of its half second (+/- `LAG_FINE`: a lag fitted on a short
    // window can slip by a period of a held tone, and a silent one fits any lag), that
    // half second's lag agreeing with the median; then the sample in it where the two
    // cycles are closest.
    let lag_at = |a: usize| {
        local
            .get((a - from) / half)
            .copied()
            .flatten()
            .filter(|l| l.abs_diff(mid) <= LAG_AGREE)
    };
    // A window's level, floored at -20 dB under its half second's (a quiet window is
    // judged against the music around it, not against its own near silence).
    let around = |a: usize| {
        let h = from + (a - from) / half * half;
        energy(h, half.min(x.len() - h))
    };
    let level = |a: usize| energy(a, SEAM_WIN).max(0.01 * around(a));
    // Searched in the first `SEAM_SEARCH_SECS` of the cycle (a short intro), then over the
    // whole cycle when no window there is good enough.
    let search = |until: usize| {
        let mut best = (f64::MAX, from, mid);
        let mut a = from + 1;
        while a + SEAM_WIN <= until {
            let Some(l0) = lag_at(a) else {
                a += SEAM_HOP;
                continue;
            };
            // A near-silent window fits any lag: it keeps its half second's.
            let fine = if energy(a, SEAM_WIN) > 100.0f64.max(0.01 * around(a)) {
                LAG_FINE
            } else {
                0
            };
            for l in l0.saturating_sub(fine).max(lo)..=(l0 + fine).min(hi) {
                let e = ssd(a, l, SEAM_WIN, 2);
                if e < best.0 {
                    best = (e, a, l);
                }
            }
            a += SEAM_HOP;
        }
        best
    };
    let early = search(from + lag0.min(SEAM_SEARCH_SECS * rate as usize));
    let good = |b: &(f64, usize, usize)| {
        10.0 * (b.0.max(1e-9) / level(b.1).max(1e-9)).log10() <= SEAM_GOOD_DB
    };
    let (e, a, lag) = if good(&early) {
        early
    } else {
        search(from + lag0)
    };
    let cost = |n: usize| {
        (x[n] - x[n + lag]).abs() + (x[n - 1] - x[n + lag - 1]).abs() + 0.01 * x[n].abs()
    };
    let intro = (a..a + SEAM_WIN).min_by(|&p, &q| cost(p).total_cmp(&cost(q)))?;
    let sig = level(a).max(1e-9);
    Some(Cut {
        intro,
        lag,
        seam_residual_db: 10.0 * (e.max(1e-9) / sig).log10(),
        cycle_residual_db,
        lag_agreement,
    })
}

/// A state loop and its cut as a `looping::Loop`, for the file writer: `first` is the
/// recording frame of the first sound (the analysed signal starts there).
pub fn as_loop(s: &Snapshots, l: &StateLoop, c: &Cut, first: u64) -> looping::Loop {
    let rel = |k: usize| s.at[k].saturating_sub(first) as usize;
    looping::Loop {
        intro: c.intro,
        repeats_from: rel(l.start),
        period: c.lag,
        period_exact: c.lag as f64,
        residual_db: c.seam_residual_db,
        matched: rel((l.start + l.matched).min(s.at.len() - 1)) - rel(l.start),
        cycles: 1,
    }
}

/// A dump: the snapshots, the origin of each byte, the recording frame of the first sound.
pub type Dump = (Snapshots, Vec<(i32, u32)>, u64);

/// Snapshots dumped with `R2A_SEQ_DUMP` (see `Extractor::finish`): the snapshot length,
/// the count, the recording frame of the first sound, then the origin of every byte (cpu
/// as i32, address as u32), then per snapshot its recording frame and its bytes.
pub fn read_dump(path: &str) -> Option<Dump> {
    let b = std::fs::read(path).ok()?;
    // Dumps without the "SEQ2" tag are of one 6809 (9 registers, then RAM from 0).
    let tagged = b.starts_with(b"SEQ2");
    let b = if tagged { &b[4..] } else { &b[..] };
    let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let len = u32_at(0) as usize;
    let n = u32_at(4) as usize;
    let first = u64::from_le_bytes(b[8..16].try_into().unwrap());
    let mut i = 16;
    let mut origin = Vec::with_capacity(len);
    for k in 0..len {
        if tagged {
            origin.push((u32_at(i) as i32, u32_at(i + 4)));
            i += 8;
        } else if k < 18 {
            origin.push((1, u32::MAX - (k as u32 / 2 + 1)));
        } else {
            origin.push((1, k as u32 - 18));
        }
    }
    let mut log = StateLog::default();
    for _ in 0..n {
        let at = u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        log.push(&b[i + 8..i + 8 + len], at);
        i += 8 + len;
    }
    Some((log.dense(), origin, first))
}

/// `seq-scan <dump> [<raw wav>]`: the state loop of a dumped recording, and its cut.
pub fn scan_cli(args: Vec<String>) {
    let Some((s, origin, first)) = args.first().and_then(|a| read_dump(a)) else {
        eprintln!("usage: seq-scan <dump.seq> [<raw.wav>]");
        return;
    };
    let fps = if s.at.len() > 1 {
        (s.at.len() - 1) as f64 / ((s.at[s.at.len() - 1] - s.at[0]) as f64 / 44100.0)
    } else {
        60.0
    };
    let from = s.at.iter().position(|&t| t > first).unwrap_or(0);
    let t0 = std::time::Instant::now();
    let (keep, rep) = mask(&s, &origin, from);
    let found = find(
        &s,
        &keep,
        from,
        fps as usize,
        (looping::CONFIRM_SECS * fps) as usize,
    );
    println!(
        "{}: {} snapshots, {rep:?}, {:.2} s",
        args[0],
        s.at.len(),
        t0.elapsed().as_secs_f64()
    );
    if found.is_empty() {
        println!("  no state loop");
        if std::env::var_os("R2A_SEQ_DEBUG").is_some() {
            // The best periods by byte share over the second half, every 4th frame.
            let n = s.at.len();
            let mut best: Vec<(f64, usize)> = (60..n / 2)
                .map(|p| {
                    let ks: Vec<usize> = (n / 2..n - p - FRAME_TOL).step_by(4).collect();
                    let m: usize = ks.iter().map(|&k| misses(&s, &keep, k, p)).sum();
                    (1.0 - m as f64 / (ks.len() * keep.len()).max(1) as f64, p)
                })
                .collect();
            best.sort_by(|a, b| b.0.total_cmp(&a.0));
            if let Some(p) = std::env::var("R2A_SEQ_WHY")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
            {
                let lo = n / 2;
                let mut bad: Vec<(usize, usize)> = keep
                    .iter()
                    .map(|&j| {
                        let d = (lo..n - p - FRAME_TOL)
                            .filter(|&k| byte_misses(&s, j, k, p))
                            .count();
                        (d, j)
                    })
                    .filter(|x| x.0 > 0)
                    .collect();
                bad.sort_by_key(|x| std::cmp::Reverse(x.0));
                for &(d, j) in bad.iter().take(12) {
                    let k0 = (lo..n - p - FRAME_TOL)
                        .find(|&k| byte_misses(&s, j, k, p))
                        .unwrap_or(lo);
                    let show = |c: usize| {
                        (c.saturating_sub(4)..c + 12)
                            .map(|k| format!("{:02X}", s.frame(k)[j]))
                            .collect::<Vec<_>>()
                            .join("")
                    };
                    println!(
                        "    {:?}: {d} missing; at {k0}: {} | {}",
                        origin[s.cols[j]],
                        show(k0),
                        show(k0 + p)
                    );
                }
            }
            for (sh, p) in best.iter().take(6) {
                println!(
                    "    period {p} frames ({:.2} s): byte share {sh:.5}",
                    *p as f64 / fps
                );
            }
        }
        return;
    }
    let secs = |k: usize| s.at[k] as f64 / 44100.0;
    let mut r = args.get(1).map(|w| hound::WavReader::open(w).unwrap());
    let raw: Vec<i16> = r.as_mut().map_or(Vec::new(), |r| {
        r.samples::<i16>().map(|x| x.unwrap()).collect()
    });
    let ch = r.as_ref().map_or(1, |r| r.spec().channels as usize);
    for l in found {
        println!(
            "  state loop: period {} frames ({:.3} s) from {:.3} s, byte share {:.5} over {} frames, {} byte(s) with a clock of their own",
            l.period,
            secs(l.start + l.period) - secs(l.start),
            secs(l.start),
            l.byte_share,
            l.matched,
            l.own_clock
        );
        if raw.is_empty() {
            continue;
        }
        let rec = &raw[first as usize * ch..];
        let x = looping::mono(rec, ch);
        let from_s = s.at[l.start].saturating_sub(first) as usize;
        let lag0 = (s.at[l.start + l.period] - s.at[l.start]) as usize;
        let spf = (s.at[1] - s.at[0]) as usize;
        let t1 = std::time::Instant::now();
        match cut(
            &x,
            44100,
            from_s,
            lag0,
            spf,
            (looping::CONFIRM_SECS * 44100.0) as usize,
        ) {
            Some(c) => {
                let lp = as_loop(&s, &l, &c, first);
                let seam = looping::seam(rec, ch, lp.intro, lp.period);
                println!(
                    "  cut: intro {:.3} s, body {} samples ({:.3} s, state {lag0}), seam residual {:.1} dB, cycle residual {:.1} dB, lags agree {:.2} ({}), seam error {} (joint {} natural {}, body p99 {} max {}) ({:.2} s)",
                    lp.intro as f64 / 44100.0,
                    lp.period,
                    lp.period as f64 / 44100.0,
                    c.seam_residual_db,
                    c.cycle_residual_db,
                    c.lag_agreement,
                    if c.confirmed() {
                        "confirmed"
                    } else {
                        "NOT CONFIRMED"
                    },
                    seam.error,
                    seam.joint,
                    seam.natural,
                    seam.p99,
                    seam.max,
                    t1.elapsed().as_secs_f64()
                );
            }
            None => println!("  no cut (recording too short)"),
        }
    }
}

/// `seq-audio <lag> <radius> <wav>`: per second, the best lag within `lag +/- radius` and
/// its residual (a diagnostic of how the audio repeats at a state period).
pub fn audio_cli(args: Vec<String>) {
    let lag: usize = args[0].parse().unwrap();
    let radius: usize = args[1].parse().unwrap();
    let mut r = hound::WavReader::open(&args[2]).unwrap();
    let ch = r.spec().channels as usize;
    let raw: Vec<i16> = r.samples::<i16>().map(|x| x.unwrap()).collect();
    let x = looping::mono(&raw, ch);
    let w = 22050usize;
    let mut a = 0usize;
    while a + w + lag + radius < x.len() {
        let sig = x[a..a + w].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / w as f64;
        let (e, l) = (lag - radius..=lag + radius)
            .step_by(2)
            .map(|l| {
                (
                    (0..w)
                        .step_by(8)
                        .map(|i| ((x[a + i] - x[a + i + l]) as f64).powi(2))
                        .sum::<f64>()
                        / (w / 8) as f64,
                    l,
                )
            })
            .min_by(|p, q| p.0.total_cmp(&q.0))
            .unwrap();
        println!(
            "{:7.2} s: best lag {:+6}  residual {:6.1} dB",
            a as f64 / 44100.0,
            l as i64 - lag as i64,
            10.0 * (e / sig.max(1e-9)).log10()
        );
        a += 2 * w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound CPU with 2 registers (4 bytes, S = register 2 at 0x1F00..) and 8 RAM bytes:
    /// a free-running counter, a byte that changes every frame, and a sequencer whose
    /// position goes round every `period` frames after `intro`.
    fn sequencer(frames: usize, intro: usize, period: usize) -> (StateLog, Vec<(i32, u32)>) {
        let origin: Vec<(i32, u32)> = vec![
            (1, u32::MAX - 1),
            (1, u32::MAX - 1),
            (1, u32::MAX - 2),
            (1, u32::MAX - 2),
        ]
        .into_iter()
        .chain((0..8).map(|a| (1, a)))
        .collect();
        let mut log = StateLog::default();
        for k in 0..frames {
            let pos = if k < intro {
                k
            } else {
                intro + (k - intro) % period
            };
            let mut snap = vec![0u8; origin.len()];
            snap[0] = (k * 7) as u8; // PC
            snap[2] = 0xF0 - (k % 3) as u8; // S
            snap[3] = 0x1F;
            snap[4] = k as u8; // free-running counter
            snap[5] = (k * 13 % 7) as u8; // changes every frame
            snap[6] = (pos / 10) as u8; // sequencer position
            snap[7] = (pos / 25 % 3) as u8;
            snap[8] = 0x55; // never changes
            log.push(&snap, (k * 735) as u64);
        }
        (log, origin)
    }

    #[test]
    fn dense_keeps_the_varying_bytes() {
        let (log, _) = sequencer(40, 0, 30);
        let s = log.dense();
        assert!(!s.cols.contains(&8) && s.cols.contains(&6));
        let j = s.cols.iter().position(|&c| c == 4).unwrap();
        assert_eq!(s.frame(39)[j], 39);
        assert_eq!(s.first[8], 0x55);
    }

    #[test]
    fn mask_leaves_out_registers_counters_and_fast_bytes() {
        let (log, origin) = sequencer(600, 0, 300);
        let s = log.dense();
        let (keep, rep) = mask(&s, &origin, 0);
        let kept: Vec<usize> = keep.iter().map(|&j| s.cols[j]).collect();
        assert_eq!(kept, vec![6, 7]);
        // PC low and S low change; their high bytes do not.
        assert_eq!((rep.registers, rep.counters, rep.fast), (2, 1, 1));
    }

    #[test]
    fn finds_the_sequencer_period() {
        let (log, origin) = sequencer(1500, 50, 300);
        let s = log.dense();
        let (keep, _) = mask(&s, &origin, 0);
        let found = find(&s, &keep, 0, 60, 300);
        let l = found.first().expect("a period");
        assert_eq!((l.period, l.own_clock), (300, 0));
        assert!(l.start <= 60, "starts at {}", l.start);
        // Not two cycles yet: nothing.
        let (log, origin) = sequencer(600, 50, 300);
        let s = log.dense();
        let (keep, _) = mask(&s, &origin, 0);
        let f = find(&s, &keep, 0, 60, 300);
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn cut_confirms_a_repeating_signal_and_not_noise() {
        let rate = 8000u32;
        let lag = 12_345usize;
        // A cycle of chirps, repeated with a small timing jitter and a little noise.
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 16) as f32 / 65536.0 - 0.5
        };
        let cycle: Vec<f32> = (0..lag)
            .map(|i| 3000.0 * ((i as f32 * 0.05).sin() * ((i / 700) % 5) as f32))
            .collect();
        let x: Vec<f32> = (0..lag * 30)
            .map(|i| cycle[i % lag] + 50.0 * noise())
            .collect();
        let c = cut(&x, rate, 100, lag, 30, 4 * rate as usize).unwrap();
        assert!(c.confirmed(), "{c:?}");
        assert!(c.lag.abs_diff(lag) <= 2, "{c:?}");
        let y: Vec<f32> = (0..lag * 30).map(|_| 3000.0 * noise()).collect();
        let c = cut(&y, rate, 100, lag, 30, 4 * rate as usize).unwrap();
        assert!(!c.confirmed(), "{c:?}");
    }
}
