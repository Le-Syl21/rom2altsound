//! Finds one cycle of a looping sound in its recording.
//!
//! The emulation is deterministic, so a sound that loops forever repeats its output: after
//! an intro, `x[n] == x[n + period]`, up to the upstream mixer's +/-1 LSB TPDF dither. The
//! period is rarely a whole number of output samples, though: the board's stream is
//! resampled to 44.1 kHz (a DCS frame is 240 samples at 31250 Hz, so `F` frames are
//! `338.688 * F` output samples), with libsamplerate's band-limited sinc converter. Each
//! cycle is then the previous one delayed by a constant fraction of a sample, and the
//! comparison delays the signal by that fraction (windowed-sinc interpolation) before
//! measuring what is left. A window passes when the residual is within the dither or far
//! below the signal. A plain integer lag is not enough: for a DCS track of 232 frames
//! (78575.616 samples) the 0.4-sample offset left -25 dB in bright windows, and only every
//! 8th cycle (0.07 sample off) passed.
//!
//! Without a known period, the smallest one that holds from some point up to the end of the
//! recording, over at least one full period and at least `CONFIRM_SECS`, wins: the
//! confirmation span is what keeps a phrase that repeats twice inside a longer loop from
//! passing for the loop.

/// Shortest period searched, in seconds.
pub const MIN_PERIOD_SECS: f64 = 0.05;
/// The repetition must hold over at least this long (and one full period), so that a
/// phrase played twice inside a longer loop is not taken for the loop.
pub const CONFIRM_SECS: f64 = 20.0;
/// With a known period (the DCS track program), the repetition must hold over the full
/// period, or this long for a longer one, and at least `CONFIRM_SECS`.
pub const HINT_CONFIRM_MAX_SECS: f64 = 60.0;
/// Envelope window and hop for the coarse search, in samples. The window overlaps four
/// hops, so that a lag that is not a whole number of hops still lines the envelopes up.
const ENV_BLOCK: usize = 4096;
const ENV_HOP: usize = 1024;
/// A lag passes the coarse search when the envelopes of the last two spans differ by at
/// most this (sum of absolute differences over sum of levels)...
const ENV_MAX_DIFF: f64 = 0.2;
/// ...and is nearly as good as the best lag (see `coarse`). At most this many coarse
/// candidates are checked sample by sample, shortest first.
const MAX_CANDIDATES: usize = 16;
/// Verification window, in samples.
const WIN: usize = 2048;
/// A window passes when its residual rms is within this many LSB (two dithered copies of
/// the same signal differ by about 0.6 LSB rms)...
const ABS_TOL: f64 = 4.0;
/// ...or at most this far below the window's own rms (-30 dB).
const REL_TOL: f64 = 0.0316;
/// Half length of the fractional delay filter (taps `-HALF..=HALF`).
const HALF: usize = 16;
/// A candidate lag is only verified when, at the whole sample, it already maps the
/// strongest part of the end of the recording within this (dB) of the signal.
const QUICK_MAX_DB: f64 = -6.0;

/// One cycle of a loop, in sample frames from the start of the analysed signal.
#[derive(Clone, Debug, PartialEq)]
pub struct Loop {
    /// Where the loop body starts (the intro is everything before).
    pub intro: usize,
    /// Where the repetition starts: from here on the signal is the same one period later.
    /// The body starts a little after it (`verify`), so the intro always holds this much
    /// audio that is not the loop, then a piece of the loop itself; 0 for a loop without
    /// an intro of its own.
    pub repeats_from: usize,
    /// The body's length, rounded to whole samples.
    pub period: usize,
    /// The period with its fractional part.
    pub period_exact: f64,
    /// The worst window's residual against its own rms, in dB (more negative is better),
    /// over the windows that carry signal (over all of them for a loop that never does).
    pub residual_db: f64,
    /// How many samples the repetition was verified over.
    pub matched: usize,
    /// Cycles in the body: 1, or more for a short loop whose single cycle is not a whole
    /// number of samples (`whole_cycles`); `period` and `period_exact` are the body's.
    pub cycles: u32,
}

/// Mixes interleaved samples down to mono.
pub fn mono(samples: &[i16], ch: usize) -> Vec<f32> {
    let ch = ch.max(1);
    samples
        .chunks_exact(ch)
        .map(|f| f.iter().map(|&s| s as f32).sum::<f32>() / ch as f32)
        .collect()
}

/// The loop in `x` (mono), if it repeats. `hint`: a period known from elsewhere (the DCS
/// track program), in samples with its fraction; it is then the only one tried.
pub fn find(x: &[f32], rate: u32, hint: Option<f64>) -> Option<Loop> {
    let secs = |s: f64| (s * rate as f64) as usize;
    let confirm = secs(CONFIRM_SECS);
    if let Some(p) = hint {
        let lag = p.round() as usize;
        let need = lag.min(secs(HINT_CONFIRM_MAX_SECS)).max(confirm);
        return verify(x, lag, p - lag as f64, need);
    }
    for m in coarse(x, rate, confirm) {
        let Some(lag) = refine(x, rate, m * ENV_HOP, ENV_HOP + 8) else {
            continue;
        };
        if let Some(found) = check(x, rate, lag, confirm) {
            return Some(shortest(x, rate, found, confirm));
        }
    }
    None
}

/// Verifies an integer lag found without a hint: a quick look at the strongest part of the
/// end of the recording, then the fraction of a sample, then every window.
fn check(x: &[f32], rate: u32, lag: usize, confirm: usize) -> Option<Loop> {
    let (a, len) = strongest(x, rate, lag)?;
    let (res, sig) = integer_residual(x, a, len, lag);
    if sig > 0.0 && 20.0 * (res / sig).log10() > QUICK_MAX_DB && res > ABS_TOL {
        return None;
    }
    let delta = fit_delta(x, a, len, lag);
    verify(x, lag, delta, lag.max(confirm))
}

/// A verified lag may be a multiple of the period (the envelope often matches best there):
/// the largest divisor whose fraction of the period verifies too gives the period.
fn shortest(x: &[f32], rate: u32, found: Loop, confirm: usize) -> Loop {
    let min = (MIN_PERIOD_SECS * rate as f64) as usize;
    let max_div = (found.period / min.max(1)).max(1);
    for d in (2..=max_div).rev() {
        let lag = (found.period_exact / d as f64).round() as usize;
        if let Some(sub) = check(x, rate, lag, confirm) {
            return sub;
        }
    }
    found
}

/// Envelope lags (in hops of `ENV_HOP` samples) whose last two spans look alike, shortest
/// first.
fn coarse(x: &[f32], rate: u32, confirm: usize) -> Vec<usize> {
    let sq: Vec<f64> = x
        .as_chunks::<ENV_HOP>()
        .0
        .iter()
        .map(|b| b.iter().map(|&s| (s as f64).powi(2)).sum::<f64>())
        .collect();
    let env: Vec<f64> = sq
        .windows(ENV_BLOCK / ENV_HOP)
        .map(|w| (w.iter().sum::<f64>() / ENV_BLOCK as f64).sqrt())
        .collect();
    let k = env.len();
    let min = ((MIN_PERIOD_SECS * rate as f64) as usize)
        .div_ceil(ENV_HOP)
        .max(1);
    let confirm = confirm.div_ceil(ENV_HOP);
    let mut diffs: Vec<(usize, f64)> = Vec::new();
    for m in min..k {
        let span = m.max(confirm);
        let Some(start) = k.checked_sub(m + span) else {
            break;
        };
        let (mut num, mut den) = (0.0, 1e-9);
        for i in start..k - m {
            num += (env[i] - env[i + m]).abs();
            den += env[i] + env[i + m];
        }
        diffs.push((m, num / den));
    }
    let Some(best) = diffs.iter().map(|d| d.1).reduce(f64::min) else {
        return Vec::new();
    };
    // Lags nearly as good as the best one; the shortest that verifies wins, and `shortest`
    // then looks for the period in the divisors of a multiple.
    let limit = ENV_MAX_DIFF.min((3.0 * best).max(best + 0.01));
    // Local minima under the limit. A whole run of equal values (silence) counts once.
    let mut out = Vec::new();
    for (i, &(m, d)) in diffs.iter().enumerate() {
        let left = i == 0 || diffs[i - 1].1 > d;
        let right = i + 1 == diffs.len() || diffs[i + 1].1 >= d;
        if d <= limit && left && right {
            out.push(m);
            if out.len() == MAX_CANDIDATES {
                break;
            }
        }
    }
    out
}

/// The lag within `lag +/- radius` that best maps the end of the signal onto what came one
/// lag before (least squares), searched on an 8x decimated signal and then sample by sample.
fn refine(x: &[f32], rate: u32, lag: usize, radius: usize) -> Option<usize> {
    let n = x.len();
    let hi = lag + radius;
    let w = n.checked_sub(hi)?.min(2 * rate as usize);
    if w < WIN {
        return None;
    }
    let from = n - hi - w;
    let ssd = |sig: &[f32], from: usize, len: usize, l: usize| -> f64 {
        sig[from..from + len]
            .iter()
            .zip(&sig[from + l..from + l + len])
            .map(|(&a, &b)| ((a - b) as f64).powi(2))
            .sum()
    };
    let lo = lag.saturating_sub(radius).max(1);
    const D: usize = 8;
    let dec: Vec<f32> = x
        .as_chunks::<D>()
        .0
        .iter()
        .map(|c| c.iter().sum::<f32>() / D as f32)
        .collect();
    let (f, len) = (from / D, w / D);
    let coarse_best = (lo / D..=hi / D)
        .filter(|&l| f + l + len <= dec.len())
        .map(|l| (l, ssd(&dec, f, len, l)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0
        * D;
    let fine = coarse_best.saturating_sub(12).max(lo)..=(coarse_best + 12).min(hi);
    fine.filter(|&l| from + l + w <= n)
        .map(|l| (l, ssd(x, from, w, l)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(l, _)| l)
}

/// The last sample index `a` for which `x[a + lag]` can be interpolated.
fn last_index(x: &[f32], lag: usize) -> Option<usize> {
    x.len().checked_sub(lag + HALF + 2)
}

/// The loudest half second in the last 10 s that can be compared at `lag`: (start, length).
fn strongest(x: &[f32], rate: u32, lag: usize) -> Option<(usize, usize)> {
    let len = (rate as usize / 2).max(WIN);
    let end = last_index(x, lag)?.checked_sub(len)?;
    let from = end.saturating_sub(10 * rate as usize);
    let energy = |a: usize| {
        x[a..a + len]
            .iter()
            .map(|&s| (s as f64).powi(2))
            .sum::<f64>()
    };
    (from..=end)
        .step_by(len / 2)
        .chain(std::iter::once(end))
        .max_by(|&a, &b| energy(a).total_cmp(&energy(b)))
        .map(|a| (a, len))
}

/// Residual and signal rms of `x[a..a + len]` against `x[a + lag..]`, whole samples.
fn integer_residual(x: &[f32], a: usize, len: usize, lag: usize) -> (f64, f64) {
    let (mut e, mut s) = (0.0, 0.0);
    for n in a..a + len {
        e += ((x[n] - x[n + lag]) as f64).powi(2);
        s += (x[n] as f64).powi(2);
    }
    ((e / len as f64).sqrt(), (s / len as f64).sqrt())
}

/// Windowed-sinc taps (Blackman window) that read a signal `delta` samples (|delta| <= 1)
/// after a sample: `y = sum(h[k] * x[i + k - HALF])` is `x(i + delta)`.
fn kernel(delta: f64) -> [f32; 2 * HALF + 1] {
    let mut h = [0f32; 2 * HALF + 1];
    let width = (HALF + 2) as f64;
    for (k, tap) in h.iter_mut().enumerate() {
        let t = k as f64 - HALF as f64 - delta;
        let sinc = if t.abs() < 1e-9 {
            1.0
        } else {
            (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
        };
        let u = (t / width + 1.0) / 2.0; // 0..1 across the window
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * u).cos()
            + 0.08 * (4.0 * std::f64::consts::PI * u).cos();
        *tap = (sinc * w) as f32;
    }
    h
}

/// One window's comparison of `x[a..a + len]` with `x` read `lag + delta` samples later:
/// (residual rms, signal rms).
fn window(x: &[f32], a: usize, len: usize, lag: usize, h: &[f32; 2 * HALF + 1]) -> (f64, f64) {
    let (mut ee, mut xx) = (0.0f64, 0.0f64);
    for n in a..a + len {
        let base = n + lag - HALF;
        let y: f32 = x[base..base + 2 * HALF + 1]
            .iter()
            .zip(h)
            .map(|(&v, &t)| v * t)
            .sum();
        let e = (x[n] - y) as f64;
        ee += e * e;
        xx += (x[n] as f64).powi(2);
    }
    let n = len as f64;
    ((ee / n).sqrt(), (xx / n).sqrt())
}

/// The fraction of a sample (-1..1) that best maps `x[a..a + len]` onto `x[a + lag..]`.
fn fit_delta(x: &[f32], a: usize, len: usize, lag: usize) -> f64 {
    let res = |d: f64| window(x, a, len, lag, &kernel(d)).0;
    let step = 0.05;
    let grid: Vec<(f64, f64)> = (-20..=20)
        .map(|i| {
            let d = i as f64 * step;
            (d, res(d))
        })
        .collect();
    let i = (0..grid.len())
        .min_by(|&i, &j| grid[i].1.total_cmp(&grid[j].1))
        .unwrap_or(20);
    if i == 0 || i + 1 == grid.len() {
        return grid[i].0;
    }
    // Parabola through the best point and its neighbours (on the squared residual).
    let (y0, y1, y2) = (
        grid[i - 1].1.powi(2),
        grid[i].1.powi(2),
        grid[i + 1].1.powi(2),
    );
    let den = y0 - 2.0 * y1 + y2;
    let off = if den > 0.0 {
        0.5 * (y0 - y2) / den
    } else {
        0.0
    };
    grid[i].0 + off.clamp(-1.0, 1.0) * step
}

fn passes(res: f64, sig: f64) -> bool {
    res <= ABS_TOL || res <= REL_TOL * sig
}

/// Checks that the signal repeats `lag + delta` samples later from some point to the end,
/// over at least `need` samples (and one period); finds where the repetition starts.
fn verify(x: &[f32], lag: usize, delta: f64, need: usize) -> Option<Loop> {
    if lag <= HALF {
        return None;
    }
    let h = kernel(delta);
    let last = last_index(x, lag)?;
    let mut start = last;
    let (mut worst, mut worst_any) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut db = |res: f64, sig: f64| {
        if sig > 0.0 {
            let d = 20.0 * (res.max(1e-9) / sig).log10();
            worst_any = worst_any.max(d);
            if sig > 10.0 * ABS_TOL {
                worst = worst.max(d);
            }
        }
    };
    // Walk back window by window while the repetition holds, then in 64-sample steps.
    while start >= WIN {
        let a = start - WIN;
        let (res, sig) = window(x, a, WIN, lag, &h);
        if !passes(res, sig) {
            if std::env::var_os("LOOP_DEBUG").is_some() {
                eprintln!("verify lag {lag}{delta:+.3}: fails at {a} (res {res:.1}, sig {sig:.1})");
            }
            break;
        }
        db(res, sig);
        start = a;
    }
    const STEP: usize = 64;
    while start > 0 {
        let a = start.saturating_sub(STEP);
        let (res, sig) = window(x, a, start - a, lag, &h);
        if !passes(res, sig) {
            break;
        }
        start = a;
    }
    let matched = last - start;
    if matched < lag.max(need) {
        return None;
    }
    if worst == f64::NEG_INFINITY {
        worst = worst_any;
    }
    let found = |intro| Loop {
        intro,
        repeats_from: start,
        period: lag,
        period_exact: lag as f64 + delta,
        residual_db: worst,
        matched,
        cycles: 1,
    };
    // When it holds from the very first sample, there is no intro.
    if start == 0 {
        return Some(found(0));
    }
    // The body may start anywhere in the matched span; one window in keeps it clear of the
    // fuzzy edge where the intro turns into the loop. Within the next period (at most a
    // second), it starts where the cycles differ least, preferably at a quiet point.
    let first = (start + WIN).min(last - lag);
    let end = (first + lag.min(44100)).min(last - lag);
    let cost = |n: usize| {
        (x[n] - x[n + lag]).abs() + (x[n - 1] - x[n + lag - 1]).abs() + 0.01 * x[n].abs()
    };
    let intro = (first.max(1)..=end.max(1))
        .min_by(|&a, &b| cost(a).total_cmp(&cost(b)))
        .unwrap_or(first);
    Some(found(intro))
}

/// Longest body made of several cycles (see `whole_cycles`), in seconds.
pub const MULTI_CYCLE_MAX_SECS: f64 = 2.0;
/// A body of several cycles is used when its length is within this of a whole sample.
const WHOLE_SAMPLE_TOL: f64 = 0.05;

/// A body of one cycle is rounded to whole samples, so each repeat of a short loop shifts
/// the waveform by the period's fraction (0.1 sample for the BSMT test tone `F0`, 2984.1
/// samples: a 209 LSB jump on btmn_106 `F2`, whose steps reach 2400). The smallest number
/// of cycles, up to `MULTI_CYCLE_MAX_SECS` and within `len` samples after the intro, whose
/// length is a whole number of samples (10 cycles of 2984.1 = 29841.0) makes the joint
/// exact; the body is then those cycles. Returns the loop unchanged when there is none.
pub fn whole_cycles(l: Loop, rate: u32, len: usize) -> Loop {
    let max = (MULTI_CYCLE_MAX_SECS * rate as f64).min(len.saturating_sub(l.intro) as f64);
    let off = |k: f64| {
        let p = k * l.period_exact;
        (p - p.round()).abs()
    };
    if off(1.0) <= WHOLE_SAMPLE_TOL {
        return l;
    }
    let k = (2..)
        .map(f64::from)
        .take_while(|&k| k * l.period_exact <= max)
        .find(|&k| off(k) <= WHOLE_SAMPLE_TOL);
    match k {
        Some(k) => Loop {
            period: (k * l.period_exact).round() as usize,
            period_exact: k * l.period_exact,
            cycles: k as u32,
            ..l
        },
        None => l,
    }
}

/// How the end of a loop body joins its start, on the interleaved `samples` (the body is
/// frames `start..start + period`, and the recording goes on past it). All in LSB, the
/// largest over the channels.
#[derive(Clone, Debug, PartialEq)]
pub struct Seam {
    /// The step played at the joint: last frame of the body to its first.
    pub joint: i32,
    /// The step the recording itself makes there (last frame of the body to the frame after
    /// it), which the joint replaces.
    pub natural: i32,
    /// How far the body's first frame is from that next frame: the discontinuity the loop
    /// adds (0 for a perfect cycle, a few LSB of dither and resampling otherwise).
    pub error: i32,
    /// The 99th percentile and the largest step between consecutive frames in the body.
    pub p99: i32,
    pub max: i32,
}

pub fn seam(samples: &[i16], ch: usize, start: usize, period: usize) -> Seam {
    let ch = ch.max(1);
    let at = |frame: usize, c: usize| samples.get(frame * ch + c).map_or(0, |&s| s as i32);
    let (first, last, next) = (start, start + period - 1, start + period);
    let per_ch = |f: &dyn Fn(usize) -> i32| (0..ch).map(f).max().unwrap_or(0);
    let joint = per_ch(&|c| (at(first, c) - at(last, c)).abs());
    let natural = per_ch(&|c| (at(next, c) - at(last, c)).abs());
    let error = per_ch(&|c| (at(first, c) - at(next, c)).abs());
    let body = &samples[start * ch..(start + period) * ch];
    let mut steps: Vec<i32> = body
        .windows(ch + 1)
        .map(|w| (w[ch] as i32 - w[0] as i32).abs())
        .collect();
    steps.sort_unstable();
    let p99 = steps
        .get(steps.len().saturating_sub(1) * 99 / 100)
        .copied()
        .unwrap_or(0);
    Seam {
        joint,
        natural,
        error,
        p99,
        max: steps.last().copied().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44100;

    /// Deterministic pseudo-random numbers in [0, 1).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        /// TPDF dither of +/-1 LSB, as PinMAME's mixer adds.
        fn dither(&mut self) -> f64 {
            self.next() - self.next()
        }
    }

    /// A "musical" cycle of `len` samples (fractional lengths allowed), smooth across its
    /// own boundary as a band-limited resampled stream is: partials that are whole
    /// harmonics of the cycle, under an amplitude pattern that is periodic too, with a
    /// change of notes half way.
    fn cycle_value(t: f64, len: f64) -> f64 {
        let tau = 2.0 * std::f64::consts::PI;
        let ph = t / len;
        let env = 0.55 + 0.45 * (tau * 3.0 * ph).sin();
        let harmonic = |hz: f64| (hz * len / RATE as f64).round();
        let (k1, k2, k3) = (harmonic(220.0), harmonic(330.0), harmonic(1250.0));
        let second_half = 0.5 - 0.5 * (tau * ph).cos(); // 0 at the joint, 1 half way
        env * (8000.0 * (tau * k1 * ph).sin()
            + 3000.0 * second_half * (tau * k2 * ph).sin()
            + 1000.0 * (tau * k3 * ph).sin())
    }

    fn signal(intro: usize, period: f64, total: usize, seed: u64) -> Vec<f32> {
        let mut rng = Rng(seed);
        (0..total)
            .map(|n| {
                let v = if n < intro {
                    // A different intro: noise bursts and a sweep.
                    let t = n as f64;
                    6000.0 * (t * t * 1e-9).sin() + 2000.0 * (rng.next() - 0.5)
                } else {
                    cycle_value(((n - intro) as f64) % period, period)
                };
                (v + rng.dither()).round() as f32
            })
            .collect()
    }

    #[test]
    fn finds_period_and_intro_with_dither() {
        let (intro, period) = (3 * RATE as usize, 4.0 * RATE as f64);
        let x = signal(intro, period, intro + 7 * period as usize, 1);
        let l = find(&x, RATE, None).expect("loop found");
        assert_eq!(l.period, period as usize);
        // The body starts in the second after the intro (where the cycles differ least).
        assert!(
            l.intro >= intro && l.intro < intro + WIN + RATE as usize,
            "{l:?}"
        );
        assert!(l.residual_db < -30.0, "{l:?}");
        // The repetition starts where the intro ends (within a window).
        assert!(l.repeats_from.abs_diff(intro) <= WIN, "{l:?}");
        assert!(l.repeats_from <= l.intro, "{l:?}");
    }

    #[test]
    fn fractional_period() {
        // A DCS-like period: 338.688 samples per frame, 600 frames.
        let period = 338.688 * 600.0;
        let x = signal(
            RATE as usize,
            period,
            RATE as usize + 9 * period as usize,
            2,
        );
        let l = find(&x, RATE, None).expect("loop found");
        assert!((l.period as f64 - period).abs() <= 1.0, "{l:?}");
        // With the hint the same period comes out.
        let h = find(&x, RATE, Some(period)).expect("loop found with the hint");
        assert!((h.period as f64 - period).abs() <= 1.0, "{h:?}");
    }

    #[test]
    fn no_intro() {
        let period = 2.5 * RATE as f64;
        let x = signal(0, period, 10 * period as usize, 3);
        let l = find(&x, RATE, None).expect("loop found");
        assert_eq!(l.period, period as usize);
        assert_eq!(l.intro, 0, "{l:?}");
        assert_eq!(l.repeats_from, 0, "{l:?}");
    }

    #[test]
    fn a_phrase_repeated_inside_a_longer_loop_is_not_the_loop() {
        // Body = A A B with A 3 s and B 3 s different: the loop is 9 s, not 3 s.
        let a = 3 * RATE as usize;
        let mut rng = Rng(9);
        let body: Vec<f64> = (0..3 * a)
            .map(|n| {
                if n < 2 * a {
                    cycle_value((n % a) as f64, a as f64)
                } else {
                    5000.0 * (n as f64 * 0.05).sin()
                }
            })
            .collect();
        let x: Vec<f32> = (0..body.len() * 5)
            .map(|n| (body[n % body.len()] + rng.dither()).round() as f32)
            .collect();
        let l = find(&x, RATE, None).expect("loop found");
        assert_eq!(l.period, 3 * a);
    }

    #[test]
    fn noise_does_not_loop() {
        let mut rng = Rng(5);
        let x: Vec<f32> = (0..60 * RATE as usize)
            .map(|_| (8000.0 * (rng.next() - 0.5)) as f32)
            .collect();
        assert_eq!(find(&x, RATE, None), None);
    }

    #[test]
    fn too_short_to_confirm() {
        // Two cycles of 15 s: less than CONFIRM_SECS of repetition.
        let period = 15.0 * RATE as f64;
        let x = signal(0, period, 2 * period as usize, 4);
        assert_eq!(find(&x, RATE, None), None);
    }

    #[test]
    fn short_fractional_periods_take_whole_cycles() {
        let l = Loop {
            intro: 100,
            repeats_from: 0,
            period: 2984,
            period_exact: 2984.1,
            residual_db: -60.0,
            matched: 0,
            cycles: 1,
        };
        let w = whole_cycles(l.clone(), RATE, 10 * RATE as usize);
        assert_eq!((w.period, w.intro, w.cycles), (29841, 100, 10));
        // Not enough recording after the intro: unchanged.
        assert_eq!(whole_cycles(l.clone(), RATE, 20000).period, 2984);
        // A long period stays one cycle (no whole multiple within 2 s).
        let long = Loop {
            period: 176795,
            period_exact: 176795.136,
            ..l
        };
        assert_eq!(whole_cycles(long, RATE, 100 * RATE as usize).period, 176795);
    }

    #[test]
    fn seam_of_a_true_cycle_is_small() {
        let period = RATE as usize; // 1 s, 220 Hz etc.
        let x: Vec<i16> = (0..3 * period)
            .map(|n| cycle_value(n as f64, period as f64) as i16)
            .collect();
        let s = seam(&x, 1, period / 2, period);
        assert!(s.error <= 1, "{s:?}");
        // A cut 50 samples short is a jump.
        let bad = seam(&x, 1, period / 2, period - 50);
        assert!(bad.error > 100, "{bad:?}");
    }
}
