// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Exercises what an editor needs from the CRDT (create, edit, mark, read,
//! save, load, merge) so the linker keeps it, and nothing else.

use skriv_crdt::CrdtDoc;
use skriv_crdt::logical::{LDoc, LNode, Para, Run};

#[cfg(feature = "automerge")]
type D = skriv_crdt::automerge_backend::AutomergeDocument;
#[cfg(all(feature = "loro", not(feature = "automerge")))]
type D = skriv_crdt::loro_backend::LoroDocument;

/// # Safety
/// None needed: takes and returns plain integers.
#[unsafe(no_mangle)]
pub extern "C" fn skriv_probe(n: u32) -> u32 {
    let body = (0..n)
        .map(|i| {
            LNode::Para(Para {
                id: format!("p{i}"),
                props: Default::default(),
                runs: vec![Run {
                    text: "Kommunen har modtaget din ansøgning.".into(),
                    marks: Default::default(),
                }],
            })
        })
        .collect();
    let l = LDoc {
        body,
        ..Default::default()
    };
    let mut d = D::create(&l, 1);
    let mut e = d.fork(2);
    e.insert_text("p0", 3, "æøå");
    e.mark("p0", 0, 5, "Δstyle:text-properties|fo:font-weight", Some("bold"));
    e.split_para("p0", 4, "p-new");
    d.merge(&mut e);
    let bytes = d.save();
    let d2 = D::load(&bytes, 3);
    d2.read().paras().len() as u32 + bytes.len() as u32
}
