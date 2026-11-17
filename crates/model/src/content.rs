// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Flow content: blocks and inlines.
//!
//! Structure is typed (a paragraph is a `Paragraph`, a table is a `Table`).
//! Properties stay in ODF's own vocabulary as ordered attributes, read through
//! typed accessors. Unknown children become `Opaque` slots in the position they
//! occupied, so a regenerated parent still emits them in order.

use crate::node::{Attrs, NodeId, NodeMeta, Slot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Paragraph(Paragraph),
    List(List),
    Table(Table),
    Section(Section),
    TrackedChanges(TrackedChanges),
    Opaque(Slot),
}

/// `text:p`, or `text:h` when `heading` is true.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub meta: NodeMeta,
    pub heading: bool,
    pub attrs: Attrs,
    pub content: Vec<Inline>,
}

impl Paragraph {
    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("text:style-name")
    }

    pub fn outline_level(&self) -> Option<u8> {
        if !self.heading {
            return None;
        }
        Some(
            self.attrs
                .get("text:outline-level")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1),
        )
    }

    /// The paragraph's text as a reader would see it (fields show their cached display).
    pub fn plain_text(&self) -> String {
        let mut s = String::new();
        for i in &self.content {
            i.push_plain_text(&mut s);
        }
        s
    }
}

/// `text:list`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct List {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub children: Vec<ListChild>,
}

impl List {
    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("text:style-name")
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ListChild {
    Item(ListItem),
    Opaque(Slot),
}

/// `text:list-item`, or `text:list-header` when `header` is true.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListItem {
    pub meta: NodeMeta,
    pub header: bool,
    pub attrs: Attrs,
    pub blocks: Vec<Block>,
}

/// `table:table`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub children: Vec<TableChild>,
}

impl Table {
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("table:name")
    }

    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("table:style-name")
    }

    /// Columns in order, with `table:number-columns-repeated` expanded.
    pub fn columns(&self) -> Vec<&TableColumn> {
        fn collect<'a>(children: &'a [TableChild], out: &mut Vec<&'a TableColumn>) {
            for c in children {
                match c {
                    TableChild::Column(col) => {
                        for _ in 0..col.repeat() {
                            out.push(col);
                        }
                    }
                    TableChild::Group(g) => collect(&g.children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.children, &mut out);
        out
    }

    pub fn rows(&self) -> Vec<&TableRow> {
        fn collect<'a>(children: &'a [TableChild], out: &mut Vec<&'a TableRow>) {
            for c in children {
                match c {
                    TableChild::Row(r) => out.push(r),
                    TableChild::Group(g) => collect(&g.children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.children, &mut out);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TableChild {
    Column(TableColumn),
    Row(TableRow),
    /// `table:table-columns`, `table:table-header-rows`, `table:table-rows` and friends.
    Group(TableGroup),
    Opaque(Slot),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableGroup {
    pub meta: NodeMeta,
    pub name: String,
    pub attrs: Attrs,
    pub children: Vec<TableChild>,
}

/// `table:table-column`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub meta: NodeMeta,
    pub attrs: Attrs,
}

impl TableColumn {
    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("table:style-name")
    }

    pub fn repeat(&self) -> usize {
        self.attrs
            .get("table:number-columns-repeated")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }
}

/// `table:table-row`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableRow {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub cells: Vec<CellChild>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CellChild {
    Cell(TableCell),
    Opaque(Slot),
}

/// `table:table-cell`, or `table:covered-table-cell` when `covered` is true.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub meta: NodeMeta,
    pub covered: bool,
    pub attrs: Attrs,
    pub blocks: Vec<Block>,
}

impl TableCell {
    pub fn column_span(&self) -> usize {
        self.attrs
            .get("table:number-columns-spanned")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }

    pub fn row_span(&self) -> usize {
        self.attrs
            .get("table:number-rows-spanned")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }
}

/// `text:section`: a named region with its own properties.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub blocks: Vec<Block>,
}

impl Section {
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("text:name")
    }

    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("text:style-name")
    }
}

/// `text:tracked-changes`: the change regions that marks in the flow point at.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackedChanges {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub children: Vec<RegionChild>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RegionChild {
    Region(ChangedRegion),
    Opaque(Slot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeKind {
    Insertion,
    Deletion,
    FormatChange,
}

impl ChangeKind {
    pub fn element(self) -> &'static str {
        match self {
            ChangeKind::Insertion => "text:insertion",
            ChangeKind::Deletion => "text:deletion",
            ChangeKind::FormatChange => "text:format-change",
        }
    }

    pub fn from_element(name: &str) -> Option<Self> {
        match name {
            "text:insertion" => Some(ChangeKind::Insertion),
            "text:deletion" => Some(ChangeKind::Deletion),
            "text:format-change" => Some(ChangeKind::FormatChange),
            _ => None,
        }
    }
}

/// `text:changed-region` with its single change element folded in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangedRegion {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub change: Change,
    /// Anything else inside the region (LibreOffice extensions, for example).
    pub extra: Vec<Slot>,
}

impl ChangedRegion {
    /// The ID that `text:change*` marks in the flow refer to.
    pub fn change_id(&self) -> Option<&str> {
        self.attrs.get("text:id").or_else(|| self.attrs.get("xml:id"))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub meta: NodeMeta,
    pub kind: ChangeKind,
    pub attrs: Attrs,
    pub info: ChangeInfo,
    /// For deletions: the deleted content, kept as real blocks so its history survives.
    pub blocks: Vec<Block>,
}

/// `office:change-info`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChangeInfo {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub author: Option<String>,
    pub date: Option<String>,
    /// The change comment paragraphs and anything unknown, in order.
    pub rest: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Inline {
    Text(Text),
    /// `text:s`, `text:tab`, `text:line-break`, `text:soft-page-break`.
    Control(Control),
    /// `text:span`.
    Span(Span),
    /// `text:a`.
    Link(Span),
    Field(Field),
    Bookmark(Mark),
    ChangeMark(Mark),
    Annotation(Annotation),
    /// `office:annotation-end`.
    AnnotationEnd(Mark),
    Frame(Frame),
    Opaque(Slot),
}

impl Inline {
    pub fn push_plain_text(&self, s: &mut String) {
        match self {
            Inline::Text(t) => s.push_str(&t.text),
            Inline::Control(c) => match c.kind {
                ControlKind::Space => {
                    for _ in 0..c.count() {
                        s.push(' ')
                    }
                }
                ControlKind::Tab => s.push('\t'),
                ControlKind::LineBreak => s.push('\n'),
                ControlKind::SoftPageBreak => {}
            },
            Inline::Span(sp) | Inline::Link(sp) => {
                for i in &sp.content {
                    i.push_plain_text(s)
                }
            }
            Inline::Field(f) => {
                for i in &f.display {
                    i.push_plain_text(s)
                }
            }
            _ => {}
        }
    }
}

/// Character data. `origin` lets an unchanged run inside a regenerated
/// paragraph keep its exact source spelling (entity choices and all).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    pub origin: Option<crate::node::Origin>,
    pub text: String,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        Text {
            origin: None,
            text: text.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControlKind {
    Space,
    Tab,
    LineBreak,
    SoftPageBreak,
}

impl ControlKind {
    pub fn element(self) -> &'static str {
        match self {
            ControlKind::Space => "text:s",
            ControlKind::Tab => "text:tab",
            ControlKind::LineBreak => "text:line-break",
            ControlKind::SoftPageBreak => "text:soft-page-break",
        }
    }

    pub fn from_element(name: &str) -> Option<Self> {
        match name {
            "text:s" => Some(ControlKind::Space),
            "text:tab" => Some(ControlKind::Tab),
            "text:line-break" => Some(ControlKind::LineBreak),
            "text:soft-page-break" => Some(ControlKind::SoftPageBreak),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub meta: NodeMeta,
    pub kind: ControlKind,
    pub attrs: Attrs,
}

impl Control {
    pub fn count(&self) -> usize {
        self.attrs
            .get("text:c")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub content: Vec<Inline>,
}

impl Span {
    pub fn style_name(&self) -> Option<&str> {
        self.attrs.get("text:style-name")
    }

    pub fn href(&self) -> Option<&str> {
        self.attrs.get("xlink:href")
    }
}

/// What a field computes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    PageNumber,
    PageCount,
    Date,
    Time,
    /// Caption numbering (`text:sequence`).
    Sequence,
    /// Cross-references (`text:bookmark-ref`, `text:reference-ref`, `text:sequence-ref`, `text:note-ref`).
    Reference,
    Chapter,
    DocumentInfo,
    Variable,
    Other,
}

/// A computed inline. The model's truth is what it points at and how it is
/// formatted (`target` + `format`). `display` is the rendered text the source
/// file cached; it is kept only so an untouched file round-trips, and is
/// recomputed by the layout engine, never trusted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub meta: NodeMeta,
    pub name: String,
    pub kind: FieldKind,
    pub attrs: Attrs,
    pub display: Vec<Inline>,
}

impl Field {
    pub fn kind_of(name: &str) -> Option<FieldKind> {
        use FieldKind::*;
        Some(match name {
            "text:page-number" => PageNumber,
            "text:page-count" => PageCount,
            "text:date" => Date,
            "text:time" => Time,
            "text:sequence" => Sequence,
            "text:bookmark-ref" | "text:reference-ref" | "text:sequence-ref" | "text:note-ref" => {
                Reference
            }
            "text:chapter" => Chapter,
            "text:title" | "text:subject" | "text:author-name" | "text:creator"
            | "text:file-name" | "text:word-count" | "text:character-count"
            | "text:paragraph-count" | "text:description" | "text:keywords"
            | "text:creation-date" | "text:modification-date" | "text:initial-creator" => {
                DocumentInfo
            }
            "text:variable-get" | "text:variable-set" | "text:user-field-get"
            | "text:user-defined" | "text:sequence-get" | "text:expression" => Variable,
            "text:placeholder" | "text:conditional-text" | "text:hidden-text"
            | "text:template-name" | "text:sender-firstname" | "text:page-variable-get" => Other,
            _ => return None,
        })
    }

    /// What the field points at, if anything: a bookmark name, a reference
    /// mark name or a sequence name.
    pub fn target(&self) -> Option<&str> {
        self.attrs
            .get("text:ref-name")
            .or_else(|| self.attrs.get("text:name"))
    }

    /// How the field formats its value.
    pub fn format(&self) -> Option<&str> {
        self.attrs
            .get("text:reference-format")
            .or_else(|| self.attrs.get("style:num-format"))
            .or_else(|| self.attrs.get("style:data-style-name"))
            .or_else(|| self.attrs.get("text:display"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkKind {
    Point,
    Start,
    End,
}

/// A zero-width marker: bookmark, change mark or annotation end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    pub meta: NodeMeta,
    pub name: String,
    pub kind: MarkKind,
    pub attrs: Attrs,
}

impl Mark {
    pub fn mark_name(&self) -> Option<&str> {
        self.attrs
            .get("text:name")
            .or_else(|| self.attrs.get("office:name"))
            .or_else(|| self.attrs.get("text:change-id"))
    }
}

/// `office:annotation`: a comment. `author`/`date`/`initials` are the
/// `dc:creator`, `dc:date` and `meta:creator-initials` children.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub author: Option<String>,
    pub date: Option<String>,
    pub initials: Option<String>,
    pub body: Vec<Block>,
}

impl Annotation {
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("office:name")
    }
}

/// `draw:frame`: a figure. Anchoring stays as ODF writes it for now; float
/// rules ("top, bottom, here") are a later layer over this.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub children: Vec<FrameChild>,
}

impl Frame {
    pub fn anchor(&self) -> Option<&str> {
        self.attrs.get("text:anchor-type")
    }

    pub fn size(&self) -> (Option<&str>, Option<&str>) {
        (self.attrs.get("svg:width"), self.attrs.get("svg:height"))
    }

    pub fn alt_text(&self) -> Option<&str> {
        self.children.iter().find_map(|c| match c {
            FrameChild::Title(t) | FrameChild::Desc(t) => Some(t.text.as_str()),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FrameChild {
    Image(Image),
    TextBox(TextBox),
    Title(Leaf),
    Desc(Leaf),
    Opaque(Slot),
}

/// `draw:image`. Embedded binary data and alternates stay as slots.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub children: Vec<Slot>,
}

impl Image {
    pub fn href(&self) -> Option<&str> {
        self.attrs.get("xlink:href")
    }
}

/// `draw:text-box`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBox {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub blocks: Vec<Block>,
}

/// An element that holds only text (`svg:title`, `svg:desc`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Leaf {
    pub meta: NodeMeta,
    pub attrs: Attrs,
    pub text: String,
}

/// Helper for code that needs the ID of whatever block it is holding.
impl Block {
    pub fn id(&self) -> Option<&NodeId> {
        match self {
            Block::Paragraph(p) => Some(&p.meta.id),
            Block::List(l) => Some(&l.meta.id),
            Block::Table(t) => Some(&t.meta.id),
            Block::Section(s) => Some(&s.meta.id),
            Block::TrackedChanges(t) => Some(&t.meta.id),
            Block::Opaque(_) => None,
        }
    }
}
