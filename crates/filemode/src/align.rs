// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Give a returned document's nodes the IDs they have in skriv.
//!
//! Paragraphs, headings and sections come back with their `xml:id`.
//! LibreOffice drops the IDs of lists, list items, tables, rows, columns
//! and cells, and the codec mints fresh ones on import. Those containers
//! are recognised by the paragraph IDs inside them: a returned list item
//! holding paragraph `p7` is the list item that held `p7`. Rows and columns
//! are matched by the paragraphs in their cells, and a cell is the cell at
//! its matched row and column.
//!
//! What has no surviving paragraph inside (a container emptied or created
//! in LibreOffice, a paragraph that lost its ID) is matched by kind,
//! position and text similarity among the nodes left over under the same
//! matched parent. Inline-object-only blocks (embeds) are matched by
//! content.
//!
//! A paragraph ID can come back twice (copy and paste in LibreOffice copies
//! the `xml:id`, or a split leaves it on both halves). The copy most like
//! the original keeps it; the others get fresh IDs.

use std::collections::{HashMap, HashSet};

use skriv_crdt::logical::{LCell, LDoc, LNode, LTable, Para};

use crate::similarity;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct AlignStats {
    /// Returned paragraphs whose ID was found in skriv's document.
    pub paras_by_id: usize,
    /// Paragraphs matched by text (their ID was missing).
    pub paras_by_text: usize,
    /// Paragraph IDs that came back more than once; the extra copies got new IDs.
    pub duplicate_ids: usize,
    /// Containers (sections, lists, items, tables) whose ID survived.
    pub containers_by_id: usize,
    /// Containers recognised by the paragraphs inside them.
    pub containers_by_paras: usize,
    /// Containers matched by position and text.
    pub containers_by_text: usize,
    pub rows_matched: usize,
    pub cols_matched: usize,
    pub embeds_matched: usize,
    /// Returned nodes that are new to skriv.
    pub new_nodes: usize,
}

/// Where the paragraphs of a document are: for each paragraph ID, the
/// nodes enclosing it, as (kind, ID): containers by their kind, `table`,
/// `row` and `col` (IDs prefixed `row:` / `col:`), `cell`.
fn owners(doc: &[LNode]) -> HashMap<String, Vec<(String, String)>> {
    type Chain = Vec<(String, String)>;
    fn walk(v: &[LNode], chain: &mut Chain, out: &mut HashMap<String, Chain>) {
        for n in v {
            match n {
                LNode::Para(p) => {
                    out.entry(p.id.clone()).or_insert_with(|| chain.clone());
                }
                LNode::Embed { .. } => {}
                LNode::Container {
                    id, kind, children, ..
                } => {
                    chain.push((kind.clone(), id.clone()));
                    walk(children, chain, out);
                    chain.pop();
                }
                LNode::Table(t) => {
                    chain.push(("table".into(), t.id.clone()));
                    for (ri, row) in t.cells.iter().enumerate() {
                        for (ci, c) in row.iter().enumerate() {
                            chain.push(("row".into(), format!("row:{}", t.rows[ri].0)));
                            chain.push(("col".into(), format!("col:{}", t.cols[ci].0)));
                            chain.push(("cell".into(), c.id.clone()));
                            walk(&c.blocks, chain, out);
                            chain.truncate(chain.len() - 3);
                        }
                    }
                    chain.pop();
                }
            }
        }
    }
    let mut out = HashMap::new();
    walk(doc, &mut Vec::new(), &mut out);
    out
}

fn para_texts(doc: &LDoc) -> HashMap<String, String> {
    doc.paras()
        .into_iter()
        .map(|p| (p.id.clone(), p.text()))
        .collect()
}

fn fresh(prefix: &str) -> String {
    format!("{prefix}{}", uuid::Uuid::now_v7())
}

/// Rename the nodes of `ret` (a returned file, imported) to the IDs their
/// counterparts have in `base`.
pub fn align(base: &LDoc, ret: &mut LDoc) -> AlignStats {
    let mut st = AlignStats::default();
    let base_texts = para_texts(base);

    // 1. Duplicate paragraph IDs: the copy most like the original keeps it.
    let mut occurrences: HashMap<String, Vec<String>> = HashMap::new();
    for p in ret.paras() {
        occurrences.entry(p.id.clone()).or_default().push(p.text());
    }
    let mut keep: HashMap<String, usize> = HashMap::new();
    for (id, texts) in &occurrences {
        if texts.len() > 1 {
            st.duplicate_ids += texts.len() - 1;
            let orig = base_texts.get(id).map(String::as_str).unwrap_or("");
            let best = texts
                .iter()
                .enumerate()
                .max_by(|a, b| {
                    similarity(a.1, orig)
                        .partial_cmp(&similarity(b.1, orig))
                        .unwrap()
                        .then(b.0.cmp(&a.0))
                })
                .map(|x| x.0)
                .unwrap_or(0);
            keep.insert(id.clone(), best);
        }
    }
    if !keep.is_empty() {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for_each_para_mut(&mut ret.body, &mut |p| {
            if let Some(&k) = keep.get(&p.id) {
                let n = seen.entry(p.id.clone()).or_insert(0);
                if *n != k {
                    p.id = fresh("sk-");
                }
                *n += 1;
            }
        });
    }

    // 2. Containers, tables, rows, columns and cells, by the paragraphs inside.
    let base_owners = owners(&base.body);
    let base_nodes = index_nodes(&base.body);
    let mut claimed: HashSet<String> = HashSet::new();
    let mut renames: HashMap<String, String> = HashMap::new();
    {
        let ret_owners = owners(&ret.body);
        // Votes: returned owner ID → base owner ID → weight. A paragraph
        // speaks mostly for the containers closest to it: a list item that
        // was split in two is the half holding the item's own paragraph,
        // not the half its sub-list moved into.
        let mut votes: HashMap<String, HashMap<String, f64>> = HashMap::new();
        for (pid, chain) in &ret_owners {
            let Some(bchain) = base_owners.get(pid) else {
                continue;
            };
            for (ri, (rk, r)) in chain.iter().enumerate() {
                for (bi, (bk, b)) in bchain.iter().enumerate() {
                    if rk == bk {
                        let dist = (chain.len() - 1 - ri) + (bchain.len() - 1 - bi);
                        let w = 0.25f64.powi(dist as i32);
                        *votes
                            .entry(r.clone())
                            .or_default()
                            .entry(b.clone())
                            .or_insert(0.0) += w;
                    }
                }
            }
        }
        // Greedy, strongest first, one to one.
        let mut pairs: Vec<(f64, String, String)> = votes
            .into_iter()
            .flat_map(|(r, m)| m.into_iter().map(move |(b, n)| (n, r.clone(), b)))
            .collect();
        pairs.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap()
                .then(a.1.cmp(&b.1))
                .then(a.2.cmp(&b.2))
        });
        let mut done: HashSet<String> = HashSet::new();
        for (_, r, b) in pairs {
            if done.contains(&r) || claimed.contains(&b) {
                continue;
            }
            done.insert(r.clone());
            claimed.insert(b.clone());
            if r != b {
                renames.insert(r, b);
            }
        }
    }
    let ret_nodes_before = index_nodes(&ret.body);
    for id in ret_nodes_before.keys() {
        if base_nodes.contains_key(id) && !renames.contains_key(id) && !claimed.contains(id) {
            // The ID itself survived (sections keep theirs).
            claimed.insert(id.clone());
            st.containers_by_id += 1;
        }
    }
    rename_nodes(&mut ret.body, &renames, &mut st);

    // 3. What is left: match by kind, position and text under the same parent.
    let mut used_paras: HashSet<String> = ret.paras().iter().map(|p| p.id.clone()).collect();
    used_paras.retain(|id| base_texts.contains_key(id));
    st.paras_by_id = used_paras.len();
    let mut ctx = Leftover {
        base_nodes: &base_nodes,
        claimed: &mut claimed,
        used_paras: &mut used_paras,
        st: &mut st,
    };
    ctx.children(&base.body, &mut ret.body);
    st
}

fn for_each_para_mut(v: &mut [LNode], f: &mut dyn FnMut(&mut Para)) {
    for n in v {
        match n {
            LNode::Para(p) => f(p),
            LNode::Container { children, .. } => for_each_para_mut(children, f),
            LNode::Table(t) => {
                for row in &mut t.cells {
                    for c in row {
                        for_each_para_mut(&mut c.blocks, f);
                    }
                }
            }
            LNode::Embed { .. } => {}
        }
    }
}

/// Every non-paragraph node by ID, with its kind.
fn index_nodes(v: &[LNode]) -> HashMap<String, String> {
    fn walk(v: &[LNode], out: &mut HashMap<String, String>) {
        for n in v {
            match n {
                LNode::Para(_) => {}
                LNode::Embed { id, .. } => {
                    out.insert(id.clone(), "embed".into());
                }
                LNode::Container {
                    id, kind, children, ..
                } => {
                    out.insert(id.clone(), kind.clone());
                    walk(children, out);
                }
                LNode::Table(t) => {
                    out.insert(t.id.clone(), "table".into());
                    for (r, _) in &t.rows {
                        out.insert(format!("row:{r}"), "row".into());
                    }
                    for (c, _) in &t.cols {
                        out.insert(format!("col:{c}"), "col".into());
                    }
                    for row in &t.cells {
                        for c in row {
                            out.insert(c.id.clone(), "cell".into());
                            walk(&c.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = HashMap::new();
    walk(v, &mut out);
    out
}

fn rename_nodes(v: &mut [LNode], renames: &HashMap<String, String>, st: &mut AlignStats) {
    for n in v {
        match n {
            LNode::Para(_) | LNode::Embed { .. } => {}
            LNode::Container { id, children, .. } => {
                if let Some(b) = renames.get(id) {
                    *id = b.clone();
                    st.containers_by_paras += 1;
                }
                rename_nodes(children, renames, st);
            }
            LNode::Table(t) => {
                if let Some(b) = renames.get(&t.id) {
                    t.id = b.clone();
                    st.containers_by_paras += 1;
                }
                for (r, _) in &mut t.rows {
                    if let Some(b) = renames.get(&format!("row:{r}")) {
                        *r = b["row:".len()..].to_owned();
                        st.rows_matched += 1;
                    }
                }
                for (c, _) in &mut t.cols {
                    if let Some(b) = renames.get(&format!("col:{c}")) {
                        *c = b["col:".len()..].to_owned();
                        st.cols_matched += 1;
                    }
                }
                for row in &mut t.cells {
                    for c in row {
                        if let Some(b) = renames.get(&c.id) {
                            c.id = b.clone();
                        }
                        rename_nodes(&mut c.blocks, renames, st);
                    }
                }
            }
        }
    }
}

struct Leftover<'a> {
    base_nodes: &'a HashMap<String, String>,
    claimed: &'a mut HashSet<String>,
    used_paras: &'a mut HashSet<String>,
    st: &'a mut AlignStats,
}

fn text_of(n: &LNode) -> String {
    match n {
        LNode::Para(p) => p.text(),
        LNode::Embed { json, .. } => json.clone(),
        LNode::Container { children, .. } => {
            children.iter().map(text_of).collect::<Vec<_>>().join("\n")
        }
        LNode::Table(t) => t
            .cells
            .iter()
            .flatten()
            .map(LCell::text)
            .collect::<Vec<_>>()
            .join("\t"),
    }
}

fn same_kind(a: &LNode, b: &LNode) -> bool {
    match (a, b) {
        (LNode::Para(_), LNode::Para(_)) => true,
        (LNode::Embed { .. }, LNode::Embed { .. }) => true,
        (LNode::Table(_), LNode::Table(_)) => true,
        (LNode::Container { kind: x, .. }, LNode::Container { kind: y, .. }) => x == y,
        _ => false,
    }
}

impl Leftover<'_> {
    fn is_known(&self, n: &LNode) -> bool {
        match n {
            LNode::Para(p) => self.used_paras.contains(&p.id),
            other => self.claimed.contains(other.id()) && self.base_nodes.contains_key(other.id()),
        }
    }

    /// Match leftovers in `ret` against leftovers in `base` (siblings under
    /// matched parents), gap by gap between nodes already matched, then
    /// descend into matched pairs.
    fn children(&mut self, base: &[LNode], ret: &mut [LNode]) {
        let base_pos: HashMap<&str, usize> =
            base.iter().enumerate().map(|(i, n)| (n.id(), i)).collect();
        // Anchors: returned nodes already matched to a sibling here.
        let mut gaps: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
        let mut cur_ret: Vec<usize> = Vec::new();
        let mut last_base = 0usize;
        for (i, n) in ret.iter().enumerate() {
            match base_pos.get(n.id()) {
                Some(&bi) if self.is_known(n) => {
                    let bs: Vec<usize> = (last_base..bi.max(last_base)).collect();
                    gaps.push((bs, std::mem::take(&mut cur_ret)));
                    last_base = bi + 1;
                }
                _ => cur_ret.push(i),
            }
        }
        gaps.push(((last_base..base.len()).collect(), cur_ret));
        let ret_ids: HashSet<String> = ret.iter().map(|n| n.id().to_owned()).collect();
        for (bs, rs) in gaps {
            let mut taken: HashSet<usize> = HashSet::new();
            for ri in rs {
                let r = &ret[ri];
                if self.is_known(r) {
                    continue;
                }
                let rt = text_of(r);
                let mut best: Option<(f64, usize)> = None;
                for &bi in &bs {
                    let b = &base[bi];
                    if taken.contains(&bi) || ret_ids.contains(b.id()) || !same_kind(b, r) {
                        continue;
                    }
                    if !matches!(b, LNode::Para(_)) && self.claimed.contains(b.id()) {
                        continue;
                    }
                    if matches!(b, LNode::Para(_)) && self.used_paras.contains(&b.id().to_owned()) {
                        continue;
                    }
                    let s = match (r, b) {
                        (LNode::Embed { json: rj, .. }, LNode::Embed { json: bj, .. }) => {
                            if crate::noise::same_obj(rj, bj) {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        _ => similarity(&text_of(b), &rt),
                    };
                    let threshold = if matches!(r, LNode::Embed { .. }) {
                        1.0
                    } else {
                        0.5
                    };
                    if s >= threshold && best.is_none_or(|(x, _)| s > x) {
                        best = Some((s, bi));
                    }
                }
                match best {
                    Some((_, bi)) => {
                        taken.insert(bi);
                        let bid = base[bi].id().to_owned();
                        match &mut ret[ri] {
                            LNode::Para(p) => {
                                p.id = bid.clone();
                                self.used_paras.insert(bid);
                                self.st.paras_by_text += 1;
                            }
                            LNode::Embed { id, .. } => {
                                *id = bid.clone();
                                self.claimed.insert(bid);
                                self.st.embeds_matched += 1;
                            }
                            LNode::Container { id, .. } => {
                                *id = bid.clone();
                                self.claimed.insert(bid);
                                self.st.containers_by_text += 1;
                            }
                            LNode::Table(t) => {
                                t.id = bid.clone();
                                self.claimed.insert(bid);
                                self.st.containers_by_text += 1;
                            }
                        }
                    }
                    None => {
                        self.st.new_nodes += 1;
                        // A new node must not carry an ID skriv already uses.
                        if self.base_nodes.contains_key(ret[ri].id()) {
                            set_id(&mut ret[ri], fresh("sk-"));
                        }
                    }
                }
            }
        }
        // Descend into matched pairs.
        for r in ret.iter_mut() {
            let Some(&bi) = base_pos.get(r.id()) else {
                continue;
            };
            match (&base[bi], r) {
                (LNode::Container { children: bc, .. }, LNode::Container { children: rc, .. }) => {
                    self.children(bc, rc)
                }
                (LNode::Table(bt), LNode::Table(rt)) => self.table(bt, rt),
                _ => {}
            }
        }
    }

    fn table(&mut self, bt: &LTable, rt: &mut LTable) {
        // Rows and columns the paragraph votes left unmatched: by position
        // and content among the leftovers.
        let base_rows: HashSet<&str> = bt.rows.iter().map(|r| r.0.as_str()).collect();
        let base_cols: HashSet<&str> = bt.cols.iter().map(|c| c.0.as_str()).collect();
        let row_text = |t: &LTable, i: usize| {
            t.cells[i]
                .iter()
                .map(LCell::text)
                .collect::<Vec<_>>()
                .join("\t")
        };
        let used_rows: HashSet<String> = rt
            .rows
            .iter()
            .filter(|r| base_rows.contains(r.0.as_str()))
            .map(|r| r.0.clone())
            .collect();
        let mut taken: HashSet<String> = used_rows.clone();
        for ri in 0..rt.rows.len() {
            if base_rows.contains(rt.rows[ri].0.as_str()) {
                continue;
            }
            let txt = row_text(rt, ri);
            let mut best: Option<(f64, usize)> = None;
            for bi in 0..bt.rows.len() {
                if taken.contains(&bt.rows[bi].0) {
                    continue;
                }
                let s = similarity(&row_text(bt, bi), &txt);
                // An empty row matches an empty row at the same position only.
                let s = if txt.trim().is_empty() && row_text(bt, bi).trim().is_empty() {
                    if bi == ri { 1.0 } else { 0.0 }
                } else {
                    s
                };
                if s >= 0.5 && best.is_none_or(|(x, _)| s > x) {
                    best = Some((s, bi));
                }
            }
            if let Some((_, bi)) = best {
                taken.insert(bt.rows[bi].0.clone());
                rt.rows[ri].0 = bt.rows[bi].0.clone();
                self.st.rows_matched += 1;
            }
        }
        // Columns: only by position when the column count is unchanged.
        if bt.cols.len() == rt.cols.len() {
            for ci in 0..rt.cols.len() {
                if !base_cols.contains(rt.cols[ci].0.as_str())
                    && !rt.cols.iter().any(|c| c.0 == bt.cols[ci].0)
                {
                    rt.cols[ci].0 = bt.cols[ci].0.clone();
                    self.st.cols_matched += 1;
                }
            }
        }
        // Cells: the base cell at the matched row and column.
        let bpos: HashMap<(&str, &str), &LCell> = bt
            .rows
            .iter()
            .enumerate()
            .flat_map(|(ri, r)| {
                bt.cols
                    .iter()
                    .enumerate()
                    .map(move |(ci, c)| ((r.0.as_str(), c.0.as_str()), &bt.cells[ri][ci]))
            })
            .collect();
        for ri in 0..rt.rows.len() {
            for ci in 0..rt.cols.len() {
                let key = (rt.rows[ri].0.as_str(), rt.cols[ci].0.as_str());
                if let Some(bc) = bpos.get(&key) {
                    let rc = &mut rt.cells[ri][ci];
                    rc.id = bc.id.clone();
                    let blocks = std::mem::take(&mut rc.blocks);
                    let mut blocks = blocks;
                    self.children(&bc.blocks, &mut blocks);
                    rt.cells[ri][ci].blocks = blocks;
                } else {
                    let rc = &mut rt.cells[ri][ci];
                    if self.base_nodes.contains_key(&rc.id) {
                        rc.id = fresh("sk-");
                    }
                }
            }
        }
    }
}

fn set_id(n: &mut LNode, id: String) {
    match n {
        LNode::Para(p) => p.id = id,
        LNode::Embed { id: i, .. } | LNode::Container { id: i, .. } => *i = id,
        LNode::Table(t) => t.id = id,
    }
}
