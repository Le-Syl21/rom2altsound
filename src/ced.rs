//! What a sound is, from a small audio tagger: CED (Consistent Ensemble Distillation, Xiaomi,
//! Apache-2.0), the "tiny" model, trained on AudioSet. It gives a probability for each of the
//! 527 AudioSet classes ("Music", "Speech", "Bell", "Explosion"...) and runs here with tract,
//! in pure Rust, on the CPU: the same file gives the same tags on every machine.
//!
//! The tags are a guess. They name a sound nobody named, and they pick its type for
//! g-sound.csv (music, callout, sfx) until a person says better (`--names`).
//!
//! `assets/ced-tiny.onnx` is the model of <https://huggingface.co/mispeech/ced-tiny> exported
//! to ONNX, with one change: the STFT gets a rank 3 input (an `Unsqueeze` before it) and the
//! shape annotations of the intermediate values are dropped, which tract needs to run it with
//! any length of sound. The probabilities match ONNX Runtime's to 3e-6.
//! `assets/audioset-labels.txt` is the class names of Google's AudioSet ontology (CC BY 4.0),
//! in the order of the model's outputs.

use std::path::Path;
use std::sync::OnceLock;

use tract_onnx::prelude::*;

use crate::altsound::Kind;

const MODEL: &[u8] = include_bytes!("../assets/ced-tiny.onnx");
const LABELS: &str = include_str!("../assets/audioset-labels.txt");
/// The model's input: mono, 16 kHz, in windows of at most 10 s.
const RATE: u32 = 16_000;
const WINDOW: usize = 10 * RATE as usize;
/// The model wants at least a second.
const MIN: usize = RATE as usize;
/// Classes that are a voice.
const SPEECH: [&str; 6] = [
    "Speech",
    "Speech synthesizer",
    "Male speech, man speaking",
    "Female speech, woman speaking",
    "Child speech, kid speaking",
    "Narration, monologue",
];
/// A sound at least this long that nothing marks as a voice is music.
const LONG_SECS: f64 = 20.0;
/// The probability from which a voice class makes a callout.
const SPEECH_MIN: f32 = 0.3;

type Model = std::sync::Arc<TypedRunnableModel>;

fn model() -> Option<&'static Model> {
    static M: OnceLock<Option<Model>> = OnceLock::new();
    M.get_or_init(|| {
        let m = tract_onnx::onnx().model_for_read(&mut &MODEL[..]).ok()?;
        let n = m.symbols.sym("audio_samples");
        let b = m.symbols.sym("batch_size");
        m.with_input_fact(0, f32::fact([b.to_dim(), n.to_dim()]).into())
            .ok()?
            .into_optimized()
            .ok()?
            .into_runnable()
            .ok()
    })
    .as_ref()
}

fn labels() -> Vec<&'static str> {
    LABELS.lines().collect()
}

/// What the tagger says about one sound.
#[derive(Debug, Clone, PartialEq)]
pub struct Tags {
    /// The most probable classes, most probable first: (name, probability 0 to 1).
    pub top: Vec<(String, f32)>,
    /// "Music".
    pub music: f32,
    /// The most probable of the voice classes.
    pub speech: f32,
}

impl Tags {
    /// The best class, as a short name for the sound ("Speech", "Bell").
    pub fn label(&self) -> &str {
        self.top.first().map_or("", |t| t.0.as_str())
    }

    /// The type of the sound for the CSVs. Music only means something for a sound that
    /// keeps playing (`looped`, or `continuous`), as in g-sound.csv every "music" sample
    /// loops; a voice makes a callout; everything else is an effect.
    pub fn kind(&self, secs: f64, looped: bool) -> Kind {
        if self.speech >= SPEECH_MIN && self.speech >= self.music {
            Kind::Callout
        } else if looped || secs >= LONG_SECS {
            Kind::Music
        } else {
            Kind::Sfx
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let r = |v: f32| (f64::from(v) * 1000.0).round() / 1000.0;
        serde_json::json!({
            "top": self.top.iter().map(|(n, p)| serde_json::json!([n, r(*p)])).collect::<Vec<_>>(),
            "music": r(self.music),
            "speech": r(self.speech),
        })
    }
}

/// Mono samples of a WAV file, and its rate.
fn read_mono(path: &Path) -> Option<(Vec<f32>, u32)> {
    let mut r = hound::WavReader::open(path).ok()?;
    let spec = r.spec();
    let ch = usize::from(spec.channels.max(1));
    let all: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>().ok()?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .ok()?
        }
    };
    let mono = all
        .chunks(ch)
        .map(|f| f.iter().sum::<f32>() / ch as f32)
        .collect();
    Some((mono, spec.sample_rate))
}

/// `x` at `from` Hz as 16 kHz: windowed-sinc interpolation, low-passed below the new Nyquist.
fn to_16k(x: &[f32], from: u32) -> Vec<f32> {
    if from == RATE {
        return x.to_vec();
    }
    let ratio = f64::from(from) / f64::from(RATE);
    let cutoff = (1.0 / ratio).min(1.0) * 0.95;
    let half = (8.0 * ratio.max(1.0)).ceil() as isize;
    let n = (x.len() as f64 / ratio).floor() as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 * ratio;
            let c = t.floor() as isize;
            let mut acc = 0.0f64;
            let mut norm = 0.0f64;
            for k in (c - half + 1)..=(c + half) {
                let d = k as f64 - t;
                let w = 0.5 + 0.5 * (std::f64::consts::PI * d / (half as f64)).cos();
                let a = std::f64::consts::PI * d * cutoff;
                let s = if a.abs() < 1e-9 { 1.0 } else { a.sin() / a };
                let h = w * s * cutoff;
                norm += h;
                if k >= 0 && (k as usize) < x.len() {
                    acc += h * f64::from(x[k as usize]);
                }
            }
            (acc / norm) as f32
        })
        .collect()
}

/// Tags a WAV file; `None` when the file cannot be read or the model cannot run.
pub fn tag(path: &Path) -> Option<Tags> {
    let (x, rate) = read_mono(path)?;
    let x = to_16k(&x, rate);
    let m = model()?;
    let mut best = vec![0f32; 527];
    let mut start = 0;
    while start < x.len().max(1) {
        let mut c = x[start..(start + WINDOW).min(x.len())].to_vec();
        // A short remainder is judged with the window before it, not alone.
        if c.len() < MIN {
            c.resize(MIN, 0.0);
        }
        start += WINDOW;
        let input = tract_ndarray::Array2::from_shape_vec((1, c.len()), c).ok()?;
        let out = m.run(tvec!(input.into_tensor().into())).ok()?;
        let p = out[0].to_plain_array_view::<f32>().ok()?;
        for (b, v) in best.iter_mut().zip(p.iter()) {
            *b = b.max(*v);
        }
    }
    let names = labels();
    if names.len() != best.len() {
        return None;
    }
    let mut order: Vec<usize> = (0..best.len()).collect();
    order.sort_by(|&a, &b| best[b].total_cmp(&best[a]).then(a.cmp(&b)));
    let at = |name: &str| {
        names
            .iter()
            .position(|n| *n == name)
            .map_or(0.0, |i| best[i])
    };
    Some(Tags {
        top: order
            .iter()
            .take(5)
            .map(|&i| (names[i].to_owned(), best[i]))
            .collect(),
        music: at("Music"),
        speech: SPEECH.iter().map(|n| at(n)).fold(0.0, f32::max),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_the_models_outputs() {
        let l = labels();
        assert_eq!(l.len(), 527);
        assert_eq!(l[0], "Speech");
        assert!(l.contains(&"Music"));
        for s in SPEECH {
            assert!(l.contains(&s), "{s}");
        }
    }

    #[test]
    fn resampling_keeps_a_tone() {
        // 1 kHz at 44.1 kHz comes out as 1 kHz at 16 kHz, about as loud.
        let x: Vec<f32> = (0..44100)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 44100.0).sin())
            .collect();
        let y = to_16k(&x, 44100);
        assert_eq!(y.len(), 16000);
        let want: Vec<f32> = (0..16000)
            .map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16000.0).sin())
            .collect();
        let err = y[200..15800]
            .iter()
            .zip(&want[200..15800])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(err < 0.02, "{err}");
    }

    #[test]
    fn kind_follows_the_voice_and_the_loop() {
        let t = |music: f32, speech: f32| Tags {
            top: vec![("x".into(), 0.5)],
            music,
            speech,
        };
        assert_eq!(t(0.1, 0.8).kind(1.0, false), Kind::Callout);
        assert_eq!(t(0.7, 0.1).kind(1.0, false), Kind::Sfx);
        assert_eq!(t(0.7, 0.1).kind(1.0, true), Kind::Music);
        assert_eq!(t(0.3, 0.1).kind(30.0, false), Kind::Music);
        // A voice that is also music (sung): the likelier wins.
        assert_eq!(t(0.9, 0.5).kind(30.0, false), Kind::Music);
    }
}
