// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Edits a person makes to a returned file, described so that both
//! LibreOffice (through UNO, `fidelity/scripts/spike4_lo.py`) and skriv
//! (through CRDT operations, [`reenact`]) can perform them.
//!
//! Every edit finds its place by a snippet of text that occurs once in the
//! document, so the two sides can't disagree about where it goes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use skriv_crdt::CrdtDoc;
use skriv_crdt::logical::{LDoc, LNode, OBJ, Para, Run};
use skriv_crdt::loro_flow_backend::LoroFlowDocument;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// Type `text` right after the snippet.
    Insert { find: String, text: String },
    /// Delete the snippet.
    Delete { find: String },
    /// Make the snippet bold (the Bold button: western, Asian and complex).
    Bold { find: String },
    /// Press Enter right after the snippet.
    Split { find: String },
    /// The snippet ends a paragraph: delete the paragraph break after it.
    Join { find: String },
    /// Delete the whole paragraph holding the snippet.
    DeletePara { find: String },
    /// Copy the paragraph holding the snippet and paste it right after itself.
    DupPara { find: String },
    /// Cut the paragraph holding `find` and paste it before the one holding `before`.
    MovePara { find: String, before: String },
    /// Apply the paragraph style "Heading `level`".
    Heading { find: String, level: u8 },
    /// Insert an empty row after the row of the cell holding the snippet.
    RowInsert { find: String },
    /// Delete the column of the cell holding the snippet.
    ColDelete { find: String },
    /// At the end of the list item holding the snippet, press Enter and type `text`.
    ListAdd { find: String, text: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub ops: Vec<Op>,
    /// Edit with change tracking on (Edit ▸ Track Changes ▸ Record).
    #[serde(default)]
    pub track: bool,
}

/// Text inserted by the scenarios: Danish, with letters outside ASCII.
pub const TYPED: &str = " Tilføjet i LibreOffice, så æ, ø og å er med.";
pub const LIST_TEXT: &str = "Nyt punkt skrevet i LibreOffice";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Body,
    List,
    Cell,
}

struct Cand<'a> {
    para: &'a Para,
    place: Place,
    /// The next sibling, when it is a paragraph.
    next: Option<&'a Para>,
}

fn candidates(doc: &LDoc) -> Vec<Cand<'_>> {
    fn walk<'a>(v: &'a [LNode], place: Place, out: &mut Vec<Cand<'a>>) {
        for (i, n) in v.iter().enumerate() {
            match n {
                LNode::Para(p) => {
                    let next = match v.get(i + 1) {
                        Some(LNode::Para(q)) => Some(q),
                        _ => None,
                    };
                    out.push(Cand {
                        para: p,
                        place,
                        next,
                    });
                }
                LNode::Container { kind, children, .. } => {
                    let pl = match kind.as_str() {
                        "list" | "item" | "header" => Place::List,
                        _ => place,
                    };
                    walk(children, if place == Place::Body { pl } else { place }, out);
                }
                LNode::Table(t) => {
                    for row in &t.cells {
                        for c in row {
                            walk(&c.blocks, Place::Cell, out);
                        }
                    }
                }
                LNode::Embed { .. } => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, Place::Body, &mut out);
    out
}

/// Plain enough to aim at: no tabs or doubled spaces, which UNO search and
/// our snippets would treat differently. Inline objects are allowed; the
/// snippets themselves never contain one.
fn plain(p: &Para) -> bool {
    let t = p.text();
    !t.contains('\t') && t.trim() == t && !t.contains("  ")
}

/// A snippet of whole words from `text` that occurs exactly once in `all`.
fn unique_snippet(text: &str, all: &str, from_word: usize, at_end: bool) -> Option<String> {
    if at_end && text.ends_with(OBJ) {
        return None;
    }
    let words: Vec<&str> = text.split(' ').collect();
    if words.len() < 3 {
        return (all.matches(text).count() == 1 && text.chars().count() >= 4)
            .then(|| text.to_owned());
    }
    for n in 2..=words.len() {
        let s = if at_end {
            words[words.len() - n..].join(" ")
        } else {
            let a = from_word.min(words.len().saturating_sub(n));
            words[a..a + n].join(" ")
        };
        if s.chars().count() >= 8 && !s.contains(OBJ) && all.matches(&s).count() == 1 {
            return Some(s);
        }
    }
    None
}

/// The scenarios this document supports, each with its snippets chosen.
pub fn scenarios(doc: &LDoc) -> Vec<Scenario> {
    let all: String = doc
        .paras()
        .iter()
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n");
    let cands = candidates(doc);
    let body: Vec<&Cand> = cands
        .iter()
        .filter(|c| c.place == Place::Body && plain(c.para) && c.para.text().chars().count() >= 40)
        .collect();
    let mut out = Vec::new();
    let mut add = |name: &str, ops: Vec<Option<Op>>, track: bool| {
        if ops.iter().all(Option::is_some) && !ops.is_empty() {
            out.push(Scenario {
                name: name.into(),
                ops: ops.into_iter().flatten().collect(),
                track,
            });
        }
    };
    let a = body.first().map(|c| c.para);
    let b = body.get(1).map(|c| c.para);
    // Copy, cut and delete of whole paragraphs select the paragraph with
    // its break and paste at the start of the next paragraph, so they aim
    // at paragraphs followed by another paragraph in the same container.
    let with_next: Vec<&Para> = body
        .iter()
        .filter(|c| c.next.is_some_and(|n| !n.props.contains_key("@h")))
        .map(|c| c.para)
        .collect();
    let wa = with_next.first().copied();
    let wb = with_next.get(1).copied();
    let snip =
        |p: Option<&Para>, w: usize| p.and_then(|p| unique_snippet(&p.text(), &all, w, false));
    let end = |p: Option<&Para>| p.and_then(|p| unique_snippet(&p.text(), &all, 0, true));

    let insert = snip(a, 2).map(|find| Op::Insert {
        find,
        text: TYPED.into(),
    });
    let delete = snip(a, 4).map(|find| Op::Delete { find });
    let bold = snip(a, 1).map(|find| Op::Bold { find });
    let split = snip(a, 3).map(|find| Op::Split { find });
    // Join: a body paragraph followed by another plain paragraph.
    let join_at = body
        .iter()
        .find(|c| {
            c.next
                .is_some_and(|n| plain(n) && !n.text().is_empty() && !n.props.contains_key("@h"))
        })
        .map(|c| c.para);
    let join = end(join_at).map(|find| Op::Join { find });
    let del_para = snip(wb.or(wa), 1).map(|find| Op::DeletePara { find });
    let dup = snip(wa, 1).map(|find| Op::DupPara { find });
    let mv = match (snip(wb, 1), snip(a, 1)) {
        (Some(find), Some(before)) => Some(Op::MovePara { find, before }),
        _ => None,
    };
    let heading = snip(b, 1).map(|find| Op::Heading { find, level: 2 });
    let cell = cands
        .iter()
        .find(|c| c.place == Place::Cell && plain(c.para) && c.para.text().chars().count() >= 4)
        .map(|c| c.para);
    let cell_type = cell
        .and_then(|p| unique_snippet(&p.text(), &all, 0, true))
        .map(|find| Op::Insert {
            find,
            text: " (rettet)".into(),
        });
    let row_insert = cell
        .and_then(|p| unique_snippet(&p.text(), &all, 0, true))
        .map(|find| Op::RowInsert { find });
    // Delete the last column: find a cell in it.
    let last_col = doc.tables().into_iter().find_map(|t| {
        if t.cols.len() < 2 {
            return None;
        }
        let ci = t.cols.len() - 1;
        t.cells.iter().find_map(|row| {
            row[ci].blocks.iter().find_map(|b| match b {
                LNode::Para(p) if plain(p) && p.text().chars().count() >= 3 => {
                    unique_snippet(&p.text(), &all, 0, true)
                }
                _ => None,
            })
        })
    });
    let col_delete = last_col.map(|find| Op::ColDelete { find });
    let item = cands
        .iter()
        .find(|c| c.place == Place::List && plain(c.para) && c.para.text().chars().count() >= 6)
        .map(|c| c.para);
    let list_add = end(item).map(|find| Op::ListAdd {
        find,
        text: LIST_TEXT.into(),
    });

    add("type", vec![insert.clone()], false);
    add("delete", vec![delete.clone()], false);
    add("bold", vec![bold.clone()], false);
    add("split", vec![split.clone()], false);
    add("join", vec![join], false);
    add("delete-paragraph", vec![del_para], false);
    add("duplicate-paragraph", vec![dup], false);
    add("move-paragraph", vec![mv], false);
    add("heading", vec![heading], false);
    add("cell-type", vec![cell_type.clone()], false);
    add("row-insert", vec![row_insert], false);
    add("column-delete", vec![col_delete], false);
    add("list-add", vec![list_add], false);
    // A short editing session: several kinds of change in one file.
    let mut session = vec![insert.clone(), bold.clone()];
    if cell_type.is_some() {
        session.push(cell_type);
    }
    session.push(snip(b, 3).map(|find| Op::Split { find }));
    add("session", session, false);
    add("tracked", vec![insert, delete], true);
    out
}

/// Where a snippet is: paragraph ID, start (in characters) and length.
pub fn find(doc: &LDoc, snippet: &str) -> Option<(String, usize, usize)> {
    let mut hit = None;
    for p in doc.paras() {
        let t = p.text();
        if let Some(i) = t.find(snippet) {
            if hit.is_some() || t[i + 1..].contains(snippet) {
                return None;
            }
            hit = Some((
                p.id.clone(),
                t[..i].chars().count(),
                snippet.chars().count(),
            ));
        }
    }
    hit
}

/// The node path to paragraph `pid`: ancestors (outermost first).
fn ancestors<'a>(doc: &'a LDoc, pid: &str) -> Vec<Anc<'a>> {
    fn walk<'a>(v: &'a [LNode], pid: &str, path: &mut Vec<Anc<'a>>) -> bool {
        for (i, n) in v.iter().enumerate() {
            match n {
                LNode::Para(p) if p.id == pid => {
                    path.push(Anc::Sibling(v, i));
                    return true;
                }
                LNode::Container { children, .. } => {
                    path.push(Anc::Node(n));
                    if walk(children, pid, path) {
                        return true;
                    }
                    path.pop();
                }
                LNode::Table(t) => {
                    for (ri, row) in t.cells.iter().enumerate() {
                        for (ci, c) in row.iter().enumerate() {
                            path.push(Anc::Cell(n, ri, ci));
                            if walk(&c.blocks, pid, path) {
                                return true;
                            }
                            path.pop();
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }
    let mut path = Vec::new();
    walk(&doc.body, pid, &mut path);
    path
}

enum Anc<'a> {
    Node(&'a LNode),
    Cell(&'a LNode, usize, usize),
    /// The paragraph itself: its siblings and index.
    Sibling(&'a [LNode], usize),
}

fn new_id() -> String {
    format!("sk-{}", uuid::Uuid::now_v7())
}

const BOLD: [&str; 3] = [
    "Δstyle:text-properties|fo:font-weight",
    "Δstyle:text-properties|style:font-weight-asian",
    "Δstyle:text-properties|style:font-weight-complex",
];

/// Perform `sc` in skriv: the result a person editing in skriv would get.
/// Returns the operations that could not be performed.
pub fn reenact(d: &mut LoroFlowDocument, sc: &Scenario) -> Vec<String> {
    let mut failed = Vec::new();
    for op in &sc.ops {
        if !reenact_op(d, op) {
            failed.push(format!("{op:?}"));
        }
        d.commit();
    }
    failed
}

fn reenact_op(d: &mut LoroFlowDocument, op: &Op) -> bool {
    let doc = d.read();
    let locate = |s: &str| find(&doc, s);
    match op {
        Op::Insert { find, text } => {
            let Some((pid, at, len)) = locate(find) else {
                return false;
            };
            d.insert_text(&pid, at + len, text)
        }
        Op::Delete { find } => {
            let Some((pid, at, len)) = locate(find) else {
                return false;
            };
            d.delete_text(&pid, at, len)
        }
        Op::Bold { find } => {
            let Some((pid, at, len)) = locate(find) else {
                return false;
            };
            BOLD.iter()
                .all(|k| d.mark(&pid, at, at + len, k, Some("bold")))
        }
        Op::Split { find } => {
            let Some((pid, at, len)) = locate(find) else {
                return false;
            };
            d.split_para(&pid, at + len, &new_id())
        }
        Op::Join { find } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let Some(Anc::Sibling(sibs, i)) = ancestors(&doc, &pid).pop() else {
                return false;
            };
            match sibs.get(i + 1) {
                Some(LNode::Para(next)) => d.join_para(&next.id),
                _ => false,
            }
        }
        Op::DeletePara { find } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            d.delete_block(&pid)
        }
        Op::DupPara { find } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let Some(mut p) = d.para(&pid) else {
                return false;
            };
            p.id = new_id();
            d.insert_after(&pid, &LNode::Para(p))
        }
        Op::MovePara { find, before } => {
            let (Some((pid, ..)), Some((bid, ..))) = (locate(find), locate(before)) else {
                return false;
            };
            let Some(p) = d.para(&pid) else { return false };
            d.delete_block(&pid) && d.insert_before(&bid, &LNode::Para(p))
        }
        Op::Heading { find, level } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let style = format!("Heading_20_{level}");
            d.set_para_prop(&pid, "@h", Some("1"))
                && d.set_para_prop(&pid, "text:outline-level", Some(&level.to_string()))
                && d.set_para_prop(&pid, "@style", Some(&style))
        }
        Op::RowInsert { find } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let path = ancestors(&doc, &pid);
            let Some((LNode::Table(t), ri, _)) = path.iter().rev().find_map(|a| match a {
                Anc::Cell(n, ri, ci) => Some((*n, *ri, *ci)),
                _ => None,
            }) else {
                return false;
            };
            d.insert_row(&t.id, ri + 1, &new_id())
        }
        Op::ColDelete { find } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let path = ancestors(&doc, &pid);
            let Some((LNode::Table(t), _, ci)) = path.iter().rev().find_map(|a| match a {
                Anc::Cell(n, ri, ci) => Some((*n, *ri, *ci)),
                _ => None,
            }) else {
                return false;
            };
            d.delete_col(&t.id, &t.cols[ci].0)
        }
        Op::ListAdd { find, text } => {
            let Some((pid, ..)) = locate(find) else {
                return false;
            };
            let path = ancestors(&doc, &pid);
            let Some(LNode::Container {
                id: item_id,
                props: item_props,
                children: item_children,
                ..
            }) = path.iter().rev().find_map(|a| match a {
                Anc::Node(n @ LNode::Container { kind, .. }) if kind == "item" => Some(*n),
                _ => None,
            })
            else {
                return false;
            };
            let Some(p) = d.para(&pid) else { return false };
            let marks: BTreeMap<String, String> =
                p.runs.last().map(|r| r.marks.clone()).unwrap_or_default();
            let para = Para {
                id: new_id(),
                props: p.props.clone(),
                runs: vec![Run {
                    text: text.clone(),
                    marks,
                }],
            };
            let para_id = para.id.clone();
            let item = LNode::Container {
                id: new_id(),
                kind: "item".into(),
                props: item_props.clone(),
                children: vec![LNode::Para(para)],
            };
            if !d.insert_after(item_id, &item) {
                return false;
            }
            // Enter at the end of the item's text splits the item: what
            // followed the text in the item (a sub-list) moves to the new
            // item, so the new line appears right under the old one, as in
            // LibreOffice and Word.
            let at = item_children
                .iter()
                .position(|c| c.id() == pid)
                .unwrap_or(0);
            let mut prev = para_id;
            for c in &item_children[at + 1..] {
                if !d.move_block_after(c.id(), &prev) {
                    return false;
                }
                prev = c.id().to_owned();
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_are_unique_whole_words() {
        let all = "Kommunen har modtaget din ansøgning om byggetilladelse.\nKommunen har svaret.";
        let s = unique_snippet(
            "Kommunen har modtaget din ansøgning om byggetilladelse.",
            all,
            0,
            false,
        )
        .unwrap();
        assert_eq!(s, "Kommunen har modtaget");
        let e = unique_snippet("Kommunen har svaret.", all, 0, true).unwrap();
        assert_eq!(e, "har svaret.");
    }
}
