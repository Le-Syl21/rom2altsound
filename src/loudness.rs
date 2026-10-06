//! EBU R128 loudness of the extracted sounds, measured the way VPX plays PinMAME's stream:
//! a mono ROM output is duplicated to two identical channels (which reads +3 LU above a
//! mono-only measurement), a stereo one stays stereo.
//!
//! Every level is measured on the DC-blocked signal (the same one as `peak_dbfs`), as
//! normalized floats (full scale = 32768).
//!
//! Per file, the energies of the R128 gating blocks (400 ms windows every 100 ms, the
//! momentary loudness at each 100 ms step) are kept, so that the ROM totals can be gated
//! over any subset of the files at the end (without the files flagged as ignoring the
//! master volume, without the loops...). Gating the union of the blocks is the integrated
//! loudness of the files played back to back, minus the blocks that would straddle two
//! files. A file shorter than one block counts as one block: itself padded with silence.

use ebur128::{EbuR128, Mode};
use serde::Serialize;

const MODE: Mode = Mode::I.union(Mode::TRUE_PEAK);
/// BS.1770 absolute gate, and relative gate below the ungated mean.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;
/// Offset of the BS.1770 loudness formula: L = -0.691 + 10 log10(energy).
const LUFS_OFFSET: f64 = -0.691;

/// The loudness of one file.
#[derive(Clone, Debug, Default)]
pub struct FileLoudness {
    /// Integrated loudness. None: shorter than one 400 ms gating block, or silent.
    pub lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    /// `lufs`, or for a file under 400 ms the integrated loudness of the file padded with
    /// silence to 400 ms: a level every file has, used to compare files with each other.
    pub level_lufs: Option<f64>,
    /// Energies of the file's gating blocks.
    pub blocks: Vec<f64>,
    pub frames: u64,
}

/// Loudness of a set of files played back to back.
#[derive(Debug, Default, Serialize)]
pub struct Aggregate {
    pub files: usize,
    pub seconds: f64,
    /// Integrated loudness of the files' gating blocks together.
    pub lufs: Option<f64>,
    /// The loudest file's true peak.
    pub true_peak_dbtp: Option<f64>,
}

/// Measures one file: `samples` interleaved, `ch` channels, DC-blocked, in LSB.
pub fn measure(samples: &[f64], ch: usize, rate: u32) -> FileLoudness {
    measure_with(samples, ch, rate, false)
}

/// `measure` with the true peak from `true_peak_interpolated` instead of the R128 meter's
/// own (whose precise resampler costs about 0.13 s per second of audio: hours of Stern SAM
/// music would take tens of CPU minutes). Within 0.1 dB of the meter (tests).
pub fn measure_fast(samples: &[f64], ch: usize, rate: u32) -> FileLoudness {
    measure_with(samples, ch, rate, true)
}

fn measure_with(samples: &[f64], ch: usize, rate: u32, fast_peak: bool) -> FileLoudness {
    let ch = ch.max(1);
    let mode = if fast_peak { Mode::I } else { MODE };
    let mut meter = EbuR128::new(2, rate, mode).expect("valid R128 meter parameters");
    let frames = samples.len() / ch;
    let step = (rate as usize).div_ceil(10).max(1); // libebur128's 100 ms
    let mut blocks = Vec::new();
    let mut fed = 0;
    for chunk in samples.chunks(step * ch) {
        feed(&mut meter, chunk, ch);
        fed += chunk.len() / ch;
        // A gating block ends at every whole 100 ms step from 400 ms on.
        if chunk.len() == step * ch && fed >= 4 * step {
            let m = meter.loudness_momentary().unwrap_or(f64::NEG_INFINITY);
            blocks.push(energy(m));
        }
    }
    let peak = if fast_peak {
        if ch == 1 {
            true_peak_interpolated(samples, rate) / 32768.0
        } else {
            (0..ch)
                .map(|c| {
                    let x: Vec<f64> = samples.iter().skip(c).step_by(ch).copied().collect();
                    true_peak_interpolated(&x, rate) / 32768.0
                })
                .fold(0.0f64, f64::max)
        }
    } else {
        (0..2)
            .filter_map(|c| meter.true_peak(c).ok())
            .fold(0.0f64, f64::max)
    };
    let lufs = finite(meter.loudness_global().ok());
    let level_lufs = lufs.or_else(|| {
        // Pad to one gating block.
        let pad = (4 * step).saturating_sub(frames);
        if frames == 0 || pad == 0 {
            return None;
        }
        feed(&mut meter, &vec![0.0; pad * ch], ch);
        finite(meter.loudness_global().ok())
    });
    if blocks.is_empty() {
        blocks.extend(level_lufs.map(energy));
    }
    FileLoudness {
        lufs,
        true_peak_dbtp: (peak > 0.0).then(|| round3(20.0 * peak.log10())),
        level_lufs,
        blocks,
        frames: frames as u64,
    }
}

/// Half-length of the interpolator, in input samples.
const TP_HALF: usize = 8;

/// The true peak of one channel (same unit as `x`): the largest magnitude of the signal
/// oversampled to at least 192 kHz (x8 at 24 kHz, as BS.1770 asks for x4 at 48 kHz), with
/// a Lanczos-windowed sinc interpolator of 16 input samples.
pub fn true_peak_interpolated(x: &[f64], rate: u32) -> f64 {
    let os = (192_000usize.div_ceil(rate.max(1) as usize)).clamp(4, 16);
    // taps[p][j]: weight of x[n + j - TP_HALF + 1] for the point n + p / os.
    let taps: Vec<[f64; 2 * TP_HALF]> = (1..os)
        .map(|p| {
            let f = p as f64 / os as f64;
            let mut t = [0.0; 2 * TP_HALF];
            for (j, w) in t.iter_mut().enumerate() {
                let d = f - (j as f64 - TP_HALF as f64 + 1.0);
                let sinc = |v: f64| {
                    if v.abs() < 1e-12 {
                        1.0
                    } else {
                        (std::f64::consts::PI * v).sin() / (std::f64::consts::PI * v)
                    }
                };
                *w = sinc(d) * sinc(d / TP_HALF as f64);
            }
            t
        })
        .collect();
    let mut peak = x.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let n = x.len();
    let at = |i: isize| {
        if i >= 0 && (i as usize) < n {
            x[i as usize]
        } else {
            0.0
        }
    };
    let mut win = [0.0f64; 2 * TP_HALF];
    for i in 0..n {
        // The os - 1 points between x[i] and x[i + 1].
        for (j, w) in win.iter_mut().enumerate() {
            *w = at(i as isize + j as isize - TP_HALF as isize + 1);
        }
        for t in &taps {
            let y: f64 = t.iter().zip(&win).map(|(a, b)| a * b).sum();
            peak = peak.max(y.abs());
        }
    }
    peak
}

/// The ROM total of a set of files.
pub fn aggregate<'a>(files: impl IntoIterator<Item = &'a FileLoudness>, rate: u32) -> Aggregate {
    let mut n = 0;
    let mut frames = 0;
    let mut peak: Option<f64> = None;
    let mut blocks: Vec<f64> = Vec::new();
    for f in files {
        n += 1;
        frames += f.frames;
        peak = match (peak, f.true_peak_dbtp) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        blocks.extend_from_slice(&f.blocks);
    }
    Aggregate {
        files: n,
        seconds: round3(frames as f64 / rate.max(1) as f64),
        lufs: gated(&blocks),
        true_peak_dbtp: peak,
    }
}

/// BS.1770 integrated loudness of a set of gating-block energies.
pub fn gated(blocks: &[f64]) -> Option<f64> {
    let abs = energy(ABSOLUTE_GATE_LUFS);
    let mean = |min: f64| {
        let (sum, n) = blocks
            .iter()
            .filter(|&&e| e >= min)
            .fold((0.0, 0usize), |(s, n), &e| (s + e, n + 1));
        (n > 0).then(|| sum / n as f64)
    };
    let ungated = mean(abs)?;
    let rel = ungated * 10f64.powf(RELATIVE_GATE_LU / 10.0);
    let m = mean(abs.max(rel))?;
    finite(Some(LUFS_OFFSET + 10.0 * m.log10()))
}

/// Median of a set of levels.
pub fn median(mut v: Vec<f64>) -> Option<f64> {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    match n {
        0 => None,
        _ if n % 2 == 1 => Some(round3(v[n / 2])),
        _ => Some(round3((v[n / 2 - 1] + v[n / 2]) / 2.0)),
    }
}

fn energy(lufs: f64) -> f64 {
    if lufs.is_finite() {
        10f64.powf((lufs - LUFS_OFFSET) / 10.0)
    } else {
        0.0
    }
}

/// Feeds a mono stream as two identical channels (VPX's playback), stereo as is,
/// normalized to full scale.
fn feed(meter: &mut EbuR128, samples: &[f64], ch: usize) {
    let norm: Vec<f64> = samples.iter().map(|x| x / 32768.0).collect();
    let res = if ch == 1 {
        meter.add_frames_planar_f64(&[&norm, &norm])
    } else {
        meter.add_frames_f64(&norm)
    };
    res.expect("frame count matches the meter's channels");
}

fn finite(x: Option<f64>) -> Option<f64> {
    x.filter(|v| v.is_finite()).map(round3)
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1 kHz sine at the given peak amplitude, `secs` long, mono.
    fn sine(amp: f64, secs: f64, rate: u32) -> Vec<f64> {
        (0..(secs * rate as f64) as usize)
            .map(|i| {
                amp * 32767.0 * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate as f64).sin()
            })
            .collect()
    }

    #[test]
    fn mono_is_measured_as_two_channels() {
        // EBU Tech 3341: a full-scale 1 kHz sine reads -3.01 LUFS on one channel, so 0.0 on
        // two identical channels.
        let l = measure(&sine(1.0, 2.0, 48000), 1, 48000);
        let lufs = l.lufs.unwrap();
        assert!(lufs.abs() < 0.1, "{lufs}");
        assert!(l.true_peak_dbtp.unwrap().abs() < 0.1);
    }

    #[test]
    fn gating_the_kept_blocks_matches_the_meter() {
        // A loud and a quiet part, so that the relative gate matters.
        let mut s = sine(0.5, 2.3, 44100);
        s.extend(sine(0.01, 1.7, 44100));
        let l = measure(&s, 1, 44100);
        let ours = gated(&l.blocks).unwrap();
        assert!(
            (ours - l.lufs.unwrap()).abs() < 0.01,
            "{ours} vs {:?}",
            l.lufs
        );
    }

    #[test]
    fn totals_are_gated_over_the_chosen_files() {
        let quiet = measure(&sine(0.1, 2.0, 44100), 1, 44100);
        let loud = measure(&sine(1.0, 2.0, 44100), 1, 44100);
        let both = aggregate([&quiet, &loud], 44100);
        assert_eq!(both.files, 2);
        assert!((both.seconds - 4.0).abs() < 0.001);
        let only_quiet = aggregate([&quiet], 44100);
        assert!(both.lufs.unwrap() > only_quiet.lufs.unwrap() + 10.0);
        assert!(only_quiet.true_peak_dbtp.unwrap() < -19.0);
    }

    #[test]
    fn short_files_get_a_padded_level() {
        let l = measure(&sine(1.0, 0.1, 44100), 1, 44100);
        assert_eq!(l.lufs, None);
        // 100 ms of a 0 LUFS tone in a 400 ms block: -6 LU.
        let level = l.level_lufs.unwrap();
        assert!((level + 6.02).abs() < 0.2, "{level}");
        assert!((gated(&l.blocks).unwrap() - level).abs() < 0.01);
    }

    #[test]
    fn interpolated_true_peak_matches_the_meter() {
        // Tones near a quarter of the rate (sample peaks well below the true peak), a
        // two-tone mix and a click.
        for (f, rate) in [
            (5997.0, 24000),
            (11000.0, 24000),
            (3001.0, 12000),
            (997.0, 48000),
        ] {
            let mut s = sine(0.5, 1.0, rate);
            let fs = rate as f64;
            for (i, v) in s.iter_mut().enumerate() {
                let t = i as f64 / fs;
                *v = 0.5 * 32767.0 * (2.0 * std::f64::consts::PI * f * t + 0.3).sin()
                    + 0.3 * 32767.0 * (2.0 * std::f64::consts::PI * (f * 0.61) * t).sin();
            }
            s[rate as usize / 2] += 9000.0;
            let a = measure(&s, 1, rate).true_peak_dbtp.unwrap();
            let b = measure_fast(&s, 1, rate).true_peak_dbtp.unwrap();
            assert!(
                (a - b).abs() < 0.1,
                "{f} Hz at {rate}: meter {a} vs ours {b}"
            );
            assert_eq!(measure(&s, 1, rate).lufs, measure_fast(&s, 1, rate).lufs);
        }
    }

    #[test]
    fn medians() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(Vec::new()), None);
    }
}
