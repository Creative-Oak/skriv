// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The layout contract (from `docs/spikes/02-03-next-spikes-plan.md`):
//!
//! ```text
//! layout(model snapshot, edit hint) -> Pages
//! locate(Pages, NodeId, offset) -> (page, line, x)
//! hit(Pages, page, x, y) -> (NodeId, offset)
//! ```
//!
//! Both paginators take the same [`input::Doc`] (styles already resolved)
//! and produce [`Pages`]. Every line carries the x position of every caret
//! stop in it, so [`locate`] and [`hit`] are the same code for both.
//! Positions are points (1/72 inch); offsets are in characters (Unicode
//! scalar values) within a paragraph's text.

pub mod fonts;
#[cfg(feature = "model")]
pub mod from_model;
pub mod input;
pub mod testdoc;

use std::hash::{Hash, Hasher};
use std::ops::Range;

use serde::{Deserialize, Serialize};

/// Which part of the document changed since the last layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditHint {
    /// Lay out everything.
    Full,
    /// Only the paragraph at this flat index (see [`input::Doc::paras`])
    /// changed.
    Para(usize),
}

pub trait Engine {
    fn name(&self) -> &'static str;
    /// Lay out `doc`. With [`EditHint::Para`], the engine may reuse its
    /// previous work for everything else.
    fn layout(&mut self, doc: &input::Doc, hint: &EditHint) -> &Pages;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pages {
    pub pages: Vec<Page>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub lines: Vec<Line>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Line {
    /// The paragraph this line belongs to: its flat index and ID.
    pub para: usize,
    pub id: String,
    /// Top of the line box and its height.
    pub y: f32,
    pub height: f32,
    pub baseline: f32,
    /// Character range of the paragraph's text shown on this line.
    pub range: Range<usize>,
    /// x of each caret stop: `carets[i]` is before character
    /// `range.start + i`; there are `range.len() + 1` of them.
    pub carets: Vec<f32>,
    pub runs: Vec<Run>,
    /// The line ends in an inserted hyphen.
    pub hyphen: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Run {
    pub x: f32,
    pub width: f32,
    pub range: Range<usize>,
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

/// Where a caret is: page, line on that page, and x.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub page: usize,
    pub line: usize,
    pub x: f32,
}

/// Caret position for character offset `offset` in paragraph `para`. At a
/// line break the caret belongs to the line the offset starts (the next
/// line), except at the end of the paragraph.
pub fn locate(pages: &Pages, para: usize, offset: usize) -> Option<Caret> {
    // Characters a kernel didn't draw (whitespace collapsed at a line
    // break) get the caret at the end of the line before them.
    let mut last = None;
    let mut before: Option<(usize, Caret)> = None;
    for (pi, p) in pages.pages.iter().enumerate() {
        for (li, l) in p.lines.iter().enumerate() {
            if l.para != para {
                continue;
            }
            if offset >= l.range.start && offset < l.range.end {
                return Some(Caret {
                    page: pi,
                    line: li,
                    x: l.carets[offset - l.range.start],
                });
            }
            if offset == l.range.end {
                last = Some(Caret {
                    page: pi,
                    line: li,
                    x: *l.carets.last()?,
                });
            }
            if l.range.end < offset && before.is_none_or(|(end, _)| l.range.end >= end) {
                before = Some((
                    l.range.end,
                    Caret {
                        page: pi,
                        line: li,
                        x: *l.carets.last()?,
                    },
                ));
            }
        }
    }
    last.or(before.map(|(_, c)| c))
}

/// The paragraph and character offset nearest to point (x, y) on `page`.
pub fn hit(pages: &Pages, page: usize, x: f32, y: f32) -> Option<(usize, usize)> {
    let p = pages.pages.get(page)?;
    // Nearest line vertically; among lines at that distance (table cells
    // side by side), the one nearest horizontally.
    let best = p
        .lines
        .iter()
        .map(|l| dist(l, y))
        .fold(f32::INFINITY, f32::min);
    let line = p
        .lines
        .iter()
        .filter(|l| dist(l, y) <= best + 0.01)
        .min_by(|a, b| xdist(a, x).total_cmp(&xdist(b, x)))?;
    let i = line
        .carets
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map(|(i, _)| i)?;
    Some((line.para, line.range.start + i))
}

fn xdist(l: &Line, x: f32) -> f32 {
    let (a, b) = (l.carets[0], *l.carets.last().unwrap_or(&l.carets[0]));
    if x < a {
        a - x
    } else if x > b {
        x - b
    } else {
        0.0
    }
}

fn dist(l: &Line, y: f32) -> f32 {
    if y < l.y {
        l.y - y
    } else if y > l.y + l.height {
        y - l.y - l.height
    } else {
        0.0
    }
}

/// A hash of the layout, with positions rounded to 1/64 pt: equal hashes
/// mean the same pages, lines, breaks and caret positions.
pub fn layout_hash(pages: &Pages) -> u64 {
    // FNV-1a: stable across platforms and Rust versions, unlike
    // `DefaultHasher`.
    struct Fnv(u64);
    impl Hasher for Fnv {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    let q = |v: f32| (v * 64.0).round() as i64;
    // Hash usize as u64: usize is 4 bytes on wasm32 and 8 natively.
    let u = |v: usize| v as u64;
    let mut h = Fnv(0xcbf2_9ce4_8422_2325);
    u(pages.pages.len()).hash(&mut h);
    for p in &pages.pages {
        u(p.lines.len()).hash(&mut h);
        for l in &p.lines {
            (u(l.para), u(l.range.start), u(l.range.end), l.hyphen).hash(&mut h);
            (q(l.y), q(l.height), q(l.baseline)).hash(&mut h);
            for c in &l.carets {
                q(*c).hash(&mut h);
            }
        }
    }
    h.finish()
}

/// Flat paragraph indices that live in tables.
pub fn table_paras(doc: &input::Doc) -> std::collections::HashSet<usize> {
    let mut out = std::collections::HashSet::new();
    let starts = doc.block_starts();
    for (b, blk) in doc.blocks.iter().enumerate() {
        if let input::Block::Table(t) = blk {
            let n: usize = t.rows.iter().flatten().map(|c| c.paras.len()).sum();
            out.extend(starts[b]..starts[b] + n);
        }
    }
    out
}

/// A paragraph to type into on `page`: in a table cell or a body
/// paragraph with at least three lines.
pub fn target(doc: &input::Doc, pages: &Pages, page: usize, in_table: bool) -> usize {
    let tables = table_paras(doc);
    let paras = doc.paras();
    for pg in pages.pages.iter().skip(page) {
        for l in &pg.lines {
            let ok = if in_table {
                tables.contains(&l.para) && paras[l.para].char_len() > 5
            } else {
                !tables.contains(&l.para) && paras[l.para].char_len() > 200
            };
            if ok {
                return l.para;
            }
        }
    }
    panic!("no target on page {page}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(para: usize, y: f32, range: std::ops::Range<usize>, x0: f32) -> Line {
        let carets = (0..=range.len()).map(|i| x0 + 6.0 * i as f32).collect();
        Line {
            para,
            id: String::new(),
            y,
            height: 14.0,
            baseline: y + 11.0,
            range,
            carets,
            runs: Vec::new(),
            hyphen: false,
        }
    }

    #[test]
    fn locate_and_hit_round_trip_with_side_by_side_cells() {
        let pages = Pages {
            pages: vec![Page {
                width: 595.0,
                height: 842.0,
                lines: vec![
                    line(0, 50.0, 0..10, 60.0),
                    line(0, 64.0, 10..20, 60.0),
                    line(1, 50.0, 0..5, 300.0),
                ],
            }],
        };
        for (para, len) in [(0, 20), (1, 5)] {
            for k in 0..=len {
                let c = locate(&pages, para, k).unwrap();
                let l = &pages.pages[c.page].lines[c.line];
                assert_eq!(
                    hit(&pages, c.page, c.x, l.y + 7.0),
                    Some((para, k)),
                    "para {para} offset {k}"
                );
            }
        }
    }

    #[test]
    fn collapsed_whitespace_gets_the_caret_at_the_end_of_the_line_before() {
        let pages = Pages {
            pages: vec![Page {
                width: 595.0,
                height: 842.0,
                lines: vec![line(0, 50.0, 0..10, 60.0), line(0, 64.0, 12..20, 60.0)],
            }],
        };
        let c = locate(&pages, 0, 11).unwrap();
        assert_eq!((c.line, c.x), (0, 120.0));
    }

    #[test]
    fn hash_ignores_sub_pixel_noise_but_not_breaks() {
        let a = Pages {
            pages: vec![Page {
                width: 1.0,
                height: 1.0,
                lines: vec![line(0, 50.0, 0..10, 60.0)],
            }],
        };
        let mut b = a.clone();
        b.pages[0].lines[0].y += 0.001;
        assert_eq!(layout_hash(&a), layout_hash(&b));
        b.pages[0].lines[0].range.end = 9;
        assert_ne!(layout_hash(&a), layout_hash(&b));
    }
}
