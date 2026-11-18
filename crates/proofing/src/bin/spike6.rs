// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 6 measurements. `cargo run --release -p skriv-proofing --bin
//! spike6 -- <out-dir>` writes JSON and Markdown to `<out-dir>`. Needs the
//! dictionaries (`fetch-dicts.sh`); the Wikipedia part of the corpus
//! (`corpus/fetch_wiki.py`, `corpus/build_wiki.py`) is used when present.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::json;
use skriv_proofing::assign::{self, ParaInfo, Policy};
use skriv_proofing::corpus::{self, Doc, PAGE_CHARS};
use skriv_proofing::detect::{self, Detection, Detector};
use skriv_proofing::spell::{self, Speller};
use skriv_proofing::text;
use skriv_proofing::{Lang, dicts_dir};

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "spike6-out".into()),
    );
    std::fs::create_dir_all(&out).expect("out dir");
    let dir = dicts_dir().expect("dictionaries missing: run crates/proofing/fetch-dicts.sh");

    // Load the dictionaries, timing it.
    let mut load = BTreeMap::new();
    let t = Instant::now();
    let da = spell::load_from_dir(&dir, "da_DK").expect("da_DK");
    load.insert("da_DK", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let en = spell::load_from_dir(&dir, "en_US").expect("en_US");
    load.insert("en_US", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let en_gb = spell::load_from_dir(&dir, "en_GB").expect("en_GB");
    load.insert("en_GB", t.elapsed().as_secs_f64() * 1e3);
    let speller = Speller::new(da, en);
    // en_GB (LGPL; for comparison) next to a second copy of da_DK.
    let speller_gb = Speller::new(spell::load_from_dir(&dir, "da_DK").expect("da_DK"), en_gb);

    let cdir = corpus::corpus_dir();
    let mut docs = corpus::load_dir(&cdir.join("handwritten")).expect("handwritten corpus");
    let wiki = corpus::load_dir(&cdir.join("wiki-build")).expect("wiki corpus");
    let have_wiki = !wiki.is_empty();
    docs.extend(wiki);
    let chars: usize = docs.iter().map(Doc::chars).sum();
    let paras: usize = docs.iter().map(|d| d.paras.len()).sum();
    eprintln!(
        "{} documents, {paras} paragraphs, {chars} chars = {:.1} pages (wiki part: {have_wiki})",
        docs.len(),
        chars as f64 / PAGE_CHARS as f64
    );

    let detection = run_detection(&docs, &speller, &out);
    let spelling = run_spelling(&docs, &speller, &speller_gb, &detection, &out);
    let typos = run_typos(&speller, &speller_gb, &out);
    let speed = run_speed(&docs, &speller, &load);

    let summary = json!({
        "corpus": {
            "documents": docs.len(),
            "paragraphs": paras,
            "chars": chars,
            "pages": chars as f64 / PAGE_CHARS as f64,
            "wikipedia_part": have_wiki,
            "per_document": docs.iter().map(|d| json!({
                "name": d.name, "default": d.default, "paragraphs": d.paras.len(),
                "chars": d.chars(), "true_switches": true_switches(d),
                "en_paragraphs": d.paras.iter().filter(|p| p.lang == Lang::En).count(),
                "inline_spans": d.paras.iter().map(|p| p.spans.len()).sum::<usize>(),
            })).collect::<Vec<_>>(),
        },
        "detection": detection.json,
        "spelling": spelling,
        "typos": typos,
        "speed": speed,
    });
    std::fs::write(
        out.join("results.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
    eprintln!("wrote {}", out.display());
}

fn true_switches(d: &Doc) -> usize {
    let mut prev = d.default;
    let mut n = 0;
    for p in &d.paras {
        if p.lang != prev {
            n += 1;
        }
        prev = p.lang;
    }
    n
}

fn detectors(speller: &Speller) -> Vec<Box<dyn Detector + '_>> {
    let masked_lingua = |confusers, low| {
        let mut d = detect::Lingua::new(confusers, low);
        d.mask_names = true;
        d
    };
    let mut masked_whatlang = detect::Whatlang::new(true);
    masked_whatlang.mask_names = true;
    vec![
        Box::new(detect::Lingua::new(false, false)),
        Box::new(masked_lingua(false, false)),
        Box::new(detect::Lingua::new(true, false)),
        Box::new(masked_lingua(true, false)),
        Box::new(detect::Lingua::new(false, true)),
        Box::new(detect::Whatlang::new(true)),
        Box::new(masked_whatlang),
        Box::new(detect::Whatlang::new(false)),
        Box::new(detect::DictVote { speller }),
        Box::new(detect::Agree(
            masked_lingua(false, false),
            detect::DictVote { speller },
        )),
        Box::new(detect::Agree(
            detect::Whatlang::new(true),
            detect::DictVote { speller },
        )),
    ]
}

struct DetectionResult {
    json: serde_json::Value,
    /// The best detector's name and its chosen policy.
    best: (String, Policy),
    /// Per document, per paragraph: the best detector's assignment.
    best_langs: Vec<Vec<Lang>>,
}

#[derive(Default, Clone, Copy)]
struct Score {
    spurious: usize,
    missed: usize,
    correct: usize,
    total: usize,
    chars: usize,
    true_switches: usize,
}

impl Score {
    fn add(&mut self, o: Score) {
        self.spurious += o.spurious;
        self.missed += o.missed;
        self.correct += o.correct;
        self.total += o.total;
        self.chars += o.chars;
        self.true_switches += o.true_switches;
    }
    fn per_100_switches(&self) -> f64 {
        100.0 * self.false_switches() as f64 / self.true_switches.max(1) as f64
    }
    fn false_switches(&self) -> usize {
        self.spurious + self.missed
    }
    fn per_10_pages(&self) -> f64 {
        self.false_switches() as f64 / (self.chars as f64 / (10 * PAGE_CHARS) as f64)
    }
    fn accuracy(&self) -> f64 {
        self.correct as f64 / self.total.max(1) as f64
    }
    fn json(&self) -> serde_json::Value {
        json!({
            "false_switches": self.false_switches(), "spurious": self.spurious, "missed": self.missed,
            "per_10_pages": self.per_10_pages(), "pages": self.chars as f64 / PAGE_CHARS as f64,
            "accuracy": self.accuracy(), "correct": self.correct, "paragraphs": self.total,
            "true_switches": self.true_switches, "per_100_true_switches": self.per_100_switches(),
        })
    }
}

#[derive(Default)]
struct Eval {
    all: Score,
    dev: Score,
    test: Score,
    hand: Score,
    wiki: Score,
    every: Score,
    long: Score,
    langs: Vec<Vec<Lang>>,
}

/// "Long" paragraphs for the long-only count: at least this many letters.
const LONG: usize = 20;

impl Eval {
    fn json(&self, p: Policy) -> serde_json::Value {
        json!({"policy": p, "all": self.all.json(), "dev": self.dev.json(), "test": self.test.json(),
            "handwritten": self.hand.json(), "wikipedia": self.wiki.json(),
            "all_paragraphs": self.every.json(), "long_paragraphs": self.long.json()})
    }
}

fn infos(d: &Doc) -> Vec<ParaInfo> {
    d.paras
        .iter()
        .map(|p| ParaInfo {
            letters: text::letters(&p.text),
            heading: p.heading(),
        })
        .collect()
}

/// Scores over all paragraphs, and over proofable paragraphs only (those
/// with at least one word the checker checks: "K-01" or "20 11 34 56" get
/// no spell checking whatever their language, so their language can't be
/// seen). Pages are counted over all paragraphs either way.
fn score_doc(d: &Doc, pred: &[Lang], proofable: &[bool], long: &[bool]) -> (Score, Score, Score) {
    let truth: Vec<Lang> = d.paras.iter().map(|p| p.lang).collect();
    let score = |keep: &dyn Fn(usize) -> bool| {
        let t: Vec<Lang> = (0..truth.len())
            .filter(|&i| keep(i))
            .map(|i| truth[i])
            .collect();
        let p: Vec<Lang> = (0..truth.len())
            .filter(|&i| keep(i))
            .map(|i| pred[i])
            .collect();
        let (spurious, missed) = assign::switch_errors(&t, &p, d.default);
        // Switches in the labels: the spurious ones of a prediction that never switches.
        let true_switches = assign::switch_errors(&vec![d.default; t.len()], &t, d.default).0;
        Score {
            true_switches,
            spurious,
            missed,
            correct: t.iter().zip(&p).filter(|(a, b)| a == b).count(),
            total: t.len(),
            chars: d.chars(),
        }
    };
    (
        score(&|_| true),
        score(&|i| proofable[i]),
        score(&|i| proofable[i] && long[i]),
    )
}

fn policies() -> Vec<Policy> {
    let mut v = vec![Policy::RAW];
    for min_letters in [0, 10, 20, 40, 80] {
        for switch_confidence in [0.0, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95, 0.99] {
            for short_confidence in [
                None,
                Some(0.5),
                Some(0.6),
                Some(0.7),
                Some(0.8),
                Some(0.9),
                Some(0.95),
            ] {
                if min_letters == 0 && short_confidence.is_some() {
                    continue;
                }
                for headings_look_ahead in [false, true] {
                    v.push(Policy {
                        hysteresis: true,
                        min_letters,
                        switch_confidence,
                        short_confidence,
                        headings_look_ahead,
                    });
                }
            }
        }
    }
    v
}

fn bucket(letters: usize) -> &'static str {
    match letters {
        0..20 => "a: <20 letters",
        20..60 => "b: 20-59",
        60..150 => "c: 60-149",
        150..400 => "d: 150-399",
        _ => "e: 400+",
    }
}

fn run_detection(docs: &[Doc], speller: &Speller, out: &std::path::Path) -> DetectionResult {
    let dets = detectors(speller);
    let pols = policies();
    let infos: Vec<Vec<ParaInfo>> = docs.iter().map(infos).collect();
    let proofable: Vec<Vec<bool>> = docs
        .iter()
        .map(|d| d.paras.iter().map(|p| text::proofable(&p.text)).collect())
        .collect();
    let dev: Vec<bool> = (0..docs.len()).map(|i| i % 2 == 0).collect();
    let mut rows = Vec::new();
    let mut md = String::new();
    let mut best_overall: Option<Best> = None;
    let mut errors_txt = String::new();
    writeln!(md, "# Spike 6: language per paragraph\n").unwrap();
    let total_chars: usize = docs.iter().map(Doc::chars).sum();
    writeln!(
        md,
        "{} documents, {} paragraphs, {:.1} pages. Dev = documents 1, 3, 5, … in name order; test = 2, 4, 6, …. \
         The policy is chosen on dev (fewest false switches, then accuracy) and reported on test and on all.\n",
        docs.len(),
        infos.iter().map(Vec::len).sum::<usize>(),
        total_chars as f64 / PAGE_CHARS as f64
    )
    .unwrap();
    writeln!(md, "False switches are counted over proofable paragraphs (at least one word the checker checks); \
        the last column counts them over every paragraph. Accuracy is over proofable paragraphs.\n").unwrap();
    writeln!(md, "| Detector | Policy | µs/para | False switches (spurious+missed) | per 10 pages | test half | handwritten | Wikipedia | per 100 true switches | Accuracy | per 10 pages, all paragraphs | per 10 pages, paragraphs of 20+ letters |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for det in &dets {
        let t = Instant::now();
        let detections: Vec<Vec<Detection>> = docs
            .iter()
            .map(|d| d.paras.iter().map(|p| det.detect(&p.text)).collect())
            .collect();
        let us = t.elapsed().as_secs_f64() * 1e6 / infos.iter().map(Vec::len).sum::<usize>() as f64;
        // Proofable paragraphs: all, dev, test, handwritten, Wikipedia;
        // predictions; every paragraph.
        let eval = |p: &Policy| -> Eval {
            let mut e = Eval::default();
            for (i, d) in docs.iter().enumerate() {
                let pred = assign::assign(&infos[i], &detections[i], d.default, p);
                let long: Vec<bool> = infos[i].iter().map(|x| x.letters >= LONG).collect();
                let (every, s, l) = score_doc(d, &pred, &proofable[i], &long);
                e.every.add(every);
                e.long.add(l);
                e.all.add(s);
                if dev[i] {
                    e.dev.add(s)
                } else {
                    e.test.add(s)
                }
                if d.name.starts_with('w') {
                    e.wiki.add(s)
                } else {
                    e.hand.add(s)
                }
                e.langs.push(pred);
            }
            e
        };
        // Raw, and the policy chosen on dev.
        let raw = eval(&Policy::RAW);
        let mut best: Option<(Policy, Score)> = None;
        for p in pols.iter().filter(|p| p.hysteresis) {
            let dv = eval(p).dev;
            let better = match &best {
                None => true,
                Some((_, b)) => {
                    (dv.false_switches(), std::cmp::Reverse(dv.correct))
                        < (b.false_switches(), std::cmp::Reverse(b.correct))
                }
            };
            if better {
                best = Some((*p, dv));
            }
        }
        let (bp, _) = best.unwrap();
        let chosen = eval(&bp);
        // The best policy on the whole corpus: an optimistic bound, since
        // it is tuned on the data it is scored on.
        let mut opt: Option<(Policy, Score)> = None;
        for p in pols.iter().filter(|p| p.hysteresis) {
            let a = eval(p).all;
            if opt.as_ref().is_none_or(|(_, b)| {
                (a.false_switches(), std::cmp::Reverse(a.correct))
                    < (b.false_switches(), std::cmp::Reverse(b.correct))
            }) {
                opt = Some((*p, a));
            }
        }
        let (op, _) = opt.unwrap();
        let optimistic = eval(&op);
        // A fixed, untuned policy for comparison.
        let fixed = Policy {
            hysteresis: true,
            min_letters: 20,
            switch_confidence: 0.9,
            short_confidence: None,
            headings_look_ahead: true,
        };
        let fixed_r = eval(&fixed);
        for (label, pol, r) in [
            ("raw", Policy::RAW, &raw),
            ("fixed", fixed, &fixed_r),
            ("tuned on dev", bp, &chosen),
            ("best on all (optimistic)", op, &optimistic),
        ] {
            writeln!(
                md,
                "| {} | {} | {:.0} | {} ({}+{}) | **{:.1}** | {:.1} | {:.1} | {:.1} | {:.0} | {:.1}% | {:.1} | {:.1} |",
                det.name(),
                if label == "raw" { "raw (no hysteresis)".to_string() } else { format!("{label}: {}", describe(&pol)) },
                us,
                r.all.false_switches(),
                r.all.spurious,
                r.all.missed,
                r.all.per_10_pages(),
                r.test.per_10_pages(),
                r.hand.per_10_pages(),
                r.wiki.per_10_pages(),
                r.all.per_100_switches(),
                r.all.accuracy() * 100.0,
                r.every.per_10_pages(),
                r.long.per_10_pages()
            )
            .unwrap();
        }
        // Accuracy by paragraph length and kind, raw and chosen.
        let mut by_len: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
        let mut by_kind: BTreeMap<String, [usize; 4]> = BTreeMap::new();
        for (i, d) in docs.iter().enumerate() {
            for (j, p) in d.paras.iter().enumerate() {
                let e = by_len.entry(bucket(infos[i][j].letters)).or_default();
                let k = by_kind.entry(p.kind.clone()).or_default();
                for a in [e, k] {
                    a[0] += 1;
                    a[1] += usize::from(raw.langs[i][j] == p.lang);
                    a[2] += usize::from(chosen.langs[i][j] == p.lang);
                    a[3] += usize::from(detections[i][j].lang == Some(p.lang));
                }
            }
        }
        // Per-document errors for the chosen policy.
        writeln!(errors_txt, "=== {} ({})", det.name(), describe(&bp)).unwrap();
        for (i, d) in docs.iter().enumerate() {
            for (j, p) in d.paras.iter().enumerate() {
                if chosen.langs[i][j] != p.lang && proofable[i][j] {
                    let dt = detections[i][j];
                    writeln!(
                        errors_txt,
                        "{} #{j} [{} {}] truth {} got {} (detector {:?} {:.2}): {}",
                        d.name,
                        p.kind,
                        infos[i][j].letters,
                        p.lang,
                        chosen.langs[i][j],
                        dt.lang.map(|l| l.code()),
                        dt.confidence,
                        p.text.chars().take(110).collect::<String>()
                    )
                    .unwrap();
                }
            }
        }
        rows.push(json!({
            "detector": det.name(),
            "us_per_paragraph": us,
            "raw": raw.json(Policy::RAW),
            "fixed": fixed_r.json(fixed),
            "tuned": chosen.json(bp),
            "optimistic": optimistic.json(op),
            "accuracy_by_length": by_len.iter().map(|(k, a)| json!({"bucket": k, "n": a[0],
                "raw": a[1] as f64 / a[0] as f64, "tuned": a[2] as f64 / a[0] as f64,
                "detector_alone": a[3] as f64 / a[0] as f64})).collect::<Vec<_>>(),
            "accuracy_by_kind": by_kind.iter().map(|(k, a)| json!({"kind": k, "n": a[0],
                "raw": a[1] as f64 / a[0] as f64, "tuned": a[2] as f64 / a[0] as f64,
                "detector_alone": a[3] as f64 / a[0] as f64})).collect::<Vec<_>>(),
        }));
        let key = (chosen.test.per_10_pages(), chosen.all.false_switches());
        if best_overall
            .as_ref()
            .is_none_or(|b| (key.1, key.0) < (b.1, b.0))
        {
            best_overall = Some((key.0, key.1, det.name(), bp, chosen.langs.clone()));
        }
    }
    writeln!(md, "\n## Accuracy by paragraph length (letters)\n").unwrap();
    writeln!(md, "Detector alone / raw assignment / tuned policy.\n").unwrap();
    let buckets: Vec<String> = rows[0]["accuracy_by_length"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| format!("{} (n={})", b["bucket"].as_str().unwrap(), b["n"]))
        .collect();
    writeln!(md, "| Detector | {} |", buckets.join(" | ")).unwrap();
    writeln!(md, "|---|{}", "---|".repeat(buckets.len())).unwrap();
    for r in &rows {
        let cells: Vec<String> = r["accuracy_by_length"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| {
                format!(
                    "{:.0} / {:.0} / {:.0}%",
                    b["detector_alone"].as_f64().unwrap() * 100.0,
                    b["raw"].as_f64().unwrap() * 100.0,
                    b["tuned"].as_f64().unwrap() * 100.0
                )
            })
            .collect();
        writeln!(
            md,
            "| {} | {} |",
            r["detector"].as_str().unwrap(),
            cells.join(" | ")
        )
        .unwrap();
    }
    writeln!(md, "\n## Accuracy by paragraph kind (tuned policy)\n").unwrap();
    let kinds: Vec<String> = rows[0]["accuracy_by_kind"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| format!("{} (n={})", b["kind"].as_str().unwrap(), b["n"]))
        .collect();
    writeln!(md, "| Detector | {} |", kinds.join(" | ")).unwrap();
    writeln!(md, "|---|{}", "---|".repeat(kinds.len())).unwrap();
    for r in &rows {
        let cells: Vec<String> = r["accuracy_by_kind"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| format!("{:.0}%", b["tuned"].as_f64().unwrap() * 100.0))
            .collect();
        writeln!(
            md,
            "| {} | {} |",
            r["detector"].as_str().unwrap(),
            cells.join(" | ")
        )
        .unwrap();
    }
    std::fs::write(out.join("detection.md"), md).unwrap();
    std::fs::write(out.join("detection-errors.txt"), errors_txt).unwrap();
    let (_, _, name, pol, langs) = best_overall.unwrap();
    DetectionResult {
        json: json!({"detectors": rows, "best": {"detector": name, "policy": pol}}),
        best: (name, pol),
        best_langs: langs,
    }
}

/// (test per 10 pages, false switches, detector, policy, assignment).
type Best = (f64, usize, String, Policy, Vec<Vec<Lang>>);

fn describe(p: &Policy) -> String {
    if !p.hysteresis {
        return "raw".into();
    }
    format!(
        "min {} letters, switch ≥ {}, short {}, headings {}",
        p.min_letters,
        p.switch_confidence,
        p.short_confidence
            .map_or("inherit".to_string(), |c| format!("≥ {c}")),
        if p.headings_look_ahead {
            "look ahead"
        } else {
            "inherit"
        }
    )
}

/// Spell checking the corpus, which is taken to be correctly spelled, so
/// every flag is a false flag (or an error in the source; see the list).
fn run_spelling(
    docs: &[Doc],
    speller: &Speller,
    speller_gb: &Speller,
    det: &DetectionResult,
    out: &std::path::Path,
) -> serde_json::Value {
    let lingua = detect::Lingua::new(false, false);
    let dict = detect::DictVote { speller };
    let seg_detector: &dyn Detector = if det.best.0.starts_with("dict") {
        &dict
    } else {
        &lingua
    };
    let modes = [
        "oracle: word language",
        "oracle: paragraph language",
        "predicted paragraph language",
        "predicted + sentence/quote segments",
        "predicted + either dictionary",
        "both dictionaries for every word",
        "predicted + either dictionary for capitalised words",
        "en_GB: oracle word language",
        "en_GB: predicted + either dictionary for capitalised words",
    ];
    let mut flags: Vec<BTreeMap<String, usize>> = vec![BTreeMap::new(); modes.len()];
    let mut nflags = vec![0usize; modes.len()];
    let mut words = 0usize;
    let mut by_lang = [[0usize; 2]; 2]; // [lang][words, flags] for mode 0
    let mut long_da = [0usize; 2];
    let mut skipped: BTreeMap<String, usize> = BTreeMap::new();
    for (i, d) in docs.iter().enumerate() {
        for (j, p) in d.paras.iter().enumerate() {
            let pred = det.best_langs[i][j];
            // Segment languages: a segment switches only if it is long
            // enough and the detector is confident.
            let segs = text::segments(&p.text);
            let seg_lang: Vec<(std::ops::Range<usize>, Lang)> = segs
                .into_iter()
                .map(|r| {
                    let s = &p.text[r.clone()];
                    let dt = seg_detector.detect(s);
                    let conf_bar = if det.best.0.starts_with("dict") {
                        0.6
                    } else {
                        0.9
                    };
                    let l = match dt.lang {
                        Some(l) if text::letters(s) >= 15 && dt.confidence >= conf_bar => l,
                        _ => pred,
                    };
                    (r, l)
                })
                .collect();
            for w in text::words(&p.text) {
                if let Some(s) = w.skip {
                    *skipped.entry(format!("{s:?}")).or_default() += 1;
                    continue;
                }
                words += 1;
                let truth = p.lang_at(w.range.start);
                let seg = seg_lang
                    .iter()
                    .find(|(r, _)| r.contains(&w.range.start))
                    .map_or(pred, |x| x.1);
                let word = &p.text[w.range.clone()];
                let cap: &[Lang] = if word.chars().next().is_some_and(char::is_uppercase) {
                    &[pred, pred.other()]
                } else {
                    &[pred]
                };
                let sets: [&[Lang]; 9] = [
                    &[truth],
                    &[p.lang],
                    &[pred],
                    &[seg],
                    &[pred, pred.other()],
                    &[Lang::Da, Lang::En],
                    cap,
                    &[truth],
                    cap,
                ];
                let mut ok0 = true;
                for (m, langs) in sets.iter().enumerate() {
                    let sp = if m >= 7 { speller_gb } else { speller };
                    let ok = sp.check(&p.text, &w, langs);
                    if m == 0 {
                        ok0 = ok;
                    }
                    if !ok {
                        nflags[m] += 1;
                        *flags[m]
                            .entry(format!("{word} [{}]", langs[0]))
                            .or_default() += 1;
                    }
                }
                by_lang[truth.index()][0] += 1;
                by_lang[truth.index()][1] += usize::from(!ok0);
                if truth == Lang::Da && text::letters(word) >= 15 {
                    long_da[0] += 1;
                    long_da[1] += usize::from(!ok0);
                }
            }
        }
    }
    let mut md = String::new();
    writeln!(md, "# Spike 6: spell checking the corpus\n").unwrap();
    writeln!(
        md,
        "{words} words checked. Paragraph languages predicted by {} ({}).\n",
        det.best.0,
        describe(&det.best.1)
    )
    .unwrap();
    writeln!(md, "Skipped words (not checked): {skipped:?}\n").unwrap();
    writeln!(md, "| Mode | Flags | per 1,000 words |\n|---|---|---|").unwrap();
    for (m, name) in modes.iter().enumerate() {
        writeln!(
            md,
            "| {name} | {} | {:.1} |",
            nflags[m],
            nflags[m] as f64 * 1000.0 / words as f64
        )
        .unwrap();
    }
    writeln!(
        md,
        "\nOracle word language: Danish {} flags / {} words, English {} / {}. Danish words of 15+ letters: {} flagged of {}.\n",
        by_lang[0][1], by_lang[0][0], by_lang[1][1], by_lang[1][0], long_da[1], long_da[0]
    )
    .unwrap();
    for m in [0, 2, 6, 7, 8] {
        writeln!(md, "## Flagged words: {}\n", modes[m]).unwrap();
        let mut v: Vec<_> = flags[m].iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let list: Vec<String> = v
            .iter()
            .map(|(w, n)| {
                if **n > 1 {
                    format!("{w} ×{n}")
                } else {
                    w.to_string()
                }
            })
            .collect();
        writeln!(md, "{}\n", list.join(", ")).unwrap();
    }
    std::fs::write(out.join("spelling.md"), md).unwrap();
    json!({
        "words": words,
        "skipped": skipped,
        "modes": modes.iter().enumerate().map(|(m, n)| json!({"mode": n, "flags": nflags[m],
            "per_1000_words": nflags[m] as f64 * 1000.0 / words as f64,
            "distinct": flags[m].len()})).collect::<Vec<_>>(),
        "oracle_by_language": {"da": {"words": by_lang[0][0], "flags": by_lang[0][1]},
                               "en": {"words": by_lang[1][0], "flags": by_lang[1][1]}},
        "danish_15_plus_letters": {"words": long_da[0], "flags": long_da[1]},
    })
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn run_typos(speller: &Speller, speller_gb: &Speller, out: &std::path::Path) -> serde_json::Value {
    let typos = corpus::parse_typos(
        &std::fs::read_to_string(corpus::corpus_dir().join("typos.tsv")).expect("typos.tsv"),
    )
    .expect("parse typos");
    let mut by_class: BTreeMap<(Lang, String), [usize; 6]> = BTreeMap::new();
    let mut lines = String::new();
    let mut times = Vec::new();
    let mut bad_right = Vec::new();
    for t in &typos {
        let words: Vec<&str> = t.wrong.split(' ').collect();
        let own = words.iter().any(|w| !speller.check_word(w, false, t.lang));
        let either = words.iter().any(|w| {
            !speller.check_word(w, false, t.lang) && !speller.check_word(w, false, t.lang.other())
        });
        let right_ok = speller.check_word(&t.right, false, t.lang);
        if !right_ok {
            bad_right.push(t.right.clone());
        }
        let (mut top1, mut top3) = (false, false);
        let mut sugg = Vec::new();
        if own && words.len() == 1 {
            let tm = Instant::now();
            sugg = speller.suggest(&t.wrong, t.lang);
            times.push(tm.elapsed().as_secs_f64() * 1e3);
            top1 = sugg.first() == Some(&t.right);
            top3 = sugg.iter().take(3).any(|s| s == &t.right);
        }
        let e = by_class.entry((t.lang, t.class.clone())).or_default();
        e[0] += 1;
        e[1] += usize::from(own);
        e[2] += usize::from(either);
        e[3] += usize::from(top1);
        e[4] += usize::from(top3);
        e[5] += usize::from(own && words.len() == 1);
        writeln!(
            lines,
            "{}\t{}\t{}\t{}\tflagged={} either={} top3={} suggestions={:?}",
            t.lang,
            t.class,
            t.wrong,
            t.right,
            own,
            either,
            top3,
            &sugg[..sugg.len().min(5)]
        )
        .unwrap();
    }
    std::fs::write(out.join("typos.txt"), &lines).unwrap();
    let mut md = String::new();
    writeln!(md, "# Spike 6: seeded misspellings\n").unwrap();
    writeln!(md, "| Lang | Class | n | Flagged (own dictionary) | Flagged if either dictionary may accept | Right word top 1 | Right word in top 3 |").unwrap();
    writeln!(md, "|---|---|---|---|---|---|---|").unwrap();
    let mut tot: BTreeMap<(Lang, bool), [usize; 6]> = BTreeMap::new();
    for ((l, c), a) in &by_class {
        writeln!(
            md,
            "| {l} | {c} | {} | {} | {} | {} | {} |",
            a[0], a[1], a[2], a[3], a[4]
        )
        .unwrap();
        let detectable = c != "realword" && c != "split";
        let e = tot.entry((*l, detectable)).or_default();
        for k in 0..6 {
            e[k] += a[k];
        }
    }
    writeln!(
        md,
        "\nTotals (detectable classes = all but realword and split):\n"
    )
    .unwrap();
    for ((l, d), a) in &tot {
        writeln!(
            md,
            "- {l}, {}: {} typos, flagged {} ({:.0}%), with either dictionary {} ({:.0}%); of the {} flagged single words, right word top 1 in {} ({:.0}%), top 3 in {} ({:.0}%)",
            if *d { "detectable" } else { "realword + split" },
            a[0], a[1], 100.0 * a[1] as f64 / a[0] as f64, a[2], 100.0 * a[2] as f64 / a[0] as f64,
            a[5], a[3], 100.0 * a[3] as f64 / a[5].max(1) as f64, a[4], 100.0 * a[4] as f64 / a[5].max(1) as f64
        )
        .unwrap();
    }
    let n = times.len();
    let (p50, p95, max) = (
        pct(&mut times, 0.5),
        pct(&mut times, 0.95),
        pct(&mut times, 1.0),
    );
    writeln!(md, "\nSuggestion time per word ({n} words): median {p50:.0} ms, p95 {p95:.0} ms, max {max:.0} ms.").unwrap();
    writeln!(md, "\nRight words the dictionary rejects: {bad_right:?}").unwrap();
    // en_GB for comparison: which English typos it flags, and top 3.
    let common: Vec<&corpus::Typo> = typos
        .iter()
        .filter(|t| t.lang == Lang::En && t.class == "common")
        .collect();
    let gb = common
        .iter()
        .filter(|t| !speller_gb.check_word(&t.wrong, false, Lang::En))
        .count();
    let gb_top3 = common
        .iter()
        .filter(|t| {
            speller_gb
                .suggest(&t.wrong, Lang::En)
                .iter()
                .take(3)
                .any(|s| s == &t.right)
        })
        .count();
    writeln!(md, "\nen_GB (comparison only) flags {gb} of the {} English common typos; right word in top 3 for {gb_top3}.", common.len()).unwrap();
    std::fs::write(out.join("typos.md"), md).unwrap();
    json!({
        "by_class": by_class.iter().map(|((l, c), a)| json!({"lang": l, "class": c, "n": a[0],
            "flagged": a[1], "flagged_either": a[2], "top1": a[3], "top3": a[4], "suggested": a[5]})).collect::<Vec<_>>(),
        "totals": tot.iter().map(|((l, d), a)| json!({"lang": l, "detectable": d, "n": a[0],
            "flagged": a[1], "flagged_either": a[2], "top1": a[3], "top3": a[4], "suggested": a[5]})).collect::<Vec<_>>(),
        "suggest_ms": {"n": n, "p50": p50, "p95": p95, "max": max},
        "right_words_rejected": bad_right,
        "en_gb_flags_common": gb,
        "en_gb_top3_common": gb_top3,
    })
}

fn run_speed(docs: &[Doc], speller: &Speller, load: &BTreeMap<&str, f64>) -> serde_json::Value {
    // Throughput: every word of the corpus against its paragraph language.
    let mut n = 0usize;
    let t = Instant::now();
    for _ in 0..3 {
        for d in docs {
            for p in &d.paras {
                let langs = [p.lang];
                for w in text::words(&p.text) {
                    if w.skip.is_none() {
                        n += 1;
                        std::hint::black_box(speller.check(&p.text, &w, &langs));
                    }
                }
            }
        }
    }
    let wps = n as f64 / t.elapsed().as_secs_f64();
    // Per keystroke: re-check one paragraph and re-detect its language.
    let mut lens: Vec<&corpus::Para> = docs.iter().flat_map(|d| d.paras.iter()).collect();
    lens.sort_by_key(|p| p.text.len());
    let median = lens[lens.len() / 2];
    let long = lens[lens.len() * 95 / 100];
    let longest = lens[lens.len() - 1];
    let lingua = detect::Lingua::new(false, false);
    let whatlang = detect::Whatlang::new(true);
    let dict = detect::DictVote { speller };
    let mut keystroke = Vec::new();
    for (label, p) in [
        ("median paragraph", median),
        ("95th percentile", long),
        ("longest", longest),
    ] {
        let time = |f: &dyn Fn()| {
            let mut v: Vec<f64> = (0..50)
                .map(|_| {
                    let t = Instant::now();
                    f();
                    t.elapsed().as_secs_f64() * 1e6
                })
                .collect();
            pct(&mut v, 0.5)
        };
        let check = time(&|| {
            std::hint::black_box(speller.check_text(&p.text, p.lang));
        });
        let li = time(&|| {
            std::hint::black_box(lingua.detect(&p.text));
        });
        let wl = time(&|| {
            std::hint::black_box(whatlang.detect(&p.text));
        });
        let dv = time(&|| {
            std::hint::black_box(dict.detect(&p.text));
        });
        keystroke.push(json!({"paragraph": label, "chars": p.text.chars().count(),
            "words": text::words(&p.text).len(), "check_us": check, "lingua_us": li,
            "whatlang_us": wl, "dict_vote_us": dv}));
    }
    json!({"load_ms": load, "check_words_per_second": wps, "keystroke": keystroke})
}
