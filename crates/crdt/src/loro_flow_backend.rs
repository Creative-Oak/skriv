// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Loro, hybrid: a movable tree for structure, rich text for flow.
//!
//! Added during Spike 3 after the tree-per-paragraph encoding
//! ([`crate::loro_backend`]) turned out to cost most of its size in
//! per-container overhead, which confounded "Loro or Automerge" with "tree
//! or text".
//!
//! - Sections, lists, list items, tables and cells are `LoroTree` nodes, as
//!   in the tree backend, so moving a section is one tree move.
//! - Each run of consecutive paragraphs under one parent is a single
//!   `flow` node holding one `LoroText`. A paragraph starts with a `\n`
//!   marker character carrying a `pid` mark (its ID) and one
//!   `para:<property>` mark per paragraph property, so properties merge per
//!   key, as Peritext marks with no expansion. Embeds are markers with an
//!   `embed` mark.
//! - Splitting a paragraph inserts a marker; the tail doesn't move, so
//!   concurrent typing in it follows the split, as with Automerge.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use loro::cursor::{Cursor, Side};
use loro::{
    ExpandType, ExportMode, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroTree, LoroValue,
    StyleConfig, StyleConfigMap, TextDelta, TreeID, TreeParentId,
};

use crate::logical::*;
use crate::loro_backend::{
    esc, get_map, get_mlist, get_str, get_text, list_strings, read_props, unesc, write_props,
};
use crate::{CrdtDoc, Peer};

pub struct LoroFlowDocument {
    pub doc: LoroDoc,
    /// Paragraph ID → (flow node, cursor on its marker).
    paras: RefCell<HashMap<String, (TreeID, Cursor)>>,
    /// Tree node ID (sections, tables…) → node.
    nodes: RefCell<HashMap<String, TreeID>>,
}

const MARKER: char = '\n';
const PARA: &str = "para:";

fn configure(doc: &LoroDoc) {
    doc.config_default_text_style(Some(StyleConfig {
        expand: ExpandType::After,
    }));
    let mut m = StyleConfigMap::new();
    // `para:…` keys are looked up as `para`.
    for k in ["obj", "link", "pid", "embed", "para"] {
        m.insert(
            k.into(),
            StyleConfig {
                expand: ExpandType::None,
            },
        );
    }
    doc.config_text_style(m);
}

/// One item read from a flow text.
struct Item {
    start: usize,
    node: LNode,
}

fn read_items(t: &LoroText) -> Vec<Item> {
    let mut out: Vec<Item> = Vec::new();
    let mut pos = 0;
    let mut orphans = 0;
    for d in t.to_delta() {
        let TextDelta::Insert { insert, attributes } = d else {
            continue;
        };
        let attrs: BTreeMap<String, String> = attributes
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| match v {
                LoroValue::String(s) => Some((k, s.to_string())),
                _ => None,
            })
            .collect();
        // A run is either one marker (it has a unique `pid`) or content.
        if let Some(pid) = attrs.get("pid")
            && insert.starts_with(MARKER)
        {
            let node = match attrs.get("embed") {
                Some(json) => LNode::Embed {
                    id: pid.clone(),
                    json: json.clone(),
                },
                None => LNode::Para(Para {
                    id: pid.clone(),
                    props: attrs
                        .iter()
                        .filter_map(|(k, v)| k.strip_prefix(PARA).map(|k| (unesc(k), v.clone())))
                        .collect(),
                    runs: Vec::new(),
                }),
            };
            out.push(Item { start: pos, node });
            let rest: String = insert.chars().skip(1).collect();
            pos += 1;
            if rest.is_empty() {
                continue;
            }
            // Should not happen (markers carry unique marks), but be safe.
            push_content(&mut out, &mut orphans, pos, rest.clone(), BTreeMap::new());
            pos += rest.chars().count();
            continue;
        }
        let marks: BTreeMap<String, String> = attrs
            .into_iter()
            .filter(|(k, _)| k != "pid" && k != "embed" && !k.starts_with(PARA))
            .map(|(k, v)| (unesc(&k), v))
            .collect();
        let n = insert.chars().count();
        push_content(&mut out, &mut orphans, pos, insert, marks);
        pos += n;
    }
    for it in &mut out {
        if let LNode::Para(p) = &mut it.node {
            p.normalize();
        }
    }
    out
}

/// Text after an embed, or before the first marker, was left behind by a
/// delete racing an edit; it becomes its own paragraph.
fn push_content(
    out: &mut Vec<Item>,
    orphans: &mut usize,
    pos: usize,
    text: String,
    marks: BTreeMap<String, String>,
) {
    if !matches!(
        out.last(),
        Some(Item {
            node: LNode::Para(_),
            ..
        })
    ) {
        *orphans += 1;
        out.push(Item {
            start: pos,
            node: LNode::Para(Para {
                id: format!("orphan-{orphans}-{pos}"),
                ..Default::default()
            }),
        });
    }
    if let Some(Item {
        node: LNode::Para(p),
        ..
    }) = out.last_mut()
    {
        p.runs.push(Run { text, marks });
    }
}

/// Write paragraphs and embeds into `t` at `at`; returns the length written.
fn write_items(t: &LoroText, at: usize, items: &[&LNode]) -> usize {
    let mut s = String::new();
    for n in items {
        s.push(MARKER);
        if let LNode::Para(p) = n {
            for r in &p.runs {
                s.push_str(&r.text);
            }
        }
    }
    if s.is_empty() {
        return 0;
    }
    t.insert(at, &s).expect("insert");
    let inherited: Vec<String> = t
        .slice_delta(at, at + 1, loro::cursor::PosType::Unicode)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|d| match d {
            TextDelta::Insert { attributes, .. } => attributes,
            _ => None,
        })
        .flat_map(|a| a.into_keys())
        .collect();
    let total = s.chars().count();
    // Marks the new text picked up from the text before it are removed;
    // the new text gets exactly its own.
    for k in &inherited {
        t.unmark(at..at + total, k).expect("unmark");
    }
    let mut pos = at;
    for n in items {
        t.mark(pos..pos + 1, "pid", n.id()).expect("pid");
        match n {
            LNode::Para(p) => {
                for (k, v) in &p.props {
                    t.mark(pos..pos + 1, &format!("{PARA}{}", esc(k)), v.as_str())
                        .expect("prop");
                }
                pos += 1;
                for r in &p.runs {
                    let len = r.text.chars().count();
                    for (k, v) in &r.marks {
                        t.mark(pos..pos + len, &esc(k), v.as_str()).expect("mark");
                    }
                    pos += len;
                }
            }
            LNode::Embed { json, .. } => {
                t.mark(pos..pos + 1, "embed", json.as_str()).expect("embed");
                pos += 1;
            }
            _ => unreachable!("only paragraphs and embeds live in a flow"),
        }
    }
    total
}

fn is_flow_item(n: &LNode) -> bool {
    matches!(n, LNode::Para(_) | LNode::Embed { .. })
}

impl LoroFlowDocument {
    fn wrap(doc: LoroDoc) -> Self {
        LoroFlowDocument {
            doc,
            paras: RefCell::new(HashMap::new()),
            nodes: RefCell::new(HashMap::new()),
        }
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree("blocks")
    }

    fn meta(&self, n: TreeID) -> LoroMap {
        self.tree().get_meta(n).expect("node meta")
    }

    fn kind(&self, n: TreeID) -> Option<String> {
        get_str(&self.meta(n), "k")
    }

    fn alive(&self, n: TreeID) -> bool {
        let tree = self.tree();
        tree.contains(n) && !tree.is_node_deleted(&n).unwrap_or(true)
    }

    /// Rebuild both indexes: tree nodes by ID, paragraphs by ID.
    fn reindex(&self) {
        let tree = self.tree();
        let mut nodes = self.nodes.borrow_mut();
        let mut paras = self.paras.borrow_mut();
        nodes.clear();
        paras.clear();
        for n in tree.get_nodes(false) {
            let m = self.meta(n.id);
            if get_str(&m, "k").as_deref() == Some("flow") {
                if let Some(t) = get_text(&m, "t") {
                    for it in read_items(&t) {
                        if it.node.id().starts_with("orphan-") {
                            continue;
                        }
                        if let Some(c) = t.get_cursor(it.start, Side::Middle) {
                            paras.entry(it.node.id().to_owned()).or_insert((n.id, c));
                        }
                    }
                }
            } else if let Some(i) = get_str(&m, "id") {
                nodes.entry(i).or_insert(n.id);
            }
        }
    }

    fn node(&self, id: &str) -> Option<TreeID> {
        if let Some(n) = self.nodes.borrow().get(id).copied()
            && self.alive(n)
        {
            return Some(n);
        }
        self.reindex();
        self.nodes.borrow().get(id).copied()
    }

    fn marker_at(&self, t: &LoroText, pos: usize, pid: &str) -> bool {
        t.slice_delta(pos, pos + 1, loro::cursor::PosType::Unicode)
            .ok()
            .into_iter()
            .flatten()
            .any(|d| match d {
                TextDelta::Insert { insert, attributes } => {
                    insert.starts_with(MARKER)
                        && attributes
                            .and_then(|a| a.get("pid").cloned())
                            .is_some_and(|v| matches!(v, LoroValue::String(s) if *s == pid))
                }
                _ => false,
            })
    }

    /// The flow text holding paragraph `pid`, its marker position and its
    /// content length.
    fn extent(&self, pid: &str) -> Option<(TreeID, LoroText, usize, usize)> {
        for attempt in 0..2 {
            if attempt == 1 {
                self.reindex();
            }
            let Some((flow, cursor)) = self.paras.borrow().get(pid).cloned() else {
                continue;
            };
            if !self.alive(flow) {
                continue;
            }
            let Some(t) = get_text(&self.meta(flow), "t") else {
                continue;
            };
            let Ok(q) = self.doc.get_cursor_pos(&cursor) else {
                continue;
            };
            let m = q.current.pos;
            if !self.marker_at(&t, m, pid) {
                continue;
            }
            let len = t.len_unicode();
            let mut end = len;
            let mut from = m + 1;
            while from < len {
                let to = (from + 512).min(len);
                let chunk = t.slice(from, to).unwrap_or_default();
                if let Some(i) = chunk.chars().position(|c| c == MARKER) {
                    end = from + i;
                    break;
                }
                from = to;
            }
            return Some((flow, t, m, end - m - 1));
        }
        None
    }

    fn write_children(&self, parent: TreeParentId, nodes: &[LNode]) {
        let mut pending: Vec<&LNode> = Vec::new();
        let flush = |pending: &mut Vec<&LNode>| {
            if !pending.is_empty() {
                self.write_flow(parent, usize::MAX, pending);
                pending.clear();
            }
        };
        for n in nodes {
            if is_flow_item(n) {
                pending.push(n);
                continue;
            }
            flush(&mut pending);
            self.write_node(parent, usize::MAX, n);
        }
        flush(&mut pending);
    }

    fn write_flow(&self, parent: TreeParentId, index: usize, items: &[&LNode]) -> TreeID {
        let tree = self.tree();
        let len = tree.children_num(parent).unwrap_or(0);
        let id = tree.create_at(parent, index.min(len)).expect("create flow");
        let m = self.meta(id);
        m.insert("k", "flow").unwrap();
        let t = m.insert_container("t", LoroText::new()).unwrap();
        write_items(&t, 0, items);
        id
    }

    fn write_node(&self, parent: TreeParentId, index: usize, n: &LNode) -> TreeID {
        let tree = self.tree();
        let len = tree.children_num(parent).unwrap_or(0);
        let id = tree.create_at(parent, index.min(len)).expect("create node");
        let m = self.meta(id);
        m.insert("id", n.id()).unwrap();
        match n {
            LNode::Container {
                kind,
                props,
                children,
                ..
            } => {
                m.insert("k", kind.as_str()).unwrap();
                write_props(&m.insert_container("p", LoroMap::new()).unwrap(), props);
                self.write_children(TreeParentId::Node(id), children);
            }
            LNode::Table(t) => {
                m.insert("k", "table").unwrap();
                write_props(&m.insert_container("p", LoroMap::new()).unwrap(), &t.props);
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
            _ => unreachable!("paragraphs go in flows"),
        }
        self.nodes.borrow_mut().insert(n.id().to_owned(), id);
        id
    }

    fn write_cell(&self, table: TreeID, row: &str, col: &str, c: &LCell) -> TreeID {
        let n = self.tree().create(TreeParentId::Node(table)).unwrap();
        let m = self.meta(n);
        m.insert("k", "cell").unwrap();
        m.insert("id", c.id.as_str()).unwrap();
        m.insert("r", row).unwrap();
        m.insert("c", col).unwrap();
        write_props(&m.insert_container("p", LoroMap::new()).unwrap(), &c.props);
        self.write_children(TreeParentId::Node(n), &c.blocks);
        n
    }

    fn read_children(&self, p: TreeParentId) -> Vec<LNode> {
        let mut out = Vec::new();
        for c in self.tree().children(p).unwrap_or_default() {
            let m = self.meta(c);
            let Some(kind) = get_str(&m, "k") else {
                continue;
            };
            match kind.as_str() {
                "flow" => {
                    if let Some(t) = get_text(&m, "t") {
                        out.extend(read_items(&t).into_iter().map(|i| i.node));
                    }
                }
                "cell" => {}
                "table" => {
                    let id = get_str(&m, "id").unwrap_or_default();
                    let props = get_map(&m, "p").map(|p| read_props(&p)).unwrap_or_default();
                    out.push(LNode::Table(self.read_table(c, id, props)));
                }
                _ => out.push(LNode::Container {
                    id: get_str(&m, "id").unwrap_or_default(),
                    kind,
                    props: get_map(&m, "p").map(|p| read_props(&p)).unwrap_or_default(),
                    children: self.read_children(TreeParentId::Node(c)),
                }),
            }
        }
        out
    }

    fn cell_nodes(&self, table: TreeID) -> HashMap<(String, String), Vec<TreeID>> {
        let mut out: HashMap<(String, String), Vec<TreeID>> = HashMap::new();
        for c in self
            .tree()
            .children(TreeParentId::Node(table))
            .unwrap_or_default()
        {
            let m = self.meta(c);
            if get_str(&m, "k").as_deref() == Some("cell")
                && let (Some(r), Some(col)) = (get_str(&m, "r"), get_str(&m, "c"))
            {
                out.entry((r, col)).or_default().push(c);
            }
        }
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

    /// Split flow `flow` after position `at`: the text from `at` on moves to
    /// a new flow node right after it. Returns the (first) flow node.
    fn split_flow(&self, flow: TreeID, t: &LoroText, at: usize) {
        let len = t.len_unicode();
        if at >= len {
            return;
        }
        let tail = t
            .slice_delta(at, len, loro::cursor::PosType::Unicode)
            .unwrap_or_default();
        t.delete(at, len - at).unwrap();
        let tree = self.tree();
        let parent = tree.parent(flow).unwrap();
        let idx = tree
            .children(parent)
            .unwrap()
            .iter()
            .position(|x| *x == flow)
            .unwrap();
        let n = tree.create_at(parent, idx + 1).unwrap();
        let m = self.meta(n);
        m.insert("k", "flow").unwrap();
        let t2 = m.insert_container("t", LoroText::new()).unwrap();
        t2.apply_delta(&tail).unwrap();
        self.reindex();
    }
}

/// Operations Spike 4's file-mode import needs beyond [`CrdtDoc`]: joining
/// paragraphs, inserting any block anywhere, and Loro versions.
impl LoroFlowDocument {
    /// Wrap a Loro document that already holds a skriv document.
    pub fn from_loro(doc: LoroDoc, peer: Peer) -> Self {
        configure(&doc);
        doc.set_peer_id(peer).expect("peer id");
        Self::wrap(doc)
    }

    /// The current version (Loro frontiers), encoded.
    pub fn version(&mut self) -> Vec<u8> {
        self.doc.commit();
        self.doc.oplog_frontiers().encode()
    }

    /// A branch starting from an earlier `version` of this document.
    pub fn fork_at(&mut self, version: &[u8], peer: Peer) -> Option<Self> {
        self.doc.commit();
        let f = loro::Frontiers::decode(version).ok()?;
        let doc = self.doc.fork_at(&f).ok()?;
        Some(Self::from_loro(doc, peer))
    }

    /// The current state without history before it (a Loro shallow snapshot).
    pub fn save_shallow(&mut self) -> Vec<u8> {
        self.doc.commit();
        let f = self.doc.oplog_frontiers();
        self.doc
            .export(ExportMode::shallow_snapshot(&f))
            .expect("shallow snapshot")
    }

    /// Everything this document has that a peer at `version` lacks.
    pub fn updates_since(&mut self, version: &[u8]) -> Option<Vec<u8>> {
        self.doc.commit();
        let f = loro::Frontiers::decode(version).ok()?;
        let vv = self.doc.frontiers_to_vv(&f)?;
        self.doc.export(ExportMode::updates(&vv)).ok()
    }

    pub fn import_updates(&mut self, bytes: &[u8]) -> bool {
        self.doc.commit();
        let ok = self.doc.import(bytes).is_ok();
        self.reindex();
        ok
    }

    /// Delete the paragraph break before `pid`: its text joins the previous
    /// paragraph, keeping its characters (and their authors). Only within one
    /// flow, and only when the previous item is a paragraph.
    pub fn join_para(&mut self, pid: &str) -> bool {
        let Some((_, t, m, _)) = self.extent(pid) else {
            return false;
        };
        if m == 0 {
            return false;
        }
        let items = read_items(&t);
        let Some(i) = items.iter().position(|it| it.start == m) else {
            return false;
        };
        if i == 0 || !matches!(items[i - 1].node, LNode::Para(_)) {
            return false;
        }
        t.delete(m, 1).is_ok()
    }

    /// Insert any block (paragraph, embed, container, table) just before `before`.
    pub fn insert_before(&mut self, before: &str, n: &LNode) -> bool {
        if let Some((flow, t, m, _)) = self.extent(before) {
            if is_flow_item(n) {
                write_items(&t, m, &[n]);
                self.reindex();
                return true;
            }
            let tree = self.tree();
            let Some(parent) = tree.parent(flow) else {
                return false;
            };
            let idx = self.index_of(parent, flow);
            if m == 0 {
                self.write_node(parent, idx, n);
            } else {
                self.split_flow(flow, &t, m);
                self.write_node(parent, idx + 1, n);
            }
            self.reindex();
            return true;
        }
        let Some(b) = self.node(before) else {
            return false;
        };
        let Some(parent) = self.tree().parent(b) else {
            return false;
        };
        let idx = self.index_of(parent, b);
        self.place(parent, idx, n);
        true
    }

    /// Insert any block just after `after`.
    pub fn insert_after(&mut self, after: &str, n: &LNode) -> bool {
        if let Some((flow, t, m, len)) = self.extent(after) {
            if is_flow_item(n) {
                write_items(&t, m + 1 + len, &[n]);
                self.reindex();
                return true;
            }
            let tree = self.tree();
            let Some(parent) = tree.parent(flow) else {
                return false;
            };
            let idx = self.index_of(parent, flow);
            self.split_flow(flow, &t, m + 1 + len);
            self.write_node(parent, idx + 1, n);
            self.reindex();
            return true;
        }
        let Some(a) = self.node(after) else {
            return false;
        };
        let Some(parent) = self.tree().parent(a) else {
            return false;
        };
        let idx = self.index_of(parent, a);
        self.place(parent, idx + 1, n);
        true
    }

    /// Insert a block as the first child of container `parent` (`None`: the body).
    pub fn insert_first(&mut self, parent: Option<&str>, n: &LNode) -> bool {
        let p = match parent {
            None => TreeParentId::Root,
            Some(id) => match self.node(id) {
                Some(x) => TreeParentId::Node(x),
                None => return false,
            },
        };
        self.place(p, 0, n);
        true
    }

    /// Move block `id` to be the first child of `parent` (`None`: the body).
    /// Paragraphs move by copy and delete, as in [`CrdtDoc::move_block_after`].
    pub fn move_first(&mut self, parent: Option<&str>, id: &str) -> bool {
        let p = match parent {
            None => TreeParentId::Root,
            Some(x) => match self.node(x) {
                Some(n) => TreeParentId::Node(n),
                None => return false,
            },
        };
        if let Some(n) = self.node(id) {
            let ok = self.tree().mov_to(n, p, 0).is_ok();
            self.reindex();
            return ok;
        }
        let Some(para) = self.para(id) else {
            return false;
        };
        if !self.delete_block(id) {
            return false;
        }
        self.place(p, 0, &LNode::Para(para));
        true
    }

    fn index_of(&self, parent: TreeParentId, n: TreeID) -> usize {
        self.tree()
            .children(parent)
            .unwrap_or_default()
            .iter()
            .position(|x| *x == n)
            .unwrap_or(0)
    }

    fn place(&mut self, parent: TreeParentId, idx: usize, n: &LNode) {
        if is_flow_item(n) {
            self.write_flow(parent, idx, &[n]);
        } else {
            self.write_node(parent, idx, n);
        }
        self.reindex();
    }

    /// Set (or remove) one property of a table column or row.
    pub fn set_line_prop(
        &mut self,
        tid: &str,
        line: &str,
        rows: bool,
        key: &str,
        value: Option<&str>,
    ) -> bool {
        let Some(n) = self.node(tid) else {
            return false;
        };
        let Some(pm) = get_map(&self.meta(n), if rows { "rp" } else { "cp" }) else {
            return false;
        };
        let Some(lp) = get_map(&pm, line) else {
            return false;
        };
        match value {
            Some(v) => lp.insert(key, v).is_ok(),
            None => lp.delete(key).is_ok(),
        }
    }
}

impl CrdtDoc for LoroFlowDocument {
    const NAME: &'static str = "loro-flow";

    fn create(l: &LDoc, peer: Peer) -> Self {
        let doc = LoroDoc::new();
        configure(&doc);
        doc.set_peer_id(peer).expect("peer id");
        doc.get_tree("blocks").enable_fractional_index(0);
        let me = Self::wrap(doc);
        me.write_children(TreeParentId::Root, &l.body);
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
            if let Some(v) = crate::loro_backend::s(&v) {
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
        Self::wrap(doc)
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
        Self::wrap(doc)
    }

    fn commit(&mut self) {
        self.doc.commit();
    }

    fn set_author(&mut self, name: &str) {
        let peer = self.doc.peer_id().to_string();
        self.doc.get_map("authors").insert(&peer, name).unwrap();
    }

    fn author_at(&self, pid: &str, pos: usize) -> Option<String> {
        let (_, t, m, _) = self.extent(pid)?;
        let peer = t.get_editor_at_unicode_pos(m + 1 + pos)?;
        get_str(&self.doc.get_map("authors"), &peer.to_string())
    }

    fn para_len(&self, pid: &str) -> Option<usize> {
        Some(self.extent(pid)?.3)
    }

    fn para(&self, pid: &str) -> Option<Para> {
        let (_, t, _, _) = self.extent(pid)?;
        read_items(&t).into_iter().find_map(|i| match i.node {
            LNode::Para(p) if p.id == pid => Some(p),
            _ => None,
        })
    }

    fn insert_text(&mut self, pid: &str, pos: usize, text: &str) -> bool {
        let Some((_, t, m, len)) = self.extent(pid) else {
            return false;
        };
        t.insert(m + 1 + pos.min(len), text).is_ok()
    }

    fn delete_text(&mut self, pid: &str, pos: usize, len: usize) -> bool {
        let Some((_, t, m, n)) = self.extent(pid) else {
            return false;
        };
        let pos = pos.min(n);
        t.delete(m + 1 + pos, len.min(n - pos)).is_ok()
    }

    fn mark(
        &mut self,
        pid: &str,
        start: usize,
        end: usize,
        key: &str,
        value: Option<&str>,
    ) -> bool {
        let Some((_, t, m, n)) = self.extent(pid) else {
            return false;
        };
        let (start, end) = (m + 1 + start.min(n), m + 1 + end.min(n));
        if start >= end {
            return false;
        }
        match value {
            Some(v) => t.mark(start..end, &esc(key), v).is_ok(),
            None => t.unmark(start..end, &esc(key)).is_ok(),
        }
    }

    fn set_para_prop(&mut self, pid: &str, key: &str, value: Option<&str>) -> bool {
        let Some((_, t, m, _)) = self.extent(pid) else {
            return false;
        };
        let k = format!("{PARA}{}", esc(key));
        match value {
            Some(v) => t.mark(m..m + 1, &k, v).is_ok(),
            None => t.unmark(m..m + 1, &k).is_ok(),
        }
    }

    fn set_container_prop(&mut self, id: &str, key: &str, value: Option<&str>) -> bool {
        if let Some(n) = self.node(id) {
            let Some(p) = get_map(&self.meta(n), "p") else {
                return false;
            };
            return match value {
                Some(v) => p.insert(key, v).is_ok(),
                None => p.delete(key).is_ok(),
            };
        }
        self.set_para_prop(id, key, value)
    }

    fn split_para(&mut self, pid: &str, pos: usize, new_id: &str) -> bool {
        let Some((flow, t, m, n)) = self.extent(pid) else {
            return false;
        };
        let Some(p) = self.para(pid) else {
            return false;
        };
        let at = m + 1 + pos.min(n);
        // Only a marker goes in; the tail stays where it is.
        t.insert(at, &MARKER.to_string()).unwrap();
        t.mark(at..at + 1, "pid", new_id).unwrap();
        for (k, v) in p.props.iter().filter(|(k, _)| *k != "@h") {
            t.mark(at..at + 1, &format!("{PARA}{}", esc(k)), v.as_str())
                .unwrap();
        }
        if let Some(c) = t.get_cursor(at, Side::Middle) {
            self.paras.borrow_mut().insert(new_id.to_owned(), (flow, c));
        }
        true
    }

    fn insert_para_after(&mut self, after: &str, para: &Para) -> bool {
        let node = LNode::Para(para.clone());
        if let Some((flow, t, m, n)) = self.extent(after) {
            write_items(&t, m + 1 + n, &[&node]);
            if let Some(c) = t.get_cursor(m + 1 + n, Side::Middle) {
                self.paras.borrow_mut().insert(para.id.clone(), (flow, c));
            }
            return true;
        }
        let Some(a) = self.node(after) else {
            return false;
        };
        let tree = self.tree();
        let Some(parent) = tree.parent(a) else {
            return false;
        };
        let idx = tree
            .children(parent)
            .unwrap_or_default()
            .iter()
            .position(|x| *x == a)
            .unwrap_or(0);
        self.write_flow(parent, idx + 1, &[&node]);
        self.reindex();
        true
    }

    fn delete_block(&mut self, id: &str) -> bool {
        if let Some(n) = self.node(id) {
            return self.tree().delete(n).is_ok();
        }
        let Some((_, t, m, n)) = self.extent(id) else {
            return false;
        };
        t.delete(m, n + 1).is_ok()
    }

    fn move_block_after(&mut self, id: &str, after: &str) -> bool {
        // Paragraphs move by copy and delete, as in Automerge.
        if self.node(id).is_none() {
            let Some(p) = self.para(id) else { return false };
            if !self.delete_block(id) {
                return false;
            }
            return self.insert_para_after(after, &p);
        }
        let n = self.node(id).unwrap();
        if let Some(a) = self.node(after) {
            return self.tree().mov_after(n, a).is_ok();
        }
        // After a paragraph: split its flow there, then move after the first half.
        let Some((flow, t, m, len)) = self.extent(after) else {
            return false;
        };
        self.split_flow(flow, &t, m + 1 + len);
        self.tree().mov_after(n, flow).is_ok()
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
        let peer = self.doc.peer_id();
        let cell = match self
            .cell_nodes(n)
            .get(&(row_id.to_owned(), col_id.to_owned()))
        {
            Some(v) => v[0],
            None => {
                let c = LCell {
                    id: format!("{row_id}|{col_id}|{peer}"),
                    ..Default::default()
                };
                self.write_cell(n, row_id, col_id, &c)
            }
        };
        let tree = self.tree();
        let flow = tree
            .children(TreeParentId::Node(cell))
            .unwrap_or_default()
            .into_iter()
            .find(|c| self.kind(*c).as_deref() == Some("flow"));
        let flow = match flow {
            Some(f) => f,
            None => self.write_flow(TreeParentId::Node(cell), 0, &[]),
        };
        let Some(t) = get_text(&self.meta(flow), "t") else {
            return false;
        };
        let items = read_items(&t);
        let (m, len) = match items.iter().position(|i| matches!(i.node, LNode::Para(_))) {
            Some(i) => {
                let start = items[i].start;
                let end = items
                    .get(i + 1)
                    .map(|x| x.start)
                    .unwrap_or_else(|| t.len_unicode());
                (start, end - start - 1)
            }
            None => {
                let p = LNode::Para(Para {
                    id: format!("{row_id}|{col_id}|p{peer}"),
                    ..Default::default()
                });
                write_items(&t, 0, &[&p]);
                (0, 0)
            }
        };
        t.insert(m + 1 + pos.min(len), text).is_ok()
    }
}
