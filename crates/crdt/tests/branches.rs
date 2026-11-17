// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tracked changes as personal branches (the Patchwork model), on the
//! 100-page document. A reviewer forks the document and makes 200 edits
//! on their branch: those are the suggestions, shown as a diff against
//! the fork point. Meanwhile 200 edits land on main. Accepting merges the
//! branch; every suggested character must still be attributed to the
//! reviewer afterwards ("history survives accepting changes").

mod common;

use std::collections::BTreeMap;
use std::time::Instant;

use common::*;
use skriv_crdt::CrdtDoc;
use skriv_crdt::workload::{Editor, Rng, strip_tokens, texts};

/// A suggestion as a reviewer sees it: paragraphs whose text differs from
/// the version the branch was forked from.
fn diff(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for (id, a) in after {
        let b = before.get(id).cloned().unwrap_or_default();
        if *a != b {
            out.push((id.clone(), b, a.clone()));
        }
    }
    for (id, b) in before {
        if !after.contains_key(id) {
            out.push((id.clone(), b.clone(), String::new()));
        }
    }
    out
}

fn branch_and_merge<D: CrdtDoc>() {
    let bytes = scaled_letter(100);
    let (l, base) = import(&bytes);
    let t = Instant::now();
    let mut main = D::create(&l, 1);
    main.set_author("Sagsbehandler");
    main.commit();
    let create_ms = t.elapsed().as_millis();
    let fork_point = texts(&main.read());

    // The reviewer's branch.
    let mut branch = main.fork(2);
    branch.set_author("Revisor");
    let mut rev = Editor::new(7, &l, "rev");
    let mut rng = Rng::new(99);
    let mut rev_tokens = Vec::new();
    for i in 0..200 {
        if i % 10 == 0 {
            // One in ten suggestions carries a marker we can find again.
            let p = rev.paras[rng.below(rev.paras.len())].clone();
            let len = branch.para_len(&p).unwrap();
            let tok = format!("⟨rev{i}⟩");
            branch.insert_text(&p, rng.below(len + 1), &tok);
            rev.touched.insert(p.clone());
            rev_tokens.push((tok, p));
        } else {
            rev.step(&mut branch);
        }
    }
    branch.commit();

    // Concurrent work on main.
    let mut ed = Editor::new(8, &l, "main");
    let mut main_tokens = Vec::new();
    for i in 0..200 {
        if i % 10 == 0 {
            let p = ed.paras[rng.below(ed.paras.len())].clone();
            let len = main.para_len(&p).unwrap();
            let tok = format!("⟨main{i}⟩");
            main.insert_text(&p, rng.below(len + 1), &tok);
            ed.touched.insert(p.clone());
            main_tokens.push((tok, p));
        } else {
            ed.step(&mut main);
        }
    }
    main.commit();

    // Review: the suggestion is the branch's diff against the fork point.
    let suggestion = diff(&fork_point, &texts(&branch.read()));
    assert!(!suggestion.is_empty());
    assert!(
        suggestion.len() <= rev.touched.len() + 1,
        "only what the reviewer touched differs"
    );

    // Accept: merge the branch into main.
    let t = Instant::now();
    main.merge(&mut branch);
    let merge_ms = t.elapsed().as_millis();
    let merged = main.read();

    // Every token, from both sides, exactly once; reviewer tokens still
    // attributed to the reviewer, main's to the case worker.
    let all = merged
        .paras()
        .iter()
        .map(|p| p.text())
        .collect::<Vec<_>>()
        .join("\n");
    let (found, _) = strip_tokens(&all);
    for (tok, _) in rev_tokens.iter().chain(&main_tokens) {
        assert_eq!(
            found.iter().filter(|x| *x == tok).count(),
            1,
            "{}: {tok}",
            D::NAME
        );
    }
    let mut attributed = 0;
    for (tok, who) in rev_tokens
        .iter()
        .map(|t| (t, "Revisor"))
        .chain(main_tokens.iter().map(|t| (t, "Sagsbehandler")))
    {
        let (tok, p) = tok;
        let Some(para) = main.para(p).or_else(|| {
            // The paragraph may have been split; search them all.
            merged
                .paras()
                .into_iter()
                .find(|x| x.text().contains(tok.as_str()))
                .cloned()
        }) else {
            continue;
        };
        let text = para.text();
        if let Some(byte) = text.find(tok.as_str()) {
            let pos = text[..byte].chars().count() + 1;
            assert_eq!(
                main.author_at(&para.id, pos).as_deref(),
                Some(who),
                "{}: {tok}",
                D::NAME
            );
            attributed += 1;
        }
    }
    assert_eq!(attributed, rev_tokens.len() + main_tokens.len());

    // The merged document, as ODT: nothing either side didn't touch changed.
    let c = write_and_check(&bytes, &merged, &base);
    c.assert_undamaged(D::NAME);
    keep(&format!("{}-branch-merge-100pages.odt", D::NAME), &c.bytes);
    let saved = main.save().len();
    println!(
        "{}: 100 pages, create {create_ms} ms, merge {merge_ms} ms, suggestion touches {} paragraphs, saved {} bytes (ODT {} bytes), {} nodes regenerated, {} reused",
        D::NAME,
        suggestion.len(),
        saved,
        bytes.len(),
        c.regenerated,
        c.reused
    );
}

each_backend!(branch_and_merge);
