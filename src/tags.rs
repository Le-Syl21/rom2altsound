//! What a sound is, according to an audio tagger the program provides ([`set_tagger`]).
//!
//! The tags name a sound nobody named, and pick its type for g-sound.csv (music, callout,
//! sfx) until a person says better (`--names`). Without a tagger, the type comes from the
//! sound's name alone (a sound that keeps playing is music, a named voice a callout).
//!
//! The rom2altsound programs use CED (the `rom2altsound-ced` package of the repository): its
//! model is too large to ship in this library.

use std::path::Path;
use std::sync::OnceLock;

use crate::altsound::Kind;

/// A sound at least this long that nothing marks as a voice is music.
const LONG_SECS: f64 = 20.0;
/// The probability from which a voice class makes a callout.
const SPEECH_MIN: f32 = 0.3;

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
    pub(crate) fn kind(&self, secs: f64, looped: bool) -> Kind {
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

/// A tagger: the tags of a WAV file, `None` when it cannot say.
pub type Tagger = Box<dyn Fn(&Path) -> Option<Tags> + Send + Sync>;

static TAGGER: OnceLock<Tagger> = OnceLock::new();

/// Installs the audio tagger, once per process, before any extraction (in a program that
/// runs extraction workers, before [`crate::run_worker_if_requested`]: the packs are written
/// by the workers). Returns false when one is installed already.
pub fn set_tagger(t: Tagger) -> bool {
    TAGGER.set(t).is_ok()
}

/// The installed tagger's tags of a WAV file.
pub(crate) fn tag(path: &Path) -> Option<Tags> {
    TAGGER.get().and_then(|t| t(path))
}

#[cfg(test)]
mod tests {
    use super::*;

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
