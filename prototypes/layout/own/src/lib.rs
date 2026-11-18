// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 2b prototype (a): our own paginator.
//!
//! - **Shaping**: Parley (HarfRust underneath), once per paragraph at
//!   unbounded width. We take its clusters: advance, text range, and the
//!   line-break opportunities from its Unicode analysis.
//! - **Line breaking**: our own. Knuth–Plass (total-fit) for justified
//!   paragraphs, greedy (first-fit, as LibreOffice does) for the rest, or
//!   greedy everywhere ([`Breaker::Greedy`]) for comparison.
//! - **Hyphenation**: TeX patterns through `hypher` (Danish: Frank Jensen's
//!   patterns, MIT OR LPPL-1.3+), as flagged penalties.
//! - **Pagination**: widows and orphans, keep-with-next, page breaks before;
//!   table rows are kept whole.
//! - **Incremental**: paragraph layouts are cached by content and width.
//!   After an edit, pagination restarts at the edited block and stops as
//!   soon as a later block starts where it started before; everything after
//!   is reused.
//!
//! Known simplifications (spike): no reshaping at line ends (ligatures and
//! kerning across a hyphen or break use the unbroken shaping), no tab
//! stops, no bidi reordering beyond Parley's runs, no footnote areas.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use layout_contract::input::{Align, Block, Doc, LineHeight, Para};
use layout_contract::{EditHint, Engine, Line, Page, Pages, Run};
use parley::{
    FontContext, FontFamily, FontFamilyName, FontStyle, FontWeight, Layout, LayoutContext,
    StyleProperty,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breaker {
    /// Knuth–Plass for justified paragraphs, greedy for ragged ones.
    Optimal,
    /// Greedy everywhere.
    Greedy,
}

pub struct OwnEngine {
    fcx: FontContext,
    lcx: LayoutContext<()>,
    pub breaker: Breaker,
    pub hyphenate: bool,
    /// Let Knuth–Plass shrink spaces (TeX does, to two thirds). LibreOffice
    /// never shrinks, so off is the LibreOffice-compatible setting.
    pub shrink: bool,
    cache: HashMap<u64, Arc<ParaBox>>,
    /// Ascent, descent and line gap of each (font, size), for the strut.
    struts: HashMap<(String, u32), (f32, f32, f32)>,
    /// Per flat paragraph index: the key of its current layout.
    keys: Vec<u64>,
    /// Per block: where it started, and the lines it placed.
    starts: Vec<Flow>,
    placed: Vec<Vec<(usize, Line)>>,
    pages: Pages,
    pub stats: Stats,
    /// Pages the last `layout` call rebuilt (the view repaints only these).
    pub rebuilt: Range<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub paras_laid_out: usize,
    pub blocks_paginated: usize,
    pub pages_rebuilt: usize,
}

/// A table row: its height and, per paragraph, (x, y within the row,
/// layout, flat offset within the table).
type TableRowBox = (f32, Vec<(f32, f32, Arc<ParaBox>, usize)>);

/// The pagination state at the start of a block.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Flow {
    page: usize,
    y: f32,
    /// Height of the footnote area already used on this page.
    notes: f32,
}

/// One paragraph broken into lines, positions relative to its own top-left
/// (x relative to the body's left edge).
#[derive(Clone, Debug)]
pub struct ParaBox {
    pub lines: Vec<PLine>,
    pub space_before: f32,
    pub space_after: f32,
    pub keep_with_next: bool,
    pub page_break_before: bool,
    pub widows: usize,
    pub orphans: usize,
}

#[derive(Clone, Debug)]
pub struct PLine {
    pub range: Range<usize>,
    pub height: f32,
    pub baseline: f32,
    pub carets: Vec<f32>,
    pub runs: Vec<Run>,
    pub hyphen: bool,
    /// Width used and width available, for typography statistics.
    pub natural: f32,
    pub avail: f32,
    pub spaces: usize,
    pub justified: bool,
}

/// One shaped cluster.
#[derive(Clone, Debug)]
struct Cl {
    chars: Range<usize>,
    advance: f32,
    space: bool,
    /// A line may break before this cluster.
    break_before: bool,
    hard_break: bool,
    ascent: f32,
    descent: f32,
    leading: f32,
    span: usize,
    letter: bool,
}

impl Default for OwnEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OwnEngine {
    pub fn new() -> Self {
        let mut fcx = FontContext {
            collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: Default::default(),
        };
        for f in layout_contract::fonts::FACES {
            fcx.collection
                .register_fonts(parley::fontique::Blob::from(f.data.to_vec()), None);
        }
        OwnEngine {
            fcx,
            lcx: LayoutContext::new(),
            breaker: Breaker::Optimal,
            hyphenate: true,
            shrink: false,
            cache: HashMap::new(),
            struts: HashMap::new(),
            keys: Vec::new(),
            starts: Vec::new(),
            placed: Vec::new(),
            pages: Pages::default(),
            stats: Stats::default(),
            rebuilt: 0..0,
        }
    }

    pub fn pages(&self) -> &Pages {
        &self.pages
    }

    fn key(&self, p: &Para, width: f32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        p.text.hash(&mut h);
        for s in &p.spans {
            (
                s.range.start,
                s.range.end,
                &s.font,
                s.size.to_bits(),
                s.bold,
                s.italic,
            )
                .hash(&mut h);
        }
        format!("{:?}", p.style).hash(&mut h);
        width.to_bits().hash(&mut h);
        (self.breaker == Breaker::Greedy, self.hyphenate, self.shrink).hash(&mut h);
        h.finish()
    }

    /// Lay out one paragraph at `width` (cached).
    pub fn para_box(&mut self, p: &Para, width: f32) -> Arc<ParaBox> {
        let k = self.key(p, width);
        if let Some(b) = self.cache.get(&k) {
            return b.clone();
        }
        self.stats.paras_laid_out += 1;
        let b = Arc::new(self.break_para(p, width));
        self.cache.insert(k, b.clone());
        b
    }

    fn shape(&mut self, p: &Para) -> Vec<Cl> {
        let text: &str = if p.text.is_empty() { " " } else { &p.text };
        let byte_of: Vec<usize> = text
            .char_indices()
            .map(|(b, _)| b)
            .chain([text.len()])
            .collect();
        let char_of = |b: usize| byte_of.partition_point(|&x| x < b);
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, text, 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Named(p.style.font.as_str().into()),
        )));
        builder.push_default(StyleProperty::FontSize(p.style.size));
        let spans: &[layout_contract::input::Span] = if p.text.is_empty() { &[] } else { &p.spans };
        for s in spans {
            let r = byte_of[s.range.start.min(byte_of.len() - 1)]
                ..byte_of[s.range.end.min(byte_of.len() - 1)];
            builder.push(
                StyleProperty::FontFamily(FontFamily::Single(FontFamilyName::Named(
                    s.font.as_str().into(),
                ))),
                r.clone(),
            );
            builder.push(StyleProperty::FontSize(s.size), r.clone());
            builder.push(
                StyleProperty::FontWeight(if s.bold {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                }),
                r.clone(),
            );
            builder.push(
                StyleProperty::FontStyle(if s.italic {
                    FontStyle::Italic
                } else {
                    FontStyle::Normal
                }),
                r,
            );
        }
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(None);
        let span_of = |c: usize| spans.iter().position(|s| s.range.contains(&c)).unwrap_or(0);
        let mut out = Vec::new();
        for line in layout.lines() {
            for run in line.runs() {
                let m = run.font_metrics();
                for cl in run.clusters() {
                    let r = cl.text_range();
                    let chars = char_of(r.start)..char_of(r.end);
                    let first = text[r.clone()].chars().next().unwrap_or(' ');
                    out.push(Cl {
                        span: span_of(chars.start),
                        chars,
                        advance: cl.advance(),
                        space: cl.is_space_or_nbsp() && first != '\u{a0}',
                        break_before: cl.is_soft_line_break(),
                        hard_break: cl.is_hard_line_break(),
                        ascent: m.ascent,
                        descent: m.descent,
                        leading: m.leading,
                        letter: first.is_alphabetic(),
                    });
                }
            }
        }
        out.sort_by_key(|c| c.chars.start);
        // Parley's clusters don't expose UAX #14 break opportunities, but
        // its analysis pass does, per character.
        let mut analysis = parley_engine::Analysis::default();
        parley_engine::Analyzer::default().analyze(
            text,
            &parley_engine::AnalysisOptions::default(),
            &mut analysis,
            &parley_engine::AnalysisDataSources::default(),
        );
        let info = analysis.char_info();
        for c in &mut out {
            c.break_before = info
                .get(c.chars.start)
                .is_some_and(|i| i.is_soft_wrap_opportunity());
        }
        out
    }

    /// Every line is at least as tall as the paragraph's own font makes it,
    /// as in LibreOffice (a line holding only a small footnote citation is
    /// still a full line).
    fn strut(&mut self, font: &str, size: f32) -> (f32, f32, f32) {
        let key = (font.to_owned(), size.to_bits());
        if let Some(v) = self.struts.get(&key) {
            return *v;
        }
        let mut builder = self.lcx.ranged_builder(&mut self.fcx, " ", 1.0, false);
        builder.push_default(StyleProperty::FontFamily(FontFamily::Single(
            FontFamilyName::Named(font.to_owned().into()),
        )));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout: Layout<()> = builder.build(" ");
        layout.break_all_lines(None);
        let mut v = (0.0, 0.0, 0.0);
        if let Some(line) = layout.lines().next()
            && let Some(run) = line.runs().next()
        {
            let m = run.font_metrics();
            v = (m.ascent, m.descent, m.leading);
        }
        self.struts.insert(key, v);
        v
    }

    fn hyphen_width(&mut self, p: &Para, span: usize) -> f32 {
        let (font, size, bold, italic) = match p.spans.get(span) {
            Some(s) => (s.font.clone(), s.size, s.bold, s.italic),
            None => (p.style.font.clone(), p.style.size, false, false),
        };
        // Liberation's hyphen is 0.333 em in Serif and Sans, regular and bold.
        let _ = (font, bold, italic);
        size * 0.333
    }

    fn break_para(&mut self, p: &Para, width: f32) -> ParaBox {
        let st = &p.style;
        let cls = self.shape(p);
        let first_width = width - st.indent_left - st.indent_right - st.first_line;
        let rest_width = width - st.indent_left - st.indent_right;
        let justify = st.align == Align::Justify;
        // Break candidates: (cluster index where the next line starts,
        // width if breaking here adds, penalty, flagged, is glue).
        let items = self.items(p, &cls);
        let breaks = if justify && self.breaker == Breaker::Optimal {
            knuth_plass(&cls, &items, first_width, rest_width, self.shrink)
                .unwrap_or_else(|| greedy(&cls, &items, first_width, rest_width))
        } else {
            greedy(&cls, &items, first_width, rest_width)
        };
        let text_len = p.char_len();
        let mut lines = Vec::new();
        let mut start = 0usize;
        for (li, &(end, hyphen, hard)) in breaks.iter().enumerate() {
            let last = li + 1 == breaks.len();
            let avail = if li == 0 { first_width } else { rest_width };
            let seg = &cls[start..end];
            // Trailing spaces hang.
            let mut vis = seg.len();
            while vis > 0 && (seg[vis - 1].space || seg[vis - 1].hard_break) {
                vis -= 1;
            }
            let hyph_w = if hyphen && vis > 0 {
                self.hyphen_width(p, seg[vis - 1].span)
            } else {
                0.0
            };
            let natural: f32 = seg[..vis].iter().map(|c| c.advance).sum::<f32>() + hyph_w;
            let spaces = seg[..vis].iter().filter(|c| c.space).count();
            let do_justify = justify && !last && !hard && spaces > 0;
            let extra = if do_justify {
                (avail - natural) / spaces as f32
            } else {
                0.0
            };
            let x0 = st.indent_left
                + if li == 0 { st.first_line } else { 0.0 }
                + match st.align {
                    Align::Center => ((avail - natural) / 2.0).max(0.0),
                    Align::End => (avail - natural).max(0.0),
                    _ => 0.0,
                };
            let chars_start = seg.first().map_or(text_len.min(start), |c| c.chars.start);
            let chars_end = if last {
                text_len
            } else {
                cls.get(end).map_or(text_len, |c| c.chars.start)
            };
            let mut carets = Vec::with_capacity(chars_end - chars_start + 1);
            let mut runs: Vec<Run> = Vec::new();
            let mut x = x0;
            for (i, c) in seg.iter().enumerate() {
                let w = if i >= vis {
                    0.0
                } else if c.space {
                    c.advance + extra
                } else {
                    c.advance
                };
                let n = c.chars.len().max(1);
                for k in 0..c.chars.len() {
                    carets.push(x + w * k as f32 / n as f32);
                }
                if i < vis {
                    let s = p.spans.get(c.span);
                    let same = runs.last().is_some_and(|r| {
                        s.is_none_or(|s| {
                            r.font == s.font
                                && r.size == s.size
                                && r.bold == s.bold
                                && r.italic == s.italic
                        })
                    });
                    if same {
                        let r = runs.last_mut().unwrap();
                        r.width += w;
                        r.range.end = c.chars.end;
                    } else {
                        runs.push(Run {
                            x,
                            width: w,
                            range: c.chars.clone(),
                            font: s.map_or(st.font.clone(), |s| s.font.clone()),
                            size: s.map_or(st.size, |s| s.size),
                            bold: s.is_some_and(|s| s.bold),
                            italic: s.is_some_and(|s| s.italic),
                        });
                    }
                }
                x += w;
            }
            if p.text.is_empty() {
                carets.clear();
            }
            carets.truncate(chars_end - chars_start);
            while carets.len() < chars_end - chars_start {
                carets.push(x);
            }
            carets.push(x);
            let strut = self.strut(&st.font, st.size);
            let (asc, desc, lead) = seg.iter().fold(strut, |a, c| {
                (a.0.max(c.ascent), a.1.max(c.descent), a.2.max(c.leading))
            });
            let natural_h = asc + desc + lead;
            let height = match st.line_height {
                LineHeight::Proportional(f) => natural_h * f,
                LineHeight::Exact(v) => v,
                LineHeight::AtLeast(v) => natural_h.max(v),
            };
            lines.push(PLine {
                range: chars_start..chars_end,
                height,
                baseline: lead / 2.0 + asc + (height - natural_h) / 2.0,
                carets,
                runs,
                hyphen,
                natural,
                avail,
                spaces,
                justified: do_justify,
            });
            start = end;
        }
        ParaBox {
            lines,
            space_before: st.space_before,
            space_after: st.space_after,
            keep_with_next: st.keep_with_next,
            page_break_before: st.page_break_before,
            widows: st.widows.max(1) as usize,
            orphans: st.orphans.max(1) as usize,
        }
    }

    /// Break opportunities, with hyphenation points.
    fn items(&mut self, p: &Para, cls: &[Cl]) -> Vec<Item> {
        let mut out = Vec::new();
        for i in 1..cls.len() {
            if cls[i - 1].hard_break {
                out.push(Item {
                    at: i,
                    penalty: f32::NEG_INFINITY,
                    width: 0.0,
                    flagged: false,
                });
            } else if cls[i].break_before {
                out.push(Item {
                    at: i,
                    penalty: 0.0,
                    width: 0.0,
                    flagged: false,
                });
            }
        }
        if self.hyphenate && p.style.hyphenate {
            let chars: Vec<char> = p.text.chars().collect();
            let mut i = 0;
            while i < cls.len() {
                if !cls[i].letter {
                    i += 1;
                    continue;
                }
                let mut j = i;
                while j < cls.len() && cls[j].letter && cls[j].chars.len() == 1 {
                    j += 1;
                }
                if j - i >= 5 {
                    let word: String = chars[cls[i].chars.start..cls[j - 1].chars.end]
                        .iter()
                        .collect();
                    let mut pos = 0;
                    let syl: Vec<usize> = hypher::hyphenate(&word, hypher::Lang::Danish)
                        .map(|s| {
                            pos += s.chars().count();
                            pos
                        })
                        .collect();
                    for &k in &syl[..syl.len().saturating_sub(1)] {
                        let w = self.hyphen_width(p, cls[i + k - 1].span);
                        out.push(Item {
                            at: i + k,
                            penalty: 50.0,
                            width: w,
                            flagged: true,
                        });
                    }
                }
                i = j.max(i + 1);
            }
        }
        out.sort_by_key(|it| it.at);
        out.dedup_by_key(|it| it.at);
        out.push(Item {
            at: cls.len(),
            penalty: f32::NEG_INFINITY,
            width: 0.0,
            flagged: false,
        });
        out
    }

    // ---------------------------------------------------------------- pages

    fn table_rows(&mut self, t: &layout_contract::input::Table) -> Vec<TableRowBox> {
        // Each row: height and, per paragraph, (x, y within row, box, flat offset within table).
        const PAD: f32 = 0.1 * 72.0 / 2.54;
        let mut flat = 0;
        let mut out = Vec::new();
        for row in &t.rows {
            let mut col = 0;
            let mut cells = Vec::new();
            let mut h: f32 = 0.0;
            for cell in row {
                let x: f32 = t.cols[..col.min(t.cols.len())].iter().sum();
                let w: f32 = t.cols[col.min(t.cols.len())..(col + cell.colspan).min(t.cols.len())]
                    .iter()
                    .sum();
                let mut y = PAD;
                for p in &cell.paras {
                    let b = self.para_box(p, (w - 2.0 * PAD).max(10.0));
                    y += b.space_before;
                    cells.push((x + PAD, y, b.clone(), flat));
                    y += b.lines.iter().map(|l| l.height).sum::<f32>() + b.space_after;
                    flat += 1;
                }
                h = h.max(y + PAD);
                col += cell.colspan;
            }
            out.push((h, cells));
        }
        out
    }

    /// Paginate blocks from `from`, starting at `state`. Stops early once a
    /// block starts where it started in the previous layout. Returns the
    /// first block not re-placed.
    fn paginate_from(
        &mut self,
        doc: &Doc,
        from: usize,
        mut state: Flow,
        stop_after: Option<usize>,
    ) -> usize {
        let spec = doc.page;
        let body_h = spec.body_height();
        let width = spec.body_width();
        let starts_flat = doc.block_starts();
        let old_starts = std::mem::take(&mut self.starts);
        let mut starts = old_starts[..from.min(old_starts.len())].to_vec();
        self.placed.truncate(doc.blocks.len().max(from));
        self.placed.resize(doc.blocks.len(), Vec::new());
        let mut b = from;
        while b < doc.blocks.len() {
            if stop_after.is_some_and(|e| b > e) && old_starts.get(b) == Some(&state) {
                starts.extend_from_slice(&old_starts[b..]);
                self.starts = starts;
                return b;
            }
            starts.push(state);
            self.stats.blocks_paginated += 1;
            let mut lines: Vec<(usize, Line)> = Vec::new();
            match &doc.blocks[b] {
                Block::Para(p) => {
                    let bx = self.para_box(p, width);
                    // Keep with next: this paragraph's last line and the next
                    // block's first lines must share a page.
                    let need_next = if bx.keep_with_next {
                        match doc.blocks.get(b + 1) {
                            Some(Block::Para(n)) => {
                                let nb = self.para_box(n, width);
                                nb.space_before
                                    + nb.lines
                                        .iter()
                                        .take(nb.orphans)
                                        .map(|l| l.height)
                                        .sum::<f32>()
                            }
                            Some(Block::Table(t)) => {
                                let rows = self.table_rows(t);
                                rows.first().map_or(0.0, |r| r.0)
                            }
                            None => 0.0,
                        }
                    } else {
                        0.0
                    };
                    if bx.page_break_before && state.y > 0.0 {
                        state = Flow {
                            page: state.page + 1,
                            y: 0.0,
                            notes: 0.0,
                        };
                    }
                    let total: f32 = bx.lines.iter().map(|l| l.height).sum();
                    let before = if state.y > 0.0 { bx.space_before } else { 0.0 };
                    if bx.keep_with_next
                        && state.y > 0.0
                        && state.y + before + total + need_next + state.notes > body_h
                    {
                        state = Flow {
                            page: state.page + 1,
                            y: 0.0,
                            notes: 0.0,
                        };
                    }
                    state.y += if state.y > 0.0 { bx.space_before } else { 0.0 };
                    // Decide page breaks inside the paragraph.
                    let n = bx.lines.len();
                    // Footnote height each line brings (its citations' notes).
                    let note_h: Vec<f32> = bx
                        .lines
                        .iter()
                        .map(|l| {
                            p.notes
                                .iter()
                                .filter(|nt| l.range.contains(&nt.at))
                                .map(|nt| self.note_height(nt, p, width))
                                .sum()
                        })
                        .collect();
                    let mut i = 0;
                    while i < n {
                        let room = body_h - state.y;
                        let mut fit = 0;
                        let mut used = 0.0;
                        let mut notes = state.notes;
                        while i + fit < n {
                            let nh = note_h[i + fit];
                            let sep = if nh > 0.0 && notes == 0.0 {
                                NOTE_SEPARATOR
                            } else {
                                0.0
                            };
                            if used + bx.lines[i + fit].height + notes + nh + sep > room + 0.01 {
                                break;
                            }
                            used += bx.lines[i + fit].height;
                            notes += nh + sep;
                            fit += 1;
                        }
                        if fit < n - i {
                            // Orphans: too few lines would stay here.
                            if i == 0 && fit < bx.orphans.min(n) && state.y > 0.0 {
                                fit = 0;
                            }
                            // Widows: too few lines would go to the next page.
                            // Pull lines over; if that leaves too few behind
                            // (orphans), the whole paragraph moves.
                            let left = n - i - fit;
                            if fit > 0 && left < bx.widows {
                                let keep = fit.saturating_sub(bx.widows - left);
                                let min_here = if i == 0 { bx.orphans } else { 1 };
                                fit = if keep >= min_here || state.y == 0.0 {
                                    keep.max(1)
                                } else {
                                    0
                                };
                            }
                            if fit == 0 && state.y == 0.0 {
                                fit = 1; // a line taller than the page
                            }
                        }
                        for (k, l) in bx.lines[i..i + fit].iter().enumerate() {
                            lines.push((
                                state.page,
                                line_at(l, &spec, state.y, starts_flat[b], &p.id, 0.0),
                            ));
                            state.y += l.height;
                            let nh = note_h[i + k];
                            if nh > 0.0 {
                                state.notes += nh
                                    + if state.notes == 0.0 {
                                        NOTE_SEPARATOR
                                    } else {
                                        0.0
                                    };
                            }
                        }
                        i += fit;
                        if i < n {
                            state = Flow {
                                page: state.page + 1,
                                y: 0.0,
                                notes: 0.0,
                            };
                        }
                    }
                    state.y += bx.space_after;
                }
                Block::Table(t) => {
                    let rows = self.table_rows(t);
                    let paras: Vec<&Para> = t
                        .rows
                        .iter()
                        .flatten()
                        .flat_map(|c| c.paras.iter())
                        .collect();
                    for (h, cells) in rows {
                        if state.y > 0.0 && state.y + h + state.notes > body_h {
                            state = Flow {
                                page: state.page + 1,
                                y: 0.0,
                                notes: 0.0,
                            };
                        }
                        for (x, y, bx, k) in cells {
                            let mut yy = state.y + y;
                            for l in &bx.lines {
                                lines.push((
                                    state.page,
                                    line_at(l, &spec, yy, starts_flat[b] + k, &paras[k].id, x),
                                ));
                                yy += l.height;
                            }
                        }
                        state.y += h;
                    }
                }
            }
            self.placed[b] = lines;
            b += 1;
        }
        self.starts = starts;
        doc.blocks.len()
    }

    /// Rebuild pages `first..=last` from the placed lines.
    fn rebuild_pages(&mut self, doc: &Doc, first: usize, last: Option<usize>) {
        let total_pages = self
            .placed
            .iter()
            .flat_map(|v| v.iter().map(|(p, _)| *p))
            .max()
            .map_or(1, |p| p + 1);
        let last = last.unwrap_or(total_pages - 1).min(total_pages - 1);
        self.pages.pages.truncate(total_pages);
        while self.pages.pages.len() < total_pages {
            self.pages.pages.push(Page::default());
        }
        for page in &mut self.pages.pages[first..=last] {
            *page = Page {
                width: doc.page.width,
                height: doc.page.height,
                lines: Vec::new(),
            };
        }
        // Blocks that can have lines on those pages.
        let lo = self
            .starts
            .partition_point(|s| s.page < first)
            .saturating_sub(1);
        let hi = self.starts.partition_point(|s| s.page <= last);
        for b in lo..hi.min(self.placed.len()) {
            for (pg, l) in &self.placed[b] {
                if (first..=last).contains(pg) {
                    self.pages.pages[*pg].lines.push(l.clone());
                }
            }
        }
        self.stats.pages_rebuilt += last + 1 - first;
        self.rebuilt = first..last + 1;
    }
}

fn line_at(
    l: &PLine,
    spec: &layout_contract::input::PageSpec,
    y: f32,
    para: usize,
    id: &str,
    dx: f32,
) -> Line {
    let x0 = spec.left + dx;
    let top = spec.body_top();
    Line {
        para,
        id: id.to_owned(),
        y: top + y,
        height: l.height,
        baseline: top + y + l.baseline,
        range: l.range.clone(),
        carets: l.carets.iter().map(|c| c + x0).collect(),
        runs: l
            .runs
            .iter()
            .map(|r| Run {
                x: r.x + x0,
                ..r.clone()
            })
            .collect(),
        hyphen: l.hyphen,
    }
}

impl Engine for OwnEngine {
    fn name(&self) -> &'static str {
        match (self.breaker, self.shrink) {
            (Breaker::Optimal, false) => "own",
            (Breaker::Optimal, true) => "own-shrink",
            (Breaker::Greedy, _) => "own-greedy",
        }
    }

    fn layout(&mut self, doc: &Doc, hint: &EditHint) -> &Pages {
        let from = match hint {
            EditHint::Para(flat)
                if !self.starts.is_empty() && self.starts.len() == doc.blocks.len() =>
            {
                let starts = doc.block_starts();
                let edited = starts.partition_point(|&s| s <= *flat).saturating_sub(1);
                // A keep-with-next paragraph before it depends on it.
                let mut b = edited;
                while b > 0
                    && matches!(&doc.blocks[b - 1], Block::Para(p) if p.style.keep_with_next)
                {
                    b -= 1;
                }
                Some((b, edited))
            }
            _ => None,
        };
        match from {
            None => {
                self.starts.clear();
                self.placed.clear();
                self.pages.pages.clear();
                self.paginate_from(
                    doc,
                    0,
                    Flow {
                        page: 0,
                        y: 0.0,
                        notes: 0.0,
                    },
                    None,
                );
                self.rebuild_pages(doc, 0, None);
            }
            Some((b, edited)) => {
                let state = self.starts[b];
                let old_last_page = self.pages.pages.len().saturating_sub(1);
                let stop = self.paginate_from(doc, b, state, Some(edited));
                let last = if stop < doc.blocks.len() {
                    // Up to the page the first reused block starts on.
                    Some(self.starts[stop].page)
                } else {
                    None
                };
                let _ = old_last_page;
                self.rebuild_pages(doc, state.page, last);
            }
        }
        let _ = &self.keys;
        &self.pages
    }
}

/// Rule and spacing above the first footnote on a page (LibreOffice's
/// default: 0.101 cm, a 0.018 cm rule, 0.101 cm).
const NOTE_SEPARATOR: f32 = (0.101 + 0.018 + 0.101) * 72.0 / 2.54;

impl OwnEngine {
    /// Height of a footnote laid out at the foot of the page, in 10 pt text.
    fn note_height(&mut self, n: &layout_contract::input::Note, p: &Para, width: f32) -> f32 {
        let mut style = p.style.clone();
        style.size = 10.0;
        style.space_before = 0.0;
        style.space_after = 0.0;
        style.indent_left = 0.0;
        style.indent_right = 0.0;
        style.first_line = 0.0;
        style.label = None;
        style.line_height = LineHeight::Proportional(1.0);
        style.align = Align::Start;
        let note = Para {
            id: String::new(),
            text: n.text.clone(),
            spans: vec![layout_contract::input::Span {
                range: 0..n.text.chars().count(),
                font: style.font.clone(),
                size: 10.0,
                bold: false,
                italic: false,
            }],
            style,
            notes: Vec::new(),
        };
        self.para_box(&note, width)
            .lines
            .iter()
            .map(|l| l.height)
            .sum()
    }
}

// ------------------------------------------------------------- breaking

#[derive(Clone, Copy, Debug)]
struct Item {
    /// The next line starts at this cluster.
    at: usize,
    penalty: f32,
    /// Width added at the end of the line when breaking here (a hyphen).
    width: f32,
    flagged: bool,
}

/// Natural width, stretch and shrink of clusters `a..b`, trailing spaces
/// excluded.
fn measure(cls: &[Cl], a: usize, b: usize) -> (f32, f32, f32) {
    let mut end = b;
    while end > a && (cls[end - 1].space || cls[end - 1].hard_break) {
        end -= 1;
    }
    let mut w = 0.0;
    let mut st = 0.0;
    let mut sh = 0.0;
    for c in &cls[a..end] {
        w += c.advance;
        if c.space {
            st += c.advance * 0.5;
            sh += c.advance / 3.0;
        }
    }
    (w, st, sh)
}

/// First fit. Returns (end cluster, hyphenated, hard break) per line.
fn greedy(cls: &[Cl], items: &[Item], first: f32, rest: f32) -> Vec<(usize, bool, bool)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut k = 0;
    while start < cls.len() {
        let width = if out.is_empty() { first } else { rest };
        let mut best: Option<usize> = None;
        while k < items.len() && items[k].at <= start {
            k += 1;
        }
        let mut j = k;
        while j < items.len() {
            let it = items[j];
            let (w, _, _) = measure(cls, start, it.at);
            if w + it.width > width && best.is_some() {
                break;
            }
            best = Some(j);
            if it.penalty == f32::NEG_INFINITY {
                break;
            }
            if w + it.width > width {
                break;
            }
            j += 1;
        }
        let Some(j) = best else { break };
        let it = items[j];
        out.push((
            it.at,
            it.flagged,
            it.penalty == f32::NEG_INFINITY && it.at < cls.len(),
        ));
        start = it.at;
        k = j + 1;
    }
    if out.is_empty() {
        out.push((cls.len(), false, false));
    }
    out
}

/// Knuth–Plass total-fit line breaking (TeX's algorithm, without looseness).
fn knuth_plass(
    cls: &[Cl],
    items: &[Item],
    first: f32,
    rest: f32,
    shrink: bool,
) -> Option<Vec<(usize, bool, bool)>> {
    const TOLERANCE: f32 = 2.0;
    const LINE_PENALTY: f32 = 10.0;
    const FLAGGED: f32 = 3000.0;
    const FITNESS: f32 = 100.0;
    #[derive(Clone)]
    struct Node {
        item: Option<usize>,
        line: usize,
        fitness: u8,
        demerits: f32,
        prev: Option<usize>,
    }
    let mut nodes = vec![Node {
        item: None,
        line: 0,
        fitness: 1,
        demerits: 0.0,
        prev: None,
    }];
    let mut active: Vec<usize> = vec![0];
    for (ii, it) in items.iter().enumerate() {
        let mut new: Vec<Node> = Vec::new();
        let mut keep = Vec::with_capacity(active.len());
        for &a in &active {
            let start = nodes[a].item.map_or(0, |i| items[i].at);
            let line = nodes[a].line;
            let width = if line == 0 { first } else { rest };
            let last = it.penalty == f32::NEG_INFINITY && it.at == cls.len();
            let (w, st, sh) = measure(cls, start, it.at);
            let sh = if shrink { sh } else { 0.0 };
            let w = w + it.width;
            let ratio = if w < width {
                if last {
                    0.0
                } else if st > 0.0 {
                    (width - w) / st
                } else {
                    f32::INFINITY
                }
            } else if w > width {
                if sh > 0.0 {
                    (width - w) / sh
                } else {
                    f32::NEG_INFINITY
                }
            } else {
                0.0
            };
            if ratio < -1.0 || it.penalty == f32::NEG_INFINITY {
                // Too long (deactivate), or a forced break ends every line.
            } else {
                keep.push(a);
            }
            if (-1.0..=TOLERANCE).contains(&ratio) || (ratio > TOLERANCE && last) {
                let ratio = ratio.min(TOLERANCE);
                let badness = 100.0 * ratio.abs().powi(3);
                let pen = if it.penalty.is_finite() {
                    it.penalty
                } else {
                    0.0
                };
                let mut d = (LINE_PENALTY + badness).powi(2);
                if pen > 0.0 {
                    d += pen * pen;
                }
                let fitness = if ratio < -0.5 {
                    0
                } else if ratio <= 0.5 {
                    1
                } else if ratio <= 1.0 {
                    2
                } else {
                    3
                };
                if it.flagged
                    && let Some(pi) = nodes[a].item
                    && items[pi].flagged
                {
                    d += FLAGGED;
                }
                if (i32::from(fitness) - i32::from(nodes[a].fitness)).abs() > 1 {
                    d += FITNESS;
                }
                let total = nodes[a].demerits + d;
                match new.iter_mut().find(|n| n.fitness == fitness) {
                    Some(n) if n.demerits <= total => {}
                    Some(n) => {
                        *n = Node {
                            item: Some(ii),
                            line: line + 1,
                            fitness,
                            demerits: total,
                            prev: Some(a),
                        }
                    }
                    None => new.push(Node {
                        item: Some(ii),
                        line: line + 1,
                        fitness,
                        demerits: total,
                        prev: Some(a),
                    }),
                }
            }
        }
        active = keep;
        for n in new {
            nodes.push(n);
            active.push(nodes.len() - 1);
        }
        if active.is_empty() {
            return None;
        }
    }
    let end = active
        .iter()
        .copied()
        .filter(|&a| nodes[a].item.is_some_and(|i| items[i].at == cls.len()))
        .min_by(|a, b| nodes[*a].demerits.total_cmp(&nodes[*b].demerits))?;
    let mut out = Vec::new();
    let mut cur = Some(end);
    while let Some(c) = cur {
        if let Some(i) = nodes[c].item {
            let it = items[i];
            out.push((
                it.at,
                it.flagged,
                it.penalty == f32::NEG_INFINITY && it.at < cls.len(),
            ));
        }
        cur = nodes[c].prev;
    }
    out.reverse();
    Some(out)
}
