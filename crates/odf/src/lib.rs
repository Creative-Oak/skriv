// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ODT reader/writer: "projection with preservation" (ADR 0006).
//!
//! [`read`] turns a `.odt` into a [`skriv_model::Document`] whose nodes
//! remember their source bytes. [`Odt::write`] projects the model back to ODT:
//! clean nodes are copied verbatim, dirty ones are regenerated, unknown
//! content rides in slots, and untouched package parts are copied raw.
//!
//! Assumption (recorded in the Spike 1 results): elements are recognised by
//! their conventional prefixes (`text:`, `table:`, `style:` …). We check that
//! each prefix we rely on is bound to the right namespace URI on the root
//! element, and refuse the file otherwise rather than misread it.

pub mod export;
pub mod import;
pub mod package;
pub mod xml;

use std::collections::HashMap;
use std::ops::Range;

use skriv_model::{Document, PartKind};

pub use export::{WriteOptions, WriteStats};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("xml: {0}")]
    Xml(String),
    #[error("not a text document: {0}")]
    NotText(String),
    #[error("namespace prefix {prefix} is bound to {found}, expected {expected}")]
    Namespace {
        prefix: String,
        found: String,
        expected: String,
    },
    #[error("unsupported: {0}")]
    Unsupported(String),
}

const NAMESPACES: &[(&str, &str)] = &[
    ("office", "urn:oasis:names:tc:opendocument:xmlns:office:1.0"),
    ("style", "urn:oasis:names:tc:opendocument:xmlns:style:1.0"),
    ("text", "urn:oasis:names:tc:opendocument:xmlns:text:1.0"),
    ("table", "urn:oasis:names:tc:opendocument:xmlns:table:1.0"),
    ("draw", "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"),
    (
        "fo",
        "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    ),
    ("xlink", "http://www.w3.org/1999/xlink"),
    ("dc", "http://purl.org/dc/elements/1.1/"),
    ("meta", "urn:oasis:names:tc:opendocument:xmlns:meta:1.0"),
    (
        "svg",
        "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    ),
];

fn check_namespaces(tree: &xml::Tree) -> Result<(), Error> {
    for (k, v) in tree.root().attrs() {
        if let Some(prefix) = k.strip_prefix("xmlns:")
            && let Some((_, expected)) = NAMESPACES.iter().find(|(p, _)| *p == prefix)
            && v != expected
        {
            return Err(Error::Namespace {
                prefix: prefix.to_owned(),
                found: v.clone(),
                expected: (*expected).to_owned(),
            });
        }
    }
    Ok(())
}

/// A part's source text and the byte ranges the model fills on write.
struct Source {
    text: String,
    holes: Vec<Hole>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum HoleKind {
    ContentAutoStyles,
    Body,
    NamedStyles,
    StylesAutoStyles,
    Meta,
}

struct Hole {
    kind: HoleKind,
    range: Range<usize>,
}

fn inner(tree: &xml::Tree, i: usize) -> Option<Range<usize>> {
    let n = tree.node(i);
    (!n.is_empty_element()).then_some(n.start_tag_end..n.end_tag_start)
}

/// A read document: the model plus what is needed to write it back faithfully.
pub struct Odt {
    pub doc: Document,
    package: package::Package,
    content: Source,
    styles: Option<Source>,
    meta: Option<Source>,
}

/// The result of a write.
pub struct Written {
    pub bytes: Vec<u8>,
    pub content_xml: String,
    pub styles_xml: Option<String>,
    pub meta_xml: Option<String>,
    pub stats: WriteStats,
}

const ENCRYPTED: &[u8] = b"manifest:encryption-data";

pub fn read(bytes: Vec<u8>) -> Result<Odt, Error> {
    let package = package::Package::open(bytes)?;
    if let Some(m) = package.read("META-INF/manifest.xml")?
        && m.windows(ENCRYPTED.len()).any(|w| w == ENCRYPTED)
    {
        return Err(Error::Unsupported("encrypted document".into()));
    }
    let content_text = package
        .read_string("content.xml")?
        .ok_or_else(|| Error::NotText("no content.xml".into()))?;
    let mut doc = Document::default();

    let tree = xml::parse(&content_text)?;
    check_namespaces(&tree)?;
    let body = tree
        .path(&["office:body", "office:text"])
        .ok_or_else(|| Error::NotText("no office:body/office:text".into()))?;
    let mut holes = Vec::new();
    {
        let imp = import::Importer {
            src: &content_text,
            tree: &tree,
            part: PartKind::Content,
        };
        if let Some(auto) = tree.child(tree.root, "office:automatic-styles")
            && let Some(r) = inner(&tree, auto)
        {
            doc.styles.content_automatic = imp.style_entries(auto);
            holes.push(Hole {
                kind: HoleKind::ContentAutoStyles,
                range: r,
            });
        }
        if let Some(r) = inner(&tree, body) {
            doc.body = imp.blocks(body);
            holes.push(Hole {
                kind: HoleKind::Body,
                range: r,
            });
        }
    }
    let content = Source {
        text: content_text,
        holes,
    };

    let styles = match package.read_string("styles.xml")? {
        None => None,
        Some(text) => {
            let tree = xml::parse(&text)?;
            check_namespaces(&tree)?;
            let imp = import::Importer {
                src: &text,
                tree: &tree,
                part: PartKind::Styles,
            };
            let mut holes = Vec::new();
            if let Some(s) = tree.child(tree.root, "office:styles")
                && let Some(r) = inner(&tree, s)
            {
                doc.styles.named = imp.style_entries(s);
                holes.push(Hole {
                    kind: HoleKind::NamedStyles,
                    range: r,
                });
            }
            if let Some(s) = tree.child(tree.root, "office:automatic-styles")
                && let Some(r) = inner(&tree, s)
            {
                doc.styles.styles_automatic = imp.style_entries(s);
                holes.push(Hole {
                    kind: HoleKind::StylesAutoStyles,
                    range: r,
                });
            }
            Some(Source { text, holes })
        }
    };

    let meta = match package.read_string("meta.xml")? {
        None => None,
        Some(text) => {
            let tree = xml::parse(&text)?;
            let imp = import::Importer {
                src: &text,
                tree: &tree,
                part: PartKind::Meta,
            };
            let mut holes = Vec::new();
            if let Some(m) = tree.child(tree.root, "office:meta")
                && let Some(r) = inner(&tree, m)
            {
                doc.meta.entries = imp.meta_entries(m);
                holes.push(Hole {
                    kind: HoleKind::Meta,
                    range: r,
                });
            }
            Some(Source { text, holes })
        }
    };

    Ok(Odt {
        doc,
        package,
        content,
        styles,
        meta,
    })
}

impl Odt {
    pub fn part_names(&self) -> &[String] {
        self.package.names()
    }

    /// The original text of a part as read.
    pub fn source(&self, part: PartKind) -> Option<&str> {
        match part {
            PartKind::Content => Some(&self.content.text),
            PartKind::Styles => self.styles.as_ref().map(|s| s.text.as_str()),
            PartKind::Meta => self.meta.as_ref().map(|s| s.text.as_str()),
        }
    }

    pub fn write(&mut self) -> Result<Written, Error> {
        self.write_with(WriteOptions::default())
    }

    pub fn write_with(&mut self, opts: WriteOptions) -> Result<Written, Error> {
        skriv_model::propagate_dirty(&mut self.doc);
        let styles_src = self.styles.as_ref().map_or("", |s| s.text.as_str());
        let meta_src = self.meta.as_ref().map_or("", |s| s.text.as_str());
        let mut stats = WriteStats::default();

        let mut part = |src: &Source| -> Result<String, Error> {
            let mut w = export::Writer::new(&self.content.text, styles_src, meta_src, opts);
            w.dialect = xml::Dialect::detect(&src.text);
            let mut pos = 0;
            for h in &src.holes {
                w.out.push_str(&src.text[pos..h.range.start]);
                match h.kind {
                    HoleKind::ContentAutoStyles => {
                        w.style_entries(&self.doc.styles.content_automatic)
                    }
                    HoleKind::Body => w.blocks(&self.doc.body),
                    HoleKind::NamedStyles => w.style_entries(&self.doc.styles.named),
                    HoleKind::StylesAutoStyles => {
                        w.style_entries(&self.doc.styles.styles_automatic)
                    }
                    HoleKind::Meta => w.meta_entries(&self.doc.meta.entries),
                }
                pos = h.range.end;
            }
            w.out.push_str(&src.text[pos..]);
            merge(&mut stats, w.stats);
            Ok(w.out)
        };

        let has = |s: &Source, k: HoleKind| s.holes.iter().any(|h| h.kind == k);
        let unplaced = |cond: bool, what: &str| -> Result<(), Error> {
            if cond {
                Err(Error::Unsupported(format!(
                    "{what} added to a document that had no place for it"
                )))
            } else {
                Ok(())
            }
        };
        unplaced(
            !self.doc.styles.content_automatic.is_empty()
                && !has(&self.content, HoleKind::ContentAutoStyles),
            "automatic styles",
        )?;
        unplaced(
            !self.doc.meta.entries.is_empty() && self.meta.is_none(),
            "metadata",
        )?;

        let content_xml = part(&self.content)?;
        let styles_xml = self.styles.as_ref().map(&mut part).transpose()?;
        let meta_xml = self.meta.as_ref().map(&mut part).transpose()?;

        let mut replace: HashMap<&str, Vec<u8>> = HashMap::new();
        if content_xml != self.content.text {
            replace.insert("content.xml", content_xml.clone().into_bytes());
        }
        if let (Some(s), Some(src)) = (&styles_xml, &self.styles)
            && *s != src.text
        {
            replace.insert("styles.xml", s.clone().into_bytes());
        }
        if let (Some(s), Some(src)) = (&meta_xml, &self.meta)
            && *s != src.text
        {
            replace.insert("meta.xml", s.clone().into_bytes());
        }
        let bytes = self.package.write(&replace)?;
        Ok(Written {
            bytes,
            content_xml,
            styles_xml,
            meta_xml,
            stats,
        })
    }
}

fn merge(into: &mut WriteStats, from: WriteStats) {
    into.verbatim_nodes += from.verbatim_nodes;
    into.verbatim_bytes += from.verbatim_bytes;
    into.regenerated_nodes += from.regenerated_nodes;
    into.passthrough_nodes += from.passthrough_nodes;
    into.stamped_nodes += from.stamped_nodes;
    into.slot_nodes += from.slot_nodes;
    into.slot_bytes += from.slot_bytes;
    for (k, v) in from.slot_names {
        *into.slot_names.entry(k).or_insert(0) += v;
    }
}

/// Mark every node dirty, so the whole document is regenerated from the model.
/// Combined with `WriteOptions { regenerate_text: true }` this tests how much
/// the model actually understands.
pub fn touch_all(doc: &mut Document) {
    skriv_model::for_each_node_mut(doc, &mut |_, m| m.dirty = true);
}

/// Give every paragraph, heading and section that lacks one an `xml:id`, by
/// patching start tags only. (Tables and frames also accept `xml:id` in ODF
/// 1.2+, but their child elements share a feature bucket, so they are left
/// out until the stamping is per element name.)
pub fn stamp_ids(doc: &mut Document) {
    use skriv_model::Feature::*;
    skriv_model::for_each_node_mut(doc, &mut |f, m| {
        if matches!(f, Paragraph | Heading | Section) && !m.id_from_source {
            m.stamp_id = true;
        }
    });
}

#[cfg(test)]
mod tests;
