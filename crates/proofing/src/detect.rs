// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Language detectors behind one trait.
//!
//! Each returns a language (or none) and a confidence in 0..=1. The scales
//! differ between detectors, so thresholds are tuned per detector.

use crate::lang::Lang;
use crate::spell::Speller;
use crate::text;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub lang: Option<Lang>,
    pub confidence: f64,
}

impl Detection {
    pub const NONE: Detection = Detection {
        lang: None,
        confidence: 0.0,
    };
}

pub trait Detector {
    fn name(&self) -> String;
    fn detect(&self, text: &str) -> Detection;
}

/// lingua-rs with Danish and English, optionally with Norwegian Bokmål and
/// Swedish as confusers (a paragraph detected as either is "not sure").
#[cfg(feature = "lingua")]
pub struct Lingua {
    inner: lingua::LanguageDetector,
    confusers: bool,
    low_accuracy: bool,
    /// Detect on [`text::mask_names`] of the paragraph.
    pub mask_names: bool,
}

#[cfg(feature = "lingua")]
impl Lingua {
    pub fn new(confusers: bool, low_accuracy: bool) -> Self {
        use lingua::Language::*;
        let langs: &[lingua::Language] = if confusers {
            #[cfg(feature = "lingua-confusers")]
            {
                &[Danish, English, Bokmal, Swedish]
            }
            #[cfg(not(feature = "lingua-confusers"))]
            panic!("built without the lingua-confusers feature")
        } else {
            &[Danish, English]
        };
        let mut b = lingua::LanguageDetectorBuilder::from_languages(langs);
        if low_accuracy {
            b.with_low_accuracy_mode();
        }
        Lingua {
            inner: b.build(),
            confusers,
            low_accuracy,
            mask_names: false,
        }
    }
}

#[cfg(feature = "lingua")]
impl Detector for Lingua {
    fn name(&self) -> String {
        format!(
            "lingua{}{}{}",
            if self.confusers {
                "-da-en-nb-sv"
            } else {
                "-da-en"
            },
            if self.low_accuracy { "-low" } else { "" },
            if self.mask_names { "+mask" } else { "" }
        )
    }

    fn detect(&self, text: &str) -> Detection {
        let masked;
        let text = if self.mask_names {
            masked = text::mask_names(text);
            &masked
        } else {
            text
        };
        let values = self.inner.compute_language_confidence_values(text);
        let Some(&(top, conf)) = values.first() else {
            return Detection::NONE;
        };
        if conf <= 0.0 {
            return Detection::NONE;
        }
        let lang = match top {
            lingua::Language::Danish => Some(Lang::Da),
            lingua::Language::English => Some(Lang::En),
            // Bokmål and Swedish, when built with them.
            #[allow(unreachable_patterns)]
            _ => None,
        };
        Detection {
            lang,
            confidence: conf,
        }
    }
}

/// whatlang, limited to Danish and English (or unrestricted, in which case
/// anything but Danish and English is "not sure").
#[cfg(feature = "whatlang")]
pub struct Whatlang {
    inner: whatlang::Detector,
    allowlist: bool,
    /// Detect on [`text::mask_names`] of the paragraph.
    pub mask_names: bool,
}

#[cfg(feature = "whatlang")]
impl Whatlang {
    pub fn new(allowlist: bool) -> Self {
        let inner = if allowlist {
            whatlang::Detector::with_allowlist(vec![whatlang::Lang::Dan, whatlang::Lang::Eng])
        } else {
            whatlang::Detector::new()
        };
        Whatlang {
            inner,
            allowlist,
            mask_names: false,
        }
    }
}

#[cfg(feature = "whatlang")]
impl Detector for Whatlang {
    fn name(&self) -> String {
        format!(
            "whatlang-{}{}",
            if self.allowlist { "da-en" } else { "all" },
            if self.mask_names { "+mask" } else { "" }
        )
    }

    fn detect(&self, text: &str) -> Detection {
        let masked;
        let text = if self.mask_names {
            masked = text::mask_names(text);
            &masked
        } else {
            text
        };
        match self.inner.detect(text) {
            Some(info) => Detection {
                lang: match info.lang() {
                    whatlang::Lang::Dan => Some(Lang::Da),
                    whatlang::Lang::Eng => Some(Lang::En),
                    _ => None,
                },
                confidence: info.confidence(),
            },
            None => Detection::NONE,
        }
    }
}

/// Two detectors that must agree. The language is theirs when they agree
/// (none otherwise), and the confidence the lower of the two.
pub struct Agree<A, B>(pub A, pub B);

impl<A: Detector, B: Detector> Detector for Agree<A, B> {
    fn name(&self) -> String {
        format!("{} & {}", self.0.name(), self.1.name())
    }

    fn detect(&self, text: &str) -> Detection {
        let (a, b) = (self.0.detect(text), self.1.detect(text));
        if a.lang.is_some() && a.lang == b.lang {
            Detection {
                lang: a.lang,
                confidence: a.confidence.min(b.confidence),
            }
        } else {
            Detection::NONE
        }
    }
}

/// Vote of the two spelling dictionaries: words only Danish knows against
/// words only English knows. Confidence is |da - en| / (da + en + 1), so
/// one word gives at most 0.5 and a long paragraph approaches 1.
pub struct DictVote<'a> {
    pub speller: &'a Speller,
}

impl DictVote<'_> {
    /// (words only in Danish, words only in English).
    pub fn counts(&self, text: &str) -> (usize, usize) {
        let (mut da, mut en) = (0, 0);
        for w in text::words(text) {
            if w.skip.is_some() {
                continue;
            }
            let d = self.speller.check(text, &w, &[Lang::Da]);
            let e = self.speller.check(text, &w, &[Lang::En]);
            match (d, e) {
                (true, false) => da += 1,
                (false, true) => en += 1,
                _ => {}
            }
        }
        (da, en)
    }
}

impl Detector for DictVote<'_> {
    fn name(&self) -> String {
        "dict-vote".into()
    }

    fn detect(&self, text: &str) -> Detection {
        let (da, en) = self.counts(text);
        let lang = match da.cmp(&en) {
            std::cmp::Ordering::Greater => Some(Lang::Da),
            std::cmp::Ordering::Less => Some(Lang::En),
            std::cmp::Ordering::Equal => None,
        };
        Detection {
            lang,
            confidence: da.abs_diff(en) as f64 / (da + en + 1) as f64,
        }
    }
}
