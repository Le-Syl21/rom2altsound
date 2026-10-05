//! Ducking study (prototype): what each DCS command does to the OTHER channels, read from
//! its track program (`dcs-effects`), and the gain of a sound over time measured in the
//! emulated audio (`duck-fit`).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::dcsrom::{self, DB_PER_LEVEL, EventKind};
use crate::soundsdat::SoundsDat;

/// DCS frame, in seconds.
const FRAME: f64 = 240.0 / 31250.0;

/// A level change a command applies to a channel it does not play on.
#[derive(Serialize)]
struct Duck {
    channel: u8,
    /// Deepest level reached, in dB (`DB_PER_LEVEL` per unit).
    depth_db: f64,
    /// The operand units of that depth (negative: lower).
    depth_units: i32,
    /// When the change starts, and when the channel is back at 0 (None: still applied when
    /// the simulation stopped), in seconds from the command.
    start_s: f64,
    attack_s: f64,
    end_s: Option<f64>,
    /// How it ends: "fade" (a mixing fade back to 0, over `release_s`), "program end"
    /// (the contribution is dropped when the command's program ends or is replaced),
    /// "step" (an immediate change back), or "held".
    restore: String,
    release_s: f64,
}

#[derive(Serialize)]
struct Report {
    id: String,
    name: String,
    kind: u8,
    channel: u8,
    /// Channels it plays streams on.
    streams: Vec<u8>,
    /// Channels its program stops explicitly (opcode 02) without playing there.
    stops: Vec<u8>,
    /// Tracks it queues (opcode 03/05), with their channel.
    queues: Vec<(String, u8)>,
    /// Its own level on its home channel (first set), in operand units.
    self_level: Option<i8>,
    ducks: Vec<Duck>,
    /// Played alone, the program ended after this long (None: still running at the cap).
    length_s: Option<f64>,
    error: Option<String>,
}

pub fn dcs_effects(args: Vec<String>) {
    let mut it = args.into_iter();
    let Some(path) = it.next() else {
        eprintln!("usage: rom2altsound dcs-effects <region.bin> <rom> [--json FILE] [--max-secs S]");
        return;
    };
    let rom = it.next().unwrap_or_default();
    let (mut json, mut max_secs) = (None, 30.0);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = it.next(),
            "--max-secs" => max_secs = it.next().and_then(|s| s.parse().ok()).unwrap_or(30.0),
            _ => {}
        }
    }
    let region = std::fs::read(&path).expect("region");
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
    let max_frames = (max_secs / FRAME) as u32;
    let mut reports = Vec::new();
    for t in tracks {
        let Some(e) = dcsrom::track_effects(&region, t, max_frames) else {
            continue;
        };
        let home_of = |track: u16| {
            dcsrom::track_effects(&region, track, 1).map_or(255, |x| x.channel)
        };
        let streams = e.stream_channels();
        let mut stops: Vec<u8> = e
            .events
            .iter()
            .filter_map(|ev| match ev.what {
                EventKind::Stop { channel }
                    if channel != e.channel && streams & (1 << channel) == 0 =>
                {
                    Some(channel)
                }
                _ => None,
            })
            .collect();
        stops.dedup();
        let queues = e
            .events
            .iter()
            .filter_map(|ev| match ev.what {
                EventKind::Queue { track } => Some((format!("0x{track:04X}"), home_of(track))),
                _ => None,
            })
            .collect();
        let self_level = e.events.iter().find_map(|ev| match ev.what {
            EventKind::Mix { channel, mode: 0, param, .. } if channel == e.channel => Some(param),
            _ => None,
        });
        let mut ducks = Vec::new();
        for ch in 0..8u8 {
            if streams & (1 << ch) != 0 || ch == e.channel {
                continue;
            }
            let lv: Vec<i32> = e.levels.iter().map(|l| l[ch as usize]).collect();
            let Some(first) = lv.iter().position(|&v| v != 0) else {
                continue;
            };
            let (imin, &vmin) = lv
                .iter()
                .enumerate()
                .min_by_key(|&(_, v)| *v)
                .unwrap();
            let (imax, &vmax) = lv
                .iter()
                .enumerate()
                .max_by_key(|&(_, v)| *v)
                .unwrap();
            let (peak_i, peak) = if -vmin >= vmax { (imin, vmin) } else { (imax, vmax) };
            // Back to 0 after the peak.
            let end = lv[peak_i..].iter().position(|&v| v == 0).map(|k| k + peak_i);
            let release_start = lv[peak_i..]
                .iter()
                .position(|&v| v != peak)
                .map(|k| k + peak_i);
            let restore = match (end, release_start) {
                (None, _) => "held".to_string(),
                (Some(end), Some(rs)) if end > rs => "fade".to_string(),
                (Some(end), _) => {
                    // Dropped at once: by a mixing op, or the program's end resetting its
                    // contributions.
                    let by_op = e.events.iter().any(|ev| {
                        ev.frame + 1 == end as u32
                            && matches!(ev.what, EventKind::Mix { channel, .. } if channel == ch)
                    });
                    if by_op { "step" } else { "program end" }.to_string()
                }
            };
            ducks.push(Duck {
                channel: ch,
                depth_db: peak as f64 / 64.0 * DB_PER_LEVEL,
                depth_units: peak / 64,
                start_s: first as f64 * FRAME,
                attack_s: (peak_i - first) as f64 * FRAME,
                end_s: end.map(|x| x as f64 * FRAME),
                restore,
                release_s: match (end, release_start) {
                    (Some(end), Some(rs)) if end > rs => (end - rs) as f64 * FRAME,
                    _ => 0.0,
                },
            });
        }
        reports.push(Report {
            id: format!("0x{t:04X}"),
            name: names.get(&t).cloned().unwrap_or_default(),
            kind: e.kind,
            channel: e.channel,
            streams: (0..8).filter(|c| streams & (1 << c) != 0).collect(),
            stops,
            queues,
            self_level,
            ducks,
            length_s: e.ended.then(|| e.frames as f64 * FRAME),
            error: e.error.clone(),
        });
    }
    println!("{count} catalog slots, {} populated", reports.len());
    for r in &reports {
        let ducks: Vec<String> = r
            .ducks
            .iter()
            .map(|d| {
                format!(
                    "ch{} {:+.1} dB ({:+}) at {:.2}s attack {:.2}s, {} {}",
                    d.channel,
                    d.depth_db,
                    d.depth_units,
                    d.start_s,
                    d.attack_s,
                    d.restore,
                    match d.end_s {
                        Some(x) => format!("back at {x:.2}s (release {:.2}s)", d.release_s),
                        None => String::new(),
                    }
                )
            })
            .collect();
        println!(
            "{} t{} ch{} streams {:?} stops {:?} queues {:?} self {:?} len {} | {} | {}{}",
            r.id,
            r.kind,
            r.channel,
            r.streams,
            r.stops,
            r.queues,
            r.self_level,
            r.length_s.map_or("-".into(), |x| format!("{x:.2}s")),
            ducks.join("; "),
            r.name,
            r.error.as_ref().map_or(String::new(), |e| format!(" ERROR {e}")),
        );
    }
    if let Some(j) = json {
        std::fs::write(&j, serde_json::to_string_pretty(&reports).unwrap()).unwrap();
    }
}

fn read_mono(path: &str) -> (Vec<f64>, u32) {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let spec = r.spec();
    let ch = spec.channels as usize;
    let s: Vec<i16> = r.samples::<i16>().filter_map(Result::ok).collect();
    let x = s
        .chunks_exact(ch)
        .map(|f| f.iter().map(|&v| v as f64).sum::<f64>() / ch as f64)
        .collect();
    (x, spec.sample_rate)
}

/// Best lag of `b` against `a` (b[i + lag] ~ a[i]) within +/- `max`, by cross-correlation
/// on the first `n` samples.
fn best_lag(a: &[f64], b: &[f64], max: isize, n: usize) -> isize {
    let mut best = (f64::MIN, 0);
    for lag in -max..=max {
        let mut acc = 0.0;
        for i in 0..n.min(a.len()) {
            let j = i as isize + lag;
            if j >= 0 && (j as usize) < b.len() {
                acc += a[i] * b[j as usize];
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
            if j >= 0 && (j as usize) < x.len() { x[j as usize] } else { 0.0 }
        })
        .collect()
}

/// `duck-fit <M.wav> <C.wav> <MC.wav> <at_secs> [--win S]`: M the music alone, C the
/// other sound alone, MC the music then C sent `at_secs` later (all three trimmed at their
/// first sound by the extraction, so M and MC start together). Fits, in short windows,
/// MC ~ gm * M + gc * C and prints gm (the music's gain, dB) over time.
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
    let at: f64 = at.parse().unwrap();
    let (m, rate) = read_mono(m);
    let (c, _) = read_mono(c);
    let (mc, _) = read_mono(mc);
    let r = rate as f64;
    // Align M on MC over the part before C (a DCS frame or two of boot jitter).
    let pre = ((at - 0.1) * r) as usize;
    let lag_m = best_lag(&mc, &m, 2000, pre.min(mc.len()));
    let len = mc.len();
    let m = shifted(&m, lag_m, len);
    // C: its onset in MC is about `at` (+ a frame or two); find it against the residual.
    let resid: Vec<f64> = mc.iter().zip(&m).map(|(a, b)| a - b).collect();
    let guess = (at * r) as isize;
    let seg = (2.0 * r) as usize;
    let res_seg = shifted(&resid, guess, seg);
    let lag_c = best_lag(&res_seg, &c, 3000, seg.min(c.len())) ;
    let c = shifted(&c, -(guess - lag_c), len);
    println!(
        "aligned: music lag {lag_m} samples, sound at {:.4} s in MC",
        (guess - lag_c) as f64 / r
    );
    let w = (win_s * r) as usize;
    println!("t_s\tmusic_gain_db\tsound_gain_db\tresidual_db\tmusic_rms_db");
    let mut i = 0;
    while i + w <= len {
        let (a, b, y) = (&m[i..i + w], &c[i..i + w], &mc[i..i + w]);
        let dot = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(x, y)| x * y).sum::<f64>();
        let (aa, bb, ab, ay, by) = (dot(a, a), dot(b, b), dot(a, b), dot(a, y), dot(b, y));
        let det = aa * bb - ab * ab;
        let db = |g: f64| 20.0 * g.abs().max(1e-6).log10();
        let (gm, gc) = if bb < 1e-3 * aa || det.abs() < 1e-12 {
            (ay / aa.max(1e-9), f64::NAN)
        } else {
            ((ay * bb - by * ab) / det, (by * aa - ay * ab) / det)
        };
        let gc_v = if gc.is_nan() { 0.0 } else { gc };
        let e: f64 = (0..w).map(|k| (y[k] - gm * a[k] - gc_v * b[k]).powi(2)).sum();
        let yy = dot(y, y).max(1e-9);
        println!(
            "{:.3}\t{:.2}\t{}\t{:.1}\t{:.1}",
            i as f64 / r,
            db(gm),
            if gc.is_nan() { "-".into() } else { format!("{:.2}", db(gc)) },
            10.0 * (e / yy).max(1e-12).log10(),
            10.0 * (aa / w as f64).max(1e-9).log10() - 90.3,
        );
        i += w;
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
