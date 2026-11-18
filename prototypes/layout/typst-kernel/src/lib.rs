// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 2b prototype (b): Typst as the layout kernel.
//!
//! The layout input is turned into Typst markup, one chunk per paragraph,
//! and compiled with Typst's own incremental machinery (comemo). For every
//! model character we record the source byte that produced it, so a glyph
//! in Typst's output (which carries a span and an offset into it) maps back
//! to a paragraph and character: that is caret mapping, both ways. An edit
//! regenerates only the edited paragraph's chunk and applies it with
//! `Source::edit`, which reparses incrementally.
//!
//! Styling mirrors what the own engine does with the same input: same
//! fonts, LibreOffice's line pitch (ascent + descent + line gap, times the
//! paragraph's proportion), space before/after summed like LibreOffice,
//! justification and Danish hyphenation, keep-with-next as sticky blocks,
//! footnotes as Typst footnotes.

use std::collections::HashMap;
use std::ops::Range;

use layout_contract::input::{Align, Block, Doc, LineHeight, Para};
use layout_contract::{EditHint, Engine, Line, Page, Pages, Run};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem, Point};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// Glyphs gathered for one line: baseline, font size, caret stops
/// (character, x, advance) and runs.
type LineAcc = (f32, f32, Vec<(usize, f32, f32)>, Vec<Run>);

/// Liberation's vertical metrics (em): ascender, descender, line gap.
const ASC: f32 = 0.891;
const DESC: f32 = 0.216;
const GAP: f32 = 0.042;

pub struct SkrivWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    pub source: Source,
}

impl World for SkrivWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, _: FileId) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }
    fn today(&self, _: Option<typst::foundations::Duration>) -> Option<Datetime> {
        None
    }
}

impl typst_ide::IdeWorld for SkrivWorld {
    fn upcast(&self) -> &dyn World {
        self
    }
}

/// Where each paragraph's markup sits in the source, and which source byte
/// each of its characters came from (relative to the chunk start).
#[derive(Clone, Debug, Default)]
struct Chunk {
    range: Range<usize>,
    chars: Vec<Option<usize>>,
}

pub struct TypstEngine {
    pub world: SkrivWorld,
    /// LibreOffice-compatible line breaking: first-fit, spaces never shrink.
    pub compat: bool,
    chunks: Vec<Chunk>,
    /// Syntax node span → (paragraph, byte offset within its chunk). Spans
    /// of untouched nodes survive an incremental reparse, so after an edit
    /// only the edited paragraph's nodes are re-indexed.
    spans: HashMap<typst::syntax::Span, (usize, usize)>,
    doc_shape: u64,
    pages: Pages,
    /// Hashes of the previous page frames, to convert only pages that changed.
    frames: Vec<u128>,
    pub timings: Timings,
}

#[derive(Clone, Debug, Default)]
pub struct Timings {
    pub generate_ms: f64,
    pub compile_ms: f64,
    pub convert_ms: f64,
    pub pages_converted: usize,
    pub warnings: usize,
    pub evict_ms: f64,
}

fn now() -> f64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::OnceLock;
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1e3
    }
    #[cfg(target_arch = "wasm32")]
    {
        0.0
    }
}

impl Default for TypstEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TypstEngine {
    pub fn new() -> Self {
        let fonts: Vec<Font> = layout_contract::fonts::FACES
            .iter()
            .flat_map(|f| Font::iter(Bytes::new(f.data)))
            .collect();
        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").expect("path"),
        )
        .intern();
        TypstEngine {
            compat: false,
            world: SkrivWorld {
                library: LazyHash::new(Library::default()),
                book: LazyHash::new(FontBook::from_fonts(&fonts)),
                fonts,
                source: Source::new(id, String::new()),
            },
            chunks: Vec::new(),
            spans: HashMap::new(),
            doc_shape: 0,
            pages: Pages::default(),
            frames: Vec::new(),
            timings: Timings::default(),
        }
    }

    pub fn pages(&self) -> &Pages {
        &self.pages
    }

    pub fn document(&self) -> Option<PagedDocument> {
        typst::compile::<PagedDocument>(&self.world).output.ok()
    }

    /// The whole Typst source for `doc`, with chunk positions.
    fn generate(doc: &Doc, compat: bool) -> (String, Vec<Chunk>) {
        let p = doc.page;
        let mut s = String::new();
        s.push_str(&format!(
            "#set page(width: {}pt, height: {}pt, margin: (top: {}pt, bottom: {}pt, left: {}pt, right: {}pt))\n",
            p.width,
            p.height,
            p.top + p.header,
            p.bottom + p.footer,
            p.left,
            p.right
        ));
        s.push_str(
            "#set text(font: \"Liberation Serif\", size: 12pt, lang: \"da\", top-edge: 0.891em, bottom-edge: -0.216em, fallback: false)\n\
             #set par(spacing: 0pt)\n#set block(spacing: 0pt)\n#set footnote.entry(separator: line(length: 25%, stroke: 0.5pt), clearance: 0.2cm, gap: 0pt)\n\
             #show footnote.entry: set text(size: 10pt)\n",
        );
        let mut chunks = Vec::new();
        let mut prev_after: Option<f32> = None;
        for b in &doc.blocks {
            match b {
                Block::Para(para) => {
                    // LibreOffice's proportional spacing also applies below a
                    // paragraph's last line; Typst's leading only sits between lines.
                    let gap = prev_after.unwrap_or(0.0) + para.style.space_before;
                    if prev_after.is_some() && gap > 0.0 {
                        s.push_str(&format!("#v({gap}pt, weak: true)\n"));
                    }
                    let start = s.len();
                    let (text, chars) = para_markup(para, compat);
                    s.push_str(&text);
                    chunks.push(Chunk {
                        range: start..s.len(),
                        chars,
                    });
                    s.push('\n');
                    prev_after = Some(para.style.space_after + extra_leading(para));
                }
                Block::Table(t) => {
                    if prev_after.is_some_and(|a| a > 0.0) {
                        s.push_str(&format!("#v({}pt, weak: true)\n", prev_after.unwrap()));
                    }
                    let cols: Vec<String> = t.cols.iter().map(|c| format!("{c}pt")).collect();
                    s.push_str(&format!(
                        "#table(columns: ({},), inset: 0.1cm, stroke: none,\n",
                        cols.join(", ")
                    ));
                    for row in &t.rows {
                        for cell in row {
                            s.push_str(&format!("table.cell(colspan: {})[", cell.colspan.max(1)));
                            for (k, para) in cell.paras.iter().enumerate() {
                                if k > 0 {
                                    s.push('\n');
                                }
                                let start = s.len();
                                let (text, chars) = para_markup(para, compat);
                                s.push_str(&text);
                                chunks.push(Chunk {
                                    range: start..s.len(),
                                    chars,
                                });
                            }
                            s.push_str("],\n");
                        }
                    }
                    s.push_str(")\n");
                    prev_after = Some(0.0);
                }
            }
        }
        (s, chunks)
    }

    fn shape_of(doc: &Doc) -> u64 {
        let mut h: u64 = doc.blocks.len() as u64;
        for p in doc.paras() {
            h = h.wrapping_mul(31).wrapping_add(p.id.len() as u64);
        }
        h
    }

    fn convert(&mut self, doc: &PagedDocument) {
        let t = now();
        let pages = doc.pages();
        let mut out = Vec::with_capacity(pages.len());
        let mut converted = 0;
        for (i, page) in pages.iter().enumerate() {
            if let (Some(old), Some(conv)) = (self.frames.get(i), self.pages.pages.get(i))
                && *old == typst::utils::hash128(&page.frame)
            {
                out.push(conv.clone());
                continue;
            }
            converted += 1;
            out.push(self.convert_page(&page.frame));
        }
        self.frames = pages
            .iter()
            .map(|p| typst::utils::hash128(&p.frame))
            .collect();
        self.pages = Pages { pages: out };
        self.timings.pages_converted = converted;
        self.timings.convert_ms = now() - t;
    }

    /// Record the spans of every leaf in chunk `i`.
    fn index_chunk(&mut self, i: usize) {
        use typst::syntax::{LinkedNode, Side};
        let r = self.chunks[i].range.clone();
        let root = LinkedNode::new(self.world.source.root());
        let mut leaf = root.leaf_at(r.start, Side::After);
        while let Some(l) = leaf {
            if l.offset() >= r.end {
                break;
            }
            if l.offset() >= r.start {
                self.spans.insert(l.span(), (i, l.offset() - r.start));
            }
            leaf = l.next_leaf();
        }
    }

    fn index_all(&mut self) {
        self.spans.clear();
        for i in 0..self.chunks.len() {
            self.index_chunk(i);
        }
    }

    /// Model position of a glyph: its node's span and offset into the node.
    fn locate_glyph(&self, span: typst::syntax::Span, off: usize) -> Option<(usize, usize)> {
        let (i, rel) = *self.spans.get(&span)?;
        let rel = rel + off;
        let c = &self.chunks[i];
        let mut best = None;
        for (k, s) in c.chars.iter().enumerate() {
            if let Some(s) = s {
                if *s <= rel {
                    best = Some(k);
                } else {
                    break;
                }
            }
        }
        best.map(|k| (i, k))
    }

    /// The source byte a model character came from.
    pub fn source_byte(&self, para: usize, ch: usize) -> Option<usize> {
        let c = self.chunks.get(para)?;
        Some(c.range.start + (*c.chars.get(ch)?)?)
    }

    /// Model position of a source byte.
    #[allow(dead_code)]
    fn locate_byte(&self, byte: usize) -> Option<(usize, usize)> {
        let i = self
            .chunks
            .partition_point(|c| c.range.start <= byte)
            .checked_sub(1)?;
        let c = &self.chunks[i];
        if byte >= c.range.end {
            return None;
        }
        let rel = byte - c.range.start;
        // The character whose source starts at or before `rel`, last such.
        let mut best = None;
        for (k, s) in c.chars.iter().enumerate() {
            if let Some(s) = s {
                if *s <= rel {
                    best = Some(k);
                } else {
                    break;
                }
            }
        }
        best.map(|k| (i, k))
    }

    fn convert_page(&self, frame: &Frame) -> Page {
        // (para, baseline) → glyph stops (char offset, x, advance) and runs.
        let mut lines: HashMap<(usize, i64), LineAcc> = HashMap::new();
        let mut stack: Vec<(&Frame, Point)> = vec![(frame, Point::zero())];
        while let Some((f, origin)) = stack.pop() {
            for (pos, item) in f.items() {
                let at = origin + *pos;
                match item {
                    FrameItem::Group(g) => {
                        let off = Point::new(g.transform.tx, g.transform.ty);
                        stack.push((&g.frame, at + off));
                    }
                    FrameItem::Text(text) => {
                        let size = text.size.to_pt() as f32;
                        let mut x = at.x.to_pt() as f32;
                        let y = at.y.to_pt() as f32;
                        let run_x = x;
                        let mut first: Option<usize> = None;
                        let mut last_end = 0;
                        let mut para_of_run = None;
                        for g in &text.glyphs {
                            let adv = g.x_advance.at(text.size).to_pt() as f32;
                            {
                                if let Some((para, ch)) =
                                    self.locate_glyph(g.span.0, usize::from(g.span.1))
                                {
                                    let n = text.text[g.range()].chars().count().max(1);
                                    let e = lines
                                        .entry((para, (y * 16.0).round() as i64))
                                        .or_insert((y, size, Vec::new(), Vec::new()));
                                    for k in 0..n {
                                        e.2.push((
                                            ch + k,
                                            x + adv * k as f32 / n as f32,
                                            adv / n as f32,
                                        ));
                                    }
                                    first.get_or_insert(ch);
                                    last_end = ch + n;
                                    para_of_run = Some(para);
                                }
                            }
                            x += adv;
                        }
                        if let (Some(para), Some(st)) = (para_of_run, first) {
                            let e = lines.get_mut(&(para, (y * 16.0).round() as i64)).unwrap();
                            let info = text.font.info();
                            e.3.push(Run {
                                x: run_x,
                                width: x - run_x,
                                range: st..last_end,
                                font: info.family.clone(),
                                size,
                                bold: info.variant.weight.to_number() >= 600,
                                italic: info.variant.style != typst::text::FontStyle::Normal,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut out: Vec<Line> = lines
            .into_iter()
            .map(|((para, _), (baseline, size, mut stops, runs))| {
                stops.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
                stops.dedup_by_key(|s| s.0);
                let start = stops.first().map_or(0, |s| s.0);
                let end = stops.last().map_or(0, |s| s.0 + 1);
                let mut carets = Vec::with_capacity(end - start + 1);
                let mut k = 0;
                let mut last_x = stops.first().map_or(0.0, |s| s.1);
                for c in start..end {
                    while k < stops.len() && stops[k].0 < c {
                        k += 1;
                    }
                    if k < stops.len() && stops[k].0 == c {
                        last_x = stops[k].1;
                    }
                    carets.push(last_x);
                }
                let tail = stops.last().map_or(0.0, |s| s.1 + s.2);
                carets.push(tail);
                Line {
                    para,
                    id: String::new(),
                    y: baseline - ASC * size,
                    height: (ASC + DESC + GAP) * size,
                    baseline,
                    range: start..end,
                    carets,
                    runs,
                    hyphen: false,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            a.y.total_cmp(&b.y)
                .then(a.carets[0].total_cmp(&b.carets[0]))
        });
        Page {
            width: frame.width().to_pt() as f32,
            height: frame.height().to_pt() as f32,
            lines: out,
        }
    }

    fn compile(&mut self) -> Option<PagedDocument> {
        let t = now();
        let res = typst::compile::<PagedDocument>(&self.world);
        self.timings.compile_ms = now() - t;
        // comemo's caches grow with every compile; without eviction a few
        // hundred keystrokes on 100 pages exhaust memory (the Typst CLI
        // evicts entries unused for 10 compilations in watch mode).
        let t = now();
        typst::comemo::evict(10);
        self.timings.evict_ms = now() - t;
        self.timings.warnings = res.warnings.len();
        match res.output {
            Ok(d) => Some(d),
            Err(e) => {
                let msg: Vec<String> = e.iter().map(|d| d.message.to_string()).take(3).collect();
                panic!("typst compile failed: {msg:?}");
            }
        }
    }
}

impl Engine for TypstEngine {
    fn name(&self) -> &'static str {
        if self.compat { "typst-compat" } else { "typst" }
    }

    fn layout(&mut self, doc: &Doc, hint: &EditHint) -> &Pages {
        let t = now();
        let shape = Self::shape_of(doc);
        match hint {
            EditHint::Para(flat) if shape == self.doc_shape && *flat < self.chunks.len() => {
                let para = doc.paras()[*flat];
                let (text, chars) = para_markup(para, self.compat);
                let old = self.chunks[*flat].range.clone();
                self.world.source.edit(old.clone(), &text);
                let delta = text.len() as isize - old.len() as isize;
                self.chunks[*flat] = Chunk {
                    range: old.start..old.start + text.len(),
                    chars,
                };
                for c in &mut self.chunks[*flat + 1..] {
                    c.range.start = (c.range.start as isize + delta) as usize;
                    c.range.end = (c.range.end as isize + delta) as usize;
                }
                self.index_chunk(*flat);
            }
            _ => {
                let (text, chunks) = Self::generate(doc, self.compat);
                self.world.source.replace(&text);
                self.chunks = chunks;
                self.index_all();
                self.doc_shape = shape;
                self.frames.clear();
                self.pages = Pages::default();
            }
        }
        self.timings.generate_ms = now() - t;
        if let Some(d) = self.compile() {
            self.convert(&d);
        }
        &self.pages
    }
}

/// The space LibreOffice adds below a line on top of the font's natural
/// line height (proportional or exact line spacing).
fn extra_leading(p: &Para) -> f32 {
    let natural = (ASC + DESC + GAP) * p.style.size;
    let line = match p.style.line_height {
        LineHeight::Proportional(f) => natural * f,
        LineHeight::Exact(v) => v,
        LineHeight::AtLeast(v) => natural.max(v),
    };
    (line - (ASC + DESC) * p.style.size).max(0.0)
}

/// Typst markup for one paragraph, and the source offset of each of its
/// characters within that markup (None for characters Typst draws
/// differently, such as a footnote citation).
pub fn para_markup(p: &Para, compat: bool) -> (String, Vec<Option<usize>>) {
    let st = &p.style;
    let mut s = String::new();
    let pitch = |f: f32| (ASC + DESC + GAP) * f - (ASC + DESC);
    let leading = match st.line_height {
        LineHeight::Proportional(f) => pitch(f) * st.size,
        LineHeight::Exact(v) => v - (ASC + DESC) * st.size,
        LineHeight::AtLeast(v) => (v - (ASC + DESC) * st.size).max(pitch(1.0) * st.size),
    };
    let align = match st.align {
        Align::Start | Align::Justify => "left",
        Align::Center => "center",
        Align::End => "right",
    };
    if compat {
        s.push_str("#set par(linebreaks: \"simple\", justification-limits: (spacing: (min: 100%, max: 1000%)));");
    }
    s.push_str(&format!(
        "#block(sticky: {}, inset: (left: {}pt, right: {}pt))[#set par(justify: {}, leading: {}pt, first-line-indent: (amount: {}pt, all: true)); #set align({}); #set text(font: \"{}\", size: {}pt, hyphenate: {});",
        st.keep_with_next,
        st.indent_left,
        st.indent_right,
        st.align == Align::Justify,
        leading,
        st.first_line,
        align,
        st.font,
        st.size,
        st.hyphenate
    ));
    if st.page_break_before {
        s.insert_str(0, "#pagebreak(weak: true)\n");
    }
    if let Some(label) = &st.label {
        s.push_str(&format!(
            "#box(width: 0pt, move(dx: -0.635cm, [{}]))",
            escape(label)
        ));
    }
    let chars: Vec<char> = p.text.chars().collect();
    let mut map = vec![None; chars.len()];
    let mut i = 0;
    for span in &p.spans {
        let plain = span.font == st.font && span.size == st.size && !span.bold && !span.italic;
        if !plain {
            s.push_str(&format!(
                "#text(font: \"{}\", size: {}pt, weight: \"{}\", style: \"{}\")[",
                span.font,
                span.size,
                if span.bold { "bold" } else { "regular" },
                if span.italic { "italic" } else { "normal" }
            ));
        }
        i = i.max(span.range.start);
        while i < span.range.end.min(chars.len()) {
            if let Some(n) = p.notes.iter().find(|n| n.at == i) {
                s.push_str(&format!("#footnote[{}]", escape(&n.text)));
                i += 1;
                continue;
            }
            let c = chars[i];
            map[i] = Some(s.len());
            if c == '\n' {
                s.push_str("#linebreak()");
            } else {
                s.push_str(&escape_char(c));
            }
            i += 1;
        }
        if !plain {
            s.push(']');
        }
    }
    s.push(']');
    (s, map)
}

fn escape_char(c: char) -> String {
    match c {
        '\\' | '*' | '_' | '`' | '#' | '$' | '<' | '>' | '@' | '[' | ']' | '=' | '-' | '+'
        | '/' | '~' | '\'' | '"' | '.' | ':' => format!("\\{c}"),
        c => c.to_string(),
    }
}

fn escape(s: &str) -> String {
    s.chars().map(escape_char).collect()
}
