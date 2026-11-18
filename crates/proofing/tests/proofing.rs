// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Detection tests run without downloads (the lingua and whatlang models
//! are compiled in). Spelling tests need `crates/proofing/fetch-dicts.sh`
//! and skip, saying so, when the dictionaries are absent.

use skriv_proofing::assign::{self, ParaInfo, Policy};
use skriv_proofing::corpus;
use skriv_proofing::detect::{Detector, Lingua, Whatlang};
use skriv_proofing::spell::Speller;
use skriv_proofing::{Lang, text};

const DA: &str = "Kommunen har modtaget din ansøgning om byggetilladelse til en tilbygning på ejendommen, og vi træffer afgørelse inden for fristen.";
const EN: &str = "The municipality has received your application for a building permit, and we will make a decision within the deadline.";
const EN_WITH_DANISH_NAMES: &str =
    "It branches off the M3 line at Østerport in the north and at København H in the south.";

#[test]
fn lingua_and_whatlang_tell_long_danish_from_english() {
    let dets: Vec<Box<dyn Detector>> = vec![
        Box::new(Lingua::new(false, false)),
        Box::new(Whatlang::new(true)),
    ];
    for d in &dets {
        assert_eq!(d.detect(DA).lang, Some(Lang::Da), "{}", d.name());
        assert_eq!(d.detect(EN).lang, Some(Lang::En), "{}", d.name());
    }
}

#[test]
fn lingua_reads_danish_place_names_as_danish_unless_masked() {
    let plain = Lingua::new(false, false);
    assert_eq!(plain.detect(EN_WITH_DANISH_NAMES).lang, Some(Lang::Da));
    let mut masked = Lingua::new(false, false);
    masked.mask_names = true;
    assert_eq!(masked.detect(EN_WITH_DANISH_NAMES).lang, Some(Lang::En));
}

#[test]
fn hysteresis_on_the_committed_corpus() {
    // Regression guard on the handwritten corpus (no downloads needed).
    let docs = corpus::load_dir(&corpus::corpus_dir().join("handwritten")).unwrap();
    let det = Lingua::new(false, false);
    let policy = Policy {
        hysteresis: true,
        min_letters: 20,
        switch_confidence: 0.9,
        short_confidence: None,
        headings_look_ahead: true,
    };
    let (mut ok, mut n, mut false_switches) = (0, 0, 0);
    for d in &docs {
        let infos: Vec<ParaInfo> = d
            .paras
            .iter()
            .map(|p| ParaInfo {
                letters: text::letters(&p.text),
                heading: p.heading(),
            })
            .collect();
        let dets: Vec<_> = d.paras.iter().map(|p| det.detect(&p.text)).collect();
        let pred = assign::assign(&infos, &dets, d.default, &policy);
        let truth: Vec<Lang> = d.paras.iter().map(|p| p.lang).collect();
        let (s, m) = assign::switch_errors(&truth, &pred, d.default);
        false_switches += s + m;
        ok += truth.iter().zip(&pred).filter(|(a, b)| a == b).count();
        n += truth.len();
    }
    // Spike 6 measured 94-95% here; the handwritten part is the hard part.
    assert!(ok * 100 >= n * 90, "{ok}/{n}");
    assert!(false_switches <= 60, "{false_switches}");
}

fn speller() -> Option<Speller> {
    match Speller::from_default_dir() {
        Some(s) => Some(s.expect("dictionaries load")),
        None => {
            eprintln!("skipping: run crates/proofing/fetch-dicts.sh for the dictionaries");
            None
        }
    }
}

#[test]
fn danish_compounds_and_typos() {
    let Some(s) = speller() else { return };
    for w in [
        "byggetilladelse",
        "klimatilpasningsplanen",
        "kommunalbestyrelsesmedlem",
        "affaldsindsamlingsordningen",
        "databehandleraftale",
        "f.eks.",
    ] {
        let dot = w.ends_with('.');
        assert!(s.check_word(w.trim_end_matches('.'), dot, Lang::Da), "{w}");
    }
    for w in ["komune", "byggetiladelse", "arbejdmiljø", "ansoegning"] {
        assert!(!s.check_word(w, false, Lang::Da), "{w}");
    }
    // Known limit: compounding accepts some misspellings as compounds.
    assert!(s.check_word("tidspunk", false, Lang::Da)); // tids + punk
    assert!(
        s.suggest("komune", Lang::Da)
            .iter()
            .take(3)
            .any(|w| w == "kommune")
    );
}

#[test]
fn english_typos_and_british_spellings_in_en_us() {
    let Some(s) = speller() else { return };
    assert!(!s.check_word("recieve", false, Lang::En));
    assert!(
        s.suggest("recieve", Lang::En)
            .iter()
            .take(3)
            .any(|w| w == "receive")
    );
    // en_US rejects British spellings that Danish organisations use.
    assert!(!s.check_word("centre", false, Lang::En));
}

#[test]
fn checking_a_mixed_paragraph() {
    let Some(s) = speller() else { return };
    let p =
        "Facilitatoren sagde: \"Today is about problems, not solutions.\" Det var en god ramme.";
    let danish_only = s.check_text(p, Lang::Da);
    assert!(danish_only.iter().any(|f| f.word == "Today"));
    let either = s.check_text_with(p, |_| &[Lang::Da, Lang::En]);
    assert!(either.is_empty(), "{either:?}");
}
