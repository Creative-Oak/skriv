// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Section moves concurrent with edits inside the moved section. Peer A
//! moves a section after another one; peer B, not knowing, types into the
//! section's paragraphs and splits one. After both sync: where did B's
//! typing go?

mod common;

use common::*;
use skriv_crdt::CrdtDoc;
use skriv_crdt::logical::{LDoc, LNode};
use skriv_crdt::workload::{Rng, sections, strip_tokens};

#[derive(Default, Debug)]
struct Outcome {
    runs: usize,
    tokens: usize,
    in_moved_section: usize,
    elsewhere: usize,
    lost: usize,
    duplicated: usize,
    moved_to_target: usize,
    section_split: usize,
    split_survived: usize,
}

fn section_text(l: &LDoc, id: &str) -> Vec<String> {
    fn walk(v: &[LNode], id: &str, inside: bool, out: &mut Vec<String>) {
        for n in v {
            match n {
                LNode::Para(p) if inside => out.push(p.text()),
                LNode::Container {
                    id: cid, children, ..
                } => walk(children, id, inside || cid == id, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&l.body, id, false, &mut out);
    out
}

fn section_moves<D: CrdtDoc>() {
    let bytes = scaled_letter(10);
    let (l, base) = import(&bytes);
    let secs: Vec<String> = sections(&l).into_iter().map(|s| s.0).collect();
    let mut origin = D::create(&l, 1);
    let mut o = Outcome::default();
    for seed in 0..100u64 {
        let mut rng = Rng::new(seed);
        let s = secs[rng.below(secs.len())].clone();
        let t = loop {
            let t = &secs[rng.below(secs.len())];
            if *t != s {
                break t.clone();
            }
        };
        let mut a = origin.fork(2);
        let mut b = origin.fork(3);
        assert!(a.move_block_after(&s, &t), "move");
        let paras = paras_in(&l, &s);
        // B presses Enter in the middle of a paragraph, then types.
        let sp = paras
            .iter()
            .find(|p| b.para_len(p).unwrap() > 20)
            .unwrap()
            .clone();
        let new_id = format!("split-{seed}");
        assert!(b.split_para(&sp, 10, &new_id));
        let mut tokens = Vec::new();
        for k in 0..rng.range(1, 6) {
            let p = &paras[rng.below(paras.len())];
            let len = b.para_len(p).unwrap();
            let tok = format!("⟨s{seed}k{k}⟩");
            assert!(b.insert_text(p, rng.below(len + 1), &tok));
            tokens.push(tok);
        }
        a.commit();
        b.commit();
        a.merge(&mut b);
        b.merge(&mut a);
        let mut la = a.read();
        let mut lb = b.read();
        la.normalize();
        lb.normalize();
        assert_eq!(la, lb, "{} seed {seed}: replicas diverged", D::NAME);

        o.runs += 1;
        let inside = section_text(&la, &s).join("\n");
        let all: String = la
            .paras()
            .iter()
            .map(|p| p.text())
            .collect::<Vec<_>>()
            .join("\n");
        for tok in &tokens {
            o.tokens += 1;
            let n_all = strip_tokens(&all).0.iter().filter(|x| *x == tok).count();
            let n_in = strip_tokens(&inside).0.iter().filter(|x| *x == tok).count();
            match (n_all, n_in) {
                (0, _) => o.lost += 1,
                (1, 1) => o.in_moved_section += 1,
                (1, 0) => o.elsewhere += 1,
                _ => o.duplicated += 1,
            }
        }
        // Where is the section now, and is it still one piece?
        let order: Vec<String> = sections(&la).into_iter().map(|x| x.0).collect();
        let pieces = order.iter().filter(|x| **x == s).count();
        if pieces > 1 {
            o.section_split += 1;
        }
        if let (Some(i), Some(j)) = (
            order.iter().position(|x| *x == t),
            order.iter().position(|x| *x == s),
        ) && j == i + 1
        {
            o.moved_to_target += 1;
        }
        if la.find(&new_id).is_some() && section_text(&la, &s).len() == paras.len() + 1 {
            o.split_survived += 1;
        }
        // The written ODT: nothing outside the moved and edited section changed.
        if seed % 10 == 0 {
            let c = write_and_check(&bytes, &la, &base);
            c.assert_undamaged(&format!("{} seed {seed}", D::NAME));
            if seed == 0 {
                keep(&format!("{}-section-move-seed0.odt", D::NAME), &c.bytes);
            }
        }
        let _ = &mut origin;
    }
    println!("{}: {o:?}", D::NAME);
    // Both libraries: nothing typed is lost or duplicated.
    assert_eq!(o.lost + o.duplicated, 0, "{}: {o:?}", D::NAME);
    match D::NAME {
        // A tree move: the edits travel with the section.
        "loro" | "loro-flow" => {
            assert_eq!(o.moved_to_target, o.runs, "{o:?}");
            assert_eq!(o.section_split, 0, "{o:?}");
            assert_eq!(o.in_moved_section, o.tokens, "{o:?}");
            assert_eq!(o.split_survived, o.runs, "{o:?}");
        }
        // Copy + delete: typing and Enter racing the move stay behind at
        // the old place, splitting the section in two (measured, see
        // docs/spikes/03-results.md).
        _ => assert!(o.elsewhere + o.section_split > 0, "{o:?}"),
    }
}

each_backend!(section_moves);
