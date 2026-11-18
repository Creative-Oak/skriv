// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structural edits on the layout input: deleting ranges (joining
//! paragraphs) and splitting paragraphs. Character style runs and footnote
//! citations follow the text.

use layout_contract::input::{Block, Doc, Para};

/// Where a flat paragraph lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loc {
    /// `doc.blocks[block]`.
    Body { block: usize },
    /// `doc.blocks[block]`, a table: row, column, paragraph in the cell.
    Cell {
        block: usize,
        row: usize,
        col: usize,
        para: usize,
    },
}

fn locate(doc: &Doc, flat: usize) -> Option<Loc> {
    let mut i = 0;
    for (b, blk) in doc.blocks.iter().enumerate() {
        match blk {
            Block::Para(_) => {
                if i == flat {
                    return Some(Loc::Body { block: b });
                }
                i += 1;
            }
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.iter().enumerate() {
                        if flat < i + cell.paras.len() {
                            return Some(Loc::Cell {
                                block: b,
                                row: r,
                                col: c,
                                para: flat - i,
                            });
                        }
                        i += cell.paras.len();
                    }
                }
            }
        }
    }
    None
}

fn byte_at(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(b, _)| b)
}

/// Remove characters `a..b` from a paragraph.
fn cut(p: &mut Para, a: usize, b: usize) {
    let len = p.char_len();
    let (a, b) = (a.min(len), b.min(len));
    if a >= b {
        return;
    }
    let (ba, bb) = (byte_at(&p.text, a), byte_at(&p.text, b));
    p.text.replace_range(ba..bb, "");
    let n = b - a;
    let shift = |x: usize| {
        if x <= a {
            x
        } else if x >= b {
            x - n
        } else {
            a
        }
    };
    for s in &mut p.spans {
        s.range = shift(s.range.start)..shift(s.range.end);
    }
    let keep_one = p.spans.first().cloned();
    p.spans.retain(|s| !s.range.is_empty());
    if p.spans.is_empty()
        && let Some(mut s) = keep_one
    {
        // Keep the style of what was deleted for the next character typed.
        s.range = 0..0;
        p.spans.push(s);
    }
    p.notes.retain(|nt| nt.at < a || nt.at >= b);
    for nt in &mut p.notes {
        nt.at = shift(nt.at);
    }
}

/// Split `p` at `at`: `p` keeps the text before, the result holds the rest.
fn split_off(p: &mut Para, at: usize, id: String) -> Para {
    let at = at.min(p.char_len());
    let byte = byte_at(&p.text, at);
    let tail_text = p.text.split_off(byte);
    let mut tail_spans = Vec::new();
    for s in &mut p.spans {
        if s.range.end > at {
            let mut t = s.clone();
            t.range = s.range.start.max(at) - at..s.range.end - at;
            tail_spans.push(t);
            s.range.end = s.range.end.min(at);
        }
    }
    let first = p.spans.first().cloned();
    p.spans
        .retain(|s| !s.range.is_empty() || s.range.start == 0);
    p.spans
        .dedup_by(|b, a| a.range.is_empty() && b.range.is_empty());
    if p.spans.is_empty()
        && let Some(mut s) = first.clone()
    {
        s.range = 0..0;
        p.spans.push(s);
    }
    tail_spans.retain(|s| !s.range.is_empty());
    if tail_spans.is_empty() {
        // An empty new paragraph types in the style at the split point.
        if let Some(mut s) = p.spans.last().cloned().or(first) {
            s.range = 0..0;
            tail_spans.push(s);
        }
    }
    let (keep, move_): (Vec<_>, Vec<_>) = p.notes.drain(..).partition(|n| n.at < at);
    p.notes = keep;
    let mut style = p.style.clone();
    // A new paragraph after a heading's end is body text in a real editor
    // (the style's "next style"); the prototype keeps the style, except
    // that it never starts a new page.
    style.page_break_before = false;
    Para {
        id,
        text: tail_text,
        spans: tail_spans,
        style,
        notes: move_
            .into_iter()
            .map(|mut n| {
                n.at -= at;
                n
            })
            .collect(),
    }
}

/// Append `q` to `p`.
fn join(p: &mut Para, q: Para) {
    let n = p.char_len();
    p.spans.retain(|s| !s.range.is_empty());
    p.text.push_str(&q.text);
    for mut s in q.spans {
        if s.range.is_empty() {
            continue;
        }
        s.range = s.range.start + n..s.range.end + n;
        p.spans.push(s);
    }
    for mut nt in q.notes {
        nt.at += n;
        p.notes.push(nt);
    }
}

/// Delete from (fa, oa) to (fb, ob), (fa, oa) <= (fb, ob).
pub fn delete(
    doc: &mut Doc,
    (fa, oa): (usize, usize),
    (fb, ob): (usize, usize),
) -> Result<(), &'static str> {
    if fa == fb {
        let p = doc.para_mut(fa).ok_or("no such paragraph")?;
        cut(p, oa, ob);
        return Ok(());
    }
    let la = locate(doc, fa).ok_or("no such paragraph")?;
    let lb = locate(doc, fb).ok_or("no such paragraph")?;
    match (la, lb) {
        (Loc::Body { block: a }, Loc::Body { block: b }) => {
            let Block::Para(mut last) = doc.blocks[b].clone() else {
                unreachable!()
            };
            cut(&mut last, 0, ob);
            // Everything in between goes, tables included.
            doc.blocks.drain(a + 1..=b);
            let Block::Para(first) = &mut doc.blocks[a] else {
                unreachable!()
            };
            let len = first.char_len();
            cut(first, oa, len);
            join(first, last);
            Ok(())
        }
        (
            Loc::Cell {
                block: ba,
                row: ra,
                col: ca,
                para: pa,
            },
            Loc::Cell {
                block: bb,
                row: rb,
                col: cb,
                para: pb,
            },
        ) if (ba, ra, ca) == (bb, rb, cb) => {
            let Block::Table(t) = &mut doc.blocks[ba] else {
                unreachable!()
            };
            let paras = &mut t.rows[ra][ca].paras;
            let mut last = paras[pb].clone();
            cut(&mut last, 0, ob);
            paras.drain(pa + 1..=pb);
            let first = &mut paras[pa];
            let len = first.char_len();
            cut(first, oa, len);
            join(first, last);
            Ok(())
        }
        _ => {
            Err("the prototype only deletes across paragraphs in the body or within one table cell")
        }
    }
}

/// Split paragraph `flat` at `offset`; the new paragraph gets `id`.
pub fn split(doc: &mut Doc, flat: usize, offset: usize, id: String) -> Result<(), &'static str> {
    match locate(doc, flat).ok_or("no such paragraph")? {
        Loc::Body { block } => {
            let Block::Para(p) = &mut doc.blocks[block] else {
                unreachable!()
            };
            let tail = split_off(p, offset, id);
            doc.blocks.insert(block + 1, Block::Para(tail));
        }
        Loc::Cell {
            block,
            row,
            col,
            para,
        } => {
            let Block::Table(t) = &mut doc.blocks[block] else {
                unreachable!()
            };
            let paras = &mut t.rows[row][col].paras;
            let tail = split_off(&mut paras[para], offset, id);
            paras.insert(para + 1, tail);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout_contract::input::{Align, LineHeight, Note, PageSpec, ParaStyle, Span};

    fn para(id: &str, runs: &[(&str, bool)]) -> Para {
        let mut text = String::new();
        let mut spans = Vec::new();
        for (t, bold) in runs {
            let s = text.chars().count();
            text.push_str(t);
            spans.push(Span {
                range: s..text.chars().count(),
                font: "Liberation Serif".into(),
                size: 12.0,
                bold: *bold,
                italic: false,
            });
        }
        Para {
            id: id.into(),
            text,
            spans,
            style: ParaStyle {
                align: Align::Start,
                space_before: 0.0,
                space_after: 0.0,
                indent_left: 0.0,
                indent_right: 0.0,
                first_line: 0.0,
                line_height: LineHeight::Proportional(1.0),
                keep_with_next: false,
                page_break_before: false,
                widows: 2,
                orphans: 2,
                hyphenate: false,
                heading: None,
                label: None,
                font: "Liberation Serif".into(),
                size: 12.0,
            },
            notes: Vec::new(),
        }
    }

    fn doc(paras: Vec<Para>) -> Doc {
        Doc {
            page: PageSpec::a4(),
            blocks: paras.into_iter().map(Block::Para).collect(),
        }
    }

    fn check(p: &Para) {
        let len = p.char_len();
        let mut at = 0;
        for s in &p.spans {
            assert_eq!(s.range.start, at, "{p:?}");
            at = s.range.end;
        }
        assert_eq!(at, len, "{p:?}");
    }

    #[test]
    fn cut_inside_and_across_runs() {
        let mut p = para("a", &[("Æble", false), ("øen", true), (" på", false)]);
        cut(&mut p, 2, 6);
        assert_eq!(p.text, "Æbn på");
        check(&p);
        assert!(p.spans[1].bold);
    }

    #[test]
    fn delete_joins_body_paragraphs_and_drops_those_between() {
        let mut d = doc(vec![
            para("a", &[("Første", false)]),
            para("b", &[("midt", false)]),
            para("c", &[("Sidste", true)]),
        ]);
        delete(&mut d, (0, 3), (2, 2)).unwrap();
        let ps = d.paras();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].text, "Førdste");
        check(ps[0]);
        assert!(ps[0].spans.last().unwrap().bold);
    }

    #[test]
    fn split_and_join_round_trip_with_notes() {
        let mut p = para("a", &[("Ålborg ", false), ("havn", true)]);
        p.notes.push(Note {
            at: 9,
            text: "note".into(),
        });
        let mut d = doc(vec![p.clone()]);
        split(&mut d, 0, 8, "b".into()).unwrap();
        {
            let ps = d.paras();
            assert_eq!(
                (ps[0].text.as_str(), ps[1].text.as_str()),
                ("Ålborg h", "avn")
            );
            check(ps[0]);
            check(ps[1]);
            assert_eq!(ps[1].notes[0].at, 1);
            assert_eq!(ps[1].id, "b");
        }
        delete(&mut d, (0, 8), (1, 0)).unwrap();
        let ps = d.paras();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].text, p.text);
        assert_eq!(ps[0].notes, p.notes);
        check(ps[0]);
    }

    #[test]
    fn split_at_end_gives_an_empty_paragraph_that_types_in_the_same_style() {
        let mut d = doc(vec![para("a", &[("Overskrift", true)])]);
        split(&mut d, 0, 10, "b".into()).unwrap();
        let p = d.para_mut(1).unwrap();
        assert_eq!(p.text, "");
        p.insert(0, 'x');
        check(p);
        assert!(p.spans[0].bold);
    }

    #[test]
    fn delete_everything_then_type() {
        let mut d = doc(vec![para("a", &[("abc", true)])]);
        delete(&mut d, (0, 0), (0, 3)).unwrap();
        let p = d.para_mut(0).unwrap();
        p.insert(0, 'x');
        check(p);
        assert!(p.spans[0].bold);
    }
}
