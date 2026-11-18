// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The labelled corpus.
//!
//! One document per `.tsv` file, one paragraph per line:
//!
//! ```text
//! #default=da
//! #source=...
//! da<TAB>p<TAB>Tekst med et citat: {{en:"The quoted part."}} Og mere tekst.
//! en<TAB>h<TAB>A heading
//! ```
//!
//! The first column is the paragraph's language (what a careful person
//! would set as the paragraph language), the second its kind (`p`, `h`,
//! `title`, `li`, `td`, `quote`, `addr`, `code`, `ref`). `{{xx:...}}` marks a
//! passage inside the paragraph in the other language; the marks are
//! removed from the text and kept as spans.
//!
//! A page is 2,500 characters of paragraph text.

use std::ops::Range;
use std::path::Path;

use crate::lang::Lang;

pub const PAGE_CHARS: usize = 2500;

/// A passage in another language: (byte range, language).
pub type Span = (Range<usize>, Lang);

#[derive(Debug, Clone)]
pub struct Para {
    pub lang: Lang,
    pub kind: String,
    pub text: String,
    /// Passages in another language than the paragraph: (byte range, language).
    pub spans: Vec<Span>,
}

impl Para {
    pub fn heading(&self) -> bool {
        self.kind == "h" || self.kind == "title"
    }

    /// The language of the byte at `at`.
    pub fn lang_at(&self, at: usize) -> Lang {
        self.spans
            .iter()
            .find(|(r, _)| r.contains(&at))
            .map_or(self.lang, |(_, l)| *l)
    }
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub name: String,
    pub default: Lang,
    pub source: String,
    pub paras: Vec<Para>,
}

impl Doc {
    pub fn chars(&self) -> usize {
        self.paras.iter().map(|p| p.text.chars().count()).sum()
    }
}

/// Parse one paragraph's text, removing `{{xx:...}}` marks.
pub fn parse_spans(raw: &str) -> Result<(String, Vec<Span>), String> {
    let mut text = String::with_capacity(raw.len());
    let mut spans = Vec::new();
    let mut rest = raw;
    while let Some(i) = rest.find("{{") {
        text.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let colon = after.find(':').ok_or("span without language")?;
        let lang = Lang::from_code(&after[..colon]).ok_or("unknown span language")?;
        let body = &after[colon + 1..];
        let end = body.find("}}").ok_or("unclosed span")?;
        let start = text.len();
        text.push_str(&body[..end]);
        spans.push((start..text.len(), lang));
        rest = &body[end + 2..];
    }
    text.push_str(rest);
    Ok((text, spans))
}

pub fn parse_doc(name: &str, src: &str) -> Result<Doc, String> {
    let mut default = Lang::Da;
    let mut source = String::new();
    let mut paras = Vec::new();
    for (n, line) in src.lines().enumerate() {
        if let Some(v) = line.strip_prefix("#default=") {
            default = Lang::from_code(v.trim()).ok_or("bad default")?;
            continue;
        }
        if let Some(v) = line.strip_prefix("#source=") {
            source = v.to_string();
            continue;
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut cols = line.splitn(3, '\t');
        let (Some(l), Some(kind), Some(raw)) = (cols.next(), cols.next(), cols.next()) else {
            return Err(format!("{name}:{}: expected three columns", n + 1));
        };
        let lang = Lang::from_code(l).ok_or_else(|| format!("{name}:{}: language", n + 1))?;
        let (text, spans) = parse_spans(raw).map_err(|e| format!("{name}:{}: {e}", n + 1))?;
        paras.push(Para {
            lang,
            kind: kind.to_string(),
            text,
            spans,
        });
    }
    Ok(Doc {
        name: name.to_string(),
        default,
        source,
        paras,
    })
}

/// All `.tsv` documents in `dir`, sorted by name; empty if `dir` is missing.
pub fn load_dir(dir: &Path) -> Result<Vec<Doc>, String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(Vec::new());
    };
    let mut paths: Vec<_> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "tsv"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| {
            let name = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let src = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
            parse_doc(&name, &src)
        })
        .collect()
}

/// `crates/proofing/corpus`.
pub fn corpus_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// A seeded misspelling.
#[derive(Debug, Clone)]
pub struct Typo {
    pub lang: Lang,
    pub wrong: String,
    pub right: String,
    pub class: String,
}

/// `lang<TAB>wrong<TAB>right<TAB>class` lines.
pub fn parse_typos(src: &str) -> Result<Vec<Typo>, String> {
    src.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() != 4 {
                return Err(format!("bad typo line: {l}"));
            }
            Ok(Typo {
                lang: Lang::from_code(c[0]).ok_or("language")?,
                wrong: c[1].to_string(),
                right: c[2].to_string(),
                class: c[3].to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spans_and_documents() {
        let (t, s) = parse_spans("Han sagde: {{en:\"Hello there.\"}} Og gik.").unwrap();
        assert_eq!(t, "Han sagde: \"Hello there.\" Og gik.");
        assert_eq!(&t[s[0].0.clone()], "\"Hello there.\"");
        let d = parse_doc(
            "x",
            "#default=en\nda\th\tOverskrift\nen\tp\tText {{da:tekst}}.\n",
        )
        .unwrap();
        assert_eq!(d.default, Lang::En);
        assert_eq!(d.paras.len(), 2);
        assert!(d.paras[0].heading());
        assert_eq!(d.paras[1].lang_at(6), Lang::Da);
        assert_eq!(d.paras[1].lang_at(0), Lang::En);
    }

    #[test]
    fn committed_corpus_parses() {
        let docs = load_dir(&corpus_dir().join("handwritten")).unwrap();
        assert!(docs.len() >= 10);
        for d in &docs {
            assert!(!d.paras.is_empty(), "{}", d.name);
        }
        let typos =
            parse_typos(&std::fs::read_to_string(corpus_dir().join("typos.tsv")).unwrap()).unwrap();
        assert!(typos.iter().filter(|t| t.lang == Lang::Da).count() >= 100);
        assert!(typos.iter().filter(|t| t.lang == Lang::En).count() >= 50);
    }
}
