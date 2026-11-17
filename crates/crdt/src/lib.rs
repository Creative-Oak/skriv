// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 3: the skriv document model in a CRDT.
//!
//! One trait, [`CrdtDoc`], with two implementations behind cargo features:
//!
//! - [`loro_backend::LoroDocument`] (feature `loro`): a **movable tree** of
//!   blocks. Every paragraph, section, list, table and cell is a tree node;
//!   each paragraph node owns a rich text with Peritext marks.
//! - [`automerge_backend::AutomergeDocument`] (feature `automerge`): **rich
//!   text with block markers**, Automerge's own model (2.2+). Each flow (the
//!   body, each table cell) is one text; a block marker starts each
//!   paragraph and carries its properties and its `parents` (enclosing
//!   sections and lists). Tables are objects referenced from a marker.
//!
//! Both store the same logical document ([`logical::LDoc`]), so every test runs
//! against both. [`codec`] maps the model to and from that logical document.

pub mod codec;
pub mod logical;
pub mod workload;

#[cfg(feature = "automerge")]
pub mod automerge_backend;
#[cfg(feature = "loro")]
pub mod loro_backend;

use logical::{LCell, LDoc, LNode, LTable, Para, Props};

/// A peer (replica) identity.
pub type Peer = u64;

/// The skriv document as a CRDT. Edits address paragraphs, tables, rows and
/// columns by stable ID; positions inside a paragraph count Unicode scalar
/// values, with each inline object counting as one.
pub trait CrdtDoc: Sized {
    const NAME: &'static str;

    /// A new document holding `doc`, created by `peer`.
    fn create(doc: &LDoc, peer: Peer) -> Self;
    /// The whole current state.
    fn read(&self) -> LDoc;
    /// A branch: a copy with its own peer that can diverge and merge back.
    fn fork(&mut self, peer: Peer) -> Self;
    /// Bring in everything `other` has that we don't.
    fn merge(&mut self, other: &mut Self);
    /// Everything, history included.
    fn save(&mut self) -> Vec<u8>;
    fn load(bytes: &[u8], peer: Peer) -> Self;
    /// End the current transaction. Backends also commit before merging and saving.
    fn commit(&mut self);

    /// Record who this peer is, so attribution can name them.
    fn set_author(&mut self, name: &str);
    /// Who inserted the character at `pos` in paragraph `pid`.
    fn author_at(&self, pid: &str, pos: usize) -> Option<String>;

    fn para_len(&self, pid: &str) -> Option<usize>;
    /// The current text of a paragraph, as runs.
    fn para(&self, pid: &str) -> Option<Para>;
    fn insert_text(&mut self, pid: &str, pos: usize, text: &str) -> bool;
    fn delete_text(&mut self, pid: &str, pos: usize, len: usize) -> bool;
    /// Set (or with `None` remove) a mark on `start..end`.
    fn mark(&mut self, pid: &str, start: usize, end: usize, key: &str, value: Option<&str>)
    -> bool;
    fn set_para_prop(&mut self, pid: &str, key: &str, value: Option<&str>) -> bool;
    /// Press Enter at `pos`: the text after it becomes paragraph `new_id`.
    fn split_para(&mut self, pid: &str, pos: usize, new_id: &str) -> bool;
    /// Insert a new paragraph after block `after` (a sibling at any level).
    fn insert_para_after(&mut self, after: &str, para: &Para) -> bool;
    /// Delete a paragraph, section or table.
    fn delete_block(&mut self, id: &str) -> bool;
    /// Move a block (typically a section) to just after block `after`.
    fn move_block_after(&mut self, id: &str, after: &str) -> bool;
    /// A container's own properties (`text:name` of a section, say).
    fn set_container_prop(&mut self, id: &str, key: &str, value: Option<&str>) -> bool;

    fn table(&self, tid: &str) -> Option<LTable>;
    /// Insert a row with its cells (one per current column, keyed by column ID).
    fn insert_row_raw(
        &mut self,
        tid: &str,
        index: usize,
        row: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool;
    fn delete_row(&mut self, tid: &str, row_id: &str) -> bool;
    /// Insert a column with its cells (one per current row, keyed by row ID).
    fn insert_col_raw(
        &mut self,
        tid: &str,
        index: usize,
        col: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool;
    fn delete_col(&mut self, tid: &str, col_id: &str) -> bool;
    /// Type into a cell's first paragraph, creating the cell if a concurrent
    /// row and column insert left it without one.
    fn cell_insert_text(
        &mut self,
        tid: &str,
        row_id: &str,
        col_id: &str,
        pos: usize,
        text: &str,
    ) -> bool;

    /// A row whose cells copy their neighbours' styles.
    fn insert_row(&mut self, tid: &str, index: usize, row_id: &str) -> bool {
        let Some(t) = self.table(tid) else {
            return false;
        };
        let like = index
            .min(t.rows.len())
            .saturating_sub(1)
            .min(t.rows.len().saturating_sub(1));
        let mut props = t.rows.get(like).map(|r| r.1.clone()).unwrap_or_default();
        props.remove("@group");
        let cells = t
            .cols
            .iter()
            .enumerate()
            .map(|(ci, (cid, _))| {
                let n = t.cells.get(like).and_then(|r| r.get(ci));
                (cid.clone(), new_cell(&format!("{row_id}|{cid}"), n))
            })
            .collect::<Vec<_>>();
        self.insert_row_raw(tid, index, (row_id, &props), &cells)
    }

    /// A column of the given width whose cells copy their neighbours' styles.
    fn insert_col(&mut self, tid: &str, index: usize, col_id: &str, width: &str) -> bool {
        let Some(t) = self.table(tid) else {
            return false;
        };
        let like = index.min(t.cols.len()).saturating_sub(1);
        let mut props = Props::new();
        props.insert("@width".into(), width.into());
        if let Some((_, p)) = t.cols.get(like)
            && let Some(g) = p.get("@group")
        {
            props.insert("@group".into(), g.clone());
        }
        let cells = t
            .rows
            .iter()
            .enumerate()
            .map(|(ri, (rid, _))| {
                let n = t.cells.get(ri).and_then(|r| r.get(like));
                (rid.clone(), new_cell(&format!("{rid}|{col_id}"), n))
            })
            .collect::<Vec<_>>();
        self.insert_col_raw(tid, index, (col_id, &props), &cells)
    }
}

/// An empty cell styled like `like`: same cell style, one empty paragraph in
/// the same paragraph style. Merged-cell properties are not copied.
pub fn new_cell(id: &str, like: Option<&LCell>) -> LCell {
    let mut props = Props::new();
    let mut pprops = Props::new();
    if let Some(n) = like {
        for k in ["table:style-name", "office:value-type", "@order"] {
            if let Some(v) = n.props.get(k) {
                props.insert(k.into(), v.clone());
            }
        }
        if let Some(LNode::Para(p)) = n.blocks.first() {
            for (k, v) in &p.props {
                if k == "text:style-name" || k == "@style" || k.starts_with('Δ') {
                    pprops.insert(k.clone(), v.clone());
                }
            }
        }
    }
    LCell {
        id: id.to_owned(),
        props,
        blocks: vec![LNode::Para(Para {
            id: format!("{id}|p"),
            props: pprops,
            runs: Vec::new(),
        })],
    }
}
