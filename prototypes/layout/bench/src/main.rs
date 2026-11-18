// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 2b measurements. `cargo run --release -p layout-bench -- <command>`.

// Measurement loops keep an explicit caret offset next to the keystroke count.
#![allow(clippy::explicit_counter_loop)]

mod scale;

use std::time::Instant;

use layout_contract::input::Doc;
use layout_contract::{EditHint, Engine};

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// The Danish municipal letter, scaled to `copies` sections (100 = 100 pages
/// in LibreOffice), as layout input.
pub fn letter(copies: usize) -> Doc {
    layout_contract::from_model::build(&letter_odt(copies, false).doc)
}

/// The scaled letter as an ODT; with `justify`, its "Text body" style is
/// justified and hyphenated (Danish, 2/2 characters), as many municipal
/// letters are.
pub fn letter_odt(copies: usize, justify: bool) -> skriv_odf::Odt {
    let src = std::fs::read(root().join("fidelity/corpus/generated/danish-municipal-letter.odt"))
        .unwrap();
    let mut odt = skriv_odf::read(src).unwrap();
    if copies > 0 {
        odt.doc = scale::scale(&odt.doc, copies);
    }
    if justify {
        for e in &mut odt.doc.styles.named {
            if let skriv_model::StyleEntry::Style(st) = e
                && st.name() == Some("Text_20_body")
            {
                st.meta.dirty = true;
                let pp = st.props_mut("style:paragraph-properties");
                pp.attrs.set("fo:text-align", "justify");
                pp.meta.dirty = true;
                let tp = st.props_mut("style:text-properties");
                tp.attrs.set("fo:hyphenate", "true");
                tp.attrs.set("fo:hyphenation-remain-char-count", "2");
                tp.attrs.set("fo:hyphenation-push-char-count", "2");
                tp.attrs.set("fo:language", "da");
                tp.attrs.set("fo:country", "DK");
                tp.meta.dirty = true;
            }
        }
    }
    // Write and re-read, so the input is exactly what LibreOffice sees.
    let bytes = odt.write().unwrap().bytes;
    skriv_odf::read(bytes).unwrap()
}

pub struct Lat {
    pub samples: Vec<f64>,
}

impl Lat {
    pub fn pct(&self, p: f64) -> f64 {
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        v[((v.len() as f64 - 1.0) * p).round() as usize]
    }
}

/// Type `n` characters into paragraph `para`, from the middle, re-laying
/// out and locating the caret after each one.
pub fn typing<E: Engine>(e: &mut E, doc: &mut Doc, para: usize, n: usize) -> Lat {
    let mut samples = Vec::with_capacity(n);
    let mut offset = doc.paras()[para].char_len() / 2;
    let text = "Æbleøen på Ålborg er smuk ";
    let chars: Vec<char> = text.chars().collect();
    for i in 0..n {
        doc.para_mut(para)
            .unwrap()
            .insert(offset, chars[i % chars.len()]);
        offset += 1;
        let t = Instant::now();
        let pages = e.layout(doc, &EditHint::Para(para));
        let caret = layout_contract::locate(pages, para, offset);
        samples.push(t.elapsed().as_secs_f64() * 1e3);
        if caret.is_none() {
            for (pi, pg) in pages.pages.iter().enumerate() {
                for l in pg.lines.iter().filter(|l| l.para == para) {
                    eprintln!("page {pi} range {:?} hyphen {}", l.range, l.hyphen);
                }
            }
            let t: String = doc.paras()[para]
                .text
                .chars()
                .skip(offset.saturating_sub(15))
                .take(30)
                .collect();
            panic!(
                "caret lost after keystroke {i} at offset {offset} of {}: {t:?}",
                doc.paras()[para].char_len()
            );
        }
    }
    Lat { samples }
}

/// Resident and peak resident memory of this process, MiB.
pub fn rss() -> (f64, f64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / 1024.0
    };
    (get("VmRSS:"), get("VmHWM:"))
}

fn latency<E: Engine>(base: &Doc, n: usize, mk: impl Fn() -> E) {
    for (label, page, table) in [
        ("page 1", 0usize, false),
        ("page 50", 49, false),
        ("table, page 80", 79, true),
    ] {
        let mut doc = base.clone();
        let mut e = mk();
        let t = Instant::now();
        let pages = e.layout(&doc, &EditHint::Full).clone();
        let full_ms = t.elapsed().as_secs_f64() * 1e3;
        let para = layout_contract::target(&doc, &pages, page, table);
        let lat = typing(&mut e, &mut doc, para, n);
        let inc = e.layout(&doc, &EditHint::Para(para)).clone();
        let fresh = mk().layout(&doc, &EditHint::Full).clone();
        println!(
            "{}, {label}: {} pages (full layout {:.0} ms); per keystroke p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms; incremental == fresh: {}; memory {:.0} MiB (peak {:.0})",
            e.name(),
            pages.pages.len(),
            full_ms,
            lat.pct(0.5),
            lat.pct(0.95),
            lat.pct(1.0),
            layout_contract::layout_hash(&inc) == layout_contract::layout_hash(&fresh),
            rss().0,
            rss().1
        );
    }
}

/// For every character on 10 pages spread through the document: does
/// hit(locate(c)) give c back? Returns (checked, ok, examples of failures).
pub fn caret_roundtrip(doc: &Doc, pages: &layout_contract::Pages) -> (usize, usize, Vec<String>) {
    let paras = doc.paras();
    let n = pages.pages.len();
    let sample: Vec<usize> = (0..10).map(|i| i * (n - 1) / 9).collect();
    let (mut checked, mut ok, mut bad) = (0, 0, Vec::new());
    for &pi in &sample {
        for l in &pages.pages[pi].lines {
            let len = paras[l.para].char_len();
            let mut offsets: Vec<usize> = l.range.clone().collect();
            if l.range.end == len {
                offsets.push(len);
            }
            for k in offsets {
                checked += 1;
                let Some(c) = layout_contract::locate(pages, l.para, k) else {
                    bad.push(format!("p{pi} para {} offset {k}: locate failed", l.para));
                    continue;
                };
                let line = &pages.pages[c.page].lines[c.line];
                let got = layout_contract::hit(pages, c.page, c.x, line.y + line.height / 2.0);
                if got == Some((l.para, k)) {
                    ok += 1;
                } else if bad.len() < 12 {
                    let t: String = paras[l.para]
                        .text
                        .chars()
                        .skip(k.saturating_sub(3))
                        .take(7)
                        .collect();
                    bad.push(format!(
                        "p{pi} para {} offset {k} ({t:?}): hit gave {got:?}",
                        l.para
                    ));
                }
            }
        }
    }
    (checked, ok, bad)
}

/// The same document with every body paragraph justified and hyphenated,
/// as municipal letters often are: this is what exercises Knuth–Plass and
/// hyphenation.
pub fn justified(mut doc: Doc) -> Doc {
    for b in &mut doc.blocks {
        if let layout_contract::input::Block::Para(p) = b
            && p.style.heading.is_none()
            && p.style.label.is_none()
            && p.style.align == layout_contract::input::Align::Start
        {
            p.style.align = layout_contract::input::Align::Justify;
            p.style.hyphenate = true;
        }
    }
    doc
}

/// `letter(100)`, justified when the environment says SKRIV_JUSTIFY=1.
pub fn test_doc() -> Doc {
    let justify = std::env::var("SKRIV_JUSTIFY").as_deref() == Ok("1");
    layout_contract::from_model::build(&letter_odt(100, justify).doc)
}

/// An engine by name.
pub fn make(name: &str) -> Box<dyn Engine> {
    match name {
        "own-greedy" => {
            let mut e = layout_own::OwnEngine::new();
            e.breaker = layout_own::Breaker::Greedy;
            Box::new(e)
        }
        "own-shrink" => {
            let mut e = layout_own::OwnEngine::new();
            e.shrink = true;
            Box::new(e)
        }
        "typst" => Box::new(layout_typst::TypstEngine::new()),
        "typst-compat" => {
            let mut e = layout_typst::TypstEngine::new();
            e.compat = true;
            Box::new(e)
        }
        _ => Box::new(layout_own::OwnEngine::new()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let doc = letter(args.get(2).and_then(|v| v.parse().ok()).unwrap_or(0));
            println!("{:?}", doc.page);
            let pages = if args.get(3).map(String::as_str) == Some("typst") {
                layout_typst::TypstEngine::new()
                    .layout(&doc, &EditHint::Full)
                    .clone()
            } else {
                layout_own::OwnEngine::new()
                    .layout(&doc, &EditHint::Full)
                    .clone()
            };
            for (pi, p) in pages.pages.iter().enumerate() {
                for l in &p.lines {
                    let para = doc.paras()[l.para];
                    let t: String = para
                        .text
                        .chars()
                        .skip(l.range.start)
                        .take(l.range.len())
                        .collect();
                    println!(
                        "p{pi} y={:6.1} h={:5.1} x={:6.1}..{:6.1} [{}] {:?} {}",
                        l.y,
                        l.height,
                        l.carets[0],
                        l.carets.last().unwrap(),
                        para.style.size,
                        para.style.align,
                        t
                    );
                }
            }
        }
        Some("latency") => {
            let which = args.get(2).cloned().unwrap_or_else(|| "own,typst".into());
            let n: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(1000);
            let base = test_doc();
            for eng in which.split(',') {
                match eng {
                    "own" => latency(&base, n, layout_own::OwnEngine::new),
                    "greedy" => latency(&base, n, || {
                        let mut e = layout_own::OwnEngine::new();
                        e.breaker = layout_own::Breaker::Greedy;
                        e
                    }),
                    _ => latency(&base, n, layout_typst::TypstEngine::new),
                }
            }
        }
        Some("typst-breakdown") => {
            let base = test_doc();
            for (label, page, table) in [
                ("page 1", 0usize, false),
                ("page 50", 49, false),
                ("table, page 80", 79, true),
            ] {
                let mut doc = base.clone();
                let mut e = layout_typst::TypstEngine::new();
                let pages = e.layout(&doc, &EditHint::Full).clone();
                let para = layout_contract::target(&doc, &pages, page, table);
                let mut offset = doc.paras()[para].char_len() / 2;
                let (mut edit, mut comp, mut conv, mut conv_pages, mut ev) =
                    (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
                for _ in 0..30 {
                    doc.para_mut(para).unwrap().insert(offset, 'x');
                    offset += 1;
                    e.layout(&doc, &EditHint::Para(para));
                    edit.push(e.timings.generate_ms);
                    comp.push(e.timings.compile_ms);
                    conv.push(e.timings.convert_ms);
                    conv_pages.push(e.timings.pages_converted as f64);
                    ev.push(e.timings.evict_ms);
                }
                let med = |v: &mut Vec<f64>| {
                    v.sort_by(|a, b| a.total_cmp(b));
                    v[v.len() / 2]
                };
                println!(
                    "typst, {label}: median edit+reparse {:.2} ms, compile {:.1} ms, cache eviction {:.1} ms, convert {:.1} ms ({} pages converted)",
                    med(&mut edit),
                    med(&mut comp),
                    med(&mut ev),
                    med(&mut conv),
                    med(&mut conv_pages)
                );
            }
        }
        Some("carets") => {
            let doc = test_doc();
            let own = layout_own::OwnEngine::new()
                .layout(&doc, &EditHint::Full)
                .clone();
            let mut te = layout_typst::TypstEngine::new();
            let typ = te.layout(&doc, &EditHint::Full).clone();
            for (name, pages) in [("own", &own), ("typst", &typ)] {
                let (checked, ok, bad) = caret_roundtrip(&doc, pages);
                println!(
                    "{name}: {ok}/{checked} caret positions round-trip ({:.3}%)",
                    100.0 * ok as f64 / checked as f64
                );
                for b in bad {
                    println!("  {b}");
                }
            }
            // Cross-check the Typst bridge against Typst's own IDE jump.
            let tdoc = te.document().unwrap();
            let src = te.world.source.clone();
            let (mut agree, mut total) = (0, 0);
            for (pi, p) in typ.pages.iter().enumerate().step_by(10) {
                for l in p.lines.iter().take(6) {
                    let k = (l.range.start + l.range.end) / 2;
                    let Some(byte) = te.source_byte(l.para, k) else {
                        continue;
                    };
                    total += 1;
                    let ours = layout_contract::locate(&typ, l.para, k).unwrap();
                    let theirs = typst_ide::jump_from_cursor(&tdoc, &src, byte + 1);
                    let line = &typ.pages[ours.page].lines[ours.line];
                    if theirs.iter().any(|pos| {
                        pos.page.get() == pi + 1
                            && pos.page.get() == ours.page + 1
                            && (pos.point.y.to_pt() as f32 - line.baseline).abs() < 1.0
                    }) {
                        agree += 1;
                    } else if total - agree <= 4 {
                        println!(
                            "  ours page {} x {:.1} baseline {:.1}; typst-ide {:?}",
                            ours.page + 1,
                            ours.x,
                            line.baseline,
                            theirs
                                .iter()
                                .map(|p| (
                                    p.page.get(),
                                    p.point.x.to_pt() as f32,
                                    p.point.y.to_pt() as f32
                                ))
                                .collect::<Vec<_>>()
                        );
                    }
                }
            }
            println!(
                "typst-ide jump_from_cursor puts {agree}/{total} sampled characters on the same page and baseline as the bridge (it returns the start of the text run, not the character's x)"
            );
        }
        Some("export") => {
            // Layout input for the wasm build, and the native hashes to compare.
            let doc = letter(100);
            let out = root().join("target/spike2b");
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(
                out.join("letter-100.json"),
                serde_json::to_vec(&doc).unwrap(),
            )
            .unwrap();
            let jdoc = layout_contract::from_model::build(&letter_odt(100, true).doc);
            std::fs::write(
                out.join("letter-100-justified.json"),
                serde_json::to_vec(&jdoc).unwrap(),
            )
            .unwrap();
            for j in [false, true] {
                let mut o = letter_odt(100, j);
                let name = if j {
                    "letter-100-justified.odt"
                } else {
                    "letter-100.odt"
                };
                std::fs::write(out.join(name), o.write().unwrap().bytes).unwrap();
            }
            let round: Doc =
                serde_json::from_slice(&std::fs::read(out.join("letter-100.json")).unwrap())
                    .unwrap();
            assert_eq!(round, doc);
            let mut g = layout_own::OwnEngine::new();
            g.breaker = layout_own::Breaker::Greedy;
            for (name, h) in [
                (
                    "own",
                    layout_contract::layout_hash(
                        layout_own::OwnEngine::new().layout(&doc, &EditHint::Full),
                    ),
                ),
                (
                    "own-greedy",
                    layout_contract::layout_hash(g.layout(&doc, &EditHint::Full)),
                ),
                (
                    "typst",
                    layout_contract::layout_hash(
                        layout_typst::TypstEngine::new().layout(&doc, &EditHint::Full),
                    ),
                ),
            ] {
                println!("native {name}: {h:016x}");
            }
            let jdoc = layout_contract::from_model::build(&letter_odt(100, true).doc);
            for name in ["own", "own-greedy", "typst"] {
                println!(
                    "native {name} (justified): {:016x}",
                    layout_contract::layout_hash(make(name).layout(&jdoc, &EditHint::Full))
                );
            }
        }
        Some("dump-json") => {
            let doc = letter(100);
            let pages = match args.get(2).map(String::as_str) {
                Some("typst") => layout_typst::TypstEngine::new()
                    .layout(&doc, &EditHint::Full)
                    .clone(),
                _ => layout_own::OwnEngine::new()
                    .layout(&doc, &EditHint::Full)
                    .clone(),
            };
            let mut out = Vec::new();
            for (pi, p) in pages.pages.iter().enumerate() {
                for l in &p.lines {
                    out.push(vec![
                        pi as f64,
                        l.para as f64,
                        f64::from(l.y),
                        f64::from(l.height),
                        l.range.start as f64,
                        l.range.end as f64,
                        f64::from(l.carets[0]),
                        f64::from(*l.carets.last().unwrap()),
                    ]);
                }
            }
            std::fs::write(args.get(3).unwrap(), serde_json::to_string(&out).unwrap()).unwrap();
        }
        Some("kp-check") => {
            let doc = justified(letter(3));
            let width = doc.page.body_width();
            for mode in [layout_own::Breaker::Optimal, layout_own::Breaker::Greedy] {
                let mut e = layout_own::OwnEngine::new();
                e.breaker = mode;
                let (mut lines, mut over, mut hyph) = (0, 0, 0);
                for p in doc
                    .paras()
                    .iter()
                    .filter(|p| p.style.align == layout_contract::input::Align::Justify)
                    .take(12)
                {
                    let b = e.para_box(p, width);
                    for l in &b.lines {
                        lines += 1;
                        if l.natural > l.avail + 0.01 {
                            over += 1;
                        }
                        if l.hyphen {
                            hyph += 1;
                        }
                    }
                    if mode == layout_own::Breaker::Optimal {
                        let l = &b.lines[0];
                        println!(
                            "  {:?}: {} lines; first line natural {:.1} avail {:.1} spaces {}",
                            mode,
                            b.lines.len(),
                            l.natural,
                            l.avail,
                            l.spaces
                        );
                    }
                }
                println!(
                    "{mode:?}: {lines} lines, {over} wider than the measure, {hyph} hyphenated"
                );
            }
        }
        Some("lines") => {
            // Line texts in page order, for comparing breaks with LibreOffice.
            let doc = test_doc();
            let mut out = String::new();
            let tables = layout_contract::table_paras(&doc);
            for eng in ["own-greedy", "own", "own-shrink", "typst", "typst-compat"] {
                let pages = make(eng).layout(&doc, &EditHint::Full).clone();
                let paras = doc.paras();
                out.clear();
                for (pi, p) in pages.pages.iter().enumerate() {
                    for l in &p.lines {
                        let t: String = paras[l.para]
                            .text
                            .chars()
                            .skip(l.range.start)
                            .take(l.range.len())
                            .collect();
                        let kind = if tables.contains(&l.para) { "T" } else { "" };
                        out.push_str(&format!(
                            "{pi}\t{kind}{}\t{}{}\n",
                            l.para,
                            t.trim_end(),
                            if l.hyphen { "-" } else { "" }
                        ));
                    }
                }
                let suffix = if std::env::var("SKRIV_JUSTIFY").as_deref() == Ok("1") {
                    "-justified"
                } else {
                    ""
                };
                std::fs::write(
                    root().join(format!("target/spike2b/lines-{eng}{suffix}.tsv")),
                    &out,
                )
                .unwrap();
                println!("{eng}: {} pages", pages.pages.len());
            }
        }
        Some("cmp-input") => {
            let src =
                std::fs::read(root().join("fidelity/corpus/generated/danish-municipal-letter.odt"))
                    .unwrap();
            let mut odt = skriv_odf::read(src).unwrap();
            odt.doc = scale::scale(&odt.doc, 2);
            let a = layout_contract::from_model::build(&odt.doc);
            let b = layout_contract::from_model::build(
                &skriv_odf::read(odt.write().unwrap().bytes).unwrap().doc,
            );
            println!("page {:?}\n     {:?}", a.page, b.page);
            for (x, y) in a.paras().iter().zip(b.paras()) {
                if x.style != y.style || x.text != y.text {
                    println!("{:?}\n{:?}\n{:?}\n{:?}", x.style, y.style, x.text, y.text);
                    break;
                }
            }
        }
        Some("hyph") => {
            // Distinct words (5+ letters) from every generated-corpus
            // document, with hypher's Danish break points.
            let mut words = std::collections::BTreeSet::new();
            for e in std::fs::read_dir(root().join("fidelity/corpus/generated")).unwrap() {
                let path = e.unwrap().path();
                if path.extension().is_some_and(|x| x == "odt") {
                    let odt = skriv_odf::read(std::fs::read(&path).unwrap()).unwrap();
                    for p in layout_contract::from_model::build(&odt.doc).paras() {
                        for w in p.text.split(|c: char| !c.is_alphabetic()) {
                            if w.chars().count() >= 5 {
                                words.insert(w.to_lowercase());
                            }
                        }
                    }
                }
            }
            let mut out = String::new();
            for w in &words {
                let syl: Vec<&str> = hypher::hyphenate(w, hypher::Lang::Danish).collect();
                out.push_str(&format!("{w}\t{}\n", syl.join("-")));
            }
            std::fs::write(root().join("target/spike2b/hypher-da.tsv"), out).unwrap();
            println!("{} words", words.len());
        }
        Some("spaces") => {
            // Inter-word space widths on justified lines (not a paragraph's
            // last line), relative to the natural space (0.25 em in
            // Liberation Serif), for each engine: the typography numbers.
            let doc = layout_contract::from_model::build(&letter_odt(100, true).doc);
            let paras = doc.paras();
            for eng in ["own-greedy", "own", "own-shrink", "typst", "typst-compat"] {
                let pages = make(eng).layout(&doc, &EditHint::Full).clone();
                let mut ratios = Vec::new();
                for p in &pages.pages {
                    for l in &p.lines {
                        let para = paras[l.para];
                        if para.style.align != layout_contract::input::Align::Justify
                            || l.range.end >= para.char_len()
                        {
                            continue;
                        }
                        let chars: Vec<char> = para
                            .text
                            .chars()
                            .skip(l.range.start)
                            .take(l.range.len())
                            .collect();
                        let last = chars.iter().rposition(|c| !c.is_whitespace()).unwrap_or(0);
                        for (i, c) in chars.iter().enumerate().take(last) {
                            if *c == ' ' {
                                let w = l.carets[i + 1] - l.carets[i];
                                ratios.push(w / (0.25 * para.style.size));
                            }
                        }
                    }
                }
                ratios.sort_by(|a, b| a.total_cmp(b));
                let pct = |q: f32| ratios[((ratios.len() - 1) as f32 * q) as usize];
                let mean = ratios.iter().sum::<f32>() / ratios.len() as f32;
                let loose = ratios.iter().filter(|r| **r > 2.0).count();
                println!(
                    "{eng}: {} spaces on justified lines; width / natural: min {:.2}, mean {:.2}, p95 {:.2}, max {:.2}; wider than 2x: {loose}",
                    ratios.len(),
                    pct(0.0),
                    mean,
                    pct(0.95),
                    pct(1.0)
                );
            }
        }
        Some("diverge") => {
            let mut doc = letter(100);
            let mut e = layout_own::OwnEngine::new();
            let pages = e.layout(&doc, &EditHint::Full).clone();
            let para = layout_contract::target(&doc, &pages, 0, false);
            let mut offset = doc.paras()[para].char_len() / 2;
            for i in 0..300 {
                doc.para_mut(para).unwrap().insert(offset, 'x');
                offset += 1;
                let inc = e.layout(&doc, &EditHint::Para(para)).clone();
                let full = layout_own::OwnEngine::new()
                    .layout(&doc, &EditHint::Full)
                    .clone();
                if inc != full {
                    println!(
                        "diverged after keystroke {i}: {} vs {} pages",
                        inc.pages.len(),
                        full.pages.len()
                    );
                    for (pi, (a, b)) in inc.pages.iter().zip(&full.pages).enumerate() {
                        if a != b {
                            println!("page {pi}: {} vs {} lines", a.lines.len(), b.lines.len());
                            for (la, lb) in a.lines.iter().zip(&b.lines) {
                                if la != lb {
                                    println!(
                                        "  inc  para {} y {} range {:?}\n  full para {} y {} range {:?}",
                                        la.para, la.y, la.range, lb.para, lb.y, lb.range
                                    );
                                    break;
                                }
                            }
                            break;
                        }
                    }
                    break;
                }
            }
        }
        Some("count") => {
            for n in [1, 3, 10, 30, 100] {
                let doc = letter(n);
                let mut e = layout_own::OwnEngine::new();
                let pages = e.layout(&doc, &EditHint::Full);
                let lines: usize = pages.pages.iter().map(|p| p.lines.len()).sum();
                println!(
                    "own: {n} copies: {} pages, {lines} lines, {} paragraphs",
                    pages.pages.len(),
                    doc.paras().len()
                );
                let mut e = layout_typst::TypstEngine::new();
                let t = Instant::now();
                let pages = e.layout(&doc, &EditHint::Full);
                let lines: usize = pages.pages.iter().map(|p| p.lines.len()).sum();
                println!(
                    "typst: {n} copies: {} pages, {lines} lines ({:.0} ms)",
                    pages.pages.len(),
                    t.elapsed().as_secs_f64() * 1e3
                );
            }
        }
        _ => {
            let doc = letter(100);
            let t = Instant::now();
            let mut e = layout_own::OwnEngine::new();
            let pages = e.layout(&doc, &EditHint::Full);
            println!(
                "own: {} pages, {} paragraphs, full layout {:.1} ms",
                pages.pages.len(),
                doc.paras().len(),
                t.elapsed().as_secs_f64() * 1e3
            );
        }
    }
}
