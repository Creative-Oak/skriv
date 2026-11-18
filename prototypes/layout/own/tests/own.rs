// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use layout_contract::testdoc::small;
use layout_contract::{EditHint, Engine, layout_hash, locate};
use layout_own::{Breaker, OwnEngine};

#[test]
fn lines_stay_within_the_measure() {
    for (justify, breaker, shrink) in [
        (false, Breaker::Greedy, false),
        (true, Breaker::Optimal, false),
        (true, Breaker::Optimal, true),
    ] {
        let doc = small(40, justify);
        let mut e = OwnEngine::new();
        e.breaker = breaker;
        e.shrink = shrink;
        let width = doc.page.body_width();
        let mut lines = 0;
        for p in doc.paras() {
            let b = e.para_box(p, width);
            for l in &b.lines {
                lines += 1;
                // Shrinking may only make up a third of each space.
                let slack = if shrink { l.spaces as f32 * 1.0 } else { 0.0 } + 0.01;
                assert!(
                    l.natural <= l.avail + slack,
                    "{:?}: {} > {}",
                    breaker,
                    l.natural,
                    l.avail
                );
            }
        }
        assert!(lines > 100);
    }
}

#[test]
fn incremental_layout_equals_a_fresh_one() {
    for justify in [false, true] {
        let mut doc = small(60, justify);
        let mut e = OwnEngine::new();
        e.layout(&doc, &EditHint::Full);
        let targets = [3usize, 17, 40];
        for (k, &para) in targets.iter().enumerate() {
            for i in 0..80 {
                let off = doc.paras()[para].char_len() / 2 + i;
                doc.para_mut(para)
                    .unwrap()
                    .insert(off, if i % 6 == 5 { ' ' } else { 'æ' });
                let pages = e.layout(&doc, &EditHint::Para(para));
                assert!(
                    locate(pages, para, off + 1).is_some(),
                    "caret, edit {k}/{i}"
                );
            }
            let fresh = OwnEngine::new().layout(&doc, &EditHint::Full).clone();
            assert_eq!(
                layout_hash(e.pages()),
                layout_hash(&fresh),
                "justify {justify}, after editing paragraph {para}"
            );
        }
    }
}

#[test]
fn knuth_plass_hyphenates_danish() {
    let doc = small(40, true);
    let pages = OwnEngine::new().layout(&doc, &EditHint::Full).clone();
    assert!(pages.pages.iter().flat_map(|p| &p.lines).any(|l| l.hyphen));
}
