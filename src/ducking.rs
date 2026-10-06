//! Ducking on DCS boards: the music's gain under another sound, measured in the emulated
//! audio, to check what the track programs say (`dcsrom::command_effects`).
//!
//! The extraction's `--check-ducking` pass uses `music_gain`. The diagnostic subcommands
//! (`dcs-effects`, `duck-fit`, `drift-check`, not in `--help`) give the same tools on files:
//! see docs/how-it-works.md, "Ducking, stops and channels".

use std::collections::BTreeMap;

use crate::dcsrom;
use crate::soundsdat::SoundsDat;

/// The fit of a recording of the music with another sound on top (MC) against the music
/// alone (M) and the sound alone (C), all three trimmed at their first sound.
pub struct Fit {
    /// Samples M was shifted by to line up with MC.
    pub music_lag: isize,
    /// Where C starts in MC, in seconds.
    pub sound_at: f64,
    /// Per window: start (s), the music's gain (dB), the sound's gain (dB, None where C is
    /// too quiet to fit), the residual relative to MC (dB).
    pub windows: Vec<(f64, f64, Option<f64>, f64)>,
}

/// Fits `MC ~ gm * M + gc * C` by least squares in windows of `win_s` seconds. `at` is
/// about when C was sent in MC (seconds); the exact place is found by correlation.
pub fn music_gain(m: &[f64], c: &[f64], mc: &[f64], rate: u32, at: f64, win_s: f64) -> Fit {
    let r = rate as f64;
    // Align M on MC over the part before C (a DCS frame or two of jitter at most). One
    // second is plenty for a lag of a few hundred samples.
    let pre = (((at - 0.1) * r).max(0.0) as usize).min((r as usize).max(1));
    let lag_m = best_lag(mc, m, 2000, pre.min(mc.len()));
    let len = mc.len();
    let m = shifted(m, lag_m, len);
    // C: its onset in MC is about `at`; find it against the residual.
    let resid: Vec<f64> = mc.iter().zip(&m).map(|(a, b)| a - b).collect();
    let guess = (at * r) as isize;
    let seg = (2.0 * r) as usize;
    let res_seg = shifted(&resid, guess, seg);
    let lag_c = best_lag(&res_seg, c, 3000, seg.min(c.len()));
    let c = shifted(c, -(guess - lag_c), len);
    let w = ((win_s * r) as usize).max(1);
    let db = |g: f64| 20.0 * g.abs().max(1e-6).log10();
    let dot = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(x, y)| x * y).sum::<f64>();
    let mut windows = Vec::new();
    let mut i = 0;
    while i + w <= len {
        let (a, b, y) = (&m[i..i + w], &c[i..i + w], &mc[i..i + w]);
        let (aa, bb, ab, ay, by) = (dot(a, a), dot(b, b), dot(a, b), dot(a, y), dot(b, y));
        let det = aa * bb - ab * ab;
        let (gm, gc) = if bb < 1e-3 * aa || det.abs() < 1e-12 {
            (ay / aa.max(1e-9), None)
        } else {
            ((ay * bb - by * ab) / det, Some((by * aa - ay * ab) / det))
        };
        let gcv = gc.unwrap_or(0.0);
        let e: f64 = (0..w)
            .map(|k| (y[k] - gm * a[k] - gcv * b[k]).powi(2))
            .sum();
        let yy = dot(y, y).max(1e-9);
        windows.push((
            i as f64 / r,
            db(gm),
            gc.map(db),
            10.0 * (e / yy).max(1e-12).log10(),
        ));
        i += w;
    }
    Fit {
        music_lag: lag_m,
        sound_at: (guess - lag_c) as f64 / r,
        windows,
    }
}

/// Best lag of `b` against `a` (b[i + lag] ~ a[i]) within +/- `max`, by cross-correlation
/// on the first `n` samples.
fn best_lag(a: &[f64], b: &[f64], max: isize, n: usize) -> isize {
    let mut best = (f64::MIN, 0);
    for lag in -max..=max {
        let mut acc = 0.0;
        for (i, &x) in a.iter().enumerate().take(n) {
            let j = i as isize + lag;
            if j >= 0 && (j as usize) < b.len() {
                acc += x * b[j as usize];
            }
        }
        if acc > best.0 {
            best = (acc, lag);
        }
    }
    best.1
}

fn shifted(x: &[f64], lag: isize, len: usize) -> Vec<f64> {
    (0..len)
        .map(|i| {
            let j = i as isize + lag;
            if j >= 0 && (j as usize) < x.len() {
                x[j as usize]
            } else {
                0.0
            }
        })
        .collect()
}

/// Interleaved samples to mono.
pub fn mono<T: Copy + Into<f64>>(s: &[T], ch: usize) -> Vec<f64> {
    let ch = ch.max(1);
    s.chunks_exact(ch)
        .map(|f| f.iter().map(|&v| v.into()).sum::<f64>() / ch as f64)
        .collect()
}

fn read_mono(path: &str) -> (Vec<f64>, u32) {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let spec = r.spec();
    let s: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
    (mono(&s, spec.channels as usize), spec.sample_rate)
}

/// `dcs-effects <region.bin> <rom> [--json FILE] [--max-secs S]`: what every command of a
/// DCS sound region (dumped with `--dump-sound-region`) does, one line per command, names
/// from the built-in sounds.dat section of `<rom>`.
pub fn dcs_effects(args: Vec<String>) {
    let mut it = args.into_iter();
    let Some(path) = it.next() else {
        eprintln!(
            "usage: rom2altsound dcs-effects <region.bin> <rom> [--json FILE] [--max-secs S]"
        );
        return;
    };
    let rom = it.next().unwrap_or_default();
    let (mut json, mut max_secs) = (None, dcsrom::EFFECTS_MAX_SECS);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = it.next(),
            "--max-secs" => max_secs = it.next().and_then(|s| s.parse().ok()).unwrap_or(max_secs),
            _ => {}
        }
    }
    let region = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let dat = SoundsDat::parse(crate::soundsdat::BUILT_IN);
    let names: BTreeMap<u16, String> = dat
        .game_entries(&rom, None)
        .into_iter()
        .filter(|e| e.bytes.len() == 2)
        .map(|e| (u16::from_be_bytes([e.bytes[0], e.bytes[1]]), e.name))
        .collect();
    let Some((count, tracks)) = dcsrom::tracks(&region) else {
        eprintln!("no DCS catalog");
        return;
    };
    let mut all = BTreeMap::new();
    for t in tracks {
        let Some(e) = dcsrom::command_effects(&region, t, max_secs) else {
            continue;
        };
        let ducks: Vec<String> = e
            .ducks
            .iter()
            .map(|d| {
                format!(
                    "ch{} {:+.2} dB ({:+}) from {:.2}s full {:.2}s, {}{}",
                    d.channel,
                    d.db,
                    d.units,
                    d.start_s,
                    d.full_s,
                    d.restore,
                    d.end_s.map_or(String::new(), |x| format!(
                        ", back at {x:.2}s (release {:.2}s)",
                        d.release_s
                    ))
                )
            })
            .collect();
        println!(
            "0x{t:04X} t{} ch{} streams {:?} stops {:?} deferred {:?} queues {:?} own {:?} len {} | {} | {}{}",
            e.track_type,
            e.channel,
            e.streams,
            e.stops,
            e.deferred,
            e.queues,
            e.own_level,
            e.length_s.map_or("-".into(), |x| format!("{x:.2}s")),
            ducks.join("; "),
            names.get(&t).map_or("", String::as_str),
            e.error
                .as_ref()
                .map_or(String::new(), |e| format!(" ERROR {e}")),
        );
        all.insert(format!("0x{t:04X}"), e);
    }
    println!("{count} catalog slots, {} populated", all.len());
    if let Some(j) = json {
        std::fs::write(&j, serde_json::to_string_pretty(&all).unwrap()).unwrap();
    }
}

/// `duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]`: M the music alone, C the
/// other sound alone, MC the music then C sent `at_secs` later (a scenario of `--only`,
/// `0x000C+3+0x01B6`; all three trimmed at their first sound by the extraction, so M and
/// MC start together). Prints the music's gain (dB) over time.
pub fn duck_fit(args: Vec<String>) {
    let mut pos = Vec::new();
    let mut win_s = 0.05;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--win" {
            win_s = it.next().and_then(|s| s.parse().ok()).unwrap_or(win_s);
        } else {
            pos.push(a);
        }
    }
    let [m, c, mc, at] = pos.as_slice() else {
        eprintln!("usage: rom2altsound duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]");
        return;
    };
    let at: f64 = at.parse().expect("at_secs");
    let (m, rate) = read_mono(m);
    let (c, _) = read_mono(c);
    let (mc, _) = read_mono(mc);
    let fit = music_gain(&m, &c, &mc, rate, at, win_s);
    println!(
        "aligned: music lag {} samples, sound at {:.4} s in MC",
        fit.music_lag, fit.sound_at
    );
    println!("t_s\tmusic_gain_db\tsound_gain_db\tresidual_db");
    for (t, gm, gc, res) in fit.windows {
        println!(
            "{t:.3}\t{gm:.2}\t{}\t{res:.1}",
            gc.map_or("-".into(), |g| format!("{g:.2}"))
        );
    }
}

/// `drift-check <A.wav> <B.wav> [--win S] [--max-lag N]`: two recordings of the same
/// command(s); per window, the lag of B that matches A best and the normalized correlation
/// there. Tells whether a board replays a command sample-exactly (DCS) or drifts.
pub fn drift_check(args: Vec<String>) {
    let (mut win_s, mut max_lag) = (0.25, 3000isize);
    let mut pos = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--win" => win_s = it.next().and_then(|s| s.parse().ok()).unwrap_or(win_s),
            "--max-lag" => max_lag = it.next().and_then(|s| s.parse().ok()).unwrap_or(max_lag),
            _ => pos.push(a),
        }
    }
    let [a, b] = pos.as_slice() else {
        eprintln!("usage: rom2altsound drift-check <A.wav> <B.wav> [--win S] [--max-lag N]");
        return;
    };
    let (a, rate) = read_mono(a);
    let (b, _) = read_mono(b);
    let w = (win_s * rate as f64) as usize;
    println!("t_s\tlag\tcorr");
    let mut i = 0;
    while i + w <= a.len().min(b.len()) {
        let x = &a[i..i + w];
        let xx: f64 = x.iter().map(|v| v * v).sum();
        let mut best = (f64::MIN, 0);
        for lag in -max_lag..=max_lag {
            let s = i as isize + lag;
            if s < 0 || s as usize + w > b.len() {
                continue;
            }
            let y = &b[s as usize..s as usize + w];
            let xy: f64 = x.iter().zip(y).map(|(p, q)| p * q).sum();
            let yy: f64 = y.iter().map(|v| v * v).sum();
            let c = xy / (xx * yy).sqrt().max(1e-9);
            if c > best.0 {
                best = (c, lag);
            }
        }
        println!("{:.2}\t{}\t{:.4}", i as f64 / rate as f64, best.1, best.0);
        i += w;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_finds_a_known_duck() {
        let rate = 8000;
        let n = 4 * rate as usize;
        let m: Vec<f64> = (0..n)
            .map(|i| ((i as f64 * 0.031).sin() + (i as f64 * 0.17).sin()) * 3000.0)
            .collect();
        let c: Vec<f64> = (0..rate as usize)
            .map(|i| (i as f64 * 0.53).sin() * 2000.0)
            .collect();
        // C sent at 2 s; the music at -6 dB while C plays.
        let at = 2 * rate as usize;
        let g = 10f64.powf(-6.0 / 20.0);
        let mc: Vec<f64> = (0..n)
            .map(|i| {
                let inside = (at..at + c.len()).contains(&i);
                m[i] * if inside { g } else { 1.0 } + if inside { c[i - at] } else { 0.0 }
            })
            .collect();
        let fit = music_gain(&m, &c, &mc, rate, 2.0, 0.05);
        assert_eq!(fit.music_lag, 0);
        assert!((fit.sound_at - 2.0).abs() < 0.001, "{}", fit.sound_at);
        let at_t = |t: f64| {
            fit.windows
                .iter()
                .find(|w| (w.0 - t).abs() < 0.026)
                .unwrap()
                .1
        };
        assert!(at_t(1.0).abs() < 0.01);
        assert!((at_t(2.5) + 6.0).abs() < 0.01, "{}", at_t(2.5));
    }
}
