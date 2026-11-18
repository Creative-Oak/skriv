// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Loro backend: a movable tree of blocks.
//!
//! - Root container `blocks` is a `LoroTree`. Every paragraph, embed,
//!   section, list, list item, table and table cell is a node. Node metadata
//!   holds `k` (kind), `id`, `p` (a `LoroMap` of properties, one entry per
//!   property) and, for paragraphs, `t` (a `LoroText` with Peritext marks).
//! - A table node holds `cols` and `rows` (`LoroMovableList`s of IDs) and
//!   `cp`/`rp` (maps from column/row ID to a property map). Its children are
//!   cell nodes tagged with their row and column ID, so cells are addressed
//!   by ID, never by position.
//! - Moving a section is one tree move; edits inside it are untouched.
//! - Attribution: `LoroText::get_editor_at_unicode_pos` gives the peer that
//!   inserted a character; the `authors` map names peers.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use loro::{
    Container, ExpandType, ExportMode, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroTree,
    LoroValue, StyleConfig, StyleConfigMap, TextDelta, TreeID, TreeParentId, ValueOrContainer,
};

use crate::logical::*;
use crate::{CrdtDoc, Peer};

pub struct LoroDocument {
    pub doc: LoroDoc,
    index: RefCell<HashMap<String, TreeID>>,
}

/// Loro reads `a:b` in a mark key as "style `a`, instance `b`" when looking up
/// expand behaviour. ODF names are full of colons, so mark keys are escaped.
pub(crate) fn esc(k: &str) -> String {
    k.replace(':', "~")
}

pub(crate) fn unesc(k: &str) -> String {
    k.replace('~', ":")
}

fn configure(doc: &LoroDoc) {
    // Formatting grows when you type at its end (Peritext); links and inline
    // objects don't.
    doc.config_default_text_style(Some(StyleConfig {
        expand: ExpandType::After,
    }));
    let mut m = StyleConfigMap::new();
    for k in ["obj", "link"] {
        m.insert(
            k.into(),
            StyleConfig {
                expand: ExpandType::None,
            },
        );
    }
    doc.config_text_style(m);
}

pub(crate) fn s(v: &ValueOrContainer) -> Option<String> {
    match v {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

pub(crate) fn get_str(m: &LoroMap, k: &str) -> Option<String> {
    m.get(k).as_ref().and_then(s)
}

pub(crate) fn get_map(m: &LoroMap, k: &str) -> Option<LoroMap> {
    match m.get(k)? {
        ValueOrContainer::Container(Container::Map(m)) => Some(m),
        _ => None,
    }
}

pub(crate) fn get_text(m: &LoroMap, k: &str) -> Option<LoroText> {
    match m.get(k)? {
        ValueOrContainer::Container(Container::Text(t)) => Some(t),
        _ => None,
    }
}

pub(crate) fn get_mlist(m: &LoroMap, k: &str) -> Option<LoroMovableList> {
    match m.get(k)? {
        ValueOrContainer::Container(Container::MovableList(l)) => Some(l),
        _ => None,
    }
}

pub(crate) fn read_props(m: &LoroMap) -> Props {
    let mut p = Props::new();
    m.for_each(|k, v| {
        if let Some(v) = s(&v) {
            p.insert(k.to_string(), v);
        }
    });
    p
}

pub(crate) fn write_props(m: &LoroMap, p: &Props) {
    for (k, v) in p {
        m.insert(k, v.as_str()).expect("map insert");
    }
}

pub(crate) fn list_strings(l: &LoroMovableList) -> Vec<String> {
    match l.get_value() {
        LoroValue::List(v) => v
            .iter()
            .filter_map(|x| match x {
                LoroValue::String(s) => Some(s.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn read_runs(t: &LoroText) -> Vec<Run> {
    let mut runs = Vec::new();
    for d in t.to_delta() {
        if let TextDelta::Insert { insert, attributes } = d {
            let mut marks = BTreeMap::new();
            for (k, v) in attributes.into_iter().flatten() {
                if let LoroValue::String(v) = v {
                    marks.insert(unesc(&k), v.to_string());
                }
            }
            runs.push(Run {
                text: insert,
                marks,
            });
        }
    }
    runs
}

fn write_runs(t: &LoroText, at: usize, runs: &[Run]) {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    if text.is_empty() {
        return;
    }
    t.insert(at, &text).expect("text insert");
    let mut pos = at;
    for r in runs {
        let n = r.text.chars().count();
        for (k, v) in &r.marks {
            t.mark(pos..pos + n, &esc(k), v.as_str()).expect("mark");
        }
        pos += n;
    }
}

impl LoroDocument {
    fn tree(&self) -> LoroTree {
        self.doc.get_tree("blocks")
    }

    fn meta(&self, n: TreeID) -> LoroMap {
        self.tree().get_meta(n).expect("node meta")
    }

    fn node(&self, id: &str) -> Option<TreeID> {
        let tree = self.tree();
        let alive = |n: &TreeID| tree.contains(*n) && !tree.is_node_deleted(n).unwrap_or(true);
        if let Some(n) = self.index.borrow().get(id)
            && alive(n)
        {
            return Some(*n);
        }
        let mut ix = self.index.borrow_mut();
        ix.clear();
        for n in tree.get_nodes(false) {
            if let Ok(m) = tree.get_meta(n.id)
                && let Some(i) = get_str(&m, "id")
            {
                ix.entry(i).or_insert(n.id);
            }
        }
        ix.get(id).copied()
    }

    fn text(&self, pid: &str) -> Option<LoroText> {
        let n = self.node(pid)?;
        get_text(&self.meta(n), "t")
    }

    fn write_node(&self, parent: TreeParentId, index: usize, n: &LNode) -> TreeID {
        let tree = self.tree();
        let len = tree.children_num(parent).unwrap_or(0);
        let id = tree.create_at(parent, index.min(len)).expect("create node");
        let m = self.meta(id);
        m.insert("id", n.id()).expect("id");
        match n {
            LNode::Para(p) => {
                m.insert("k", "p").unwrap();
                let pm = m.insert_container("p", LoroMap::new()).unwrap();
                write_props(&pm, &p.props);
                let t = m.insert_container("t", LoroText::new()).unwrap();
                write_runs(&t, 0, &p.runs);
            }
            LNode::Embed { json, .. } => {
                m.insert("k", "e").unwrap();
                m.insert("j", json.as_str()).unwrap();
            }
            LNode::Container {
                kind,
                props,
                children,
                ..
            } => {
                m.insert("k", kind.as_str()).unwrap();
                let pm = m.insert_container("p", LoroMap::new()).unwrap();
                write_props(&pm, props);
                for (i, c) in children.iter().enumerate() {
                    self.write_node(TreeParentId::Node(id), i, c);
                }
            }
            LNode::Table(t) => {
                m.insert("k", "table").unwrap();
                let pm = m.insert_container("p", LoroMap::new()).unwrap();
                write_props(&pm, &t.props);
                let cols = m.insert_container("cols", LoroMovableList::new()).unwrap();
                let cp = m.insert_container("cp", LoroMap::new()).unwrap();
                for (cid, p) in &t.cols {
                    cols.push(cid.as_str()).unwrap();
                    write_props(&cp.insert_container(cid, LoroMap::new()).unwrap(), p);
                }
                let rows = m.insert_container("rows", LoroMovableList::new()).unwrap();
                let rp = m.insert_container("rp", LoroMap::new()).unwrap();
                for (ri, (rid, p)) in t.rows.iter().enumerate() {
                    rows.push(rid.as_str()).unwrap();
                    write_props(&rp.insert_container(rid, LoroMap::new()).unwrap(), p);
                    for (ci, c) in t.cells[ri].iter().enumerate() {
                        self.write_cell(id, rid, &t.cols[ci].0, c);
                    }
                }
            }
        }
        self.index.borrow_mut().insert(n.id().to_owned(), id);
        id
    }

    fn write_cell(&self, table: TreeID, row: &str, col: &str, c: &LCell) -> TreeID {
        let tree = self.tree();
        let n = tree.create(TreeParentId::Node(table)).unwrap();
        let m = self.meta(n);
        m.insert("k", "cell").unwrap();
        m.insert("id", c.id.as_str()).unwrap();
        m.insert("r", row).unwrap();
        m.insert("c", col).unwrap();
        write_props(&m.insert_container("p", LoroMap::new()).unwrap(), &c.props);
        for (i, b) in c.blocks.iter().enumerate() {
            self.write_node(TreeParentId::Node(n), i, b);
        }
        n
    }

    fn read_node(&self, n: TreeID) -> Option<LNode> {
        let m = self.meta(n);
        let id = get_str(&m, "id").unwrap_or_default();
        let kind = get_str(&m, "k")?;
        let props = get_map(&m, "p").map(|p| read_props(&p)).unwrap_or_default();
        Some(match kind.as_str() {
            "p" => {
                let runs = get_text(&m, "t").map(|t| read_runs(&t)).unwrap_or_default();
                LNode::Para(Para { id, props, runs })
            }
            "e" => LNode::Embed {
                id,
                json: get_str(&m, "j").unwrap_or_default(),
            },
            "table" => LNode::Table(self.read_table(n, id, props)),
            "cell" => return None,
            _ => LNode::Container {
                id,
                kind,
                props,
                children: self.read_children(TreeParentId::Node(n)),
            },
        })
    }

    fn read_children(&self, p: TreeParentId) -> Vec<LNode> {
        self.tree()
            .children(p)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| self.read_node(c))
            .collect()
    }

    fn cell_nodes(&self, table: TreeID) -> HashMap<(String, String), Vec<TreeID>> {
        let tree = self.tree();
        let mut out: HashMap<(String, String), Vec<TreeID>> = HashMap::new();
        for c in tree.children(TreeParentId::Node(table)).unwrap_or_default() {
            let m = self.meta(c);
            if get_str(&m, "k").as_deref() == Some("cell")
                && let (Some(r), Some(col)) = (get_str(&m, "r"), get_str(&m, "c"))
            {
                out.entry((r, col)).or_default().push(c);
            }
        }
        // Two peers can create the same missing cell concurrently; read
        // them as one, in a stable order.
        for v in out.values_mut() {
            v.sort();
        }
        out
    }

    fn read_table(&self, n: TreeID, id: String, props: Props) -> LTable {
        let m = self.meta(n);
        let cp = get_map(&m, "cp");
        let rp = get_map(&m, "rp");
        let pm = |map: &Option<LoroMap>, k: &str| {
            map.as_ref()
                .and_then(|x| get_map(x, k))
                .map(|x| read_props(&x))
                .unwrap_or_default()
        };
        let cols: Vec<(String, Props)> = get_mlist(&m, "cols")
            .map(|l| list_strings(&l))
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                let p = pm(&cp, &c);
                (c, p)
            })
            .collect();
        let rows: Vec<(String, Props)> = get_mlist(&m, "rows")
            .map(|l| list_strings(&l))
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let p = pm(&rp, &r);
                (r, p)
            })
            .collect();
        let nodes = self.cell_nodes(n);
        let cells = rows
            .iter()
            .map(|(r, _)| {
                cols.iter()
                    .map(|(c, _)| match nodes.get(&(r.clone(), c.clone())) {
                        Some(ns) => {
                            let m0 = self.meta(ns[0]);
                            LCell {
                                id: get_str(&m0, "id").unwrap_or_default(),
                                props: get_map(&m0, "p")
                                    .map(|p| read_props(&p))
                                    .unwrap_or_default(),
                                blocks: ns
                                    .iter()
                                    .flat_map(|x| self.read_children(TreeParentId::Node(*x)))
                                    .collect(),
                            }
                        }
                        None => LCell {
                            id: format!("{r}|{c}"),
                            ..Default::default()
                        },
                    })
                    .collect()
            })
            .collect();
        LTable {
            id,
            props,
            cols,
            rows,
            cells,
        }
    }

    fn sibling_slot(&self, after: &str) -> Option<(TreeParentId, usize)> {
        let tree = self.tree();
        let a = self.node(after)?;
        let parent = tree.parent(a)?;
        let idx = tree.children(parent)?.iter().position(|x| *x == a)?;
        Some((parent, idx + 1))
    }
}

impl CrdtDoc for LoroDocument {
    const NAME: &'static str = "loro";

    fn create(l: &LDoc, peer: Peer) -> Self {
        let doc = LoroDoc::new();
        configure(&doc);
        doc.set_peer_id(peer).expect("peer id");
        let tree = doc.get_tree("blocks");
        tree.enable_fractional_index(0);
        let me = LoroDocument {
            doc,
            index: RefCell::new(HashMap::new()),
        };
        for (i, n) in l.body.iter().enumerate() {
            me.write_node(TreeParentId::Root, i, n);
        }
        let styles = me.doc.get_map("styles");
        for (k, v) in &l.styles {
            styles.insert(k, v.as_str()).unwrap();
        }
        me.doc
            .get_map("doc")
            .insert("meta", l.meta.as_str())
            .unwrap();
        me.doc.commit();
        me
    }

    fn read(&self) -> LDoc {
        let body = self.read_children(TreeParentId::Root);
        let mut styles = BTreeMap::new();
        self.doc.get_map("styles").for_each(|k, v| {
            if let Some(v) = s(&v) {
                styles.insert(k.to_string(), v);
            }
        });
        let meta = get_str(&self.doc.get_map("doc"), "meta").unwrap_or_default();
        LDoc { body, styles, meta }
    }

    fn fork(&mut self, peer: Peer) -> Self {
        self.doc.commit();
        let doc = self.doc.fork();
        configure(&doc);
        doc.set_peer_id(peer).expect("peer id");
        LoroDocument {
            doc,
            index: RefCell::new(self.index.borrow().clone()),
        }
    }

    fn merge(&mut self, other: &mut Self) {
        self.doc.commit();
        other.doc.commit();
        let bytes = other
            .doc
            .export(ExportMode::updates(&self.doc.oplog_vv()))
            .expect("export updates");
        self.doc.import(&bytes).expect("import");
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.export(ExportMode::Snapshot).expect("snapshot")
    }

    fn load(bytes: &[u8], peer: Peer) -> Self {
        let doc = LoroDoc::new();
        configure(&doc);
        doc.import(bytes).expect("import snapshot");
        doc.set_peer_id(peer).expect("peer id");
        LoroDocument {
            doc,
            index: RefCell::new(HashMap::new()),
        }
    }

    fn commit(&mut self) {
        self.doc.commit();
    }

    fn set_author(&mut self, name: &str) {
        let peer = self.doc.peer_id().to_string();
        self.doc.get_map("authors").insert(&peer, name).unwrap();
    }

    fn author_at(&self, pid: &str, pos: usize) -> Option<String> {
        let peer = self.text(pid)?.get_editor_at_unicode_pos(pos)?;
        get_str(&self.doc.get_map("authors"), &peer.to_string())
    }

    fn para_len(&self, pid: &str) -> Option<usize> {
        Some(self.text(pid)?.len_unicode())
    }

    fn para(&self, pid: &str) -> Option<Para> {
        match self.read_node(self.node(pid)?)? {
            LNode::Para(p) => Some(p),
            _ => None,
        }
    }

    fn insert_text(&mut self, pid: &str, pos: usize, text: &str) -> bool {
        let Some(t) = self.text(pid) else {
            return false;
        };
        t.insert(pos.min(t.len_unicode()), text).is_ok()
    }

    fn delete_text(&mut self, pid: &str, pos: usize, len: usize) -> bool {
        let Some(t) = self.text(pid) else {
            return false;
        };
        let n = t.len_unicode();
        let pos = pos.min(n);
        t.delete(pos, len.min(n - pos)).is_ok()
    }

    fn mark(
        &mut self,
        pid: &str,
        start: usize,
        end: usize,
        key: &str,
        value: Option<&str>,
    ) -> bool {
        let Some(t) = self.text(pid) else {
            return false;
        };
        let n = t.len_unicode();
        let (start, end) = (start.min(n), end.min(n));
        if start >= end {
            return false;
        }
        match value {
            Some(v) => t.mark(start..end, &esc(key), v).is_ok(),
            None => t.unmark(start..end, &esc(key)).is_ok(),
        }
    }

    fn set_para_prop(&mut self, pid: &str, key: &str, value: Option<&str>) -> bool {
        self.set_container_prop(pid, key, value)
    }

    fn set_container_prop(&mut self, id: &str, key: &str, value: Option<&str>) -> bool {
        let Some(n) = self.node(id) else { return false };
        let Some(p) = get_map(&self.meta(n), "p") else {
            return false;
        };
        match value {
            Some(v) => p.insert(key, v).is_ok(),
            None => p.delete(key).is_ok(),
        }
    }

    fn split_para(&mut self, pid: &str, pos: usize, new_id: &str) -> bool {
        let Some(p) = self.para(pid) else {
            return false;
        };
        let Some(t) = self.text(pid) else {
            return false;
        };
        let n = t.len_unicode();
        let pos = pos.min(n);
        // The tail moves to the new paragraph by delete + insert: a
        // concurrent edit inside it stays behind in the first paragraph.
        let mut tail = Vec::new();
        let mut at = 0;
        for r in &p.runs {
            let len = r.text.chars().count();
            if at + len > pos {
                let skip = pos.saturating_sub(at);
                tail.push(Run {
                    text: r.text.chars().skip(skip).collect(),
                    marks: r.marks.clone(),
                });
            }
            at += len;
        }
        if pos < n && t.delete(pos, n - pos).is_err() {
            return false;
        }
        let mut props = p.props.clone();
        props.remove("@h");
        let new = Para {
            id: new_id.into(),
            props,
            runs: tail,
        };
        self.insert_para_after(pid, &new)
    }

    fn insert_para_after(&mut self, after: &str, para: &Para) -> bool {
        let Some((parent, idx)) = self.sibling_slot(after) else {
            return false;
        };
        self.write_node(parent, idx, &LNode::Para(para.clone()));
        true
    }

    fn delete_block(&mut self, id: &str) -> bool {
        let Some(n) = self.node(id) else { return false };
        self.tree().delete(n).is_ok()
    }

    fn move_block_after(&mut self, id: &str, after: &str) -> bool {
        let (Some(n), Some(a)) = (self.node(id), self.node(after)) else {
            return false;
        };
        self.tree().mov_after(n, a).is_ok()
    }

    fn table(&self, tid: &str) -> Option<LTable> {
        let n = self.node(tid)?;
        let m = self.meta(n);
        let props = get_map(&m, "p").map(|p| read_props(&p)).unwrap_or_default();
        Some(self.read_table(n, tid.to_owned(), props))
    }

    fn insert_row_raw(
        &mut self,
        tid: &str,
        index: usize,
        row: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool {
        let Some(n) = self.node(tid) else {
            return false;
        };
        let m = self.meta(n);
        let (Some(rows), Some(rp)) = (get_mlist(&m, "rows"), get_map(&m, "rp")) else {
            return false;
        };
        rows.insert(index.min(rows.len()), row.0).unwrap();
        write_props(&rp.insert_container(row.0, LoroMap::new()).unwrap(), row.1);
        for (col, c) in cells {
            self.write_cell(n, row.0, col, c);
        }
        true
    }

    fn delete_row(&mut self, tid: &str, row_id: &str) -> bool {
        self.delete_line(tid, row_id, "rows", "r")
    }

    fn insert_col_raw(
        &mut self,
        tid: &str,
        index: usize,
        col: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool {
        let Some(n) = self.node(tid) else {
            return false;
        };
        let m = self.meta(n);
        let (Some(cols), Some(cp)) = (get_mlist(&m, "cols"), get_map(&m, "cp")) else {
            return false;
        };
        cols.insert(index.min(cols.len()), col.0).unwrap();
        write_props(&cp.insert_container(col.0, LoroMap::new()).unwrap(), col.1);
        for (row, c) in cells {
            self.write_cell(n, row, col.0, c);
        }
        true
    }

    fn delete_col(&mut self, tid: &str, col_id: &str) -> bool {
        self.delete_line(tid, col_id, "cols", "c")
    }

    fn cell_insert_text(
        &mut self,
        tid: &str,
        row_id: &str,
        col_id: &str,
        pos: usize,
        text: &str,
    ) -> bool {
        let Some(n) = self.node(tid) else {
            return false;
        };
        let cells = self.cell_nodes(n);
        let tree = self.tree();
        let cell = match cells.get(&(row_id.to_owned(), col_id.to_owned())) {
            Some(v) => v[0],
            None => {
                let peer = self.doc.peer_id();
                let c = LCell {
                    id: format!("{row_id}|{col_id}|{peer}"),
                    props: Props::new(),
                    blocks: Vec::new(),
                };
                self.write_cell(n, row_id, col_id, &c)
            }
        };
        let first = tree
            .children(TreeParentId::Node(cell))
            .unwrap_or_default()
            .into_iter()
            .find(|c| get_str(&self.meta(*c), "k").as_deref() == Some("p"));
        let p = match first {
            Some(p) => p,
            None => {
                let peer = self.doc.peer_id();
                let para = Para {
                    id: format!("{row_id}|{col_id}|p{peer}"),
                    ..Default::default()
                };
                self.write_node(TreeParentId::Node(cell), 0, &LNode::Para(para))
            }
        };
        let Some(t) = get_text(&self.meta(p), "t") else {
            return false;
        };
        t.insert(pos.min(t.len_unicode()), text).is_ok()
    }
}

impl LoroDocument {
    fn delete_line(&mut self, tid: &str, id: &str, list: &str, tag: &str) -> bool {
        let Some(n) = self.node(tid) else {
            return false;
        };
        let m = self.meta(n);
        let Some(l) = get_mlist(&m, list) else {
            return false;
        };
        let Some(i) = list_strings(&l).iter().position(|x| x == id) else {
            return false;
        };
        l.delete(i, 1).unwrap();
        let tree = self.tree();
        for c in tree.children(TreeParentId::Node(n)).unwrap_or_default() {
            if get_str(&self.meta(c), tag).as_deref() == Some(id) {
                tree.delete(c).unwrap();
            }
        }
        true
    }
}
