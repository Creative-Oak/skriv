// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Build layout input from a skriv model document: resolve paragraph and
//! character styles through ODF inheritance, flatten sections, number
//! lists, and size table columns.
//!
//! Spike scope: what the Danish municipal letter and the generated corpus
//! use. Not handled: frames and images (skipped), footnote bodies (only the
//! citation is shown), tab stops (a tab is a space), multi-column sections.

use skriv_model::*;

use crate::fonts::family_for;
use crate::input::{self, Align, LineHeight, PageSpec, ParaStyle};

/// An ODF length in points. Percentages return None.
pub fn length(v: &str) -> Option<f32> {
    let v = v.trim();
    let (num, unit) = v.split_at(
        v.find(|c: char| c.is_ascii_alphabetic() || c == '%')
            .unwrap_or(v.len()),
    );
    let n: f32 = num.trim().parse().ok()?;
    Some(match unit {
        "pt" | "" => n,
        "cm" => n * 72.0 / 2.54,
        "mm" => n * 72.0 / 25.4,
        "in" | "inch" => n * 72.0,
        "pc" => n * 12.0,
        "px" => n * 0.75,
        _ => return None,
    })
}

fn percent(v: &str) -> Option<f32> {
    v.trim()
        .strip_suffix('%')?
        .trim()
        .parse::<f32>()
        .ok()
        .map(|p| p / 100.0)
}

/// A style and its ancestors, nearest first, then the family default.
fn chain<'a>(styles: &'a Styles, family: &str, name: Option<&str>) -> Vec<&'a Style> {
    let mut out = Vec::new();
    let mut cur = name.map(str::to_owned);
    while let Some(n) = cur {
        if out.len() > 32 {
            break;
        }
        match styles.find(family, &n) {
            Some((s, _)) => {
                out.push(s);
                cur = s.parent().map(str::to_owned);
            }
            None => break,
        }
    }
    if let Some(d) = styles.default_style(family) {
        out.push(d);
    }
    out
}

fn get<'a>(chain: &[&'a Style], set: &str, attr: &str) -> Option<&'a str> {
    chain
        .iter()
        .find_map(|s| s.props(set).and_then(|p| p.attrs.get(attr)))
}

/// Font size, following percentages up the chain.
fn font_size(chain: &[&Style], base: f32) -> f32 {
    for (i, s) in chain.iter().enumerate() {
        if let Some(v) = s
            .props("style:text-properties")
            .and_then(|p| p.attrs.get("fo:font-size"))
        {
            if let Some(pt) = length(v) {
                return pt;
            }
            if let Some(p) = percent(v) {
                return p * font_size(&chain[i + 1..], base);
            }
        }
    }
    base
}

#[derive(Clone)]
struct CharStyle {
    font: String,
    size: f32,
    bold: bool,
    italic: bool,
}

fn char_style(chain: &[&Style], parent: &CharStyle) -> CharStyle {
    let font = get(chain, "style:text-properties", "style:font-name")
        .map(|f| family_for(f).to_owned())
        .unwrap_or_else(|| parent.font.clone());
    let size = font_size(chain, parent.size);
    let bold = get(chain, "style:text-properties", "fo:font-weight").map_or(parent.bold, |w| {
        w == "bold" || w.parse::<u32>().is_ok_and(|n| n >= 600)
    });
    let italic = get(chain, "style:text-properties", "fo:font-style")
        .map_or(parent.italic, |s| s == "italic" || s == "oblique");
    CharStyle {
        font,
        size,
        bold,
        italic,
    }
}

/// Page size and margins from the first page layout in styles.xml (it is
/// carried in a slot; the model doesn't type page layouts yet).
fn page_spec(doc: &Document) -> PageSpec {
    let mut spec = PageSpec::a4();
    for e in doc.styles.styles_automatic.iter().chain(&doc.styles.named) {
        let StyleEntry::Opaque(slot) = e else {
            continue;
        };
        let Some(i) = slot.raw.find("<style:page-layout-properties") else {
            continue;
        };
        let tag = &slot.raw[i..slot.raw[i..].find('>').map_or(slot.raw.len(), |e| i + e)];
        let attr = |name: &str| {
            let k = format!("{name}=\"");
            let s = tag.find(&k)? + k.len();
            let e = tag[s..].find('"')? + s;
            length(&tag[s..e])
        };
        if let Some(v) = attr("fo:page-width") {
            spec.width = v;
        }
        if let Some(v) = attr("fo:page-height") {
            spec.height = v;
        }
        if let Some(v) = attr("fo:margin-top") {
            spec.top = v;
        }
        if let Some(v) = attr("fo:margin-bottom") {
            spec.bottom = v;
        }
        if let Some(v) = attr("fo:margin-left") {
            spec.left = v;
        }
        if let Some(v) = attr("fo:margin-right") {
            spec.right = v;
        }
        // Header and footer: the area is its min-height (or one line of
        // 12 pt text, whichever is more) plus its spacing to the body.
        let area = |kind: &str| -> f32 {
            let open = format!("<style:{kind}-style>");
            let Some(i) = slot.raw.find(&open) else {
                return 0.0;
            };
            let rest = &slot.raw[i..];
            let Some(j) = rest.find("<style:header-footer-properties") else {
                return 0.0;
            };
            let tag = &rest[j..rest[j..].find('>').map_or(rest.len(), |e| j + e)];
            let get = |name: &str| {
                let k = format!("{name}=\"");
                let s = tag.find(&k)? + k.len();
                let e = tag[s..].find('"')? + s;
                length(&tag[s..e])
            };
            let min = get("fo:min-height").unwrap_or(0.0);
            let gap = get("fo:margin-bottom")
                .or_else(|| get("fo:margin-top"))
                .unwrap_or(0.0);
            min.max(13.8) + gap
        };
        spec.header = area("header");
        spec.footer = area("footer");
        break;
    }
    spec
}

struct Builder<'a> {
    styles: &'a Styles,
    body_width: f32,
}

pub fn build(doc: &Document) -> input::Doc {
    let page = page_spec(doc);
    let b = Builder {
        styles: &doc.styles,
        body_width: page.body_width(),
    };
    let mut blocks = Vec::new();
    b.blocks(&doc.body, &mut blocks, &mut ListCtx::default());
    input::Doc { page, blocks }
}

#[derive(Default)]
struct ListCtx {
    /// (level, counter, bullet?) for the lists we are inside.
    stack: Vec<(u8, usize, bool)>,
}

impl Builder<'_> {
    fn blocks(&self, v: &[Block], out: &mut Vec<input::Block>, lists: &mut ListCtx) {
        for b in v {
            match b {
                Block::Paragraph(p) => out.push(input::Block::Para(self.para(p, None, 0.0))),
                Block::Section(s) => self.blocks(&s.blocks, out, lists),
                Block::List(l) => {
                    let level = lists.stack.len() as u8 + 1;
                    let bullet = l
                        .style_name()
                        .and_then(|n| self.styles.list_style(n))
                        .and_then(|ls| ls.level(level))
                        .is_some_and(|lv| lv.kind == ListLevelKind::Bullet);
                    lists.stack.push((level, 0, bullet));
                    for c in &l.children {
                        if let ListChild::Item(item) = c {
                            if let Some(top) = lists.stack.last_mut() {
                                top.1 += 1;
                            }
                            let (lvl, n, bullet) = *lists.stack.last().unwrap();
                            let label = if bullet {
                                "•".to_owned()
                            } else {
                                format!("{n}.")
                            };
                            let indent = f32::from(lvl) * 0.635 * 72.0 / 2.54;
                            let mut first = true;
                            for b in &item.blocks {
                                match b {
                                    Block::Paragraph(p) => {
                                        let lab = first.then(|| label.clone());
                                        first = false;
                                        out.push(input::Block::Para(self.para(p, lab, indent)));
                                    }
                                    other => self.blocks(std::slice::from_ref(other), out, lists),
                                }
                            }
                        }
                    }
                    lists.stack.pop();
                }
                Block::Table(t) => out.push(input::Block::Table(self.table(t))),
                Block::TrackedChanges(_) | Block::Opaque(_) => {}
            }
        }
    }

    fn table(&self, t: &Table) -> input::Table {
        let mut cols: Vec<f32> = t
            .columns()
            .iter()
            .map(|c| {
                c.style_name()
                    .and_then(|n| {
                        self.styles.resolve(
                            "table-column",
                            n,
                            "style:table-column-properties",
                            "style:column-width",
                        )
                    })
                    .and_then(|r| length(r.value))
                    .unwrap_or(0.0)
            })
            .collect();
        let known: f32 = cols.iter().sum();
        if known <= 0.0 || cols.contains(&0.0) {
            let n = cols.len().max(1) as f32;
            cols.iter_mut().for_each(|c| *c = self.body_width / n);
        }
        let rows = t
            .rows()
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .filter_map(|c| match c {
                        CellChild::Cell(c) if !c.covered => Some(input::Cell {
                            colspan: c.column_span(),
                            paras: {
                                let mut v = Vec::new();
                                self.blocks(&c.blocks, &mut v, &mut ListCtx::default());
                                v.into_iter()
                                    .filter_map(|b| match b {
                                        input::Block::Para(p) => Some(p),
                                        input::Block::Table(_) => None,
                                    })
                                    .collect()
                            },
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        input::Table {
            id: t.meta.id.0.clone(),
            cols,
            rows,
        }
    }

    fn para(&self, p: &Paragraph, label: Option<String>, list_indent: f32) -> input::Para {
        let ch = chain(self.styles, "paragraph", p.style_name());
        let pp = |attr| get(&ch, "style:paragraph-properties", attr);
        let len = |attr| pp(attr).and_then(length).unwrap_or(0.0);
        let base = char_style(
            &ch,
            &CharStyle {
                font: "Liberation Serif".into(),
                size: 12.0,
                bold: false,
                italic: false,
            },
        );
        let align = match pp("fo:text-align") {
            Some("justify") => Align::Justify,
            Some("center") => Align::Center,
            Some("end") | Some("right") => Align::End,
            _ => Align::Start,
        };
        let line_height = match pp("fo:line-height") {
            Some(v) if percent(v).is_some() => LineHeight::Proportional(percent(v).unwrap()),
            Some(v) if length(v).is_some() => LineHeight::Exact(length(v).unwrap()),
            _ => match pp("style:line-height-at-least").and_then(length) {
                Some(v) => LineHeight::AtLeast(v),
                None => LineHeight::Proportional(1.0),
            },
        };
        let style = ParaStyle {
            align,
            space_before: len("fo:margin-top"),
            space_after: len("fo:margin-bottom"),
            indent_left: len("fo:margin-left") + list_indent,
            indent_right: len("fo:margin-right"),
            first_line: len("fo:text-indent"),
            line_height,
            keep_with_next: pp("fo:keep-with-next") == Some("always") || p.heading,
            page_break_before: pp("fo:break-before") == Some("page"),
            widows: pp("fo:widows").and_then(|v| v.parse().ok()).unwrap_or(2),
            orphans: pp("fo:orphans").and_then(|v| v.parse().ok()).unwrap_or(2),
            hyphenate: get(&ch, "style:text-properties", "fo:hyphenate") == Some("true")
                || align == Align::Justify,
            heading: p.outline_level(),
            label,
            font: base.font.clone(),
            size: base.size,
        };
        let mut text = String::new();
        let mut spans: Vec<input::Span> = Vec::new();
        let mut notes = Vec::new();
        self.inlines(&p.content, &base, &mut text, &mut spans, &mut notes);
        input::Para {
            id: p.meta.id.0.clone(),
            text,
            spans,
            style,
            notes,
        }
    }

    fn push(text: &mut String, spans: &mut Vec<input::Span>, s: &str, cs: &CharStyle) {
        if s.is_empty() {
            return;
        }
        let start = text.chars().count();
        text.push_str(s);
        let end = start + s.chars().count();
        if let Some(last) = spans.last_mut()
            && last.font == cs.font
            && last.size == cs.size
            && last.bold == cs.bold
            && last.italic == cs.italic
            && last.range.end == start
        {
            last.range.end = end;
            return;
        }
        spans.push(input::Span {
            range: start..end,
            font: cs.font.clone(),
            size: cs.size,
            bold: cs.bold,
            italic: cs.italic,
        });
    }

    fn inlines(
        &self,
        v: &[Inline],
        cs: &CharStyle,
        text: &mut String,
        spans: &mut Vec<input::Span>,
        notes: &mut Vec<input::Note>,
    ) {
        for i in v {
            match i {
                Inline::Text(t) => {
                    let clean: String = t
                        .text
                        .chars()
                        .map(|c| if c == '\n' { ' ' } else { c })
                        .collect();
                    Self::push(text, spans, &clean, cs);
                }
                Inline::Control(c) => match c.kind {
                    ControlKind::Space => Self::push(text, spans, &" ".repeat(c.count()), cs),
                    ControlKind::Tab => Self::push(text, spans, " ", cs),
                    ControlKind::LineBreak => Self::push(text, spans, "\n", cs),
                    ControlKind::SoftPageBreak => {}
                },
                Inline::Span(s) | Inline::Link(s) => {
                    let ch = chain(self.styles, "text", s.style_name());
                    let inner = char_style(&ch, cs);
                    self.inlines(&s.content, &inner, text, spans, notes);
                }
                Inline::Field(f) => self.inlines(&f.display, cs, text, spans, notes),
                Inline::Opaque(slot) if slot.name.as_deref() == Some("text:note") => {
                    // The citation, as a small figure.
                    let raw = &slot.raw;
                    let cit = raw
                        .find("<text:note-citation")
                        .and_then(|s| raw[s..].find('>').map(|e| s + e + 1))
                        .and_then(|s| raw[s..].find('<').map(|e| &raw[s..s + e]))
                        .unwrap_or("*");
                    // The note's own text: everything in note-body, tags removed.
                    let body = raw.find("<text:note-body").map_or("", |s| &raw[s..]);
                    let mut plain = String::new();
                    let mut in_tag = false;
                    for c in body.chars() {
                        match c {
                            '<' => in_tag = true,
                            '>' => in_tag = false,
                            c if !in_tag => plain.push(c),
                            _ => {}
                        }
                    }
                    notes.push(input::Note {
                        at: text.chars().count(),
                        text: plain
                            .replace("&amp;", "&")
                            .replace("&lt;", "<")
                            .replace("&gt;", ">"),
                    });
                    let small = CharStyle {
                        size: cs.size * 0.58,
                        ..cs.clone()
                    };
                    Self::push(text, spans, cit, &small);
                }
                _ => {}
            }
        }
    }
}
