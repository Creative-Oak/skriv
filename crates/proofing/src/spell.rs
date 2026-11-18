// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spell checking with Hunspell-format dictionaries, through spellbook
//! (a Rust port of Nuspell's checker and suggester, MPL-2.0).

use std::ops::Range;
use std::path::Path;

use crate::lang::Lang;
use crate::text::{self, Word};

pub type Dictionary = spellbook::Dictionary;

/// Load a Hunspell `.aff`/`.dic` pair.
///
/// `da_DK.dic` (Stavekontrolden 2.9.101) lists three words with an
/// unescaped slash (`"A/S"`, `"c/o"`, `"I/S"`); Hunspell reads `S"` as an
/// unknown flag and carries on, spellbook refuses the file. Slashes whose
/// "flags" aren't valid numeric flags are escaped before parsing.
pub fn load_dictionary(aff: &str, dic: &str) -> Result<Dictionary, String> {
    let numeric = aff
        .lines()
        .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["FLAG", "num"]);
    let dic = if numeric {
        escape_bad_slashes(dic)
    } else {
        dic.to_string()
    };
    spellbook::Dictionary::new(aff, &dic).map_err(|e| format!("{e:?}"))
}

fn escape_bad_slashes(dic: &str) -> String {
    let mut out = String::with_capacity(dic.len() + 64);
    for line in dic.lines() {
        let end = line.find(['\t', ' ']).unwrap_or(line.len());
        let head = &line[..end];
        if let Some(i) = head.find('/') {
            let flags = &head[i + 1..];
            if !flags.chars().all(|c| c.is_ascii_digit() || c == ',') {
                out.push_str(&head[..i]);
                out.push_str("\\/");
                out.push_str(flags);
                out.push_str(&line[end..]);
                out.push('\n');
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Load `<dir>/<name>.aff` and `<dir>/<name>.dic`.
pub fn load_from_dir(dir: &Path, name: &str) -> Result<Dictionary, String> {
    let aff =
        std::fs::read_to_string(dir.join(format!("{name}.aff"))).map_err(|e| e.to_string())?;
    let dic =
        std::fs::read_to_string(dir.join(format!("{name}.dic"))).map_err(|e| e.to_string())?;
    load_dictionary(&aff, &dic)
}

/// A misspelled word in a paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    pub range: Range<usize>,
    pub word: String,
}

/// The Danish and English dictionaries.
pub struct Speller {
    dicts: [Dictionary; 2],
}

impl Speller {
    pub fn new(da: Dictionary, en: Dictionary) -> Self {
        Speller { dicts: [da, en] }
    }

    /// `da_DK` and `en_US` from [`crate::dicts_dir`], if fetched.
    pub fn from_default_dir() -> Option<Result<Self, String>> {
        let dir = crate::dicts_dir()?;
        Some((|| {
            Ok(Speller::new(
                load_from_dir(&dir, "da_DK")?,
                load_from_dir(&dir, "en_US")?,
            ))
        })())
    }

    pub fn dict(&self, lang: Lang) -> &Dictionary {
        &self.dicts[lang.index()]
    }

    /// Whether `word` is correct in `lang`. A word followed by a full stop
    /// is also tried with the stop, for abbreviations ("f.eks.", "jf.").
    pub fn check_word(&self, word: &str, dot: bool, lang: Lang) -> bool {
        let d = self.dict(lang);
        let word = word.trim_end_matches('\u{00AD}');
        if d.check(word) {
            return true;
        }
        if dot && d.check(&format!("{word}.")) {
            return true;
        }
        // Typographic apostrophe: en_US maps it with ICONV, da_DK doesn't.
        if word.contains('’') && d.check(&word.replace('’', "'")) {
            return true;
        }
        // Possessive of a name or abbreviation: "KL's", "Aarhus'".
        if lang == Lang::Da
            && let Some(stem) = word
                .strip_suffix("'s")
                .or_else(|| word.strip_suffix("’s"))
                .or_else(|| word.strip_suffix('\''))
            && !stem.is_empty()
            && d.check(stem)
        {
            return true;
        }
        false
    }

    /// Whether `w` (a word of `text`) is correct in any of `langs`.
    pub fn check(&self, text: &str, w: &Word, langs: &[Lang]) -> bool {
        let word = &text[w.range.clone()];
        langs.iter().any(|&l| self.check_word(word, w.dot, l))
    }

    /// Misspelled words of a paragraph, each word checked against the
    /// languages `langs_for(word)` returns (accepted if any accepts it).
    pub fn check_text_with<'a>(
        &self,
        text: &str,
        mut langs_for: impl FnMut(&Word) -> &'a [Lang],
    ) -> Vec<Flag> {
        text::words(text)
            .into_iter()
            .filter(|w| w.skip.is_none())
            .filter(|w| !self.check(text, w, langs_for(w)))
            .map(|w| Flag {
                word: text[w.range.clone()].to_string(),
                range: w.range,
            })
            .collect()
    }

    /// Misspelled words of a paragraph in one language.
    pub fn check_text(&self, text: &str, lang: Lang) -> Vec<Flag> {
        let langs = [lang];
        self.check_text_with(text, |_| &langs)
    }

    /// Suggestions for a misspelled word, best first.
    pub fn suggest(&self, word: &str, lang: Lang) -> Vec<String> {
        let mut out = Vec::new();
        self.dict(lang).suggest(word, &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nFLAG num\nTRY esrntliadkogmufbvphæøå\nSFX 1 Y 1\nSFX 1 0 n .\n";
    const DIC: &str = "4\nkommune/1\nhus\n\"A/S\"\nf.eks.\n";

    #[test]
    fn loads_a_dictionary_with_a_bare_slash() {
        let d = load_dictionary(AFF, DIC).expect("escaped");
        assert!(d.check("kommunen"));
        assert!(d.check("\"A/S\""));
        let en = load_dictionary("SET UTF-8\n", "1\nhouse\n").unwrap();
        let s = Speller::new(d, en);
        assert!(s.check_word("f.eks", true, Lang::Da));
        assert!(!s.check_word("f.eks", false, Lang::Da));
        assert!(s.check_word("house", false, Lang::En));
        let flags = s.check_text("Kommunen har et hus.", Lang::Da);
        assert_eq!(flags.len(), 2); // "har", "et": not in this toy dictionary
        assert!(
            s.suggest("komune", Lang::Da)
                .contains(&"kommune".to_string())
        );
    }
}
