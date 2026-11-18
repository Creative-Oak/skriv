// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Model → XML, with preservation.
//!
//! A clean node with a source span is copied byte for byte. A dirty node gets
//! its own tags regenerated from the model, and its children are visited the
//! same way, so editing one paragraph regenerates that paragraph's tags and
//! nothing else. Slots are always copied verbatim.

use skriv_model::*;

use crate::xml::{Dialect, escape_attr, escape_para_text, escape_text};

/// Byte accounting for one write, used by the fidelity report to show how
/// much of the output was copied, regenerated, or carried in slots.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteStats {
    /// Clean nodes copied whole.
    pub verbatim_nodes: usize,
    pub verbatim_bytes: usize,
    /// Nodes whose own tags were rebuilt from the model.
    pub regenerated_nodes: usize,
    /// Clean nodes opened up because something inside them changed.
    pub passthrough_nodes: usize,
    /// Clean nodes whose start tag got an `xml:id` added.
    pub stamped_nodes: usize,
    pub slot_nodes: usize,
    pub slot_bytes: usize,
    /// Slot element names and counts: what the model doesn't understand yet.
    pub slot_names: std::collections::BTreeMap<String, usize>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WriteOptions {
    /// Ignore text-run source spans and re-escape all character data.
    pub regenerate_text: bool,
    /// Test knob: write regenerated start tags with their attributes in
    /// reverse order. Regenerated XML otherwise reproduces LibreOffice's
    /// serialisation exactly, which would make "untouched elements stay
    /// byte-identical" impossible to tell apart from "everything happened to
    /// come out the same". Never set this outside the fidelity harness.
    pub scramble_regenerated: bool,
}

enum Open {
    /// Nothing to close (an empty element written from source).
    Closed,
    /// Copy the original end tag `src[start..end]`.
    Source(usize, usize, PartKind),
    Regen {
        name: String,
        marker: usize,
        expanded: bool,
    },
}

pub struct Writer<'a> {
    pub content: &'a str,
    pub styles: &'a str,
    pub meta: &'a str,
    pub out: String,
    pub stats: WriteStats,
    pub opts: WriteOptions,
    /// Escaping conventions of the part being written.
    pub dialect: Dialect,
    /// Whether the paragraph content written so far ends in white space (or
    /// is empty), for encoding spaces in regenerated text.
    prev_space: bool,
}

impl<'a> Writer<'a> {
    pub fn new(content: &'a str, styles: &'a str, meta: &'a str, opts: WriteOptions) -> Self {
        Writer {
            content,
            styles,
            meta,
            out: String::new(),
            stats: WriteStats::default(),
            opts,
            dialect: Dialect::default(),
            prev_space: true,
        }
    }

    fn src(&self, part: PartKind) -> &'a str {
        match part {
            PartKind::Content => self.content,
            PartKind::Styles => self.styles,
            PartKind::Meta => self.meta,
        }
    }

    /// Copy a start tag from the source, inserting `xml:id` if asked to stamp.
    fn source_start_tag(&mut self, meta: &NodeMeta, o: Origin) {
        let src = self.src(o.part);
        let tag = &src[o.start..o.start_tag_end];
        if meta.stamp_id && !tag.contains("xml:id=") {
            let cut = if tag.ends_with("/>") {
                tag.len() - 2
            } else {
                tag.len() - 1
            };
            self.out.push_str(&tag[..cut]);
            self.out.push_str(" xml:id=\"");
            escape_attr(meta.id.as_str(), self.dialect, &mut self.out);
            self.out.push('"');
            self.out.push_str(&tag[cut..]);
            self.stats.stamped_nodes += 1;
        } else {
            self.out.push_str(tag);
        }
    }

    /// Start writing a node.
    ///
    /// - Clean, nothing changed below: copied whole; returns None and the
    ///   caller writes nothing more.
    /// - Clean itself but something below changed (or only an ID stamp):
    ///   original start tag, then the caller writes the children, then
    ///   [`Self::end`] writes the original end tag.
    /// - Dirty: start tag regenerated from the model.
    fn begin(&mut self, name: &str, attrs: &Attrs, meta: &NodeMeta) -> Option<Open> {
        if let Some(o) = meta.origin
            && !meta.dirty
        {
            let src = self.src(o.part);
            if !meta.descendants_dirty {
                if meta.stamp_id {
                    self.source_start_tag(meta, o);
                    self.out.push_str(&src[o.start_tag_end..o.end]);
                } else {
                    self.out.push_str(&src[o.start..o.end]);
                    self.stats.verbatim_nodes += 1;
                    self.stats.verbatim_bytes += o.end - o.start;
                }
                return None;
            }
            self.source_start_tag(meta, o);
            self.stats.passthrough_nodes += 1;
            if o.is_empty_element() {
                return Some(Open::Closed);
            }
            let end_tag = src[o.start_tag_end..o.end]
                .rfind("</")
                .map(|i| o.start_tag_end + i..o.end);
            return Some(match end_tag {
                Some(r) => Open::Source(r.start, r.end, o.part),
                None => Open::Closed,
            });
        }
        self.stats.regenerated_nodes += 1;
        self.out.push('<');
        self.out.push_str(name);
        let ordered: Vec<&Attr> = if self.opts.scramble_regenerated {
            // Reverse, but keep the pairs LibreOffice is order-sensitive to
            // in their required order: the scramble tests preservation, and
            // whether that table is complete, not LibreOffice's parser.
            let mut v: Vec<&Attr> = attrs.0.iter().rev().collect();
            for (first, then) in ORDER_SENSITIVE {
                let f = v.iter().position(|a| a.name == *first);
                let t = v.iter().position(|a| a.name == *then);
                if let (Some(f), Some(t)) = (f, t)
                    && f > t
                {
                    let a = v.remove(f);
                    v.insert(t, a);
                }
            }
            v
        } else {
            attrs.iter().collect()
        };
        for a in ordered {
            self.out.push(' ');
            self.out.push_str(&a.name);
            self.out.push_str("=\"");
            escape_attr(&a.value, self.dialect, &mut self.out);
            self.out.push('"');
        }
        if meta.stamp_id && attrs.get("xml:id").is_none() {
            self.out.push_str(" xml:id=\"");
            escape_attr(meta.id.as_str(), self.dialect, &mut self.out);
            self.out.push('"');
        }
        self.out.push('>');
        // Keep the original's spelling of an empty element: `<x/>` from
        // LibreOffice, `<x></x>` from some other producers.
        let expanded = meta.origin.is_some_and(|o| !o.is_empty_element());
        Some(Open::Regen {
            name: name.to_owned(),
            marker: self.out.len(),
            expanded,
        })
    }

    fn end(&mut self, open: Open) {
        match open {
            Open::Closed => {}
            Open::Source(s, e, part) => {
                let src = self.src(part);
                self.out.push_str(&src[s..e]);
            }
            Open::Regen {
                name,
                marker,
                expanded,
            } => {
                if self.out.len() == marker && !expanded {
                    self.out.pop();
                    self.out.push_str("/>");
                } else {
                    self.out.push_str("</");
                    self.out.push_str(&name);
                    self.out.push('>');
                }
            }
        }
    }

    fn slot(&mut self, s: &Slot) {
        self.out.push_str(&s.raw);
        if let Some(n) = &s.name {
            self.stats.slot_nodes += 1;
            self.stats.slot_bytes += s.raw.len();
            *self.stats.slot_names.entry(n.clone()).or_insert(0) += 1;
        }
    }

    /// An element with no children of its own in the model.
    fn leaf(&mut self, name: &str, attrs: &Attrs, meta: &NodeMeta) {
        if let Some(o) = self.begin(name, attrs, meta) {
            self.end(o);
        }
    }

    fn text_element(&mut self, name: &str, text: &str) {
        self.stats.regenerated_nodes += 1;
        self.out.push('<');
        self.out.push_str(name);
        self.out.push('>');
        escape_text(text, self.dialect, &mut self.out);
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push('>');
    }

    // ---------- blocks ----------

    pub fn blocks(&mut self, blocks: &[Block]) {
        for b in blocks {
            self.block(b);
        }
    }

    pub fn block(&mut self, b: &Block) {
        match b {
            Block::Paragraph(p) => {
                let name = if p.heading { "text:h" } else { "text:p" };
                // Paragraphs nest (comments, text boxes): keep the outer
                // paragraph's state.
                let outer = std::mem::replace(&mut self.prev_space, true);
                if let Some(o) = self.begin(name, &p.attrs, &p.meta) {
                    self.inlines(&p.content);
                    self.end(o);
                }
                self.prev_space = outer;
            }
            Block::List(l) => {
                if let Some(o) = self.begin("text:list", &l.attrs, &l.meta) {
                    for c in &l.children {
                        match c {
                            ListChild::Item(i) => {
                                let name = if i.header {
                                    "text:list-header"
                                } else {
                                    "text:list-item"
                                };
                                if let Some(oi) = self.begin(name, &i.attrs, &i.meta) {
                                    self.blocks(&i.blocks);
                                    self.end(oi);
                                }
                            }
                            ListChild::Opaque(s) => self.slot(s),
                        }
                    }
                    self.end(o);
                }
            }
            Block::Table(t) => {
                if let Some(o) = self.begin("table:table", &t.attrs, &t.meta) {
                    self.table_children(&t.children);
                    self.end(o);
                }
            }
            Block::Section(s) => {
                if let Some(o) = self.begin("text:section", &s.attrs, &s.meta) {
                    self.blocks(&s.blocks);
                    self.end(o);
                }
            }
            Block::TrackedChanges(t) => {
                if let Some(o) = self.begin("text:tracked-changes", &t.attrs, &t.meta) {
                    for c in &t.children {
                        match c {
                            RegionChild::Region(r) => self.region(r),
                            RegionChild::Opaque(s) => self.slot(s),
                        }
                    }
                    self.end(o);
                }
            }
            Block::Opaque(s) => self.slot(s),
        }
    }

    fn table_children(&mut self, children: &[TableChild]) {
        for c in children {
            match c {
                TableChild::Column(col) => self.leaf("table:table-column", &col.attrs, &col.meta),
                TableChild::Row(r) => {
                    if let Some(o) = self.begin("table:table-row", &r.attrs, &r.meta) {
                        for c in &r.cells {
                            match c {
                                CellChild::Cell(cell) => {
                                    let name = if cell.covered {
                                        "table:covered-table-cell"
                                    } else {
                                        "table:table-cell"
                                    };
                                    if let Some(oc) = self.begin(name, &cell.attrs, &cell.meta) {
                                        self.blocks(&cell.blocks);
                                        self.end(oc);
                                    }
                                }
                                CellChild::Opaque(s) => self.slot(s),
                            }
                        }
                        self.end(o);
                    }
                }
                TableChild::Group(g) => {
                    if let Some(o) = self.begin(&g.name, &g.attrs, &g.meta) {
                        self.table_children(&g.children);
                        self.end(o);
                    }
                }
                TableChild::Opaque(s) => self.slot(s),
            }
        }
    }

    fn region(&mut self, r: &ChangedRegion) {
        let Some(o) = self.begin("text:changed-region", &r.attrs, &r.meta) else {
            return;
        };
        let c = &r.change;
        if let Some(oc) = self.begin(c.kind.element(), &c.attrs, &c.meta) {
            let info = &c.info;
            if let Some(oi) = self.begin("office:change-info", &info.attrs, &info.meta) {
                if info.meta.dirty {
                    if let Some(a) = &info.author {
                        self.text_element("dc:creator", a);
                    }
                    if let Some(d) = &info.date {
                        self.text_element("dc:date", d);
                    }
                } else {
                    self.change_info_head(info);
                }
                self.blocks(&info.rest);
                self.end(oi);
            }
            self.blocks(&c.blocks);
            self.end(oc);
        }
        for s in &r.extra {
            self.slot(s);
        }
        self.end(o);
    }

    /// When a change-info is only passed through, its `dc:creator`/`dc:date`
    /// children are still the originals: copy them from source.
    fn change_info_head(&mut self, info: &ChangeInfo) {
        let Some(o) = info.meta.origin else { return };
        let src = self.src(o.part);
        let inner = &src[o.start_tag_end..o.end];
        let mut cut = 0;
        for tag in ["dc:creator", "dc:date"] {
            let close = format!("</{tag}>");
            if inner[cut..].starts_with(&format!("<{tag}"))
                && let Some(e) = inner[cut..].find(&close)
            {
                cut += e + close.len();
            }
        }
        self.out.push_str(&inner[..cut]);
    }

    // ---------- inlines ----------

    pub fn inlines(&mut self, inlines: &[Inline]) {
        for i in inlines {
            self.inline(i);
        }
    }

    fn inline(&mut self, i: &Inline) {
        match i {
            Inline::Text(t) => match t.origin {
                Some(o) => {
                    if self.opts.regenerate_text {
                        escape_text(&t.text, self.dialect, &mut self.out);
                    } else {
                        let src = self.src(o.part);
                        self.out.push_str(&src[o.start..o.end]);
                    }
                    self.note_written(i);
                }
                // Text typed or changed in skriv: its white space is meant
                // literally, so encode it so consumers don't collapse it.
                None => {
                    escape_para_text(&t.text, self.dialect, &mut self.prev_space, &mut self.out)
                }
            },
            Inline::Control(c) => {
                self.leaf(c.kind.element(), &c.attrs, &c.meta);
                self.note_written(i);
            }
            Inline::Span(s) | Inline::Link(s) => {
                let name = if matches!(i, Inline::Link(_)) {
                    "text:a"
                } else {
                    "text:span"
                };
                if let Some(o) = self.begin(name, &s.attrs, &s.meta) {
                    self.inlines(&s.content);
                    self.end(o);
                } else {
                    self.note_written(i);
                }
            }
            Inline::Field(f) => {
                if let Some(o) = self.begin(&f.name, &f.attrs, &f.meta) {
                    self.inlines(&f.display);
                    self.end(o);
                } else {
                    self.note_written(i);
                }
            }
            Inline::Bookmark(m) | Inline::ChangeMark(m) | Inline::AnnotationEnd(m) => {
                self.leaf(&m.name, &m.attrs, &m.meta)
            }
            Inline::Annotation(a) => {
                if let Some(o) = self.begin("office:annotation", &a.attrs, &a.meta) {
                    if a.meta.dirty {
                        if let Some(v) = &a.author {
                            self.text_element("dc:creator", v);
                        }
                        if let Some(v) = &a.date {
                            self.text_element("dc:date", v);
                        }
                        if let Some(v) = &a.initials {
                            self.text_element("meta:creator-initials", v);
                        }
                    } else {
                        self.annotation_head(a);
                    }
                    self.blocks(&a.body);
                    self.end(o);
                }
            }
            Inline::Frame(f) => {
                if let Some(o) = self.begin("draw:frame", &f.attrs, &f.meta) {
                    for c in &f.children {
                        match c {
                            FrameChild::Image(im) => {
                                if let Some(oi) = self.begin("draw:image", &im.attrs, &im.meta) {
                                    for s in &im.children {
                                        self.slot(s);
                                    }
                                    self.end(oi);
                                }
                            }
                            FrameChild::TextBox(tb) => {
                                if let Some(ot) = self.begin("draw:text-box", &tb.attrs, &tb.meta) {
                                    self.blocks(&tb.blocks);
                                    self.end(ot);
                                }
                            }
                            FrameChild::Title(l) | FrameChild::Desc(l) => {
                                let name = if matches!(c, FrameChild::Title(_)) {
                                    "svg:title"
                                } else {
                                    "svg:desc"
                                };
                                if let Some(ol) = self.begin(name, &l.attrs, &l.meta) {
                                    if l.meta.dirty {
                                        escape_text(&l.text, self.dialect, &mut self.out);
                                    }
                                    self.end(ol);
                                }
                            }
                            FrameChild::Opaque(s) => self.slot(s),
                        }
                    }
                    self.end(o);
                }
            }
            Inline::Opaque(s) => self.slot(s),
        }
    }

    /// Update [`Self::prev_space`] after `i` was written without going
    /// through [`escape_para_text`] (copied from source, or a control).
    fn note_written(&mut self, i: &Inline) {
        let mut s = String::new();
        i.push_plain_text(&mut s);
        if let Some(c) = s.chars().last() {
            self.prev_space = matches!(c, ' ' | '\t' | '\n' | '\r');
        }
    }

    /// The source bytes of a passed-through comment's metadata children.
    fn annotation_head(&mut self, a: &Annotation) {
        let Some(o) = a.meta.origin else { return };
        let src = self.src(o.part);
        let inner = &src[o.start_tag_end..o.end];
        let mut cut = 0;
        for tag in ["dc:creator", "dc:date", "meta:creator-initials"] {
            let close = format!("</{tag}>");
            if inner[cut..].starts_with(&format!("<{tag}"))
                && let Some(e) = inner[cut..].find(&close)
            {
                cut += e + close.len();
            }
        }
        self.out.push_str(&inner[..cut]);
    }

    // ---------- styles ----------

    pub fn style_entries(&mut self, entries: &[StyleEntry]) {
        for e in entries {
            match e {
                StyleEntry::Style(s) => {
                    let name = if s.default {
                        "style:default-style"
                    } else {
                        "style:style"
                    };
                    if let Some(o) = self.begin(name, &s.attrs, &s.meta) {
                        self.style_children(&s.children);
                        self.end(o);
                    }
                }
                StyleEntry::List(l) => {
                    let name = if l.outline {
                        "text:outline-style"
                    } else {
                        "text:list-style"
                    };
                    if let Some(o) = self.begin(name, &l.attrs, &l.meta) {
                        for lv in &l.levels {
                            match lv {
                                ListLevelChild::Level(lv) => {
                                    if let Some(ol) =
                                        self.begin(lv.kind.element(), &lv.attrs, &lv.meta)
                                    {
                                        self.style_children(&lv.children);
                                        self.end(ol);
                                    }
                                }
                                ListLevelChild::Opaque(s) => self.slot(s),
                            }
                        }
                        self.end(o);
                    }
                }
                StyleEntry::Opaque(s) => self.slot(s),
            }
        }
    }

    fn style_children(&mut self, children: &[StyleChild]) {
        for c in children {
            match c {
                StyleChild::Props(p) => {
                    if let Some(o) = self.begin(&p.name, &p.attrs, &p.meta) {
                        for s in &p.children {
                            self.slot(s);
                        }
                        self.end(o);
                    }
                }
                StyleChild::Opaque(s) => self.slot(s),
            }
        }
    }

    // ---------- meta ----------

    pub fn meta_entries(&mut self, entries: &[MetaEntry]) {
        for e in entries {
            match e {
                MetaEntry::Field(f) => {
                    if let Some(o) = self.begin(&f.name, &f.attrs, &f.meta) {
                        escape_text(&f.text, self.dialect, &mut self.out);
                        self.end(o);
                    }
                }
                MetaEntry::Opaque(s) => self.slot(s),
            }
        }
    }
}
