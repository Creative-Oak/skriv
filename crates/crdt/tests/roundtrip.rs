// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Every generated-corpus document goes model → CRDT → saved bytes → loaded
//! → model → ODT unchanged.

mod common;

use common::*;
use skriv_crdt::CrdtDoc;

fn roundtrip<D: CrdtDoc>() {
    for name in corpus_names() {
        let bytes = corpus(&name);
        let (l, base) = import(&bytes);
        let mut d = D::create(&l, 1);
        let saved = d.save();
        let d2 = D::load(&saved, 2);
        let mut back = d2.read();
        back.normalize();
        assert_eq!(back.outline(), l.outline(), "{}: {name} outline", D::NAME);
        assert_eq!(back, l, "{}: {name} logical document", D::NAME);
        let c = write_and_check(&bytes, &back, &base);
        c.assert_undamaged(&format!("{} {name}", D::NAME));
        assert_eq!(c.touched, 0, "{name}: nothing was edited");
        assert!(
            c.result.identical_parts.values().all(|v| *v),
            "{name}: an unedited document is byte-identical"
        );
    }
}

each_backend!(roundtrip);

/// With no baseline, every node is regenerated from the CRDT. The output
/// must still mean the same: every body element survives canonically
/// (automatic styles may be renamed, direct formatting re-derived).
fn regenerate_everything<D: CrdtDoc>() {
    for name in corpus_names() {
        let bytes = corpus(&name);
        let (l, _) = import(&bytes);
        let mut d = D::create(&l, 1);
        let d2 = D::load(&d.save(), 2);
        let back = d2.read();
        let mut odt = skriv_odf::read(bytes.clone()).unwrap();
        let orig = odt
            .source(skriv_model::PartKind::Content)
            .unwrap()
            .to_owned();
        odt.doc = skriv_crdt::codec::export(&back, &Default::default()).doc;
        let w = odt.write().unwrap();
        let (ok, total, lost) = canonical_survival(&orig, &w.content_xml);
        assert_eq!(ok, total, "{} {name}: lost {lost:#?}", D::NAME);
    }
}

each_backend!(regenerate_everything);
