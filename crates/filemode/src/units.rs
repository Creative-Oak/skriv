// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! How far apart two documents are, in change units a reviewer would see.
//!
//! `b` must be aligned to `a` ([`crate::align`]). One unit is: one character
//! inserted or deleted; one character whose formatting differs (per
//! property); one paragraph or container property; one paragraph, row,
//! column or inline-object block added or removed (plus its characters).
//!
//! The spike's "spurious changes" are the units between the branch made
//! from a returned file and the same edits made directly in skriv,
//! relative to the units between skriv's document and the branch.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use skriv_crdt::logical::{LCell, LDoc, LNode, LTable, Para, Props};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Units {
    pub text: usize,
    pub format: usize,
    pub props: usize,
    pub structure: usize,
}

impl Units {
    pub fn total(&self) -> usize {
        self.text + self.format + self.props + self.structure
    }

    fn add(&mut self, o: Units) {
        self.text += o.text;
        self.format += o.format;
        self.props += o.props;
        self.structure += o.structure;
    }
}

/// Where the units are, for reports: one line per differing node.
#[derive(Clone, Debug, Default)]
pub struct Detail {
    pub lines: Vec<String>,
}

pub fn units(a: &LDoc, b: &LDoc) -> Units {
    units_detail(a, b, &mut Detail::default())
}

pub fn units_detail(a: &LDoc, b: &LDoc, detail: &mut Detail) -> Units {
    let cx = Cx {
        a: index(&a.body),
        b: index(&b.body),
    };
    children(&cx, &a.body, &b.body, detail)
}

/// Every node of each side by ID, so a node that moved to another parent
/// counts as one move, not as a deletion and an insertion.
struct Cx<'a> {
    a: HashMap<&'a str, &'a LNode>,
    b: HashMap<&'a str, &'a LNode>,
}

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

fn size(n: &LNode) -> Units {
    match n {
        LNode::Para(p) => Units {
            text: p.len(),
            structure: 1,
            ..Default::default()
        },
        LNode::Embed { .. } => Units {
            structure: 1,
            ..Default::default()
        },
        LNode::Container { children, .. } => {
            let mut u = Units {
                structure: 1,
                ..Default::default()
            };
            children.iter().for_each(|c| u.add(size(c)));
            u
        }
        LNode::Table(t) => {
            let mut u = Units {
                structure: 1,
                ..Default::default()
            };
            t.cells
                .iter()
                .flatten()
                .flat_map(|c| &c.blocks)
                .for_each(|c| u.add(size(c)));
            u
        }
    }
}

fn prop_diff(a: &Props, b: &Props) -> String {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| a.get(*k) != b.get(*k))
        .map(|k| format!("{k}: {:?} → {:?}", a.get(k), b.get(k)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn props(a: &Props, b: &Props) -> usize {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter().filter(|k| a.get(*k) != b.get(*k)).count()
}

fn col_props(a: &Props, b: &Props) -> usize {
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| {
            !crate::noise::same_prop(
                crate::noise::Scope::Column,
                k,
                a.get(*k).map(String::as_str),
                b.get(*k).map(String::as_str),
            )
        })
        .count()
}

fn children(cx: &Cx, a: &[LNode], b: &[LNode], detail: &mut Detail) -> Units {
    let mut u = Units::default();
    let bm: HashMap<&str, &LNode> = b.iter().map(|n| (n.id(), n)).collect();
    let am: HashMap<&str, &LNode> = a.iter().map(|n| (n.id(), n)).collect();
    for n in a {
        match bm.get(n.id()) {
            None if cx.b.contains_key(n.id()) => {} // moved away: counted where it went
            None => {
                let s = size(n);
                detail
                    .lines
                    .push(format!("- {} ({} units)", short(n), s.total()));
                u.add(s);
            }
            Some(m) => u.add(node(cx, n, m, detail)),
        }
    }
    for n in b {
        if !am.contains_key(n.id()) {
            if let Some(from) = cx.a.get(n.id()) {
                detail.lines.push(format!("~ {} moved here", short(n)));
                u.structure += 1;
                u.add(node(cx, from, n, detail));
                continue;
            }
            let s = size(n);
            detail
                .lines
                .push(format!("+ {} ({} units)", short(n), s.total()));
            u.add(s);
        }
    }
    // Order: siblings present on both sides but in a different order count
    // one structural unit each (a move).
    let ao: Vec<&str> = a
        .iter()
        .map(LNode::id)
        .filter(|i| bm.contains_key(i))
        .collect();
    let bo: Vec<&str> = b
        .iter()
        .map(LNode::id)
        .filter(|i| am.contains_key(i))
        .collect();
    if ao != bo {
        let moved = moved(&ao, &bo);
        if moved > 0 {
            detail.lines.push(format!("~ {moved} moved"));
        }
        u.structure += moved;
    }
    u
}

/// How many items must move to turn `a` into `b` (len − LCS).
fn moved(a: &[&str], b: &[&str]) -> usize {
    let pos: HashMap<&str, usize> = a.iter().enumerate().map(|(i, x)| (*x, i)).collect();
    let seq: Vec<usize> = b.iter().map(|x| pos[x]).collect();
    let mut tails: Vec<usize> = Vec::new();
    for x in seq {
        let p = tails.partition_point(|&t| t < x);
        if p == tails.len() {
            tails.push(x);
        } else {
            tails[p] = x;
        }
    }
    b.len() - tails.len()
}

fn short(n: &LNode) -> String {
    let t: String = match n {
        LNode::Para(p) => p.text(),
        LNode::Embed { .. } => "[object]".into(),
        LNode::Container { kind, .. } => format!("[{kind}]"),
        LNode::Table(_) => "[table]".into(),
    };
    let t: String = t.chars().take(60).collect();
    format!("{t:?}")
}

fn node(cx: &Cx, a: &LNode, b: &LNode, detail: &mut Detail) -> Units {
    match (a, b) {
        (LNode::Para(x), LNode::Para(y)) => para(x, y, detail),
        (LNode::Embed { json: x, .. }, LNode::Embed { json: y, .. }) => {
            if x == y {
                Units::default()
            } else {
                if std::env::var_os("SKRIV_DEBUG_OBJ").is_some() {
                    eprintln!("OBJ A {x}\nOBJ B {y}");
                }
                detail.lines.push("~ object changed".into());
                Units {
                    structure: 1,
                    ..Default::default()
                }
            }
        }
        (
            LNode::Container {
                kind: ka,
                props: pa,
                children: ca,
                ..
            },
            LNode::Container {
                kind: kb,
                props: pb,
                children: cb,
                ..
            },
        ) if ka == kb => {
            let mut u = Units {
                props: props(pa, pb),
                ..Default::default()
            };
            if u.props > 0 {
                detail.lines.push(format!("~ [{ka}] {}", prop_diff(pa, pb)));
            }
            u.add(children(cx, ca, cb, detail));
            u
        }
        (LNode::Table(x), LNode::Table(y)) => table(cx, x, y, detail),
        _ => {
            let mut u = size(a);
            u.add(size(b));
            u
        }
    }
}

fn char_marks(p: &Para) -> Vec<&BTreeMap<String, String>> {
    p.runs
        .iter()
        .flat_map(|r| std::iter::repeat_n(&r.marks, r.text.chars().count()))
        .collect()
}

fn para(a: &Para, b: &Para, detail: &mut Detail) -> Units {
    let mut u = Units {
        props: props(&a.props, &b.props),
        ..Default::default()
    };
    if u.props > 0 {
        let keys: BTreeSet<&String> = a.props.keys().chain(b.props.keys()).collect();
        let diff: Vec<String> = keys
            .into_iter()
            .filter(|k| a.props.get(*k) != b.props.get(*k))
            .map(|k| format!("{k}: {:?} → {:?}", a.props.get(k), b.props.get(k)))
            .collect();
        detail.lines.push(format!(
            "~ {} props {}",
            short(&LNode::Para(a.clone())),
            diff.join(", ")
        ));
    }
    let ac: Vec<char> = a.text().chars().collect();
    let bc: Vec<char> = b.text().chars().collect();
    let am = char_marks(a);
    let bm = char_marks(b);
    let ops = similar::capture_diff_slices(similar::Algorithm::Myers, &ac, &bc);
    let mut fmt = 0;
    let mut fmt_keys: BTreeSet<String> = BTreeSet::new();
    let mut example: Option<String> = None;
    for op in ops {
        match op {
            similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for i in 0..len {
                    let (x, y) = (am[old_index + i], bm[new_index + i]);
                    if x != y {
                        let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
                        for k in keys {
                            if x.get(k) != y.get(k) {
                                fmt += 1;
                                fmt_keys.insert(k.clone());
                                if example.is_none() {
                                    example = Some(format!("{k}: {:?} → {:?}", x.get(k), y.get(k)));
                                }
                            }
                        }
                    }
                }
            }
            similar::DiffOp::Delete { old_len, .. } => u.text += old_len,
            similar::DiffOp::Insert { new_len, .. } => u.text += new_len,
            similar::DiffOp::Replace {
                old_len, new_len, ..
            } => u.text += old_len + new_len,
        }
    }
    u.format = fmt;
    if u.text > 0 {
        detail.lines.push(format!(
            "~ {} → {:?}: {} characters",
            short(&LNode::Para(a.clone())),
            b.text().chars().take(60).collect::<String>(),
            u.text
        ));
    }
    if fmt > 0 {
        detail.lines.push(format!(
            "~ {} formatting of {fmt} characters: {} (e.g. {})",
            short(&LNode::Para(a.clone())),
            fmt_keys.into_iter().collect::<Vec<_>>().join(", "),
            example.unwrap_or_default()
        ));
    }
    u
}

fn table(cx: &Cx, a: &LTable, b: &LTable, detail: &mut Detail) -> Units {
    let mut u = Units {
        props: props(&a.props, &b.props),
        ..Default::default()
    };
    let brow: HashMap<&str, usize> = b
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.0.as_str(), i))
        .collect();
    let bcol: HashMap<&str, usize> = b
        .cols
        .iter()
        .enumerate()
        .map(|(i, c)| (c.0.as_str(), i))
        .collect();
    let arow: HashMap<&str, usize> = a
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.0.as_str(), i))
        .collect();
    let acol: HashMap<&str, usize> = a
        .cols
        .iter()
        .enumerate()
        .map(|(i, c)| (c.0.as_str(), i))
        .collect();
    let cell_size = |c: &LCell| {
        let mut s = Units::default();
        c.blocks.iter().for_each(|x| s.add(size(x)));
        s
    };
    for (ri, (r, rp)) in a.rows.iter().enumerate() {
        match brow.get(r.as_str()) {
            None => {
                u.structure += 1;
                detail.lines.push("- [row]".into());
                for c in &a.cells[ri] {
                    u.add(cell_size(c));
                }
            }
            Some(&bri) => u.props += props(rp, &b.rows[bri].1),
        }
    }
    for (ri, (r, _)) in b.rows.iter().enumerate() {
        if !arow.contains_key(r.as_str()) {
            u.structure += 1;
            detail.lines.push("+ [row]".into());
            for c in &b.cells[ri] {
                u.add(cell_size(c));
            }
        }
    }
    for (ci, (c, cp)) in a.cols.iter().enumerate() {
        match bcol.get(c.as_str()) {
            None => {
                u.structure += 1;
                detail.lines.push("- [column]".into());
                for (ri, (r, _)) in a.rows.iter().enumerate() {
                    if brow.contains_key(r.as_str()) {
                        u.add(cell_size(&a.cells[ri][ci]));
                    }
                }
            }
            Some(&bci) => {
                let n = col_props(cp, &b.cols[bci].1);
                if n > 0 {
                    detail
                        .lines
                        .push(format!("~ [column] {}", prop_diff(cp, &b.cols[bci].1)));
                }
                u.props += n;
            }
        }
    }
    for (ci, (c, _)) in b.cols.iter().enumerate() {
        if !acol.contains_key(c.as_str()) {
            u.structure += 1;
            detail.lines.push("+ [column]".into());
            for (ri, (r, _)) in b.rows.iter().enumerate() {
                if arow.contains_key(r.as_str()) {
                    u.add(cell_size(&b.cells[ri][ci]));
                }
            }
        }
    }
    for (ri, (r, _)) in a.rows.iter().enumerate() {
        let Some(&bri) = brow.get(r.as_str()) else {
            continue;
        };
        for (ci, (c, _)) in a.cols.iter().enumerate() {
            let Some(&bci) = bcol.get(c.as_str()) else {
                continue;
            };
            let (x, y) = (&a.cells[ri][ci], &b.cells[bri][bci]);
            let n = props(&x.props, &y.props);
            if n > 0 {
                detail
                    .lines
                    .push(format!("~ [cell] {}", prop_diff(&x.props, &y.props)));
            }
            u.props += n;
            u.add(children(cx, &x.blocks, &y.blocks, detail));
        }
    }
    u
}
