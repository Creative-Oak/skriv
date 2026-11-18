// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Edits on the 100-page test letter: after every edit, the incremental
//! layout equals a fresh one, and only the pages that changed are dirty.
//! Needs `layout-bench export` (see prototypes/layout/README.md).

use std::time::Instant;

use view_engine::Editor;

fn letter() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../target/spike2b/letter-100.json"
    );
    std::fs::read_to_string(path).expect("run `layout-bench export` first")
}

fn fresh_hash(ed: &Editor) -> String {
    let json = serde_json::to_string(ed.doc()).unwrap();
    Editor::new(&json).unwrap().hash()
}

fn outcome(s: String) -> serde_json::Value {
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    v
}

/// A long body paragraph on `page`.
fn target(ed: &Editor, page: usize) -> usize {
    let lines: serde_json::Value = serde_json::from_str(&ed.page(page)).unwrap();
    lines
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["p"].as_u64().unwrap() as usize)
        .find(|&p| ed.para_text(p).chars().count() > 200)
        .unwrap()
}

#[test]
fn typing_enter_and_backspace_match_a_fresh_layout() {
    let mut ed = Editor::new(&letter()).unwrap();
    let pages = ed.page_count();
    assert!(pages >= 100, "{pages}");
    let p = target(&ed, 50);
    let mut counts = std::collections::BTreeMap::new();
    let t = Instant::now();
    for i in 0..200 {
        let v = outcome(ed.insert(p, 100 + i, if i % 7 == 6 { " " } else { "æ" }));
        *counts
            .entry(v["dirty"].as_array().unwrap().len())
            .or_insert(0) += 1;
    }
    // A keystroke that adds a line pushes the pages below it, up to the
    // next forced page break; most keystrokes repaint one page.
    eprintln!(
        "200 keystrokes: {:?}; dirty pages -> keystrokes: {counts:?}",
        t.elapsed()
    );
    assert!(counts.get(&1).copied().unwrap_or(0) > 150, "{counts:?}");
    assert!(*counts.keys().last().unwrap() <= 12, "{counts:?}");
    assert_eq!(ed.hash(), fresh_hash(&ed));

    let before = ed.para_count();
    let v = outcome(ed.split(p, 50, None));
    assert_eq!(v["structural"], true);
    assert_eq!(ed.para_count(), before + 1);
    assert_eq!(v["caret"], serde_json::json!([p + 1, 0]));
    assert_eq!(ed.hash(), fresh_hash(&ed));
    // Pages before the edit are clean.
    let dirty: Vec<u64> = v["dirty"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_u64().unwrap())
        .collect();
    assert!(dirty.iter().all(|&d| d >= 49), "{dirty:?}");

    // Backspace at the start of the new paragraph joins it back.
    let len = ed.para_text(p).chars().count();
    outcome(ed.delete(p, len, p + 1, 0));
    assert_eq!(ed.para_count(), before);
    assert_eq!(ed.hash(), fresh_hash(&ed));

    // A selection from page 3 to page 4, deleted.
    let a = target(&ed, 3);
    let b = target(&ed, 4);
    if a != b {
        outcome(ed.delete(a, 10, b, 10));
        assert_eq!(ed.hash(), fresh_hash(&ed));
    }
}

#[test]
fn carets_move_down_and_up_across_a_page_break() {
    let ed = Editor::new(&letter()).unwrap();
    // The last line on page 3, then one line down: page 4.
    let lines: serde_json::Value = serde_json::from_str(&ed.page(3)).unwrap();
    let last = lines
        .as_array()
        .unwrap()
        .iter()
        .max_by(|a, b| {
            a["y"]
                .as_f64()
                .unwrap()
                .total_cmp(&b["y"].as_f64().unwrap())
        })
        .unwrap();
    let (p, s) = (
        last["p"].as_u64().unwrap() as usize,
        last["s"].as_u64().unwrap() as usize,
    );
    let c = ed.locate(p, s + 1);
    assert_eq!(c[0], 3.0);
    let down = ed.vertical(p, s + 1, c[2], 1);
    let c2 = ed.locate(down[0] as usize, down[1] as usize);
    assert_eq!(c2[0], 4.0, "{down:?}");
    let up = ed.vertical(down[0] as usize, down[1] as usize, c[2], -1);
    assert_eq!(ed.locate(up[0] as usize, up[1] as usize)[0], 3.0);
}
