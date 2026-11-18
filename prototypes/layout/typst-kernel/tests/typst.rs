// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use layout_contract::testdoc::small;
use layout_contract::{EditHint, Engine, hit, layout_hash, locate};
use layout_typst::TypstEngine;

#[test]
fn compiles_maps_carets_and_edits_incrementally() {
    let mut doc = small(25, true);
    let mut e = TypstEngine::new();
    let pages = e.layout(&doc, &EditHint::Full).clone();
    assert!(pages.pages.len() >= 2);
    // Caret round trip on every character of the first page.
    let (mut ok, mut total) = (0, 0);
    for l in &pages.pages[0].lines {
        for k in l.range.clone() {
            total += 1;
            let c = locate(&pages, l.para, k).unwrap();
            let line = &pages.pages[c.page].lines[c.line];
            if hit(&pages, c.page, c.x, line.y + line.height / 2.0) == Some((l.para, k)) {
                ok += 1;
            }
        }
    }
    assert!(ok * 100 >= total * 98, "{ok}/{total}");
    // Edits through Source::edit give the same layout as a fresh compile.
    for i in 0..20 {
        let off = doc.paras()[6].char_len() / 2;
        doc.para_mut(6)
            .unwrap()
            .insert(off, if i % 5 == 4 { ' ' } else { 'ø' });
        e.layout(&doc, &EditHint::Para(6));
    }
    let fresh = TypstEngine::new().layout(&doc, &EditHint::Full).clone();
    assert_eq!(layout_hash(e.pages()), layout_hash(&fresh));
}
