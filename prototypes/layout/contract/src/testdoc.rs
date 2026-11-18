// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A small synthetic document for tests: Danish paragraphs, a heading, a
//! table, justified and ragged text.

use crate::input::*;

const WORDS: &[&str] = &[
    "kommunen",
    "afgørelse",
    "ansøgning",
    "byggetilladelse",
    "ejendommen",
    "frist",
    "klage",
    "nævnet",
    "gården",
    "færge",
    "ålens",
    "sagsbehandler",
    "tilbygning",
    "Æbleøen",
    "på",
    "og",
    "er",
    "den",
    "vi",
    "har",
];

fn para(id: &str, text: String, align: Align, heading: bool) -> Para {
    let size = if heading { 16.0 } else { 12.0 };
    let n = text.chars().count();
    Para {
        id: id.into(),
        spans: vec![Span {
            range: 0..n,
            font: if heading {
                "Liberation Sans"
            } else {
                "Liberation Serif"
            }
            .into(),
            size,
            bold: heading,
            italic: false,
        }],
        text,
        style: ParaStyle {
            align,
            space_before: if heading { 12.0 } else { 0.0 },
            space_after: 7.0,
            indent_left: 0.0,
            indent_right: 0.0,
            first_line: 0.0,
            line_height: LineHeight::Proportional(1.15),
            keep_with_next: heading,
            page_break_before: false,
            widows: 2,
            orphans: 2,
            hyphenate: align == Align::Justify,
            heading: heading.then_some(2),
            label: None,
            font: "Liberation Serif".into(),
            size,
        },
        notes: Vec::new(),
    }
}

/// `n` paragraphs of pseudo-random Danish words; every fifth a heading,
/// every seventh followed by a two-column table.
pub fn small(n: usize, justify: bool) -> Doc {
    let mut seed: u64 = 7;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    let mut blocks = Vec::new();
    for i in 0..n {
        let heading = i % 5 == 0;
        let words = if heading { 4 } else { 20 + next() % 60 };
        let text: Vec<&str> = (0..words).map(|_| WORDS[next() % WORDS.len()]).collect();
        let align = if justify && !heading {
            Align::Justify
        } else {
            Align::Start
        };
        blocks.push(Block::Para(para(
            &format!("p{i}"),
            text.join(" "),
            align,
            heading,
        )));
        if i % 7 == 3 {
            let cell = |k: usize| Cell {
                colspan: 1,
                paras: vec![para(
                    &format!("t{i}-{k}"),
                    format!("{} {}", WORDS[k], WORDS[k + 1]),
                    Align::Start,
                    false,
                )],
            };
            blocks.push(Block::Table(Table {
                id: format!("t{i}"),
                cols: vec![200.0, 281.0],
                rows: vec![vec![cell(0), cell(2)], vec![cell(4), cell(6)]],
            }));
        }
    }
    Doc {
        page: PageSpec::a4(),
        blocks,
    }
}
