// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Automerge backend: rich text with block markers (Automerge 2.2+'s model).
//!
//! - `body` is one `Text`. Each paragraph starts with a block marker (a map
//!   inserted into the text) holding `type` (`p`, `e` for an embed, `t` for
//!   a table reference), `id`, `attrs` (one entry per property) and
//!   `parents`: the IDs of the sections, lists and list items around it,
//!   outermost first. Their own properties live in `containers`.
//! - `tables` maps a table ID to `props`, `cols`/`rows` (lists of IDs),
//!   `cp`/`rp` (properties by ID) and `cells`: a map keyed `row|col`, each
//!   cell holding `props` and its own `t` text with block markers.
//! - Automerge has no move. Moving a section copies its blocks to the new
//!   place and deletes the old ones; concurrent edits inside the old copy
//!   are not carried along (Spike 3 measures what happens to them).
//! - Attribution: the actor that inserted a character is in its op ID; the
//!   `authors` map names actors.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use automerge::iter::Span;
use automerge::marks::{ExpandMark, Mark, MarkSet, UpdateSpansConfig};
use automerge::transaction::Transactable;
use automerge::{ActorId, AutoCommit, Cursor, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value};

use crate::logical::*;
use crate::{CrdtDoc, Peer};

pub struct AutomergeDocument {
    pub doc: AutoCommit,
    /// Paragraph ID → (its text, a cursor on its marker, the next block's ID).
    cache: RefCell<Option<HashMap<String, Loc>>>,
}

#[derive(Clone)]
struct Loc {
    text: ObjId,
    cursor: Cursor,
    next: Option<String>,
}

/// One block in a flow, as read.
#[derive(Clone, Debug)]
struct Item {
    start: usize,
    end: usize,
    id: String,
    kind: String,
    parents: Vec<String>,
    attrs: Props,
    json: Option<String>,
    runs: Vec<Run>,
}

fn actor(peer: Peer) -> ActorId {
    ActorId::from(peer.to_be_bytes().to_vec())
}

fn expand(key: &str) -> ExpandMark {
    if key == "obj" || key == "link" {
        ExpandMark::None
    } else {
        ExpandMark::After
    }
}

fn hstr(v: Option<&automerge::hydrate::Value>) -> Option<String> {
    match v? {
        automerge::hydrate::Value::Scalar(ScalarValue::Str(s)) => Some(s.to_string()),
        _ => None,
    }
}

impl AutomergeDocument {
    fn wrap(doc: AutoCommit) -> Self {
        AutomergeDocument {
            doc,
            cache: RefCell::new(None),
        }
    }

    fn invalidate(&self) {
        *self.cache.borrow_mut() = None;
    }

    fn obj(&self, parent: &ObjId, key: &str) -> Option<ObjId> {
        match self.doc.get(parent, key).ok()?? {
            (Value::Object(_), id) => Some(id),
            _ => None,
        }
    }

    fn str(&self, parent: &ObjId, key: &str) -> Option<String> {
        match self.doc.get(parent, key).ok()?? {
            (Value::Scalar(s), _) => match s.as_ref() {
                ScalarValue::Str(s) => Some(s.to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    fn body(&self) -> ObjId {
        self.obj(&ROOT, "body").expect("body")
    }

    fn props(&self, m: &ObjId) -> Props {
        let mut p = Props::new();
        for item in self.doc.map_range(m, ..) {
            if let automerge::ValueRef::Scalar(automerge::ScalarValueRef::Str(s)) = item.value {
                p.insert(item.key.to_string(), s.to_string());
            }
        }
        p
    }

    fn write_props(&mut self, m: &ObjId, p: &Props) {
        for (k, v) in p {
            self.doc.put(m, k.as_str(), v.as_str()).unwrap();
        }
    }

    fn strings(&self, list: &ObjId) -> Vec<String> {
        self.doc
            .list_range(list, ..)
            .filter_map(|i| match i.value {
                automerge::ValueRef::Scalar(automerge::ScalarValueRef::Str(s)) => {
                    Some(s.to_string())
                }
                _ => None,
            })
            .collect()
    }

    /// Insert a block marker and its content at `at`; returns the index after it.
    fn write_item(&mut self, text: &ObjId, at: usize, parents: &[String], n: &LNode) -> usize {
        let b = self.doc.split_block(text, at).unwrap();
        self.doc.put(&b, "id", n.id()).unwrap();
        let ps = self.doc.put_object(&b, "parents", ObjType::List).unwrap();
        for (i, p) in parents.iter().enumerate() {
            self.doc.insert(&ps, i, p.as_str()).unwrap();
        }
        match n {
            LNode::Para(p) => {
                self.doc.put(&b, "type", "p").unwrap();
                let a = self.doc.put_object(&b, "attrs", ObjType::Map).unwrap();
                self.write_props(&a, &p.props);
                let n = self.write_runs(text, at + 1, &p.runs);
                self.clear_inherited(text, at, at + 1 + n, &p.runs);
                at + 1 + n
            }
            LNode::Embed { json, .. } => {
                self.doc.put(&b, "type", "e").unwrap();
                self.doc.put(&b, "json", json.as_str()).unwrap();
                at + 1
            }
            LNode::Table(t) => {
                self.doc.put(&b, "type", "t").unwrap();
                self.write_table(t);
                at + 1
            }
            LNode::Container { .. } => unreachable!("containers are flattened into parents"),
        }
    }

    /// A marker inserted right after a mark that expands takes that mark
    /// on, and so does the text typed after it. Text written here must have
    /// exactly its own marks: remove the inherited ones.
    fn clear_inherited(&mut self, text: &ObjId, start: usize, end: usize, runs: &[Run]) {
        let Ok(set) = self.doc.get_marks(text, start, None) else {
            return;
        };
        let inherited: Vec<String> = set.iter().map(|(k, _)| k.to_string()).collect();
        for k in inherited {
            self.doc
                .unmark(text, &k, start, start + 1, expand(&k))
                .unwrap();
            let mut pos = start + 1;
            for r in runs {
                let n = r.text.chars().count();
                if !r.marks.contains_key(&k) && n > 0 {
                    self.doc.unmark(text, &k, pos, pos + n, expand(&k)).unwrap();
                }
                pos += n;
            }
            debug_assert_eq!(pos, end);
        }
    }

    fn write_runs(&mut self, text: &ObjId, at: usize, runs: &[Run]) -> usize {
        let s: String = runs.iter().map(|r| r.text.as_str()).collect();
        if s.is_empty() {
            return 0;
        }
        self.doc.splice_text(text, at, 0, &s).unwrap();
        let mut pos = at;
        for r in runs {
            let n = r.text.chars().count();
            for (k, v) in &r.marks {
                self.doc
                    .mark(
                        text,
                        Mark::new(k.clone(), v.as_str(), pos, pos + n),
                        expand(k),
                    )
                    .unwrap();
            }
            pos += n;
        }
        pos - at
    }

    /// Write a whole flow into an empty text in one `update_spans` call:
    /// containers are flattened into `parents`, tables are written as
    /// objects and referenced from their marker. Inserting block by block
    /// was quadratic (84 s for 100 pages; see the Spike 3 results).
    fn write_flow(&mut self, text: &ObjId, nodes: &[LNode]) {
        let mut spans = Vec::new();
        self.flow_spans(&mut Vec::new(), nodes, &mut spans);
        let mut config = UpdateSpansConfig::default().with_default_expand(ExpandMark::After);
        for k in ["obj", "link"] {
            config = config.with_mark_expand(k, ExpandMark::None);
        }
        self.doc
            .update_spans(text, config, spans)
            .expect("update_spans");
    }

    fn flow_spans(&mut self, parents: &mut Vec<String>, nodes: &[LNode], out: &mut Vec<Span>) {
        use automerge::hydrate::Value as H;
        for n in nodes {
            if let LNode::Container {
                id,
                kind,
                props,
                children,
            } = n
            {
                let cs = self.obj(&ROOT, "containers").unwrap();
                let c = self.doc.put_object(&cs, id.as_str(), ObjType::Map).unwrap();
                self.doc.put(&c, "kind", kind.as_str()).unwrap();
                let p = self.doc.put_object(&c, "props", ObjType::Map).unwrap();
                self.write_props(&p, props);
                parents.push(id.clone());
                self.flow_spans(parents, children, out);
                parents.pop();
                continue;
            }
            let mut block: HashMap<String, H> = HashMap::new();
            block.insert("id".into(), H::from(n.id()));
            block.insert(
                "parents".into(),
                H::from(
                    parents
                        .iter()
                        .map(|p| H::from(p.as_str()))
                        .collect::<Vec<_>>(),
                ),
            );
            match n {
                LNode::Para(p) => {
                    block.insert("type".into(), H::from("p"));
                    let attrs: HashMap<String, H> = p
                        .props
                        .iter()
                        .map(|(k, v)| (k.clone(), H::from(v.as_str())))
                        .collect();
                    block.insert(
                        "attrs".into(),
                        H::from(automerge::hydrate::Map::from(attrs)),
                    );
                    out.push(Span::Block(automerge::hydrate::Map::from(block)));
                    for r in &p.runs {
                        let marks: MarkSet = r
                            .marks
                            .iter()
                            .map(|(k, v)| (k.clone(), ScalarValue::from(v.as_str())))
                            .collect();
                        out.push(Span::Text {
                            text: r.text.clone(),
                            marks: (!r.marks.is_empty()).then(|| std::sync::Arc::new(marks)),
                        });
                    }
                }
                LNode::Embed { json, .. } => {
                    block.insert("type".into(), H::from("e"));
                    block.insert("json".into(), H::from(json.as_str()));
                    out.push(Span::Block(automerge::hydrate::Map::from(block)));
                }
                LNode::Table(t) => {
                    block.insert("type".into(), H::from("t"));
                    self.write_table(t);
                    out.push(Span::Block(automerge::hydrate::Map::from(block)));
                }
                LNode::Container { .. } => unreachable!(),
            }
        }
    }

    fn write_table(&mut self, t: &LTable) {
        let tables = self.obj(&ROOT, "tables").unwrap();
        let tm = self
            .doc
            .put_object(&tables, t.id.as_str(), ObjType::Map)
            .unwrap();
        let p = self.doc.put_object(&tm, "props", ObjType::Map).unwrap();
        self.write_props(&p, &t.props);
        let cols = self.doc.put_object(&tm, "cols", ObjType::List).unwrap();
        let cp = self.doc.put_object(&tm, "cp", ObjType::Map).unwrap();
        for (i, (cid, props)) in t.cols.iter().enumerate() {
            self.doc.insert(&cols, i, cid.as_str()).unwrap();
            let m = self
                .doc
                .put_object(&cp, cid.as_str(), ObjType::Map)
                .unwrap();
            self.write_props(&m, props);
        }
        let rows = self.doc.put_object(&tm, "rows", ObjType::List).unwrap();
        let rp = self.doc.put_object(&tm, "rp", ObjType::Map).unwrap();
        let cells = self.doc.put_object(&tm, "cells", ObjType::Map).unwrap();
        for (ri, (rid, props)) in t.rows.iter().enumerate() {
            self.doc.insert(&rows, ri, rid.as_str()).unwrap();
            let m = self
                .doc
                .put_object(&rp, rid.as_str(), ObjType::Map)
                .unwrap();
            self.write_props(&m, props);
            for (ci, c) in t.cells[ri].iter().enumerate() {
                self.write_cell(&cells, rid, &t.cols[ci].0, c);
            }
        }
    }

    fn write_cell(&mut self, cells: &ObjId, row: &str, col: &str, c: &LCell) {
        let key = format!("{row}|{col}");
        let m = self
            .doc
            .put_object(cells, key.as_str(), ObjType::Map)
            .unwrap();
        self.doc.put(&m, "id", c.id.as_str()).unwrap();
        let p = self.doc.put_object(&m, "props", ObjType::Map).unwrap();
        self.write_props(&p, &c.props);
        let t = self.doc.put_object(&m, "t", ObjType::Text).unwrap();
        self.write_flow(&t, &c.blocks);
    }

    /// Every block in a text, with its extent.
    fn items(&self, text: &ObjId) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::new();
        let mut pos = 0;
        let mut orphans = 0;
        for span in self.doc.spans(text).expect("spans") {
            match span {
                Span::Block(m) => {
                    if let Some(last) = out.last_mut() {
                        last.end = pos;
                    }
                    let parents = match m.get("parents") {
                        Some(automerge::hydrate::Value::List(l)) => l
                            .iter()
                            .filter_map(|v| match &v.value {
                                automerge::hydrate::Value::Scalar(ScalarValue::Str(s)) => {
                                    Some(s.to_string())
                                }
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    let mut attrs = Props::new();
                    if let Some(automerge::hydrate::Value::Map(a)) = m.get("attrs") {
                        for (k, v) in a.iter() {
                            if let automerge::hydrate::Value::Scalar(ScalarValue::Str(s)) = &v.value
                            {
                                attrs.insert(k.clone(), s.to_string());
                            }
                        }
                    }
                    out.push(Item {
                        start: pos,
                        end: pos + 1,
                        id: hstr(m.get("id")).unwrap_or_default(),
                        kind: hstr(m.get("type")).unwrap_or_else(|| "p".into()),
                        parents,
                        attrs,
                        json: hstr(m.get("json")),
                        runs: Vec::new(),
                    });
                    pos += 1;
                }
                Span::Text { text, marks } => {
                    let n = text.chars().count();
                    let mut mk = BTreeMap::new();
                    if let Some(ms) = marks {
                        for (k, v) in ms.iter() {
                            if let ScalarValue::Str(s) = v {
                                mk.insert(k.to_string(), s.to_string());
                            }
                        }
                    }
                    // Text after a non-paragraph block (left behind by a
                    // move or delete racing an edit) becomes its own paragraph.
                    if !matches!(out.last(), Some(i) if i.kind == "p") {
                        orphans += 1;
                        let parents = out.last().map(|i| i.parents.clone()).unwrap_or_default();
                        out.push(Item {
                            start: pos,
                            end: pos,
                            id: format!("orphan-{orphans}-{pos}"),
                            kind: "p".into(),
                            parents,
                            attrs: Props::new(),
                            json: None,
                            runs: Vec::new(),
                        });
                    }
                    out.last_mut().unwrap().runs.push(Run { text, marks: mk });
                    pos += n;
                }
            }
        }
        if let Some(last) = out.last_mut() {
            last.end = pos;
        }
        out
    }

    fn item_node(&self, i: &Item) -> LNode {
        match i.kind.as_str() {
            "e" => LNode::Embed {
                id: i.id.clone(),
                json: i.json.clone().unwrap_or_default(),
            },
            "t" => LNode::Table(self.read_table(&i.id).unwrap_or_default()),
            _ => {
                let mut p = Para {
                    id: i.id.clone(),
                    props: i.attrs.clone(),
                    runs: i.runs.clone(),
                };
                p.normalize();
                LNode::Para(p)
            }
        }
    }

    /// Rebuild the container tree from the flat items and their parents.
    fn read_flow(&self, text: &ObjId) -> Vec<LNode> {
        let containers = self.obj(&ROOT, "containers");
        let mut root: Vec<LNode> = Vec::new();
        let mut open: Vec<(String, Vec<LNode>)> = Vec::new();
        let close = |open: &mut Vec<(String, Vec<LNode>)>, root: &mut Vec<LNode>| {
            let (id, children) = open.pop().unwrap();
            let (kind, props) = match containers.as_ref().and_then(|cs| self.obj(cs, &id)) {
                Some(c) => (
                    self.str(&c, "kind").unwrap_or_else(|| "section".into()),
                    self.obj(&c, "props")
                        .map(|p| self.props(&p))
                        .unwrap_or_default(),
                ),
                None => ("section".into(), Props::new()),
            };
            let node = LNode::Container {
                id,
                kind,
                props,
                children,
            };
            match open.last_mut() {
                Some((_, v)) => v.push(node),
                None => root.push(node),
            }
        };
        for item in self.items(text) {
            let common = open
                .iter()
                .zip(&item.parents)
                .take_while(|((a, _), b)| a == *b)
                .count();
            while open.len() > common {
                close(&mut open, &mut root);
            }
            for p in &item.parents[common..] {
                open.push((p.clone(), Vec::new()));
            }
            let node = self.item_node(&item);
            match open.last_mut() {
                Some((_, v)) => v.push(node),
                None => root.push(node),
            }
        }
        while !open.is_empty() {
            close(&mut open, &mut root);
        }
        root
    }

    fn table_obj(&self, tid: &str) -> Option<ObjId> {
        self.obj(&self.obj(&ROOT, "tables")?, tid)
    }

    /// All cell maps under `row|col`; concurrent creation leaves several.
    fn cell_objs(&self, cells: &ObjId, key: &str) -> Vec<ObjId> {
        self.doc
            .get_all(cells, key)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(v, id)| matches!(v, Value::Object(ObjType::Map)).then_some(id))
            .collect()
    }

    fn read_table(&self, tid: &str) -> Option<LTable> {
        let tm = self.table_obj(tid)?;
        let cp = self.obj(&tm, "cp")?;
        let rp = self.obj(&tm, "rp")?;
        let cells_obj = self.obj(&tm, "cells")?;
        let pm = |m: &ObjId, k: &str| self.obj(m, k).map(|x| self.props(&x)).unwrap_or_default();
        let cols: Vec<(String, Props)> = self
            .strings(&self.obj(&tm, "cols")?)
            .into_iter()
            .map(|c| {
                let p = pm(&cp, &c);
                (c, p)
            })
            .collect();
        let rows: Vec<(String, Props)> = self
            .strings(&self.obj(&tm, "rows")?)
            .into_iter()
            .map(|r| {
                let p = pm(&rp, &r);
                (r, p)
            })
            .collect();
        let cells = rows
            .iter()
            .map(|(r, _)| {
                cols.iter()
                    .map(|(c, _)| {
                        let objs = self.cell_objs(&cells_obj, &format!("{r}|{c}"));
                        match objs.first() {
                            None => LCell {
                                id: format!("{r}|{c}"),
                                ..Default::default()
                            },
                            Some(first) => LCell {
                                id: self.str(first, "id").unwrap_or_default(),
                                props: pm(first, "props"),
                                blocks: objs
                                    .iter()
                                    .filter_map(|o| self.obj(o, "t"))
                                    .flat_map(|t| self.read_flow(&t))
                                    .collect(),
                            },
                        }
                    })
                    .collect()
            })
            .collect();
        Some(LTable {
            id: tid.to_owned(),
            props: pm(&tm, "props"),
            cols,
            rows,
            cells,
        })
    }

    fn all_texts(&self) -> Vec<ObjId> {
        let mut out = vec![self.body()];
        if let Some(tables) = self.obj(&ROOT, "tables") {
            for t in self.doc.map_range(&tables, ..) {
                let tm = t.id();
                if let Some(cells) = self.obj(&tm, "cells") {
                    let keys: Vec<String> = self.doc.keys(&cells).collect();
                    for k in keys {
                        for c in self.cell_objs(&cells, &k) {
                            if let Some(txt) = self.obj(&c, "t") {
                                out.push(txt);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn locate(&self, pid: &str) -> Option<Loc> {
        if self.cache.borrow().is_none() {
            let mut m = HashMap::new();
            for text in self.all_texts() {
                let items = self.items(&text);
                for (i, it) in items.iter().enumerate() {
                    if it.id.starts_with("orphan-") {
                        continue;
                    }
                    let cursor = self.doc.get_cursor(&text, it.start, None).ok()?;
                    m.entry(it.id.clone()).or_insert(Loc {
                        text: text.clone(),
                        cursor,
                        next: items.get(i + 1).map(|n| n.id.clone()),
                    });
                }
            }
            *self.cache.borrow_mut() = Some(m);
        }
        self.cache.borrow().as_ref()?.get(pid).cloned()
    }

    fn pos(&self, l: &Loc) -> Option<usize> {
        self.doc.get_cursor_position(&l.text, &l.cursor, None).ok()
    }

    /// Marker index and content length of a paragraph.
    fn extent(&self, pid: &str) -> Option<(ObjId, usize, usize)> {
        let l = self.locate(pid)?;
        let m = self.pos(&l)?;
        let end = match l.next.as_deref().and_then(|n| self.locate(n)) {
            Some(n) => self.pos(&n)?,
            None => self.doc.length(&l.text),
        };
        Some((l.text, m, end.saturating_sub(m + 1)))
    }

    fn marker(&self, text: &ObjId, at: usize) -> Option<ObjId> {
        match self.doc.get(text, at).ok()?? {
            (Value::Object(ObjType::Map), id) => Some(id),
            _ => None,
        }
    }

    /// The flow range a block covers: a container covers every block that
    /// has it among its parents.
    fn range_of(items: &[Item], id: &str) -> Option<(usize, usize)> {
        let hit: Vec<&Item> = items
            .iter()
            .filter(|i| i.id == id || i.parents.iter().any(|p| p == id))
            .collect();
        Some((hit.first()?.start, hit.last()?.end))
    }

    /// The parents a block placed after `after` gets, and where it goes.
    fn slot_after(items: &[Item], after: &str) -> Option<(usize, Vec<String>)> {
        let (_, end) = Self::range_of(items, after)?;
        if let Some(i) = items.iter().find(|i| i.id == after) {
            return Some((end, i.parents.clone()));
        }
        let first = items
            .iter()
            .find(|i| i.parents.iter().any(|p| p == after))?;
        let k = first.parents.iter().position(|p| p == after)?;
        Some((end, first.parents[..k].to_vec()))
    }

    fn container_ids(&self) -> Vec<String> {
        self.obj(&ROOT, "containers")
            .map(|c| self.doc.keys(&c).collect())
            .unwrap_or_default()
    }
}

impl CrdtDoc for AutomergeDocument {
    const NAME: &'static str = "automerge";

    fn create(l: &LDoc, peer: Peer) -> Self {
        let mut me = Self::wrap(AutoCommit::new().with_actor(actor(peer)));
        let body = me.doc.put_object(ROOT, "body", ObjType::Text).unwrap();
        for k in ["tables", "containers", "styles", "authors"] {
            me.doc.put_object(ROOT, k, ObjType::Map).unwrap();
        }
        me.write_flow(&body, &l.body);
        let styles = me.obj(&ROOT, "styles").unwrap();
        for (k, v) in &l.styles {
            me.doc.put(&styles, k.as_str(), v.as_str()).unwrap();
        }
        me.doc.put(ROOT, "meta", l.meta.as_str()).unwrap();
        me.doc.commit();
        me
    }

    fn read(&self) -> LDoc {
        let body = self.read_flow(&self.body());
        let mut styles = BTreeMap::new();
        if let Some(s) = self.obj(&ROOT, "styles") {
            for item in self.doc.map_range(&s, ..) {
                if let automerge::ValueRef::Scalar(automerge::ScalarValueRef::Str(v)) = item.value {
                    styles.insert(item.key.to_string(), v.to_string());
                }
            }
        }
        LDoc {
            body,
            styles,
            meta: self.str(&ROOT, "meta").unwrap_or_default(),
        }
    }

    fn fork(&mut self, peer: Peer) -> Self {
        self.doc.commit();
        Self::wrap(self.doc.fork().with_actor(actor(peer)))
    }

    fn merge(&mut self, other: &mut Self) {
        self.doc.merge(&mut other.doc).expect("merge");
        self.invalidate();
    }

    fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }

    fn load(bytes: &[u8], peer: Peer) -> Self {
        Self::wrap(
            AutoCommit::load(bytes)
                .expect("load")
                .with_actor(actor(peer)),
        )
    }

    fn commit(&mut self) {
        self.doc.commit();
    }

    fn set_author(&mut self, name: &str) {
        let a = self.obj(&ROOT, "authors").unwrap();
        let me = self.doc.get_actor().to_hex_string();
        self.doc.put(&a, me.as_str(), name).unwrap();
    }

    fn author_at(&self, pid: &str, pos: usize) -> Option<String> {
        let (text, m, _) = self.extent(pid)?;
        let (_, id) = self.doc.get(&text, m + 1 + pos).ok()??;
        let automerge::ObjId::Id(_, actor, _) = id else {
            return None;
        };
        self.str(&self.obj(&ROOT, "authors")?, &actor.to_hex_string())
    }

    fn para_len(&self, pid: &str) -> Option<usize> {
        Some(self.extent(pid)?.2)
    }

    fn para(&self, pid: &str) -> Option<Para> {
        let l = self.locate(pid)?;
        let it = self.items(&l.text).into_iter().find(|i| i.id == pid)?;
        match self.item_node(&it) {
            LNode::Para(p) => Some(p),
            _ => None,
        }
    }

    fn insert_text(&mut self, pid: &str, pos: usize, text: &str) -> bool {
        let Some((t, m, len)) = self.extent(pid) else {
            return false;
        };
        self.doc
            .splice_text(&t, m + 1 + pos.min(len), 0, text)
            .is_ok()
    }

    fn delete_text(&mut self, pid: &str, pos: usize, len: usize) -> bool {
        let Some((t, m, n)) = self.extent(pid) else {
            return false;
        };
        let pos = pos.min(n);
        let len = len.min(n - pos) as isize;
        self.doc.splice_text(&t, m + 1 + pos, len, "").is_ok()
    }

    fn mark(
        &mut self,
        pid: &str,
        start: usize,
        end: usize,
        key: &str,
        value: Option<&str>,
    ) -> bool {
        let Some((t, m, n)) = self.extent(pid) else {
            return false;
        };
        let (start, end) = (m + 1 + start.min(n), m + 1 + end.min(n));
        if start >= end {
            return false;
        }
        match value {
            Some(v) => self
                .doc
                .mark(&t, Mark::new(key.to_owned(), v, start, end), expand(key))
                .is_ok(),
            None => self.doc.unmark(&t, key, start, end, expand(key)).is_ok(),
        }
    }

    fn set_para_prop(&mut self, pid: &str, key: &str, value: Option<&str>) -> bool {
        let Some((t, m, _)) = self.extent(pid) else {
            return false;
        };
        let Some(b) = self.marker(&t, m) else {
            return false;
        };
        let a = match self.obj(&b, "attrs") {
            Some(a) => a,
            None => self.doc.put_object(&b, "attrs", ObjType::Map).unwrap(),
        };
        match value {
            Some(v) => self.doc.put(&a, key, v).is_ok(),
            None => self.doc.delete(&a, key).is_ok(),
        }
    }

    fn set_container_prop(&mut self, id: &str, key: &str, value: Option<&str>) -> bool {
        if self.locate(id).is_some() {
            return self.set_para_prop(id, key, value);
        }
        let Some(c) = self
            .obj(&ROOT, "containers")
            .and_then(|cs| self.obj(&cs, id))
        else {
            return false;
        };
        let Some(p) = self.obj(&c, "props") else {
            return false;
        };
        match value {
            Some(v) => self.doc.put(&p, key, v).is_ok(),
            None => self.doc.delete(&p, key).is_ok(),
        }
    }

    fn split_para(&mut self, pid: &str, pos: usize, new_id: &str) -> bool {
        let Some((t, m, n)) = self.extent(pid) else {
            return false;
        };
        let Some(old) = self.marker(&t, m) else {
            return false;
        };
        let at = m + 1 + pos.min(n);
        let parents: Vec<String> = self
            .obj(&old, "parents")
            .map(|p| self.strings(&p))
            .unwrap_or_default();
        let attrs = self
            .obj(&old, "attrs")
            .map(|a| self.props(&a))
            .unwrap_or_default();
        // The text after the split point stays where it is; only a marker
        // is inserted, so concurrent edits to the tail follow it.
        let b = self.doc.split_block(&t, at).unwrap();
        self.doc.put(&b, "type", "p").unwrap();
        self.doc.put(&b, "id", new_id).unwrap();
        let ps = self.doc.put_object(&b, "parents", ObjType::List).unwrap();
        for (i, p) in parents.iter().enumerate() {
            self.doc.insert(&ps, i, p.as_str()).unwrap();
        }
        let a = self.doc.put_object(&b, "attrs", ObjType::Map).unwrap();
        let mut attrs = attrs;
        attrs.remove("@h");
        self.write_props(&a, &attrs);
        // Keep the cache: the new paragraph sits between pid and its next.
        if let Some(c) = self.cache.borrow_mut().as_mut()
            && let Ok(cursor) = self.doc.get_cursor(&t, at, None)
        {
            let next = c.get(pid).and_then(|l| l.next.clone());
            c.insert(
                new_id.to_owned(),
                Loc {
                    text: t.clone(),
                    cursor,
                    next,
                },
            );
            if let Some(l) = c.get_mut(pid) {
                l.next = Some(new_id.to_owned());
            }
        }
        true
    }

    fn insert_para_after(&mut self, after: &str, para: &Para) -> bool {
        let texts = self.all_texts();
        for t in texts {
            let items = self.items(&t);
            if let Some((at, parents)) = Self::slot_after(&items, after) {
                self.write_item(&t, at, &parents, &LNode::Para(para.clone()));
                self.invalidate();
                return true;
            }
        }
        false
    }

    fn delete_block(&mut self, id: &str) -> bool {
        for t in self.all_texts() {
            let items = self.items(&t);
            if let Some((s, e)) = Self::range_of(&items, id) {
                self.doc.splice_text(&t, s, (e - s) as isize, "").unwrap();
                self.invalidate();
                return true;
            }
        }
        false
    }

    fn move_block_after(&mut self, id: &str, after: &str) -> bool {
        let t = self.body();
        let items = self.items(&t);
        let Some((s, e)) = Self::range_of(&items, id) else {
            return false;
        };
        let Some((at, dest_parents)) = Self::slot_after(&items, after) else {
            return false;
        };
        if at > s && at < e {
            return false;
        }
        // The moved blocks keep everything from the moved block inward and
        // take the destination's ancestors instead of their old ones.
        let moved: Vec<&Item> = items
            .iter()
            .filter(|i| i.start >= s && i.end <= e && i.start < e)
            .collect();
        let cut = match moved.first() {
            Some(first) if first.id == id => first.parents.len(),
            Some(first) => first.parents.iter().position(|p| p == id).unwrap_or(0),
            None => return false,
        };
        let mut pos = at;
        for it in &moved {
            let mut parents = dest_parents.clone();
            parents.extend(it.parents[cut..].iter().cloned());
            let node = match it.kind.as_str() {
                "e" => LNode::Embed {
                    id: it.id.clone(),
                    json: it.json.clone().unwrap_or_default(),
                },
                // The table object stays put; only its marker moves.
                "t" => {
                    let b = self.doc.split_block(&t, pos).unwrap();
                    self.doc.put(&b, "type", "t").unwrap();
                    self.doc.put(&b, "id", it.id.as_str()).unwrap();
                    let ps = self.doc.put_object(&b, "parents", ObjType::List).unwrap();
                    for (i, p) in parents.iter().enumerate() {
                        self.doc.insert(&ps, i, p.as_str()).unwrap();
                    }
                    pos += 1;
                    continue;
                }
                _ => LNode::Para(Para {
                    id: it.id.clone(),
                    props: it.attrs.clone(),
                    runs: it.runs.clone(),
                }),
            };
            pos = self.write_item(&t, pos, &parents, &node);
        }
        let inserted = pos - at;
        let (ds, de) = if at <= s {
            (s + inserted, e + inserted)
        } else {
            (s, e)
        };
        self.doc
            .splice_text(&t, ds, (de - ds) as isize, "")
            .unwrap();
        self.invalidate();
        let _ = self.container_ids();
        true
    }

    fn table(&self, tid: &str) -> Option<LTable> {
        self.read_table(tid)
    }

    fn insert_row_raw(
        &mut self,
        tid: &str,
        index: usize,
        row: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool {
        let Some(tm) = self.table_obj(tid) else {
            return false;
        };
        let (Some(rows), Some(rp), Some(co)) = (
            self.obj(&tm, "rows"),
            self.obj(&tm, "rp"),
            self.obj(&tm, "cells"),
        ) else {
            return false;
        };
        let n = self.doc.length(&rows);
        self.doc.insert(&rows, index.min(n), row.0).unwrap();
        let m = self.doc.put_object(&rp, row.0, ObjType::Map).unwrap();
        self.write_props(&m, row.1);
        for (col, c) in cells {
            self.write_cell(&co, row.0, col, c);
        }
        self.invalidate();
        true
    }

    fn delete_row(&mut self, tid: &str, row_id: &str) -> bool {
        self.delete_line(tid, row_id, "rows")
    }

    fn insert_col_raw(
        &mut self,
        tid: &str,
        index: usize,
        col: (&str, &Props),
        cells: &[(String, LCell)],
    ) -> bool {
        let Some(tm) = self.table_obj(tid) else {
            return false;
        };
        let (Some(cols), Some(cp), Some(co)) = (
            self.obj(&tm, "cols"),
            self.obj(&tm, "cp"),
            self.obj(&tm, "cells"),
        ) else {
            return false;
        };
        let n = self.doc.length(&cols);
        self.doc.insert(&cols, index.min(n), col.0).unwrap();
        let m = self.doc.put_object(&cp, col.0, ObjType::Map).unwrap();
        self.write_props(&m, col.1);
        for (row, c) in cells {
            self.write_cell(&co, row, col.0, c);
        }
        self.invalidate();
        true
    }

    fn delete_col(&mut self, tid: &str, col_id: &str) -> bool {
        self.delete_line(tid, col_id, "cols")
    }

    fn cell_insert_text(
        &mut self,
        tid: &str,
        row_id: &str,
        col_id: &str,
        pos: usize,
        text: &str,
    ) -> bool {
        let Some(tm) = self.table_obj(tid) else {
            return false;
        };
        let Some(co) = self.obj(&tm, "cells") else {
            return false;
        };
        let key = format!("{row_id}|{col_id}");
        let t = match self
            .cell_objs(&co, &key)
            .first()
            .and_then(|c| self.obj(c, "t"))
        {
            Some(t) => t,
            None => {
                let me = self.doc.get_actor().to_hex_string();
                let c = LCell {
                    id: format!("{key}|{me}"),
                    ..Default::default()
                };
                self.write_cell(&co, row_id, col_id, &c);
                self.invalidate();
                let c = self.cell_objs(&co, &key);
                // Read back what we just wrote: ours is the winner locally.
                match c.first().and_then(|c| self.obj(c, "t")) {
                    Some(t) => t,
                    None => return false,
                }
            }
        };
        let items = self.items(&t);
        let (m, len) = match items.iter().find(|i| i.kind == "p") {
            Some(i) => (i.start, i.end - i.start - 1),
            None => {
                let me = self.doc.get_actor().to_hex_string();
                let p = Para {
                    id: format!("{key}|p{me}"),
                    ..Default::default()
                };
                self.write_item(&t, 0, &[], &LNode::Para(p));
                self.invalidate();
                (0, 0)
            }
        };
        self.doc
            .splice_text(&t, m + 1 + pos.min(len), 0, text)
            .is_ok()
    }
}

impl AutomergeDocument {
    fn delete_line(&mut self, tid: &str, id: &str, list: &str) -> bool {
        let Some(tm) = self.table_obj(tid) else {
            return false;
        };
        let Some(l) = self.obj(&tm, list) else {
            return false;
        };
        let Some(i) = self.strings(&l).iter().position(|x| x == id) else {
            return false;
        };
        self.doc.delete(&l, i).unwrap();
        self.invalidate();
        true
    }
}
