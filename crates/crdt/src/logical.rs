// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The logical document both CRDT backends store: what the CRDT has to carry,
//! independent of how each library encodes it.
//!
//! - Paragraphs are rich text: runs of characters with Peritext-style marks.
//!   Character styles and direct formatting are marks (`cs`, and one `Δ…`
//!   mark per ODF property). Inline objects the CRDT doesn't edit (fields,
//!   frames, notes, comments, bookmarks) are a single U+FFFC character with an
//!   `obj` mark holding the model inline as JSON.
//! - Paragraph properties are a map with one entry per ODF attribute (and one
//!   per direct-formatting property, `Δ<property-set>|<attr>`), so concurrent
//!   changes to different properties merge.
//! - Sections, lists and list items are containers with a property map.
//! - Tables are a grid keyed by row and column IDs, never by position, so the
//!   table stays rectangular whatever is inserted or deleted concurrently.
//! - Anything the CRDT carries but doesn't edit is an embed: a slot or a block
//!   as JSON, atomic, last writer wins.

use std::collections::BTreeMap;

/// One property per key. Keys starting with `@` are ours, `Δ` keys are
/// direct-formatting properties, everything else is an ODF attribute.
pub type Props = BTreeMap<String, String>;

/// The object replacement character: one inline object in the text.
pub const OBJ: char = '\u{FFFC}';

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Run {
    pub text: String,
    pub marks: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Para {
    pub id: String,
    pub props: Props,
    pub runs: Vec<Run>,
}

impl Para {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Merge adjacent runs with the same marks and drop empty ones, so two
    /// encodings of the same text compare equal.
    pub fn normalize(&mut self) {
        let mut out: Vec<Run> = Vec::with_capacity(self.runs.len());
        for r in self.runs.drain(..) {
            if r.text.is_empty() {
                continue;
            }
            match out.last_mut() {
                Some(last) if last.marks == r.marks => last.text.push_str(&r.text),
                _ => out.push(r),
            }
        }
        self.runs = out;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LNode {
    Para(Para),
    /// Carried, not edited: a slot, or a block kind the CRDT doesn't model.
    Embed {
        id: String,
        json: String,
    },
    Container {
        id: String,
        /// `section`, `list`, `item` (list item) or `header` (list header).
        kind: String,
        props: Props,
        children: Vec<LNode>,
    },
    Table(LTable),
}

impl LNode {
    pub fn id(&self) -> &str {
        match self {
            LNode::Para(p) => &p.id,
            LNode::Embed { id, .. } | LNode::Container { id, .. } => id,
            LNode::Table(t) => &t.id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LTable {
    pub id: String,
    pub props: Props,
    /// Column ID and properties (`@width` is the explicit width).
    pub cols: Vec<(String, Props)>,
    pub rows: Vec<(String, Props)>,
    /// `cells[row][col]`, always rectangular.
    pub cells: Vec<Vec<LCell>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LCell {
    pub id: String,
    pub props: Props,
    /// Paragraphs and embeds only.
    pub blocks: Vec<LNode>,
}

impl LCell {
    pub fn text(&self) -> String {
        let mut s = String::new();
        for b in &self.blocks {
            if let LNode::Para(p) = b {
                if !s.is_empty() {
                    s.push('\n');
                }
                s.push_str(&p.text());
            }
        }
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LDoc {
    pub body: Vec<LNode>,
    /// Style entries as JSON, keyed `<scope>:<index>`; atomic per entry.
    pub styles: BTreeMap<String, String>,
    /// `office:meta` as JSON.
    pub meta: String,
}

impl LDoc {
    pub fn normalize(&mut self) {
        fn nodes(v: &mut [LNode]) {
            for n in v {
                match n {
                    LNode::Para(p) => p.normalize(),
                    LNode::Container { children, .. } => nodes(children),
                    LNode::Table(t) => {
                        for row in &mut t.cells {
                            for c in row {
                                nodes(&mut c.blocks)
                            }
                        }
                    }
                    LNode::Embed { .. } => {}
                }
            }
        }
        nodes(&mut self.body);
    }

    /// Every paragraph, depth first, including those in tables.
    pub fn paras(&self) -> Vec<&Para> {
        fn walk<'a>(v: &'a [LNode], out: &mut Vec<&'a Para>) {
            for n in v {
                match n {
                    LNode::Para(p) => out.push(p),
                    LNode::Container { children, .. } => walk(children, out),
                    LNode::Table(t) => {
                        for row in &t.cells {
                            for c in row {
                                walk(&c.blocks, out)
                            }
                        }
                    }
                    LNode::Embed { .. } => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.body, &mut out);
        out
    }

    /// Top-level paragraphs only (not in containers or tables).
    pub fn find(&self, id: &str) -> Option<&LNode> {
        fn walk<'a>(v: &'a [LNode], id: &str) -> Option<&'a LNode> {
            for n in v {
                if n.id() == id {
                    return Some(n);
                }
                let found = match n {
                    LNode::Container { children, .. } => walk(children, id),
                    LNode::Table(t) => t.cells.iter().flatten().find_map(|c| walk(&c.blocks, id)),
                    _ => None,
                };
                if found.is_some() {
                    return found;
                }
            }
            None
        }
        walk(&self.body, id)
    }

    pub fn tables(&self) -> Vec<&LTable> {
        fn walk<'a>(v: &'a [LNode], out: &mut Vec<&'a LTable>) {
            for n in v {
                match n {
                    LNode::Table(t) => out.push(t),
                    LNode::Container { children, .. } => walk(children, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.body, &mut out);
        out
    }

    /// Plain text of the whole document, one line per paragraph, with
    /// containers and tables shown, for humans (merge review) and tests.
    pub fn outline(&self) -> String {
        fn walk(v: &[LNode], depth: usize, out: &mut String) {
            let pad = "  ".repeat(depth);
            for n in v {
                match n {
                    LNode::Para(p) => {
                        let text = p.text().replace(OBJ, "◆");
                        let h = if p.props.contains_key("@h") { "# " } else { "" };
                        out.push_str(&format!("{pad}{h}{text}\n"));
                    }
                    LNode::Embed { .. } => {}
                    LNode::Container {
                        kind,
                        props,
                        children,
                        ..
                    } => {
                        let name = props
                            .get("text:name")
                            .map(|n| format!(" \"{n}\""))
                            .unwrap_or_default();
                        out.push_str(&format!("{pad}[{kind}{name}]\n"));
                        walk(children, depth + 1, out);
                    }
                    LNode::Table(t) => {
                        out.push_str(&format!("{pad}[table {}×{}]\n", t.rows.len(), t.cols.len()));
                        for row in &t.cells {
                            let cells: Vec<String> = row
                                .iter()
                                .map(|c| c.text().replace('\n', " / ").replace(OBJ, "◆"))
                                .collect();
                            out.push_str(&format!("{pad}  | {} |\n", cells.join(" | ")));
                        }
                    }
                }
            }
        }
        let mut out = String::new();
        walk(&self.body, 0, &mut out);
        out
    }
}
