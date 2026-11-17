// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XML tree → model. Known elements become typed nodes that remember their
//! source span; anything else becomes a slot holding its exact source text.

use skriv_model::*;

use crate::xml::{Kind, Tree, unescape};

pub struct Importer<'a> {
    pub src: &'a str,
    pub tree: &'a Tree,
    pub part: PartKind,
}

const TABLE_GROUPS: &[&str] = &[
    "table:table-columns",
    "table:table-header-columns",
    "table:table-column-group",
    "table:table-header-rows",
    "table:table-rows",
    "table:table-row-group",
];

impl<'a> Importer<'a> {
    fn origin(&self, i: usize) -> Origin {
        let n = self.tree.node(i);
        Origin {
            part: self.part,
            start: n.start,
            end: n.end,
            start_tag_end: n.start_tag_end,
        }
    }

    fn attrs(&self, i: usize) -> Attrs {
        Attrs(
            self.tree
                .node(i)
                .attrs()
                .iter()
                .map(|(k, v)| Attr {
                    name: k.clone(),
                    value: v.clone(),
                })
                .collect(),
        )
    }

    fn meta(&self, i: usize) -> NodeMeta {
        NodeMeta::imported(self.origin(i), self.tree.node(i).attr("xml:id"))
    }

    pub fn slot(&self, i: usize) -> Slot {
        let n = self.tree.node(i);
        Slot::odf(n.name(), &self.src[n.start..n.end])
    }

    fn name(&self, i: usize) -> &str {
        self.tree.node(i).name().unwrap_or("")
    }

    fn children(&self, i: usize) -> &[usize] {
        self.tree.node(i).children()
    }

    // ---------- blocks ----------

    pub fn blocks(&self, parent: usize) -> Vec<Block> {
        self.children(parent)
            .iter()
            .map(|&c| self.block(c))
            .collect()
    }

    pub fn block(&self, i: usize) -> Block {
        match self.name(i) {
            "text:p" | "text:h" => Block::Paragraph(self.paragraph(i)),
            "text:list" => Block::List(self.list(i)),
            "table:table" => Block::Table(self.table(i)),
            "text:section" => Block::Section(Section {
                meta: self.meta(i),
                attrs: self.attrs(i),
                blocks: self.blocks(i),
            }),
            "text:tracked-changes" => Block::TrackedChanges(self.tracked_changes(i)),
            _ => Block::Opaque(self.slot(i)),
        }
    }

    fn paragraph(&self, i: usize) -> Paragraph {
        Paragraph {
            meta: self.meta(i),
            heading: self.name(i) == "text:h",
            attrs: self.attrs(i),
            content: self.inlines(i),
        }
    }

    fn list(&self, i: usize) -> List {
        let children = self
            .children(i)
            .iter()
            .map(|&c| match self.name(c) {
                n @ ("text:list-item" | "text:list-header") => ListChild::Item(ListItem {
                    meta: self.meta(c),
                    header: n == "text:list-header",
                    attrs: self.attrs(c),
                    blocks: self.blocks(c),
                }),
                _ => ListChild::Opaque(self.slot(c)),
            })
            .collect();
        List {
            meta: self.meta(i),
            attrs: self.attrs(i),
            children,
        }
    }

    fn table(&self, i: usize) -> Table {
        Table {
            meta: self.meta(i),
            attrs: self.attrs(i),
            children: self.table_children(i),
        }
    }

    fn table_children(&self, i: usize) -> Vec<TableChild> {
        self.children(i)
            .iter()
            .map(|&c| match self.name(c) {
                "table:table-column" => TableChild::Column(TableColumn {
                    meta: self.meta(c),
                    attrs: self.attrs(c),
                }),
                "table:table-row" => TableChild::Row(self.row(c)),
                n if TABLE_GROUPS.contains(&n) => TableChild::Group(TableGroup {
                    meta: self.meta(c),
                    name: n.to_owned(),
                    attrs: self.attrs(c),
                    children: self.table_children(c),
                }),
                _ => TableChild::Opaque(self.slot(c)),
            })
            .collect()
    }

    fn row(&self, i: usize) -> TableRow {
        let cells = self
            .children(i)
            .iter()
            .map(|&c| match self.name(c) {
                n @ ("table:table-cell" | "table:covered-table-cell") => {
                    CellChild::Cell(TableCell {
                        meta: self.meta(c),
                        covered: n == "table:covered-table-cell",
                        attrs: self.attrs(c),
                        blocks: self.blocks(c),
                    })
                }
                _ => CellChild::Opaque(self.slot(c)),
            })
            .collect();
        TableRow {
            meta: self.meta(i),
            attrs: self.attrs(i),
            cells,
        }
    }

    fn tracked_changes(&self, i: usize) -> TrackedChanges {
        let children = self
            .children(i)
            .iter()
            .map(|&c| match self.region(c) {
                Some(r) => RegionChild::Region(r),
                None => RegionChild::Opaque(self.slot(c)),
            })
            .collect();
        TrackedChanges {
            meta: self.meta(i),
            attrs: self.attrs(i),
            children,
        }
    }

    /// A changed region we can model has exactly one change element, first,
    /// whose first child is `office:change-info`. Anything else stays opaque.
    fn region(&self, i: usize) -> Option<ChangedRegion> {
        if self.name(i) != "text:changed-region" {
            return None;
        }
        let kids = self.children(i);
        let (&first, rest) = kids.split_first()?;
        let kind = ChangeKind::from_element(self.name(first))?;
        if rest
            .iter()
            .any(|&r| ChangeKind::from_element(self.name(r)).is_some())
        {
            return None;
        }
        let ckids = self.children(first);
        let (&info, after_info) = ckids.split_first()?;
        if self.name(info) != "office:change-info" {
            return None;
        }
        let mut author = None;
        let mut date = None;
        let mut info_rest = Vec::new();
        for &c in self.children(info) {
            match self.name(c) {
                "dc:creator" if author.is_none() && info_rest.is_empty() => {
                    author = Some(self.tree.text_only(self.src, c)?)
                }
                "dc:date" if date.is_none() && info_rest.is_empty() => {
                    date = Some(self.tree.text_only(self.src, c)?)
                }
                _ => info_rest.push(self.block(c)),
            }
        }
        Some(ChangedRegion {
            meta: self.meta(i),
            attrs: self.attrs(i),
            change: Change {
                meta: self.meta(first),
                kind,
                attrs: self.attrs(first),
                info: ChangeInfo {
                    meta: self.meta(info),
                    attrs: self.attrs(info),
                    author,
                    date,
                    rest: info_rest,
                },
                blocks: after_info.iter().map(|&c| self.block(c)).collect(),
            },
            extra: rest.iter().map(|&c| self.slot(c)).collect(),
        })
    }

    // ---------- inlines ----------

    pub fn inlines(&self, parent: usize) -> Vec<Inline> {
        self.children(parent)
            .iter()
            .map(|&c| self.inline(c))
            .collect()
    }

    fn mark(&self, i: usize, kind: MarkKind) -> Mark {
        Mark {
            meta: self.meta(i),
            name: self.name(i).to_owned(),
            kind,
            attrs: self.attrs(i),
        }
    }

    fn inline(&self, i: usize) -> Inline {
        let n = self.tree.node(i);
        match n.kind {
            Kind::Text => {
                return Inline::Text(Text {
                    origin: Some(self.origin(i)),
                    text: unescape(&self.src[n.start..n.end]),
                });
            }
            Kind::Other => return Inline::Opaque(self.slot(i)),
            Kind::Element { .. } => {}
        }
        let name = self.name(i);
        if let Some(kind) = ControlKind::from_element(name)
            && self.children(i).is_empty()
        {
            return Inline::Control(Control {
                meta: self.meta(i),
                kind,
                attrs: self.attrs(i),
            });
        }
        if let Some(kind) = Field::kind_of(name) {
            return Inline::Field(Field {
                meta: self.meta(i),
                name: name.to_owned(),
                kind,
                attrs: self.attrs(i),
                display: self.inlines(i),
            });
        }
        match name {
            "text:span" | "text:a" => {
                let s = Span {
                    meta: self.meta(i),
                    attrs: self.attrs(i),
                    content: self.inlines(i),
                };
                if name == "text:a" {
                    Inline::Link(s)
                } else {
                    Inline::Span(s)
                }
            }
            "text:bookmark" | "text:reference-mark" => {
                Inline::Bookmark(self.mark(i, MarkKind::Point))
            }
            "text:bookmark-start" | "text:reference-mark-start" => {
                Inline::Bookmark(self.mark(i, MarkKind::Start))
            }
            "text:bookmark-end" | "text:reference-mark-end" => {
                Inline::Bookmark(self.mark(i, MarkKind::End))
            }
            "text:change" => Inline::ChangeMark(self.mark(i, MarkKind::Point)),
            "text:change-start" => Inline::ChangeMark(self.mark(i, MarkKind::Start)),
            "text:change-end" => Inline::ChangeMark(self.mark(i, MarkKind::End)),
            "office:annotation-end" => Inline::AnnotationEnd(self.mark(i, MarkKind::End)),
            "office:annotation" => match self.annotation(i) {
                Some(a) => Inline::Annotation(a),
                None => Inline::Opaque(self.slot(i)),
            },
            "draw:frame" => Inline::Frame(self.frame(i)),
            _ => Inline::Opaque(self.slot(i)),
        }
    }

    /// Comments: `dc:creator`, `dc:date`, `meta:creator-initials` (in that
    /// order, each optional) and then body blocks.
    fn annotation(&self, i: usize) -> Option<Annotation> {
        let mut a = Annotation {
            meta: self.meta(i),
            attrs: self.attrs(i),
            author: None,
            date: None,
            initials: None,
            body: Vec::new(),
        };
        let mut stage = 0;
        for &c in self.children(i) {
            let name = self.name(c);
            let next = match name {
                "dc:creator" => 1,
                "dc:date" => 2,
                "meta:creator-initials" => 3,
                _ => 4,
            };
            if next < 4 {
                if next <= stage {
                    return None; // out of order: keep the whole comment verbatim
                }
                let t = self.tree.text_only(self.src, c)?;
                match next {
                    1 => a.author = Some(t),
                    2 => a.date = Some(t),
                    _ => a.initials = Some(t),
                }
                stage = next;
            } else {
                stage = 4;
                a.body.push(self.block(c));
            }
        }
        Some(a)
    }

    fn frame(&self, i: usize) -> Frame {
        let children = self
            .children(i)
            .iter()
            .map(|&c| match self.name(c) {
                "draw:image" => FrameChild::Image(Image {
                    meta: self.meta(c),
                    attrs: self.attrs(c),
                    children: self.children(c).iter().map(|&k| self.slot(k)).collect(),
                }),
                "draw:text-box" => FrameChild::TextBox(TextBox {
                    meta: self.meta(c),
                    attrs: self.attrs(c),
                    blocks: self.blocks(c),
                }),
                n @ ("svg:title" | "svg:desc") => match self.tree.text_only(self.src, c) {
                    Some(text) => {
                        let leaf = Leaf {
                            meta: self.meta(c),
                            attrs: self.attrs(c),
                            text,
                        };
                        if n == "svg:title" {
                            FrameChild::Title(leaf)
                        } else {
                            FrameChild::Desc(leaf)
                        }
                    }
                    None => FrameChild::Opaque(self.slot(c)),
                },
                _ => FrameChild::Opaque(self.slot(c)),
            })
            .collect();
        Frame {
            meta: self.meta(i),
            attrs: self.attrs(i),
            children,
        }
    }

    // ---------- styles ----------

    pub fn style_entries(&self, parent: usize) -> Vec<StyleEntry> {
        self.children(parent)
            .iter()
            .map(|&c| match self.name(c) {
                n @ ("style:style" | "style:default-style") => StyleEntry::Style(Style {
                    meta: self.meta(c),
                    default: n == "style:default-style",
                    attrs: self.attrs(c),
                    children: self.style_children(c),
                }),
                n @ ("text:list-style" | "text:outline-style") => StyleEntry::List(ListStyle {
                    meta: self.meta(c),
                    outline: n == "text:outline-style",
                    attrs: self.attrs(c),
                    levels: self
                        .children(c)
                        .iter()
                        .map(|&l| match ListLevelKind::from_element(self.name(l)) {
                            Some(kind) => ListLevelChild::Level(ListLevel {
                                meta: self.meta(l),
                                kind,
                                attrs: self.attrs(l),
                                children: self.style_children(l),
                            }),
                            None => ListLevelChild::Opaque(self.slot(l)),
                        })
                        .collect(),
                }),
                _ => StyleEntry::Opaque(self.slot(c)),
            })
            .collect()
    }

    fn style_children(&self, i: usize) -> Vec<StyleChild> {
        self.children(i)
            .iter()
            .map(|&c| {
                let name = self.name(c);
                if name.starts_with("style:") && name.ends_with("-properties") {
                    StyleChild::Props(PropertySet {
                        meta: self.meta(c),
                        name: name.to_owned(),
                        attrs: self.attrs(c),
                        children: self.children(c).iter().map(|&k| self.slot(k)).collect(),
                    })
                } else {
                    StyleChild::Opaque(self.slot(c))
                }
            })
            .collect()
    }

    // ---------- meta ----------

    pub fn meta_entries(&self, parent: usize) -> Vec<MetaEntry> {
        self.children(parent)
            .iter()
            .map(|&c| {
                let n = self.tree.node(c);
                match (&n.kind, self.tree.text_only(self.src, c)) {
                    (Kind::Element { name, .. }, Some(text)) => MetaEntry::Field(MetaField {
                        meta: self.meta(c),
                        name: name.clone(),
                        attrs: self.attrs(c),
                        text,
                    }),
                    _ => MetaEntry::Opaque(self.slot(c)),
                }
            })
            .collect()
    }
}
