// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Words and segments of a paragraph, for spell checking and detection.
//!
//! The word rules follow what Word does by default: words with digits,
//! words in UPPERCASE, and internet and file addresses are not checked.

use std::ops::Range;

/// Why a word is not spell checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// Contains a digit ("BR18", "3-udledningerne").
    Digits,
    /// Two or more letters, all uppercase ("GDPR", "ESDH").
    Caps,
    /// A single letter.
    Single,
    /// Part of an e-mail address, URL, path, file name, option or identifier.
    Address,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// Byte range in the paragraph.
    pub range: Range<usize>,
    /// The word is directly followed by a full stop, so it may be an
    /// abbreviation ("f.eks.", "jf.") that the dictionary lists with the stop.
    pub dot: bool,
    pub skip: Option<Skip>,
}

fn is_joiner(c: char) -> bool {
    matches!(c, '-' | '\'' | '’' | '.' | '\u{00AD}')
}

/// Quote characters that start or end a quoted segment.
fn is_quote(c: char) -> bool {
    matches!(c, '"' | '“' | '”' | '„' | '«' | '»')
}

/// Whether a whitespace-separated chunk is an address, path, file name,
/// command-line option or identifier rather than prose.
fn is_address(chunk: &str) -> bool {
    let t = chunk
        .trim_start_matches(|c: char| "([{\"'«“„".contains(c))
        .trim_end_matches(|c: char| ".,;:!?)]}\"'»”".contains(c));
    if t.is_empty() {
        return false;
    }
    if t.contains('@')
        || t.contains("://")
        || t.starts_with("www.")
        || t.contains('\\')
        || t.contains('_')
        || t.contains('%')
        || t.contains('=')
        || t.starts_with('/')
        || (t.starts_with('.')
            && t.chars()
                .nth(1)
                .is_some_and(|c| c.is_alphanumeric() || c == '/'))
        || t.starts_with('~')
        || t.starts_with('$')
        || t.starts_with("--")
        || t.matches('/').count() >= 2
    {
        return true;
    }
    // Domain or file name: "borger.dk", "config.toml", "skabelonserver.service".
    let segs: Vec<&str> = t.split('.').collect();
    if segs.len() >= 2
        && segs
            .iter()
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '-'))
    {
        let last = segs[segs.len() - 1];
        let first = segs[0];
        // A one-letter first segment is an abbreviation ("f.eks", "e.g").
        if (2..=7).contains(&last.len())
            && last.chars().all(|c| c.is_ascii_lowercase())
            && first.chars().count() > 1
            && !(segs.len() == 2 && first.chars().count() <= 3)
        {
            return true;
        }
    }
    false
}

/// The words of a paragraph, with the reason a word is skipped.
pub fn words(text: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut chunk_start = 0;
    for (i, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if c.is_whitespace() {
            if chunk_start < i {
                chunk_words(text, chunk_start..i, &mut out);
            }
            chunk_start = i + c.len_utf8();
        }
    }
    out
}

fn chunk_words(text: &str, chunk: Range<usize>, out: &mut Vec<Word>) {
    let s = &text[chunk.clone()];
    let address = is_address(s);
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].1.is_alphanumeric() {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + 1;
        while j < chars.len() {
            let c = chars[j].1;
            if c.is_alphanumeric() {
                j += 1;
            } else if is_joiner(c) && j + 1 < chars.len() && chars[j + 1].1.is_alphanumeric() {
                j += 2;
            } else {
                break;
            }
        }
        let b0 = chunk.start + chars[start].0;
        let b1 = chunk.start + chars.get(j).map_or(s.len(), |c| c.0);
        let word = &text[b0..b1];
        let dot = chars.get(j).is_some_and(|c| c.1 == '.');
        let letters = word.chars().filter(|c| c.is_alphabetic()).count();
        let skip = if address {
            Some(Skip::Address)
        } else if word.chars().any(|c| c.is_numeric()) {
            Some(Skip::Digits)
        } else if letters >= 2 && word.chars().all(|c| !c.is_lowercase()) {
            Some(Skip::Caps)
        } else if letters < 2 {
            Some(Skip::Single)
        } else {
            None
        };
        out.push(Word {
            range: b0..b1,
            dot,
            skip,
        });
        i = j;
    }
}

/// `text` without names: capitalised words that don't start a sentence,
/// and words with numbers. Character n-gram detectors read "Østerport" or
/// "Søren" as strong evidence for Danish, so an English paragraph about a
/// Danish place can come out Danish (lingua does this; see Spike 6).
pub fn mask_names(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut sentence_start = true;
    for chunk in text.split_whitespace() {
        let first = chunk.chars().find(|c| c.is_alphanumeric());
        let name = match first {
            Some(c) => {
                (c.is_uppercase() && !sentence_start) || chunk.chars().any(|c| c.is_numeric())
            }
            None => false,
        };
        if !name {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(chunk);
        }
        sentence_start = chunk.ends_with(['.', '!', '?', ':']);
    }
    out
}

/// Whether a paragraph has any word the spell checker would check.
pub fn proofable(text: &str) -> bool {
    words(text).iter().any(|w| w.skip.is_none())
}

/// Number of letters in `text`.
pub fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphabetic()).count()
}

/// Split a paragraph into segments: sentences, with quoted passages as
/// their own segments, so `sagde: "Fail fast, learn fast." Vi skal` is three.
pub fn segments(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut in_quote = false;
    for k in 0..chars.len() {
        let (i, c) = chars[k];
        let next = chars.get(k + 1).map(|x| x.1);
        let after = chars.get(k + 2).map(|x| x.1);
        let mut cut_before = None;
        let mut cut_after = None;
        if is_quote(c) {
            let prev = k.checked_sub(1).map(|p| chars[p].1);
            let opening = match c {
                '“' | '„' | '«' => true,
                '”' | '»' => false,
                _ => !in_quote && prev.is_none_or(|p| p.is_whitespace() || "([:".contains(p)),
            };
            if opening {
                in_quote = true;
                cut_before = Some(i);
            } else if in_quote {
                in_quote = false;
                cut_after = Some(i + c.len_utf8());
            }
        } else if matches!(c, '.' | '!' | '?')
            && next.is_some_and(char::is_whitespace)
            && after.is_some_and(|a| a.is_uppercase() || is_quote(a) || a.is_numeric())
            && !in_quote
        {
            cut_after = Some(i + 1);
        }
        for cut in [cut_before, cut_after].into_iter().flatten() {
            if cut > start && !text[start..cut].trim().is_empty() {
                out.push(start..cut);
            }
            if cut >= start {
                start = cut;
            }
        }
    }
    if start < text.len() && !text[start..].trim().is_empty() {
        out.push(start..text.len());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked(text: &str) -> Vec<&str> {
        words(text)
            .into_iter()
            .filter(|w| w.skip.is_none())
            .map(|w| &text[w.range])
            .collect()
    }

    #[test]
    fn words_and_skips() {
        assert_eq!(
            checked("Kommunen har modtaget din e-mail, jf. BR18 og GDPR."),
            ["Kommunen", "har", "modtaget", "din", "e-mail", "jf", "og"]
        );
        let w = words("jf. byggelovens § 16");
        assert!(w[0].dot);
        assert_eq!(
            checked("Skriv til hr@kommune.dk eller se borger.dk, config.toml og /opt/skabeloner."),
            ["Skriv", "til", "eller", "se", "og"]
        );
        assert_eq!(
            checked("pc'er og it-løsninger"),
            ["pc'er", "og", "it-løsninger"]
        );
        assert_eq!(checked("f.eks. og bl.a."), ["f.eks", "og", "bl.a"]);
        assert_eq!(checked("template_dir: mappen"), ["mappen"]);
    }

    #[test]
    fn masks_names() {
        assert_eq!(
            mask_names("It branches off the M3 line at Østerport. The south is at København H."),
            "It branches off the line at The south is at"
        );
        assert!(!proofable("K-01"));
        assert!(!proofable("20 11 34 56"));
        assert!(proofable("Høj"));
    }

    #[test]
    fn segments_split_quotes_and_sentences() {
        let t =
            "Projektlederen sagde: \"Fail fast, learn fast.\" Vi skal huske det. Næste sætning.";
        let s: Vec<&str> = segments(t).into_iter().map(|r| t[r].trim()).collect();
        assert_eq!(
            s,
            [
                "Projektlederen sagde:",
                "\"Fail fast, learn fast.\"",
                "Vi skal huske det.",
                "Næste sætning."
            ]
        );
    }
}
