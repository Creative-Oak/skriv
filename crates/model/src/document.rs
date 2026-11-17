// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The document, its metadata, and walking it.

use crate::content::*;
use crate::node::{Attrs, NodeMeta, Slot};
use crate::style::{ListLevelChild, StyleChild, StyleEntry, Styles};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// The children of `office:text`, in order.
    pub body: Vec<Block>,
    pub styles: Styles,
    pub meta: Meta,
}

/// `office:meta`. Standard entries (`dc:title`, `meta:user-defined`, …) are
/// kept as name + attributes + text; anything with element children is a slot.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub entries: Vec<MetaEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MetaEntry {
    Field(MetaField),
    Opaque(Slot),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetaField {
    pub meta: NodeMeta,
    pub name: String,
    pub attrs: Attrs,
    pub text: String,
}

impl Meta {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            MetaEntry::Field(f) if f.name == name => Some(f.text.as_str()),
            _ => None,
        })
    }

    pub fn user_defined(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|e| match e {
            MetaEntry::Field(f)
                if f.name == "meta:user-defined" && f.attrs.get("meta:name") == Some(key) =>
            {
                Some(f.text.as_str())
            }
            _ => None,
        })
    }

    pub fn set_user_defined(&mut self, key: &str, value: &str) {
        for e in &mut self.entries {
            if let MetaEntry::Field(f) = e
                && f.name == "meta:user-defined"
                && f.attrs.get("meta:name") == Some(key)
            {
                f.text = value.to_owned();
                f.meta.dirty = true;
                return;
            }
        }
        self.entries.push(MetaEntry::Field(MetaField {
            meta: NodeMeta::new(),
            name: "meta:user-defined".to_owned(),
            attrs: Attrs::from([("meta:name", key)]),
            text: value.to_owned(),
        }));
    }
}

/// Feature buckets, used by the fidelity report to score per feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Feature {
    Paragraph,
    Heading,
    List,
    Table,
    Section,
    Frame,
    Field,
    Comment,
    TrackedChange,
    Bookmark,
    Span,
    Style,
    ListStyle,
    Meta,
}

impl Feature {
    pub fn label(self) -> &'static str {
        match self {
            Feature::Paragraph => "paragraphs",
            Feature::Heading => "headings",
            Feature::List => "lists",
            Feature::Table => "tables",
            Feature::Section => "sections",
            Feature::Frame => "images/frames",
            Feature::Field => "fields",
            Feature::Comment => "comments",
            Feature::TrackedChange => "tracked changes",
            Feature::Bookmark => "bookmarks",
            Feature::Span => "spans/links",
            Feature::Style => "styles",
            Feature::ListStyle => "list styles",
            Feature::Meta => "metadata",
        }
    }
}

/// Visit every node's bookkeeping. The callback sees the node's feature, so a
/// caller can mark a sample dirty, stamp IDs, or count.
pub fn for_each_node_mut(doc: &mut Document, f: &mut dyn FnMut(Feature, &mut NodeMeta)) {
    for b in &mut doc.body {
        walk_block(b, f);
    }
    for list in [
        &mut doc.styles.named,
        &mut doc.styles.content_automatic,
        &mut doc.styles.styles_automatic,
    ] {
        for e in list.iter_mut() {
            match e {
                StyleEntry::Style(s) => {
                    f(Feature::Style, &mut s.meta);
                    for c in &mut s.children {
                        if let StyleChild::Props(p) = c {
                            f(Feature::Style, &mut p.meta);
                        }
                    }
                }
                StyleEntry::List(l) => {
                    f(Feature::ListStyle, &mut l.meta);
                    for lv in &mut l.levels {
                        if let ListLevelChild::Level(lv) = lv {
                            f(Feature::ListStyle, &mut lv.meta);
                        }
                    }
                }
                StyleEntry::Opaque(_) => {}
            }
        }
    }
    for e in &mut doc.meta.entries {
        if let MetaEntry::Field(m) = e {
            f(Feature::Meta, &mut m.meta);
        }
    }
}

fn walk_blocks(blocks: &mut [Block], f: &mut dyn FnMut(Feature, &mut NodeMeta)) {
    for b in blocks {
        walk_block(b, f);
    }
}

fn walk_block(b: &mut Block, f: &mut dyn FnMut(Feature, &mut NodeMeta)) {
    match b {
        Block::Paragraph(p) => {
            f(
                if p.heading {
                    Feature::Heading
                } else {
                    Feature::Paragraph
                },
                &mut p.meta,
            );
            walk_inlines(&mut p.content, f);
        }
        Block::List(l) => {
            f(Feature::List, &mut l.meta);
            for c in &mut l.children {
                if let ListChild::Item(i) = c {
                    f(Feature::List, &mut i.meta);
                    walk_blocks(&mut i.blocks, f);
                }
            }
        }
        Block::Table(t) => {
            f(Feature::Table, &mut t.meta);
            walk_table(&mut t.children, f);
        }
        Block::Section(s) => {
            f(Feature::Section, &mut s.meta);
            walk_blocks(&mut s.blocks, f);
        }
        Block::TrackedChanges(t) => {
            f(Feature::TrackedChange, &mut t.meta);
            for r in &mut t.children {
                if let RegionChild::Region(r) = r {
                    f(Feature::TrackedChange, &mut r.meta);
                    f(Feature::TrackedChange, &mut r.change.meta);
                    f(Feature::TrackedChange, &mut r.change.info.meta);
                    walk_blocks(&mut r.change.info.rest, f);
                    walk_blocks(&mut r.change.blocks, f);
                }
            }
        }
        Block::Opaque(_) => {}
    }
}

fn walk_table(children: &mut [TableChild], f: &mut dyn FnMut(Feature, &mut NodeMeta)) {
    for c in children {
        match c {
            TableChild::Column(col) => f(Feature::Table, &mut col.meta),
            TableChild::Row(r) => {
                f(Feature::Table, &mut r.meta);
                for c in &mut r.cells {
                    if let CellChild::Cell(c) = c {
                        f(Feature::Table, &mut c.meta);
                        walk_blocks(&mut c.blocks, f);
                    }
                }
            }
            TableChild::Group(g) => {
                f(Feature::Table, &mut g.meta);
                walk_table(&mut g.children, f);
            }
            TableChild::Opaque(_) => {}
        }
    }
}

fn walk_inlines(inlines: &mut [Inline], f: &mut dyn FnMut(Feature, &mut NodeMeta)) {
    for i in inlines {
        match i {
            Inline::Text(_) | Inline::Opaque(_) => {}
            Inline::Control(c) => f(Feature::Paragraph, &mut c.meta),
            Inline::Span(s) | Inline::Link(s) => {
                f(Feature::Span, &mut s.meta);
                walk_inlines(&mut s.content, f);
            }
            Inline::Field(fl) => {
                f(Feature::Field, &mut fl.meta);
                walk_inlines(&mut fl.display, f);
            }
            Inline::Bookmark(m) => f(Feature::Bookmark, &mut m.meta),
            Inline::ChangeMark(m) => f(Feature::TrackedChange, &mut m.meta),
            Inline::AnnotationEnd(m) => f(Feature::Comment, &mut m.meta),
            Inline::Annotation(a) => {
                f(Feature::Comment, &mut a.meta);
                walk_blocks(&mut a.body, f);
            }
            Inline::Frame(fr) => {
                f(Feature::Frame, &mut fr.meta);
                for c in &mut fr.children {
                    match c {
                        FrameChild::Image(im) => f(Feature::Frame, &mut im.meta),
                        FrameChild::TextBox(tb) => {
                            f(Feature::Frame, &mut tb.meta);
                            walk_blocks(&mut tb.blocks, f);
                        }
                        FrameChild::Title(l) | FrameChild::Desc(l) => {
                            f(Feature::Frame, &mut l.meta)
                        }
                        FrameChild::Opaque(_) => {}
                    }
                }
            }
        }
    }
}

/// Every paragraph and heading, depth first, including those inside lists,
/// tables, sections, frames and comments.
pub fn paragraphs_mut(doc: &mut Document) -> Vec<&mut Paragraph> {
    fn from_blocks<'a>(blocks: &'a mut [Block], out: &mut Vec<&'a mut Paragraph>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p),
                Block::List(l) => {
                    for c in &mut l.children {
                        if let ListChild::Item(i) = c {
                            from_blocks(&mut i.blocks, out);
                        }
                    }
                }
                Block::Table(t) => from_table(&mut t.children, out),
                Block::Section(s) => from_blocks(&mut s.blocks, out),
                _ => {}
            }
        }
    }
    fn from_table<'a>(children: &'a mut [TableChild], out: &mut Vec<&'a mut Paragraph>) {
        for c in children {
            match c {
                TableChild::Row(r) => {
                    for c in &mut r.cells {
                        if let CellChild::Cell(c) = c {
                            from_blocks(&mut c.blocks, out);
                        }
                    }
                }
                TableChild::Group(g) => from_table(&mut g.children, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    from_blocks(&mut doc.body, &mut out);
    out
}

/// Count of nodes per feature, for reports.
pub fn feature_counts(doc: &mut Document) -> std::collections::BTreeMap<Feature, usize> {
    let mut m = std::collections::BTreeMap::new();
    for_each_node_mut(doc, &mut |f, _| *m.entry(f).or_insert(0) += 1);
    m
}
