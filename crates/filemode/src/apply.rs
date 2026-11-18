// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Turn the differences between skriv's document and a returned file into
//! CRDT operations, so the returned file becomes a branch whose changes are
//! real edits: attributed, mergeable, reviewable one by one.
//!
//! `target` must be aligned to `base` first ([`crate::align`]). The
//! operations are chosen to look like what the person did:
//!
//! - text is diffed per character, so typing a word is an insertion of
//!   that word, not a rewritten paragraph;
//! - a paragraph that now holds its old text plus the next paragraph's is a
//!   join (the paragraph break was deleted), and a new paragraph that holds
//!   the tail of the one before it is a split (Enter was pressed), so the
//!   moved characters keep their authors;
//! - siblings that changed order are moved, not deleted and re-created.
//!
//! What this can't express is counted in [`ApplyStats::unsupported`].

use std::collections::{BTreeMap, HashMap, HashSet};

use skriv_crdt::CrdtDoc;
use skriv_crdt::logical::{LCell, LDoc, LNode, LTable, Para, Props};
use skriv_crdt::loro_flow_backend::LoroFlowDocument;

use crate::noise::{self, Scope};
use crate::similarity;

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct ApplyStats {
    pub joins: usize,
    pub splits: usize,
    pub blocks_inserted: usize,
    pub blocks_deleted: usize,
    pub blocks_moved: usize,
    pub chars_inserted: usize,
    pub chars_deleted: usize,
    pub mark_ops: usize,
    pub prop_ops: usize,
    pub rows_inserted: usize,
    pub rows_deleted: usize,
    pub cols_inserted: usize,
    pub cols_deleted: usize,
    pub embeds_replaced: usize,
    /// Operations that failed or changes there is no operation for.
    pub unsupported: Vec<String>,
}

pub fn apply(d: &mut LoroFlowDocument, base: &LDoc, target: &LDoc) -> ApplyStats {
    let mut st = ApplyStats::default();
    let mut ctx = Ctx {
        base: index(&base.body),
        target_ids: index(&target.body).into_keys().map(str::to_owned).collect(),
        st: &mut st,
    };
    ctx.children(d, None, &base.body, &target.body);
    d.commit();
    st
}

/// Every node by ID, at any depth (cells' blocks included).
fn index(v: &[LNode]) -> HashMap<&str, &LNode> {
    fn walk<'a>(v: &'a [LNode], out: &mut HashMap<&'a str, &'a LNode>) {
        for n in v {
            out.insert(n.id(), n);
            match n {
                LNode::Container { children, .. } => walk(children, out),
                LNode::Table(t) => {
                    for row in &t.cells {
                        for c in row {
                            walk(&c.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = HashMap::new();
    walk(v, &mut out);
    out
}

fn trace(what: std::fmt::Arguments) {
    if std::env::var_os("SKRIV_TRACE_APPLY").is_some() {
        eprintln!("apply: {what}");
    }
}

fn para_of(n: &LNode) -> Option<&Para> {
    match n {
        LNode::Para(p) => Some(p),
        _ => None,
    }
}

struct Ctx<'a, 'b> {
    /// skriv's document, by ID: where a node that moved came from.
    base: HashMap<&'a str, &'a LNode>,
    /// Every ID in the target: a node missing under one parent but present
    /// under another has moved, not gone.
    target_ids: HashSet<String>,
    st: &'b mut ApplyStats,
}

impl Ctx<'_, '_> {
    fn in_base(&self, id: &str) -> bool {
        self.base.contains_key(id)
    }

    /// Bring the children of one parent (`None`: the body) from `base` to `target`.
    fn children(
        &mut self,
        d: &mut LoroFlowDocument,
        parent: Option<&str>,
        base: &[LNode],
        target: &[LNode],
    ) {
        let bset: HashSet<&str> = base.iter().map(LNode::id).collect();
        let tmap: HashMap<&str, &LNode> = target.iter().map(|n| (n.id(), n)).collect();

        // 1. Joins: a paragraph gone from the target whose text now ends
        //    the paragraph before it.
        let mut joined: HashSet<String> = HashSet::new();
        let mut i = 1;
        while i < base.len() {
            let Some(p) = para_of(&base[i - 1]) else {
                i += 1;
                continue;
            };
            let Some(LNode::Para(tp)) = tmap.get(p.id.as_str()) else {
                i += 1;
                continue;
            };
            let tt = tp.text();
            let mut acc = p.text();
            let mut j = i;
            while j < base.len() {
                let Some(x) = para_of(&base[j]) else { break };
                if self.target_ids.contains(&x.id) {
                    break;
                }
                let joined_text = format!("{acc}{}", x.text());
                let with = similarity(&tt, &joined_text);
                let without = similarity(&tt, &acc);
                if with >= 0.7 && with > without + 0.02 && d.join_para(&x.id) {
                    trace(format_args!("join {}", x.id));
                    self.st.joins += 1;
                    joined.insert(x.id.clone());
                    acc = joined_text;
                    j += 1;
                } else {
                    break;
                }
            }
            i = j.max(i + 1);
        }

        // 2. Splits: a new paragraph holding the tail of the one before it.
        let mut created: HashSet<String> = HashSet::new();
        // Base order of split-off paragraphs: just after their origin.
        let mut order: HashMap<String, f64> = base
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id().to_owned(), i as f64))
            .collect();
        let mut j = 1;
        while j < target.len() {
            let Some(tp) = para_of(&target[j - 1]) else {
                j += 1;
                continue;
            };
            if !bset.contains(tp.id.as_str()) {
                j += 1;
                continue;
            }
            let Some(cur) = d.para(&tp.id) else {
                j += 1;
                continue;
            };
            let mut origin = tp.id.clone();
            let mut head = tp.text();
            let mut rest = cur.text();
            let mut k = j;
            while k < target.len() {
                let Some(y) = para_of(&target[k]) else { break };
                if self.in_base(&y.id) {
                    break;
                }
                let yt = y.text();
                let with = similarity(&rest, &format!("{head}{yt}"));
                let without = similarity(&rest, &head);
                if !(with >= 0.7 && with > without + 0.02) {
                    break;
                }
                let at = split_point(&rest, &head, &yt);
                if !d.split_para(&origin, at, &y.id) {
                    break;
                }
                trace(format_args!("split {origin} at {at} -> {}", y.id));
                self.st.splits += 1;
                created.insert(y.id.clone());
                let o = order.get(&origin).copied().unwrap_or(0.0);
                order.insert(y.id.clone(), o + 0.001);
                rest = rest.chars().skip(at).collect();
                origin = y.id.clone();
                head = yt;
                k += 1;
            }
            j = k.max(j + 1);
        }

        // 3. Deletions: gone from the whole target, not just from here.
        for n in base {
            let id = n.id();
            if !self.target_ids.contains(id) && !joined.contains(id) {
                trace(format_args!("delete {id}"));
                if d.delete_block(id) {
                    self.st.blocks_deleted += 1;
                } else {
                    self.st.unsupported.push(format!("delete {id}"));
                }
            }
        }

        // 4. Placement. Siblings outside the longest run in base order
        //    move; nodes from elsewhere move in; new nodes are inserted.
        let kept: Vec<(usize, f64)> = target
            .iter()
            .enumerate()
            .filter_map(|(ti, n)| {
                let id = n.id();
                (bset.contains(id) || created.contains(id)).then(|| (ti, order[id]))
            })
            .collect();
        let stay: HashSet<usize> = lis(&kept.iter().map(|x| x.1).collect::<Vec<_>>())
            .into_iter()
            .map(|i| kept[i].0)
            .collect();
        let mut prev: Option<String> = None;
        let mut moved_in: Vec<&LNode> = Vec::new();
        for (ti, n) in target.iter().enumerate() {
            let id = n.id();
            let here = bset.contains(id) || created.contains(id);
            if here && stay.contains(&ti) {
                // In place.
            } else if here || self.in_base(id) {
                if self.move_to(d, parent, prev.as_deref(), id) {
                    self.st.blocks_moved += 1;
                } else {
                    self.st.unsupported.push(format!("move {id}"));
                }
                if !here {
                    moved_in.push(n);
                }
            } else {
                let next = target[ti + 1..]
                    .iter()
                    .map(LNode::id)
                    .find(|x| bset.contains(x) || created.contains(*x));
                self.place_new(d, parent, prev.as_deref(), next, n);
            }
            prev = Some(id.to_owned());
        }

        // 5. Content of everything that existed (here or elsewhere).
        for n in target {
            let id = n.id();
            if created.contains(id) {
                if let LNode::Para(tp) = n {
                    sync_para(d, tp, self.st);
                }
                continue;
            }
            if let Some(b) = self.base.get(id).copied() {
                self.content(d, b, n);
            }
        }
        let _ = moved_in;
    }

    /// Move `id` to just after `prev`, or to the front of `parent`.
    fn move_to(
        &mut self,
        d: &mut LoroFlowDocument,
        parent: Option<&str>,
        prev: Option<&str>,
        id: &str,
    ) -> bool {
        trace(format_args!("move {id} after {prev:?} in {parent:?}"));
        match prev {
            Some(p) => d.move_block_after(id, p),
            None => d.move_first(parent, id),
        }
    }

    /// Insert a node skriv doesn't have. A container is inserted empty and
    /// filled child by child, so nodes that moved into it keep their identity.
    fn place_new(
        &mut self,
        d: &mut LoroFlowDocument,
        parent: Option<&str>,
        prev: Option<&str>,
        next: Option<&str>,
        n: &LNode,
    ) {
        let shell;
        let to_insert = match n {
            LNode::Container {
                id,
                kind,
                props,
                children,
            } if children.iter().any(|c| self.contains_existing(c)) => {
                shell = LNode::Container {
                    id: id.clone(),
                    kind: kind.clone(),
                    props: props.clone(),
                    children: Vec::new(),
                };
                &shell
            }
            _ => n,
        };
        trace(format_args!(
            "insert {} after {prev:?} before {next:?} in {parent:?}",
            n.id()
        ));
        let ok = match (prev, next) {
            (Some(p), _) => d.insert_after(p, to_insert),
            (None, Some(x)) => d.insert_before(x, to_insert),
            (None, None) => d.insert_first(parent, to_insert),
        };
        if !ok {
            self.st.unsupported.push(format!("insert {}", n.id()));
            return;
        }
        self.st.blocks_inserted += 1;
        if !std::ptr::eq(to_insert, n) {
            // Fill the shell.
            if let LNode::Container { id, children, .. } = n {
                let mut prev_c: Option<String> = None;
                for c in children {
                    if self.in_base(c.id()) {
                        if self.move_to(d, Some(id), prev_c.as_deref(), c.id()) {
                            self.st.blocks_moved += 1;
                        } else {
                            self.st.unsupported.push(format!("move {}", c.id()));
                        }
                        if let Some(b) = self.base.get(c.id()).copied() {
                            self.content(d, b, c);
                        }
                    } else {
                        self.place_new(d, Some(id), prev_c.as_deref(), None, c);
                    }
                    prev_c = Some(c.id().to_owned());
                }
            }
        } else {
            count_inserted(n, self.st);
        }
    }

    fn contains_existing(&self, n: &LNode) -> bool {
        self.in_base(n.id())
            || match n {
                LNode::Container { children, .. } => {
                    children.iter().any(|c| self.contains_existing(c))
                }
                _ => false,
            }
    }

    /// Bring one existing node's content from `b` to `n`.
    fn content(&mut self, d: &mut LoroFlowDocument, b: &LNode, n: &LNode) {
        let id = n.id();
        match (b, n) {
            (LNode::Para(_), LNode::Para(tp)) => sync_para(d, tp, self.st),
            (
                LNode::Container {
                    kind: bk,
                    props: bp,
                    children: bc,
                    ..
                },
                LNode::Container {
                    kind: tk,
                    props: tp,
                    children: tc,
                    ..
                },
            ) if bk == tk => {
                sync_props(
                    if bk == "section" {
                        Scope::Section
                    } else {
                        Scope::Container
                    },
                    bp,
                    tp,
                    self.st,
                    |k, v| d.set_container_prop(id, k, v),
                );
                self.children(d, Some(id), bc, tc);
            }
            (LNode::Table(bt), LNode::Table(tt)) => self.table(d, bt, tt),
            (LNode::Embed { json: bj, .. }, LNode::Embed { json: tj, .. }) => {
                if !noise::same_obj(bj, tj) {
                    let replacement = LNode::Embed {
                        id: format!("{id}~new"),
                        json: tj.clone(),
                    };
                    if d.insert_after(id, &replacement) && d.delete_block(id) {
                        self.st.embeds_replaced += 1;
                    } else {
                        self.st.unsupported.push(format!("replace embed {id}"));
                    }
                }
            }
            _ => self.st.unsupported.push(format!("kind changed {id}")),
        }
    }
}

/// Where in `rest` (the paragraph's current text) the paragraph was split,
/// given what the two halves look like now.
fn split_point(rest: &str, head: &str, tail: &str) -> usize {
    const SEP: char = '\u{0}';
    let old: Vec<char> = rest.chars().collect();
    let new: Vec<char> = head
        .chars()
        .chain(std::iter::once(SEP))
        .chain(tail.chars())
        .collect();
    let sep = head.chars().count();
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &old, &new);
    // The old index at which the new text reaches the separator.
    for op in ops {
        let (o, n, len_o, len_n) = match op {
            similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => (old_index, new_index, len, len),
            similar::DiffOp::Delete {
                old_index,
                new_index,
                old_len,
            } => (old_index, new_index, old_len, 0),
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (old_index, new_index, 0, new_len),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (old_index, new_index, old_len, new_len),
        };
        if sep >= n && sep < n + len_n {
            return if len_o == len_n { o + (sep - n) } else { o };
        }
        if sep < n {
            return o;
        }
        let _ = len_o;
    }
    old.len()
}

/// Indices (into `v`) of a longest strictly increasing subsequence.
fn lis(v: &[f64]) -> Vec<usize> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; v.len()];
    for i in 0..v.len() {
        let pos = tails.partition_point(|&t| v[t] < v[i]);
        if pos > 0 {
            prev[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut out = Vec::new();
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        out.push(i);
        cur = prev[i];
    }
    out.reverse();
    out
}

fn count_inserted(n: &LNode, st: &mut ApplyStats) {
    match n {
        LNode::Para(p) => st.chars_inserted += p.len(),
        LNode::Container { children, .. } => children.iter().for_each(|c| count_inserted(c, st)),
        LNode::Table(t) => t
            .cells
            .iter()
            .flatten()
            .flat_map(|c| &c.blocks)
            .for_each(|c| count_inserted(c, st)),
        LNode::Embed { .. } => {}
    }
}

/// Write the target's value of every property that differs (by the
/// equivalences in [`noise`]), and nothing else.
fn sync_props(
    scope: Scope,
    base: &Props,
    target: &Props,
    st: &mut ApplyStats,
    mut set: impl FnMut(&str, Option<&str>) -> bool,
) {
    let (cb, ct) = if scope == Scope::Para {
        (
            noise::canon_para_props(base),
            noise::canon_para_props(target),
        )
    } else {
        (base.clone(), target.clone())
    };
    let keys: std::collections::BTreeSet<&String> = cb.keys().chain(ct.keys()).collect();
    for k in keys.into_iter().filter(|k| !noise::ignore(scope, k)) {
        if noise::same_prop(
            scope,
            k,
            cb.get(k).map(String::as_str),
            ct.get(k).map(String::as_str),
        ) {
            continue;
        }
        // The style lives in one of two keys (see `canon_para_props`).
        let raw: Vec<&str> = if scope == Scope::Para && k == "@style" {
            vec!["@style", "text:style-name"]
        } else {
            vec![k.as_str()]
        };
        for r in raw {
            if base.get(r) == target.get(r) {
                continue;
            }
            if set(r, target.get(r).map(String::as_str)) {
                st.prop_ops += 1;
            } else {
                st.unsupported.push(format!("prop {r}"));
            }
        }
    }
}

/// One comparison key per character: the character, or for an inline
/// object its canonical form.
fn char_keys(p: &Para) -> Vec<String> {
    let mut out = Vec::new();
    for r in &p.runs {
        for c in r.text.chars() {
            match (c, r.marks.get("obj")) {
                (skriv_crdt::logical::OBJ, Some(o)) => {
                    out.push(format!("\u{FFFC}{}", noise::obj_key(o)))
                }
                _ => out.push(c.to_string()),
            }
        }
    }
    out
}

/// Turn a word diff into character edits `(old start, old end, new start,
/// new end)`, front to back:
///
/// - a short run of spaces and punctuation left equal between two changes
///   is folded into them (otherwise the comma of a typed sentence is
///   "matched" to a comma of the deleted one);
/// - a pure insertion or deletion slides left while the text allows (" X"
///   after "din" rather than "X " after "din "), so a tie is settled the
///   way typing at the end of a word would.
fn char_edits(
    ops: &[similar::DiffOp],
    ot: &[String],
    os: &[usize],
    ns: &[usize],
    old: &[char],
    new: &[char],
) -> Vec<(usize, usize, usize, usize)> {
    // Token-level segments: (equal?, old range, new range).
    let mut segs: Vec<(bool, usize, usize, usize, usize)> = ops
        .iter()
        .map(|op| match *op {
            similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => (true, old_index, old_index + len, new_index, new_index + len),
            similar::DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => (false, old_index, old_index + old_len, new_index, new_index),
            similar::DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => (false, old_index, old_index, new_index, new_index + new_len),
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => (
                false,
                old_index,
                old_index + old_len,
                new_index,
                new_index + new_len,
            ),
        })
        .collect();
    let is_word = |t: &str| t.chars().next().is_some_and(char::is_alphanumeric);
    let chars =
        |sg: &(bool, usize, usize, usize, usize)| (os[sg.2] - os[sg.1]) + (ns[sg.4] - ns[sg.3]);
    let merge = |segs: Vec<(bool, usize, usize, usize, usize)>| {
        let mut merged: Vec<(bool, usize, usize, usize, usize)> = Vec::new();
        for sg in segs {
            match merged.last_mut() {
                Some(last) if !last.0 && !sg.0 => {
                    last.2 = sg.2;
                    last.4 = sg.4;
                }
                _ => merged.push(sg),
            }
        }
        merged
    };
    // Semantic cleanup (as in diff-match-patch): an equal stretch between
    // two changes that is no longer than either of them, or is only spaces
    // and punctuation, is folded into them. Repeat until nothing changes.
    loop {
        let mut changed = false;
        for i in 1..segs.len().saturating_sub(1) {
            let (eq, a, b, ..) = segs[i];
            if !eq || segs[i - 1].0 || segs[i + 1].0 {
                continue;
            }
            let len = os[b] - os[a];
            let small = len <= chars(&segs[i - 1]).min(chars(&segs[i + 1]));
            let filler = b - a <= 3 && ot[a..b].iter().all(|t| !is_word(t));
            if small || filler {
                segs[i].0 = false;
                changed = true;
            }
        }
        segs = merge(segs);
        if !changed {
            break;
        }
    }
    let merged = segs;
    let mut out = Vec::new();
    let mut floor = 0; // where the previous edit ended, in old characters
    for (eq, a, b, na, nb) in merged {
        if eq {
            continue;
        }
        let (mut cs, mut ce, mut ncs, mut nce) = (os[a], os[b], ns[na], ns[nb]);
        if ce == cs && nce > ncs {
            while cs > floor && ncs > 0 && old[cs - 1] == new[nce - 1] {
                cs -= 1;
                ce -= 1;
                ncs -= 1;
                nce -= 1;
            }
        } else if nce == ncs && ce > cs {
            while cs > floor && old[cs - 1] == old[ce - 1] {
                cs -= 1;
                ce -= 1;
            }
        }
        out.push((cs, ce, ncs, nce));
        floor = ce;
    }
    out
}

/// Group per-character keys into words (runs of letters and digits); every
/// other character, and every inline object, is a token of its own. Returns
/// the tokens and, for each token index, its first character (plus the end).
fn tokens(keys: &[String]) -> (Vec<String>, Vec<usize>) {
    let mut toks = Vec::new();
    let mut starts = Vec::new();
    let mut i = 0;
    let is_word = |k: &str| {
        let mut cs = k.chars();
        matches!((cs.next(), cs.next()), (Some(c), None) if c.is_alphanumeric())
    };
    while i < keys.len() {
        starts.push(i);
        if is_word(&keys[i]) {
            let mut w = String::new();
            while i < keys.len() && is_word(&keys[i]) {
                w.push_str(&keys[i]);
                i += 1;
            }
            toks.push(w);
        } else {
            toks.push(keys[i].clone());
            i += 1;
        }
    }
    starts.push(keys.len());
    (toks, starts)
}

/// Per-character marks of a paragraph.
fn char_marks(p: &Para) -> Vec<&BTreeMap<String, String>> {
    p.runs
        .iter()
        .flat_map(|r| std::iter::repeat_n(&r.marks, r.text.chars().count()))
        .collect()
}

fn sync_para(d: &mut LoroFlowDocument, target: &Para, st: &mut ApplyStats) {
    let id = target.id.as_str();
    let Some(cur) = d.para(id) else {
        st.unsupported.push(format!("para {id} missing"));
        return;
    };
    sync_props(Scope::Para, &cur.props, &target.props, st, |k, v| {
        d.set_para_prop(id, k, v)
    });

    // Text, per character, applied back to front so positions hold. An
    // inline object is equal to another when it is the same object.
    let new: Vec<char> = target.text().chars().collect();
    let (ok, nk) = (char_keys(&cur), char_keys(target));
    if ok != nk {
        // Diff by words, then turn word ranges back into characters: a typed
        // word is new, a deleted word is gone, and the letters of one are
        // never matched against leftovers of the other (which would
        // scatter authorship letter by letter).
        let (ot, os) = tokens(&ok);
        let (nt, ns) = tokens(&nk);
        let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &ot, &nt);
        let old_chars: Vec<char> = cur.text().chars().collect();
        for (cs, ce, ncs, nce) in char_edits(&ops, &ot, &os, &ns, &old_chars, &new)
            .into_iter()
            .rev()
        {
            if ce > cs {
                d.delete_text(id, cs, ce - cs);
                st.chars_deleted += ce - cs;
            }
            if nce > ncs {
                let s: String = new[ncs..nce].iter().collect();
                d.insert_text(id, cs, &s);
                st.chars_inserted += nce - ncs;
            }
        }
    }

    // Marks: wherever a key's value differs, set the target's.
    let Some(cur) = d.para(id) else { return };
    let cm = char_marks(&cur);
    let tm = char_marks(target);
    if cm.len() != tm.len() {
        st.unsupported
            .push(format!("para {id} text did not converge"));
        return;
    }
    let keys: std::collections::BTreeSet<&String> =
        cm.iter().chain(tm.iter()).flat_map(|m| m.keys()).collect();
    let same = |k: &str, a: Option<&String>, b: Option<&String>| {
        if k == "obj" {
            match (a, b) {
                (Some(x), Some(y)) => noise::same_obj(x, y),
                (None, None) => true,
                _ => false,
            }
        } else {
            noise::same_value(a.map(String::as_str), b.map(String::as_str))
        }
    };
    for k in keys.into_iter().filter(|k| !noise::ignore(Scope::Run, k)) {
        let mut i = 0;
        while i < tm.len() {
            let want = tm[i].get(k);
            if same(k, cm[i].get(k), want) {
                i += 1;
                continue;
            }
            let start = i;
            while i < tm.len() && tm[i].get(k) == want && !same(k, cm[i].get(k), want) {
                i += 1;
            }
            if d.mark(id, start, i, k, want.map(String::as_str)) {
                st.mark_ops += 1;
            } else {
                st.unsupported.push(format!("mark {k}"));
            }
        }
    }
}

impl Ctx<'_, '_> {
    fn table(&mut self, d: &mut LoroFlowDocument, bt: &LTable, tt: &LTable) {
        let tid = bt.id.as_str();
        sync_props(Scope::Table, &bt.props, &tt.props, self.st, |k, v| {
            d.set_container_prop(tid, k, v)
        });
        let trows: HashSet<&str> = tt.rows.iter().map(|r| r.0.as_str()).collect();
        let tcols: HashSet<&str> = tt.cols.iter().map(|c| c.0.as_str()).collect();
        let brows: HashSet<&str> = bt.rows.iter().map(|r| r.0.as_str()).collect();
        let bcols: HashSet<&str> = bt.cols.iter().map(|c| c.0.as_str()).collect();

        for (c, _) in &bt.cols {
            if !tcols.contains(c.as_str()) {
                if d.delete_col(tid, c) {
                    self.st.cols_deleted += 1;
                } else {
                    self.st.unsupported.push(format!("delete column {c}"));
                }
            }
        }
        for (r, _) in &bt.rows {
            if !trows.contains(r.as_str()) {
                if d.delete_row(tid, r) {
                    self.st.rows_deleted += 1;
                } else {
                    self.st.unsupported.push(format!("delete row {r}"));
                }
            }
        }
        // New columns, with their cells in the rows that already exist.
        for (ci, (c, cp)) in tt.cols.iter().enumerate() {
            if bcols.contains(c.as_str()) {
                continue;
            }
            let cells: Vec<(String, LCell)> = tt
                .rows
                .iter()
                .enumerate()
                .filter(|(_, r)| brows.contains(r.0.as_str()))
                .map(|(ri, r)| (r.0.clone(), tt.cells[ri][ci].clone()))
                .collect();
            if d.insert_col_raw(tid, ci, (c, cp), &cells) {
                self.st.cols_inserted += 1;
            } else {
                self.st.unsupported.push(format!("insert column {c}"));
            }
        }
        // New rows, with all their cells.
        for (ri, (r, rp)) in tt.rows.iter().enumerate() {
            if brows.contains(r.as_str()) {
                continue;
            }
            let cells: Vec<(String, LCell)> = tt
                .cols
                .iter()
                .enumerate()
                .map(|(ci, c)| (c.0.clone(), tt.cells[ri][ci].clone()))
                .collect();
            if d.insert_row_raw(tid, ri, (r, rp), &cells) {
                self.st.rows_inserted += 1;
                for (_, c) in &cells {
                    c.blocks.iter().for_each(|b| count_inserted(b, self.st));
                }
            } else {
                self.st.unsupported.push(format!("insert row {r}"));
            }
        }
        // Row and column properties.
        for (r, rp) in &tt.rows {
            if let Some((_, bp)) = bt.rows.iter().find(|x| &x.0 == r) {
                sync_props(Scope::Row, bp, rp, self.st, |k, v| {
                    d.set_line_prop(tid, r, true, k, v)
                });
            }
        }
        for (c, cp) in &tt.cols {
            if let Some((_, bp)) = bt.cols.iter().find(|x| &x.0 == c) {
                sync_props(Scope::Column, bp, cp, self.st, |k, v| {
                    d.set_line_prop(tid, c, false, k, v)
                });
            }
        }
        // Row order changes are not expressed (no operation moves a row).
        let common_b: Vec<&str> = bt
            .rows
            .iter()
            .map(|r| r.0.as_str())
            .filter(|r| trows.contains(r))
            .collect();
        let common_t: Vec<&str> = tt
            .rows
            .iter()
            .map(|r| r.0.as_str())
            .filter(|r| brows.contains(r))
            .collect();
        if common_b != common_t {
            self.st.unsupported.push("row order".into());
        }
        // Cells that existed: their content.
        for (ri, (r, _)) in tt.rows.iter().enumerate() {
            let Some(bri) = bt.rows.iter().position(|x| &x.0 == r) else {
                continue;
            };
            for (ci, (c, _)) in tt.cols.iter().enumerate() {
                let Some(bci) = bt.cols.iter().position(|x| &x.0 == c) else {
                    continue;
                };
                let (bc, tc) = (&bt.cells[bri][bci], &tt.cells[ri][ci]);
                let differs = bc
                    .props
                    .keys()
                    .chain(tc.props.keys())
                    .filter(|k| !noise::ignore(Scope::Cell, k))
                    .any(|k| {
                        !noise::same_value(
                            bc.props.get(k).map(String::as_str),
                            tc.props.get(k).map(String::as_str),
                        )
                    });
                if differs {
                    self.st.unsupported.push("cell properties".into());
                }
                self.children(d, Some(&bc.id), &bc.blocks, &tc.blocks);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_points() {
        assert_eq!(split_point("Hello world", "Hello ", "world"), 6);
        assert_eq!(split_point("Hello world", "Hello", "world"), 5);
        assert_eq!(
            split_point("Hello brave world", "Hello", " brave world!"),
            5
        );
    }

    #[test]
    fn longest_increasing() {
        let v = [0.0, 3.0, 1.0, 2.0];
        assert_eq!(lis(&v), vec![0, 2, 3]);
    }
}
