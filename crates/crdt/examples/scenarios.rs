// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Merge scenarios for the Spike 3 blind review.
//!
//! ```text
//! cargo run -p skriv-crdt --example scenarios
//! ```
//!
//! Each scenario starts from the same short municipal document. Two people
//! edit it at the same time without seeing each other's work; then their
//! copies sync. Each of the three CRDT encodings (Loro tree, Loro hybrid,
//! Automerge) produces an outcome; they are shown as "Outcome A/B/C",
//! assigned per scenario by a seeded shuffle. The key is written to a separate file, ROT13-encoded so it isn't
//! read by accident. Every outcome is also written as ODT and checked for
//! damage outside what was edited.

use std::fmt::Write as _;
use std::path::PathBuf;

use skriv_crdt::CrdtDoc;
use skriv_crdt::automerge_backend::AutomergeDocument;
use skriv_crdt::codec::{self, Baseline};
use skriv_crdt::logical::{LDoc, LNode, OBJ, Para};
use skriv_crdt::loro_backend::LoroDocument;
use skriv_crdt::loro_flow_backend::LoroFlowDocument;
use skriv_crdt::workload::{Rng, sections, top_paras};
use skriv_model::*;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------
// The starting document.

fn para(style: &str, heading: bool, text: &str) -> Block {
    let mut attrs = Attrs::default();
    attrs.set("text:style-name", style);
    if heading {
        attrs.set("text:outline-level", "2");
    }
    Block::Paragraph(Paragraph {
        meta: NodeMeta::new(),
        heading,
        attrs,
        content: vec![Inline::Text(Text::new(text))],
    })
}

fn section(name: &str, blocks: Vec<Block>) -> Block {
    let mut attrs = Attrs::default();
    attrs.set("text:name", name);
    Block::Section(Section {
        meta: NodeMeta::new(),
        attrs,
        blocks,
    })
}

fn list(items: &[&str]) -> Block {
    Block::List(List {
        meta: NodeMeta::new(),
        attrs: Attrs::default(),
        children: items
            .iter()
            .map(|t| {
                ListChild::Item(ListItem {
                    meta: NodeMeta::new(),
                    header: false,
                    attrs: Attrs::default(),
                    blocks: vec![para("Text_20_body", false, t)],
                })
            })
            .collect(),
    })
}

/// A short decision letter, built on the generated table document so the
/// table and its styles are real LibreOffice output.
fn base_odt() -> Vec<u8> {
    let src =
        std::fs::read(root().join("fidelity/corpus/generated/tables-widths-merged.odt")).unwrap();
    let mut odt = skriv_odf::read(src).unwrap();
    let table = odt
        .doc
        .body
        .iter()
        .find(|b| matches!(b, Block::Table(_)))
        .cloned()
        .unwrap();
    let decls = odt.doc.body[0].clone();
    let mut table = table;
    if let Block::Table(t) = &mut table {
        let mut d = Document {
            body: vec![Block::Table(t.clone())],
            ..Default::default()
        };
        skriv_model::for_each_node_mut(&mut d, &mut |_, m| {
            m.origin = None;
            m.dirty = true;
        });
        if let Block::Table(t2) = d.body.remove(0) {
            *t = t2;
        }
    }
    odt.doc.body = vec![
        decls,
        para("Heading_20_2", true, "Afgørelse"),
        para(
            "Text_20_body",
            false,
            "Kommunen har modtaget din ansøgning om byggetilladelse.",
        ),
        para(
            "Text_20_body",
            false,
            "Vi har vurderet sagen efter byggeloven og bygningsreglementet.",
        ),
        para(
            "Text_20_body",
            false,
            "Afgørelsen kan påklages inden fire uger.",
        ),
        table,
        section(
            "Baggrund",
            vec![
                para("Text_20_body", false, "Ejendommen ligger i landzone."),
                para(
                    "Text_20_body",
                    false,
                    "Tilbygningen er på 40 m² og opføres i træ.",
                ),
            ],
        ),
        section(
            "Vurdering",
            vec![
                para(
                    "Text_20_body",
                    false,
                    "Projektet overholder bebyggelsesprocenten.",
                ),
                para("Text_20_body", false, "Naboerne er blevet hørt."),
            ],
        ),
        section(
            "Klagevejledning",
            vec![para(
                "Text_20_body",
                false,
                "Du kan klage til Planklagenævnet.",
            )],
        ),
        list(&["Send klagen digitalt.", "Angiv sagsnummeret."]),
        para("Text_20_body", false, "Med venlig hilsen"),
    ];
    odt.write().unwrap().bytes
}

// ---------------------------------------------------------------------------
// Edits, as data, so one scenario runs against both libraries.

#[derive(Clone, Debug)]
enum Op {
    /// Type `text` in top-level paragraph number `p`, after the first
    /// occurrence of `after` (or at the start when empty).
    Type {
        p: usize,
        after: &'static str,
        text: &'static str,
    },
    /// Delete the first occurrence of `what` in paragraph `p`.
    Delete {
        p: usize,
        what: &'static str,
    },
    /// Format the first occurrence of `what`.
    Format {
        p: usize,
        what: &'static str,
        key: &'static str,
        value: &'static str,
    },
    Prop {
        p: usize,
        key: &'static str,
        value: &'static str,
    },
    /// Press Enter after `after`.
    Enter {
        p: usize,
        after: &'static str,
    },
    /// Type into the paragraph the last Enter created.
    TypeNew {
        text: &'static str,
    },
    DeletePara {
        p: usize,
    },
    /// Move section number `s` to after section number `after`.
    MoveSection {
        s: usize,
        after: usize,
    },
    DeleteSection {
        s: usize,
    },
    RenameSection {
        s: usize,
        name: &'static str,
    },
    InsertRow {
        at: usize,
    },
    DeleteRow {
        at: usize,
    },
    InsertCol {
        at: usize,
    },
    DeleteCol {
        at: usize,
    },
    /// Type into cell (row, col) of the table, at `pos`.
    Cell {
        r: usize,
        c: usize,
        pos: usize,
        text: &'static str,
    },
}

struct Scenario {
    title: &'static str,
    one: (&'static str, Vec<Op>),
    two: (&'static str, Vec<Op>),
    /// Person two works on a branch (a suggestion) that is then accepted.
    suggestion: bool,
}

const BOLD: &str = "Δstyle:text-properties|fo:font-weight";
const ITALIC: &str = "Δstyle:text-properties|fo:font-style";
const MARGIN: &str = "Δstyle:paragraph-properties|fo:margin-left";

fn scenarios() -> Vec<Scenario> {
    use Op::*;
    // Top-level paragraphs, in order: 0 heading, 1-3 body, 4-5 in Baggrund,
    // 6-7 in Vurdering, 8 in Klagevejledning, 9-10 list items, 11 closing.
    vec![
        Scenario {
            title: "Both type at the same place",
            one: (
                "adds “skriftlige” before “ansøgning”",
                vec![Type {
                    p: 1,
                    after: "din ",
                    text: "skriftlige ",
                }],
            ),
            two: (
                "adds “nye” before “ansøgning”",
                vec![Type {
                    p: 1,
                    after: "din ",
                    text: "nye ",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A sentence is deleted while a word in it is corrected",
            one: (
                "deletes the sentence about the deadline",
                vec![Delete {
                    p: 3,
                    what: "Afgørelsen kan påklages inden fire uger.",
                }],
            ),
            two: (
                "changes “fire” to “seks”",
                vec![
                    Delete { p: 3, what: "fire" },
                    Type {
                        p: 3,
                        after: "inden ",
                        text: "seks",
                    },
                ],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Overlapping formatting",
            one: (
                "makes “byggeloven og bygningsreglementet” bold",
                vec![Format {
                    p: 2,
                    what: "byggeloven og bygningsreglementet",
                    key: BOLD,
                    value: "bold",
                }],
            ),
            two: (
                "makes “sagen efter byggeloven” italic",
                vec![Format {
                    p: 2,
                    what: "sagen efter byggeloven",
                    key: ITALIC,
                    value: "italic",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Typing right after a word someone else made bold",
            one: (
                "makes “byggetilladelse” bold",
                vec![Format {
                    p: 1,
                    what: "byggetilladelse",
                    key: BOLD,
                    value: "bold",
                }],
            ),
            two: (
                "types “n” right after “byggetilladelse”",
                vec![Type {
                    p: 1,
                    after: "byggetilladelse",
                    text: "n",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Enter pressed while someone types at the end of the paragraph",
            one: (
                "presses Enter after “byggeloven”",
                vec![Enter {
                    p: 2,
                    after: "byggeloven",
                }],
            ),
            two: (
                "adds “ og lokalplanen” before the full stop",
                vec![Type {
                    p: 2,
                    after: "bygningsreglementet",
                    text: " og lokalplanen",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A paragraph is deleted while someone edits it",
            one: (
                "deletes the paragraph “Vi har vurderet …”",
                vec![DeletePara { p: 2 }],
            ),
            two: (
                "adds “grundigt ” before “vurderet”",
                vec![Type {
                    p: 2,
                    after: "har ",
                    text: "grundigt ",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Two different paragraph properties at once",
            one: (
                "indents the paragraph by 1 cm",
                vec![Prop {
                    p: 3,
                    key: MARGIN,
                    value: "1cm",
                }],
            ),
            two: (
                "makes the whole paragraph bold",
                vec![Prop {
                    p: 3,
                    key: "Δstyle:text-properties|fo:font-weight",
                    value: "bold",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "The same paragraph property set to two values",
            one: (
                "indents the paragraph by 1 cm",
                vec![Prop {
                    p: 1,
                    key: MARGIN,
                    value: "1cm",
                }],
            ),
            two: (
                "indents the paragraph by 2 cm",
                vec![Prop {
                    p: 1,
                    key: MARGIN,
                    value: "2cm",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A row is added while a column is deleted",
            one: (
                "adds a row after “Carport” and types “Garage” in it",
                vec![
                    InsertRow { at: 3 },
                    Cell {
                        r: 3,
                        c: 0,
                        pos: 0,
                        text: "Garage",
                    },
                ],
            ),
            two: ("deletes the “Gebyr” column", vec![DeleteCol { at: 1 }]),
            suggestion: false,
        },
        Scenario {
            title: "Both type in the same cell",
            one: (
                "changes “40 dage” to “40 hverdage”",
                vec![Cell {
                    r: 1,
                    c: 2,
                    pos: 3,
                    text: "hver",
                }],
            ),
            two: (
                "adds “ (maks.)” after “40 dage”",
                vec![Cell {
                    r: 1,
                    c: 2,
                    pos: 7,
                    text: " (maks.)",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A row is deleted while someone edits a cell in it",
            one: ("deletes the “Carport” row", vec![DeleteRow { at: 2 }]),
            two: (
                "changes “1.200 kr.” to “1.500 kr.”",
                vec![
                    Cell {
                        r: 2,
                        c: 1,
                        pos: 0,
                        text: "1.500 kr. (før ",
                    },
                    Cell {
                        r: 2,
                        c: 1,
                        pos: 23,
                        text: ")",
                    },
                ],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Both add a row in the same place",
            one: (
                "adds a row after the header: “Garage”",
                vec![
                    InsertRow { at: 1 },
                    Cell {
                        r: 1,
                        c: 0,
                        pos: 0,
                        text: "Garage",
                    },
                ],
            ),
            two: (
                "adds a row after the header: “Udhus”",
                vec![
                    InsertRow { at: 1 },
                    Cell {
                        r: 1,
                        c: 0,
                        pos: 0,
                        text: "Udhus",
                    },
                ],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A column and a row are added at the same time",
            one: (
                "adds a column “Note” at the end",
                vec![
                    InsertCol { at: 3 },
                    Cell {
                        r: 0,
                        c: 3,
                        pos: 0,
                        text: "Note",
                    },
                ],
            ),
            two: (
                "adds a row “Garage” at the end",
                vec![
                    InsertRow { at: 4 },
                    Cell {
                        r: 4,
                        c: 0,
                        pos: 0,
                        text: "Garage",
                    },
                ],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A section is moved while someone types in it",
            one: (
                "moves the section “Baggrund” after “Vurdering”",
                vec![MoveSection { s: 0, after: 1 }],
            ),
            two: (
                "adds “ (matr. nr. 12a)” in “Baggrund”",
                vec![Type {
                    p: 4,
                    after: "Ejendommen",
                    text: " (matr. nr. 12a)",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A section is moved while someone presses Enter in it",
            one: (
                "moves the section “Baggrund” after “Klagevejledning”",
                vec![MoveSection { s: 0, after: 2 }],
            ),
            two: (
                "presses Enter after “40 m²” in “Baggrund”",
                vec![Enter {
                    p: 5,
                    after: "40 m²",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "Both move the same section to different places",
            one: (
                "moves “Klagevejledning” to after “Baggrund”",
                vec![MoveSection { s: 2, after: 0 }],
            ),
            two: (
                "moves “Baggrund” to after “Klagevejledning”",
                vec![MoveSection { s: 0, after: 2 }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A section is deleted while someone writes in it",
            one: (
                "deletes the section “Vurdering”",
                vec![DeleteSection { s: 1 }],
            ),
            two: (
                "adds a sentence to “Naboerne er blevet hørt.”",
                vec![Type {
                    p: 7,
                    after: "hørt.",
                    text: " Der er ikke kommet indsigelser.",
                }],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A section is renamed while it is moved",
            one: (
                "renames “Vurdering” to “Kommunens vurdering”",
                vec![RenameSection {
                    s: 1,
                    name: "Kommunens vurdering",
                }],
            ),
            two: (
                "moves “Vurdering” to the top of the sections",
                vec![
                    MoveSection { s: 1, after: 0 },
                    MoveSection { s: 0, after: 1 },
                ],
            ),
            suggestion: false,
        },
        Scenario {
            title: "A suggestion rewrites a sentence that was edited meanwhile; the suggestion is accepted",
            one: (
                "(on the document) changes “fire uger” to “4 uger”",
                vec![
                    Delete { p: 3, what: "fire" },
                    Type {
                        p: 3,
                        after: "inden ",
                        text: "4",
                    },
                ],
            ),
            two: (
                "(suggestion) changes “påklages” to “indbringes for Planklagenævnet”",
                vec![
                    Delete {
                        p: 3,
                        what: "påklages",
                    },
                    Type {
                        p: 3,
                        after: "kan ",
                        text: "indbringes for Planklagenævnet",
                    },
                ],
            ),
            suggestion: true,
        },
        Scenario {
            title: "A suggestion deletes a paragraph that was edited meanwhile; the suggestion is accepted",
            one: (
                "(on the document) adds “ senest i dag” to the first paragraph",
                vec![Type {
                    p: 1,
                    after: "byggetilladelse",
                    text: " senest i dag",
                }],
            ),
            two: (
                "(suggestion) deletes the first paragraph",
                vec![DeletePara { p: 1 }],
            ),
            suggestion: true,
        },
        Scenario {
            title: "A list item is split while the next item is edited",
            one: (
                "presses Enter after “Send klagen digitalt.” and types “Brug MitID.”",
                vec![
                    Enter {
                        p: 9,
                        after: "digitalt.",
                    },
                    TypeNew {
                        text: "Brug MitID.",
                    },
                ],
            ),
            two: (
                "adds “ (2026-0193)” after “sagsnummeret”",
                vec![Type {
                    p: 10,
                    after: "sagsnummeret",
                    text: " (2026-0193)",
                }],
            ),
            suggestion: false,
        },
    ]
}

// ---------------------------------------------------------------------------

struct Ctx {
    paras: Vec<String>,
    sections: Vec<String>,
    table: String,
    rows: Vec<String>,
    cols: Vec<String>,
    tag: &'static str,
    minted: usize,
    last: Option<String>,
}

fn char_pos(text: &str, needle: &str, after: bool) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let byte = text.find(needle)?;
    let end = if after { byte + needle.len() } else { byte };
    Some(text[..end].chars().count())
}

fn apply<D: CrdtDoc>(d: &mut D, cx: &mut Ctx, op: &Op) {
    let text = |d: &D, p: usize| d.para(&cx.paras[p]).map(|x| x.text()).unwrap_or_default();
    match op {
        Op::Type { p, after, text: t } => {
            let pos = char_pos(&text(d, *p), after, true).expect("anchor");
            assert!(d.insert_text(&cx.paras[*p], pos, t));
        }
        Op::Delete { p, what } => {
            let pos = char_pos(&text(d, *p), what, false).expect("anchor");
            assert!(d.delete_text(&cx.paras[*p], pos, what.chars().count()));
        }
        Op::Format {
            p,
            what,
            key,
            value,
        } => {
            let pos = char_pos(&text(d, *p), what, false).expect("anchor");
            assert!(d.mark(
                &cx.paras[*p],
                pos,
                pos + what.chars().count(),
                key,
                Some(value)
            ));
        }
        Op::Prop { p, key, value } => assert!(d.set_para_prop(&cx.paras[*p], key, Some(value))),
        Op::Enter { p, after } => {
            let pos = char_pos(&text(d, *p), after, true).expect("anchor");
            cx.minted += 1;
            let id = format!("{}-new{}", cx.tag, cx.minted);
            assert!(d.split_para(&cx.paras[*p], pos, &id));
            cx.last = Some(id);
        }
        Op::TypeNew { text: t } => {
            let id = cx.last.clone().expect("an Enter first");
            assert!(d.insert_text(&id, 0, t));
        }
        Op::DeletePara { p } => assert!(d.delete_block(&cx.paras[*p])),
        Op::MoveSection { s, after } => {
            assert!(d.move_block_after(&cx.sections[*s], &cx.sections[*after]))
        }
        Op::DeleteSection { s } => assert!(d.delete_block(&cx.sections[*s])),
        Op::RenameSection { s, name } => {
            assert!(d.set_container_prop(&cx.sections[*s], "text:name", Some(name)))
        }
        Op::InsertRow { at } => {
            cx.minted += 1;
            let id = format!("{}-row{}", cx.tag, cx.minted);
            assert!(d.insert_row(&cx.table, *at, &id));
            cx.rows.insert(*at, id);
        }
        Op::DeleteRow { at } => {
            assert!(d.delete_row(&cx.table, &cx.rows[*at]));
            cx.rows.remove(*at);
        }
        Op::InsertCol { at } => {
            cx.minted += 1;
            let id = format!("{}-col{}", cx.tag, cx.minted);
            assert!(d.insert_col(&cx.table, *at, &id, "3cm"));
            cx.cols.insert(*at, id);
        }
        Op::DeleteCol { at } => {
            assert!(d.delete_col(&cx.table, &cx.cols[*at]));
            cx.cols.remove(*at);
        }
        Op::Cell { r, c, pos, text: t } => {
            assert!(d.cell_insert_text(&cx.table, &cx.rows[*r], &cx.cols[*c], *pos, t));
        }
    }
}

/// Paragraph text with formatting shown as `[b]…[/b]` and `[i]…[/i]`.
fn render_para(p: &Para) -> String {
    let mut s = String::new();
    let (mut b, mut i) = (false, false);
    for r in &p.runs {
        let t = r.text.replace(OBJ, "");
        if t.is_empty() {
            continue;
        }
        let nb = r.marks.get(BOLD).is_some_and(|v| v == "bold");
        let ni = r.marks.get(ITALIC).is_some_and(|v| v == "italic");
        if i && !ni {
            s.push_str("[/i]");
        }
        if b && !nb {
            s.push_str("[/b]");
        }
        if nb && !b {
            s.push_str("[b]");
        }
        if ni && !i {
            s.push_str("[i]");
        }
        s.push_str(&t);
        (b, i) = (nb, ni);
    }
    if i {
        s.push_str("[/i]");
    }
    if b {
        s.push_str("[/b]");
    }
    let mut prefix = String::new();
    if p.props.contains_key("@h") {
        prefix.push_str("## ");
    }
    if let Some(m) = p.props.get(MARGIN) {
        prefix.push_str(&format!("[indented {m}] "));
    }
    if p.props.get(BOLD).is_some_and(|v| v == "bold") {
        prefix.push_str("[whole paragraph bold] ");
    }
    format!("{prefix}{s}")
}

/// Mark lines that differ from the starting document with ▶, and list
/// lines that are gone (a plain longest-common-subsequence line diff).
fn mark_changes(start: &str, out: &str) -> String {
    let a: Vec<&str> = start.lines().collect();
    let b: Vec<&str> = out.lines().collect();
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for x in (0..a.len()).rev() {
        for y in (0..b.len()).rev() {
            lcs[x][y] = if a[x] == b[y] {
                lcs[x + 1][y + 1] + 1
            } else {
                lcs[x + 1][y].max(lcs[x][y + 1])
            };
        }
    }
    let (mut x, mut y) = (0, 0);
    let mut res = String::new();
    while x < a.len() || y < b.len() {
        if x < a.len() && y < b.len() && a[x] == b[y] {
            res.push_str(&format!("  {}\n", b[y]));
            x += 1;
            y += 1;
        } else if y < b.len() && (x == a.len() || lcs[x][y + 1] >= lcs[x + 1][y]) {
            res.push_str(&format!("▶ {}\n", b[y]));
            y += 1;
        } else {
            res.push_str(&format!("✕ (removed) {}\n", a[x].trim_start()));
            x += 1;
        }
    }
    res
}

fn render(l: &LDoc) -> String {
    fn nodes(v: &[LNode], depth: usize, out: &mut String) {
        let pad = "    ".repeat(depth);
        for n in v {
            match n {
                LNode::Para(p) => {
                    let _ = writeln!(out, "{pad}{}", render_para(p));
                }
                LNode::Embed { .. } => {}
                LNode::Container {
                    kind,
                    props,
                    children,
                    ..
                } => {
                    match kind.as_str() {
                        "section" => {
                            let _ = writeln!(
                                out,
                                "{pad}┌ Section “{}”",
                                props.get("text:name").cloned().unwrap_or_default()
                            );
                        }
                        "list" => {}
                        _ => {
                            let _ = write!(out, "{pad}•");
                        }
                    }
                    let mut inner = String::new();
                    nodes(
                        children,
                        if kind == "item" {
                            0
                        } else {
                            depth + usize::from(kind == "section")
                        },
                        &mut inner,
                    );
                    if kind == "item" {
                        out.push(' ');
                        out.push_str(inner.trim_start());
                    } else {
                        out.push_str(&inner);
                    }
                    if kind == "section" {
                        let _ = writeln!(out, "{pad}└");
                    }
                }
                LNode::Table(t) => {
                    let rows: Vec<Vec<String>> = t
                        .cells
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|c| {
                                    if c.props.contains_key("@covered") {
                                        "(merged with the cell to the left)".to_owned()
                                    } else {
                                        c.text().replace(['\n'], " / ").replace(OBJ, "")
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    let w = t.cols.len();
                    let widths: Vec<usize> = (0..w)
                        .map(|c| {
                            rows.iter()
                                .map(|r| r[c].chars().count())
                                .max()
                                .unwrap_or(0)
                                .max(3)
                        })
                        .collect();
                    for (i, r) in rows.iter().enumerate() {
                        let cells: Vec<String> = r
                            .iter()
                            .enumerate()
                            .map(|(c, x)| format!("{x:<width$}", width = widths[c]))
                            .collect();
                        let _ = writeln!(out, "{pad}| {} |", cells.join(" | "));
                        if i == 0 {
                            let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
                            let _ = writeln!(out, "{pad}|-{}-|", sep.join("-|-"));
                        }
                    }
                }
            }
        }
    }
    let mut out = String::new();
    nodes(&l.body, 0, &mut out);
    out
}

fn run<D: CrdtDoc>(sc: &Scenario, l: &LDoc, base: &Baseline, src: &[u8]) -> (String, String) {
    let mut origin = D::create(l, 1);
    let start = origin.read();
    let t = start.tables()[0];
    let mk = |tag| Ctx {
        paras: top_paras(&start),
        sections: sections(&start).into_iter().map(|s| s.0).collect(),
        table: t.id.clone(),
        rows: t.rows.iter().map(|r| r.0.clone()).collect(),
        cols: t.cols.iter().map(|c| c.0.clone()).collect(),
        tag,
        minted: 0,
        last: None,
    };
    let mut one = origin.fork(2);
    let mut two = origin.fork(3);
    one.set_author("Person 1");
    two.set_author("Person 2");
    let mut c1 = mk("p1");
    let mut c2 = mk("p2");
    for op in &sc.one.1 {
        apply(&mut one, &mut c1, op);
    }
    for op in &sc.two.1 {
        apply(&mut two, &mut c2, op);
    }
    one.commit();
    two.commit();
    // For a suggestion, person two's branch is merged into the document.
    one.merge(&mut two);
    if !sc.suggestion {
        two.merge(&mut one);
        let (mut a, mut b) = (one.read(), two.read());
        a.normalize();
        b.normalize();
        assert_eq!(a, b, "{}: {} replicas diverged", sc.title, D::NAME);
    }
    let merged = one.read();
    // Write the outcome as ODT and check nothing else changed.
    let mut odt = skriv_odf::read(src.to_vec()).unwrap();
    odt.doc = codec::export(&merged, base).doc;
    let w = odt.write().expect("write");
    skriv_odf::read(w.bytes.clone()).expect("outcome reads back");
    let (s, m) = (render(&start), render(&merged));
    let marked = mark_changes(&s, &m);
    (s, marked)
}

fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

fn main() {
    let src = base_odt();
    let odt = skriv_odf::read(src.clone()).unwrap();
    let (l, base) = codec::import(&odt.doc);
    let dir = root().join("docs/spikes/03-blind-review");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("starting-document.odt"), &src).unwrap();

    let mut md = String::new();
    let mut key = String::new();
    let mut identical = 0;
    let scs = scenarios();
    md.push_str("# Spike 3 blind review: merge outcomes\n\n");
    md.push_str(
        "Two people edited the same document at the same time without seeing each \
         other's work. Then their copies were combined automatically. For each \
         scenario there are three outcomes, **A**, **B** and **C**, produced by \
         three different systems. Which system is which changes from scenario to \
         scenario, and two outcomes may be identical.\n\n\
         For each outcome, please mark **acceptable** (what you would expect, or \
         something you can live with without fixing it by hand) or **not \
         acceptable**, and add a note if you like. Lines that differ from the \
         starting document are marked ▶; lines that disappeared are listed as \
         ✕ (removed). Bold and italic text are shown as `[b]…[/b]` and \
         `[i]…[/i]`; sections are drawn with ┌ and └.\n\n\
         Generated by `cargo run -p skriv-crdt --example scenarios`. Do not open \
         `key.txt` until all ratings are in.\n\n",
    );
    md.push_str("## The starting document\n\n```text\n");
    let mut start_shown = false;
    let mut body = String::new();
    for (i, sc) in scs.iter().enumerate() {
        let (start, loro) = run::<LoroDocument>(sc, &l, &base, &src);
        let (_, flow) = run::<LoroFlowDocument>(sc, &l, &base, &src);
        let (_, am) = run::<AutomergeDocument>(sc, &l, &base, &src);
        if !start_shown {
            md.push_str(&start);
            md.push_str("```\n\n");
            start_shown = true;
        }
        let mut outcomes = vec![
            ("loro (tree)", loro),
            ("loro (hybrid)", flow),
            ("automerge", am),
        ];
        // A seeded shuffle decides which system is A, B and C here.
        let mut rng = Rng::new(0x5EED_0003 + i as u64);
        for k in (1..outcomes.len()).rev() {
            outcomes.swap(k, rng.below(k + 1));
        }
        if outcomes[0].1 == outcomes[1].1 && outcomes[1].1 == outcomes[2].1 {
            identical += 1;
        }
        let n = i + 1;
        let _ = writeln!(
            key,
            "{n}: A={} B={} C={}",
            outcomes[0].0, outcomes[1].0, outcomes[2].0
        );
        let _ = write!(
            body,
            "## {n}. {}\n\n- **Person 1** {}.\n- **{}** {}.\n\n",
            sc.title,
            sc.one.0,
            if sc.suggestion {
                "Person 2, as a suggestion that is then accepted,"
            } else {
                "Person 2"
            },
            sc.two.0
        );
        for (label, (_, text)) in ["A", "B", "C"].iter().zip(&outcomes) {
            let _ = write!(
                body,
                "**Outcome {label}**\n\n```text\n{text}```\n\n- [ ] acceptable  - [ ] not acceptable  Note:\n\n"
            );
        }
    }
    md.push_str(&body);
    std::fs::write(dir.join("scenarios.md"), md).unwrap();
    std::fs::write(
        dir.join("key.txt"),
        format!(
            "ROT13. Decode only after all ratings are in (e.g. `tr 'A-Za-z' 'N-ZA-Mn-za-m' < key.txt`).\n\n{}",
            rot13(&key)
        ),
    )
    .unwrap();
    println!(
        "{} scenarios written to {}; {identical} have the same outcome from all three systems",
        scs.len(),
        dir.display()
    );
}
