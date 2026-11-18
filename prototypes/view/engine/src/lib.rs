// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 2: the editing core both view prototypes share.
//!
//! It owns the document (the layout input from Spike 2b, styles already
//! resolved), applies edits to it and re-lays it out with Spike 2b's own
//! engine (`layout-own`, greedy breaking, as LibreOffice). The views ask
//! it which pages changed and what is on them, and use [`locate`] / [`hit`]
//! from the layout contract to place carets and resolve clicks.
//!
//! Paragraphs are addressed by flat index (document order, tables row by
//! row, see [`Doc::paras`]); offsets are characters (Unicode scalar values).
//! The views key DOM elements by paragraph ID, which survives splits and
//! merges; flat indices don't.
//!
//! Everything crossing into JavaScript is JSON or plain numbers, so the
//! wasm-bindgen glue stays trivial.

pub mod edit;

use std::hash::{Hash, Hasher};

use layout_contract::input::{Block, Doc};
use layout_contract::{EditHint, Engine, Pages};
use layout_own::{Breaker, OwnEngine};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// What an edit did, for the view: how long layout took and which pages
/// now look different.
#[derive(Serialize, Debug, Default)]
pub struct Outcome {
    pub ok: bool,
    /// Why an edit was refused (the prototype doesn't do everything).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Pages in the document now.
    pub pages: usize,
    /// Pages whose content or geometry changed, in order.
    pub dirty: Vec<usize>,
    /// Paragraphs were added or removed: the view must re-read the
    /// structure.
    pub structural: bool,
    /// Where the caret goes after the edit.
    pub caret: (usize, usize),
}

#[wasm_bindgen]
pub struct Editor {
    doc: Doc,
    engine: OwnEngine,
    /// Per page: a hash of what is on it, keyed by paragraph ID (not flat
    /// index, so pages that only see renumbered paragraphs stay clean).
    page_hashes: Vec<u64>,
    next_id: u64,
}

#[wasm_bindgen]
impl Editor {
    /// Load a layout input document (JSON, as `layout-bench export` writes
    /// it) and lay it out.
    #[wasm_bindgen(constructor)]
    pub fn new(json: &str) -> Result<Editor, String> {
        let doc: Doc = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let mut engine = OwnEngine::new();
        engine.breaker = Breaker::Greedy;
        let mut ed = Editor {
            doc,
            engine,
            page_hashes: Vec::new(),
            next_id: 0,
        };
        ed.relayout(&EditHint::Full);
        Ok(ed)
    }

    /// Replace the whole document (view A uses this after structural
    /// edits made in ProseMirror). Returns an [`Outcome`] as JSON.
    pub fn load(&mut self, json: &str) -> String {
        match serde_json::from_str::<Doc>(json) {
            Ok(doc) => {
                self.doc = doc;
                let dirty = self.relayout(&EditHint::Full);
                self.outcome(dirty, true, (0, 0))
            }
            Err(e) => refuse(&e.to_string()),
        }
    }

    pub fn page_count(&self) -> usize {
        self.engine.pages().pages.len()
    }

    pub fn para_count(&self) -> usize {
        self.doc.paras().len()
    }

    /// Page size and margins, in points (JSON).
    pub fn page_spec(&self) -> String {
        let p = &self.doc.page;
        serde_json::json!({
            "width": p.width, "height": p.height,
            "top": p.top, "bottom": p.bottom, "left": p.left, "right": p.right,
        })
        .to_string()
    }

    /// The block structure, for building the semantic DOM (JSON):
    /// `[{t:"p", ...para}, {t:"table", id, cols, rows:[[{span, paras:[para]}]]}]`
    /// where `para = {id, flat, h, label}`.
    pub fn structure(&self) -> String {
        let mut flat = 0usize;
        let mut para = |p: &layout_contract::input::Para| {
            let v = serde_json::json!({
                "id": p.id, "flat": flat, "h": p.style.heading.unwrap_or(0),
                "label": p.style.label, "indent": p.style.indent_left,
            });
            flat += 1;
            v
        };
        let blocks: Vec<_> = self
            .doc
            .blocks
            .iter()
            .map(|b| match b {
                Block::Para(p) => {
                    let mut v = para(p);
                    v["t"] = "p".into();
                    v
                }
                Block::Table(t) => serde_json::json!({
                    "t": "table", "id": t.id, "cols": t.cols,
                    "rows": t.rows.iter().map(|r| r.iter().map(|c| serde_json::json!({
                        "span": c.colspan,
                        "paras": c.paras.iter().map(&mut para).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>()).collect::<Vec<_>>(),
                }),
            })
            .collect();
        serde_json::Value::from(blocks).to_string()
    }

    /// The whole document as layout input JSON (view A builds its
    /// ProseMirror document from it).
    pub fn doc_json(&self) -> String {
        serde_json::to_string(&self.doc).unwrap_or_default()
    }

    pub fn para_text(&self, flat: usize) -> String {
        self.doc
            .paras()
            .get(flat)
            .map(|p| p.text.clone())
            .unwrap_or_default()
    }

    /// One page's lines (JSON): `[{id, p, y, h, b, s, e, hy, runs:[[x, w,
    /// text, start, font, size, bold, italic]]}]`. A line's runs carry the
    /// whitespace collapsed at its end, so the DOM text reads continuously.
    pub fn page(&self, index: usize) -> String {
        let pages = self.engine.pages();
        let Some(page) = pages.pages.get(index) else {
            return "[]".into();
        };
        let paras = self.doc.paras();
        let lines: Vec<_> = page
            .lines
            .iter()
            .map(|l| {
                let text: Vec<char> = paras[l.para].text.chars().collect();
                // The next line of this paragraph starts where this one's
                // text (with its collapsed trailing space) ends.
                let next = next_line_start(pages, index, l).unwrap_or(text.len());
                let n = l.runs.len();
                let runs: Vec<_> = l
                    .runs
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let end = if i + 1 == n {
                            next.max(r.range.end)
                        } else {
                            r.range.end
                        };
                        let s: String = text[r.range.start.min(text.len())..end.min(text.len())]
                            .iter()
                            .collect();
                        serde_json::json!([
                            r.x,
                            r.width,
                            s,
                            r.range.start,
                            r.font,
                            r.size,
                            r.bold,
                            r.italic
                        ])
                    })
                    .collect();
                serde_json::json!({
                    "id": l.id, "p": l.para, "y": l.y, "h": l.height, "b": l.baseline,
                    "s": l.range.start, "e": next.max(l.range.end), "hy": l.hyphen,
                    "x0": l.carets.first(), "runs": runs,
                })
            })
            .collect();
        serde_json::Value::from(lines).to_string()
    }

    /// Caret for (paragraph, offset): `[page, line, x, y, height]` or `[]`.
    pub fn locate(&self, flat: usize, offset: usize) -> Vec<f32> {
        let pages = self.engine.pages();
        match layout_contract::locate(pages, flat, offset) {
            Some(c) => {
                let l = &pages.pages[c.page].lines[c.line];
                vec![c.page as f32, c.line as f32, c.x, l.y, l.height]
            }
            None => Vec::new(),
        }
    }

    /// Paragraph and offset nearest to (x, y) on `page`: `[flat, offset]`.
    pub fn hit(&self, page: usize, x: f32, y: f32) -> Vec<u32> {
        match layout_contract::hit(self.engine.pages(), page, x, y) {
            Some((p, o)) => vec![p as u32, o as u32],
            None => Vec::new(),
        }
    }

    /// The caret one line up (`dir` = -1) or down (+1) from (flat,
    /// offset), keeping `x`: `[flat, offset]`, crossing pages.
    pub fn vertical(&self, flat: usize, offset: usize, x: f32, dir: i32) -> Vec<u32> {
        let pages = self.engine.pages();
        let Some(c) = layout_contract::locate(pages, flat, offset) else {
            return Vec::new();
        };
        let here = &pages.pages[c.page].lines[c.line];
        // The nearest line top above or below this line on `page`.
        let next_y = |page: usize, from: Option<f32>| -> Option<f32> {
            let ys = pages.pages[page].lines.iter().map(|l| (l.y, l.height));
            let ys: Vec<_> = match (dir < 0, from) {
                (true, Some(top)) => ys
                    .filter(|(y, h)| y + h <= top + 0.01)
                    .map(|(y, _)| y)
                    .collect(),
                (false, Some(bottom)) => ys
                    .filter(|(y, _)| *y >= bottom - 0.01)
                    .map(|(y, _)| y)
                    .collect(),
                (_, None) => ys.map(|(y, _)| y).collect(),
            };
            if dir < 0 {
                ys.into_iter().reduce(f32::max)
            } else {
                ys.into_iter().reduce(f32::min)
            }
        };
        let from = if dir < 0 {
            here.y
        } else {
            here.y + here.height
        };
        let target = match next_y(c.page, Some(from)) {
            Some(y) => Some((c.page, y)),
            None if dir < 0 && c.page > 0 => next_y(c.page - 1, None).map(|y| (c.page - 1, y)),
            None if dir > 0 && c.page + 1 < pages.pages.len() => {
                next_y(c.page + 1, None).map(|y| (c.page + 1, y))
            }
            None => None,
        };
        match target.and_then(|(p, y)| layout_contract::hit(pages, p, x, y + 0.5)) {
            Some((p, o)) => vec![p as u32, o as u32],
            None => Vec::new(),
        }
    }

    /// Highlight rectangles for the selection from (fa, oa) to (fb, ob),
    /// in document order: `[page, x, y, w, h, ...]` flattened.
    pub fn selection_rects(&self, fa: usize, oa: usize, fb: usize, ob: usize) -> Vec<f32> {
        let ((fa, oa), (fb, ob)) = ordered((fa, oa), (fb, ob));
        let mut out = Vec::new();
        for (pi, page) in self.engine.pages().pages.iter().enumerate() {
            for l in &page.lines {
                if l.para < fa || l.para > fb {
                    continue;
                }
                let lo = if l.para == fa { oa } else { 0 };
                let hi = if l.para == fb { ob } else { usize::MAX };
                let a = lo.max(l.range.start);
                let b = hi.min(l.range.end);
                if a > b || (a == b && !(l.para < fb && b == l.range.end)) {
                    continue;
                }
                let x0 = l.carets[a - l.range.start];
                let mut x1 = l.carets[b - l.range.start];
                if b == l.range.end && hi > l.range.end {
                    x1 += 4.0; // the line's end is selected too
                }
                out.extend([pi as f32, x0, l.y, (x1 - x0).max(0.0), l.height]);
            }
        }
        out
    }

    /// Type `text` at (flat, offset).
    pub fn insert(&mut self, flat: usize, offset: usize, text: &str) -> String {
        let Some(p) = self.doc.para_mut(flat) else {
            return refuse("no such paragraph");
        };
        let mut off = offset.min(p.char_len());
        for c in text.chars() {
            if c == '\r' {
                continue;
            }
            p.insert(off, c);
            off += 1;
        }
        let dirty = self.relayout(&EditHint::Para(flat));
        self.outcome(dirty, false, (flat, off))
    }

    /// Delete from (fa, oa) to (fb, ob). Across paragraphs, the first and
    /// last are joined; both must be body paragraphs or in the same table
    /// cell.
    pub fn delete(&mut self, fa: usize, oa: usize, fb: usize, ob: usize) -> String {
        let ((fa, oa), (fb, ob)) = ordered((fa, oa), (fb, ob));
        let before = self.doc.paras().len();
        match edit::delete(&mut self.doc, (fa, oa), (fb, ob)) {
            Ok(()) => {
                let structural = self.doc.paras().len() != before;
                let hint = if structural {
                    EditHint::Full
                } else {
                    EditHint::Para(fa)
                };
                let dirty = self.relayout(&hint);
                self.outcome(dirty, structural, (fa, oa))
            }
            Err(e) => refuse(e),
        }
    }

    /// Split the paragraph at (flat, offset) (Enter). The new paragraph
    /// gets `id`, or a fresh one.
    pub fn split(&mut self, flat: usize, offset: usize, id: Option<String>) -> String {
        self.next_id += 1;
        let id = id.unwrap_or_else(|| format!("new-{}", self.next_id));
        match edit::split(&mut self.doc, flat, offset, id) {
            Ok(()) => {
                let dirty = self.relayout(&EditHint::Full);
                self.outcome(dirty, true, (flat + 1, 0))
            }
            Err(e) => refuse(e),
        }
    }

    /// Replace one paragraph's text and character runs (view A, after a
    /// ProseMirror transaction that changed only that paragraph). `spans`
    /// is JSON `[[start, end, font, size, bold, italic]]`.
    pub fn set_para(&mut self, flat: usize, text: &str, spans: &str) -> String {
        let spans: Vec<(usize, usize, String, f32, bool, bool)> = match serde_json::from_str(spans)
        {
            Ok(s) => s,
            Err(e) => return refuse(&e.to_string()),
        };
        let Some(p) = self.doc.para_mut(flat) else {
            return refuse("no such paragraph");
        };
        p.text = text.to_owned();
        p.spans = spans
            .into_iter()
            .map(
                |(s, e, font, size, bold, italic)| layout_contract::input::Span {
                    range: s..e,
                    font,
                    size,
                    bold,
                    italic,
                },
            )
            .collect();
        let len = p.char_len();
        p.notes.retain(|n| n.at < len.max(1));
        let dirty = self.relayout(&EditHint::Para(flat));
        self.outcome(dirty, false, (flat, 0))
    }

    /// The layout hash of the current pages (determinism checks).
    pub fn hash(&self) -> String {
        format!("{:016x}", layout_contract::layout_hash(self.engine.pages()))
    }
}

impl Editor {
    /// Lay out again and return the pages whose content changed.
    fn relayout(&mut self, hint: &EditHint) -> Vec<usize> {
        let old_len = self.page_hashes.len();
        self.engine.layout(&self.doc, hint);
        let pages = self.engine.pages();
        let rebuilt = match hint {
            EditHint::Full => 0..pages.pages.len(),
            EditHint::Para(_) => self.engine.rebuilt.clone(),
        };
        let paras = self.doc.paras();
        self.page_hashes.resize(pages.pages.len(), 0);
        let mut dirty = Vec::new();
        for i in 0..pages.pages.len() {
            if rebuilt.contains(&i) || i >= old_len {
                let h = page_hash(&pages.pages[i], &paras);
                if h != self.page_hashes[i] {
                    self.page_hashes[i] = h;
                    dirty.push(i);
                }
            }
        }
        dirty
    }

    fn outcome(&self, dirty: Vec<usize>, structural: bool, caret: (usize, usize)) -> String {
        serde_json::to_string(&Outcome {
            ok: true,
            error: None,
            pages: self.engine.pages().pages.len(),
            dirty,
            structural,
            caret,
        })
        .unwrap_or_default()
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn pages(&self) -> &Pages {
        self.engine.pages()
    }
}

fn refuse(why: &str) -> String {
    serde_json::to_string(&Outcome {
        ok: false,
        error: Some(why.to_owned()),
        ..Outcome::default()
    })
    .unwrap_or_default()
}

fn ordered(a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
    if a <= b { (a, b) } else { (b, a) }
}

/// Where the line after `l` (same paragraph) starts, looking on this page
/// and the next.
fn next_line_start(pages: &Pages, page: usize, l: &layout_contract::Line) -> Option<usize> {
    pages
        .pages
        .iter()
        .skip(page)
        .take(2)
        .flat_map(|p| p.lines.iter())
        .filter(|m| {
            m.para == l.para && m.range.start >= l.range.end && m.range.start > l.range.start
        })
        .map(|m| m.range.start)
        .min()
}

/// What a page shows, independent of flat paragraph numbering.
fn page_hash(p: &layout_contract::Page, paras: &[&layout_contract::input::Para]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let q = |v: f32| (v * 64.0).round() as i64;
    p.lines.len().hash(&mut h);
    for l in &p.lines {
        (&l.id, l.range.start, l.range.end, l.hyphen).hash(&mut h);
        paras[l.para]
            .text
            .chars()
            .skip(l.range.start)
            .take(l.range.len() + 1)
            .for_each(|c| c.hash(&mut h));
        (q(l.y), q(l.height), q(l.baseline)).hash(&mut h);
        for c in &l.carets {
            q(*c).hash(&mut h);
        }
    }
    h.finish()
}
