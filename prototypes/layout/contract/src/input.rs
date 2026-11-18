// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What a paginator lays out: the model with styles resolved, flattened
//! to paragraphs and simple tables. Lengths are in points.

use std::ops::Range;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub page: PageSpec,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSpec {
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    /// Space reserved for the page header and footer (content plus their
    /// spacing), taken from the body.
    #[serde(default)]
    pub header: f32,
    #[serde(default)]
    pub footer: f32,
}

impl PageSpec {
    pub fn a4() -> Self {
        let cm = 72.0 / 2.54;
        PageSpec {
            width: 21.0 * cm,
            height: 29.7 * cm,
            top: 2.0 * cm,
            bottom: 2.0 * cm,
            left: 2.0 * cm,
            right: 2.0 * cm,
            header: 0.0,
            footer: 0.0,
        }
    }

    pub fn body_width(&self) -> f32 {
        self.width - self.left - self.right
    }

    pub fn body_height(&self) -> f32 {
        self.height - self.top - self.bottom - self.header - self.footer
    }

    /// Top of the body area.
    pub fn body_top(&self) -> f32 {
        self.top + self.header
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Para(Para),
    Table(Table),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Para {
    pub id: String,
    pub text: String,
    /// Character style runs, contiguous, covering the whole text.
    pub spans: Vec<Span>,
    pub style: ParaStyle,
    /// Footnotes cited in this paragraph: the citation's character offset
    /// and the note's text (laid out at the foot of the page).
    #[serde(default)]
    pub notes: Vec<Note>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub at: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub range: Range<usize>,
    pub font: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Align {
    Start,
    Center,
    End,
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum LineHeight {
    /// A multiple of the font's natural line height (1.0 = single).
    Proportional(f32),
    Exact(f32),
    AtLeast(f32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParaStyle {
    pub align: Align,
    pub space_before: f32,
    pub space_after: f32,
    pub indent_left: f32,
    pub indent_right: f32,
    pub first_line: f32,
    pub line_height: LineHeight,
    pub keep_with_next: bool,
    pub page_break_before: bool,
    pub widows: u8,
    pub orphans: u8,
    pub hyphenate: bool,
    pub heading: Option<u8>,
    /// A list label drawn in the indent before the first line ("1.", "•").
    pub label: Option<String>,
    /// The default character style (used for empty paragraphs and labels).
    pub font: String,
    pub size: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: String,
    pub cols: Vec<f32>,
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub paras: Vec<Para>,
    pub colspan: usize,
}

impl Para {
    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    /// Type a character: it takes the style of the character before it.
    pub fn insert(&mut self, offset: usize, c: char) {
        let byte = self
            .text
            .char_indices()
            .nth(offset)
            .map_or(self.text.len(), |(b, _)| b);
        self.text.insert(byte, c);
        let mut hit = false;
        for s in &mut self.spans {
            if !hit
                && (offset > s.range.start && offset <= s.range.end
                    || offset == 0 && s.range.start == 0)
            {
                s.range.end += 1;
                hit = true;
            } else if hit || s.range.start >= offset {
                s.range.start += 1;
                s.range.end += 1;
            }
        }
        if self.spans.is_empty() {
            self.spans.push(Span {
                range: 0..1,
                font: self.style.font.clone(),
                size: self.style.size,
                bold: false,
                italic: false,
            });
        }
    }
}

impl Doc {
    /// Every paragraph in document order, tables row by row; the index in
    /// this list is the paragraph's flat index ([`crate::Line::para`]).
    pub fn paras(&self) -> Vec<&Para> {
        let mut out = Vec::new();
        for b in &self.blocks {
            match b {
                Block::Para(p) => out.push(p),
                Block::Table(t) => {
                    for row in &t.rows {
                        for c in row {
                            out.extend(c.paras.iter());
                        }
                    }
                }
            }
        }
        out
    }

    pub fn para_mut(&mut self, flat: usize) -> Option<&mut Para> {
        let mut i = 0;
        for b in &mut self.blocks {
            match b {
                Block::Para(p) => {
                    if i == flat {
                        return Some(p);
                    }
                    i += 1;
                }
                Block::Table(t) => {
                    for row in &mut t.rows {
                        for c in row {
                            for p in &mut c.paras {
                                if i == flat {
                                    return Some(p);
                                }
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Flat index of the first paragraph of each block.
    pub fn block_starts(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = 0;
        for b in &self.blocks {
            out.push(i);
            i += match b {
                Block::Para(_) => 1,
                Block::Table(t) => t.rows.iter().flatten().map(|c| c.paras.len()).sum(),
            };
        }
        out
    }
}
