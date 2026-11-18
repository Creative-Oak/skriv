// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 3 measurements. Run in release:
//!
//! ```text
//! cargo run --release -p skriv-crdt --example spike3 -- history [sessions] [ops]
//! ```
//!
//! `history`: one year of editing, as 250 sessions × 400 operations by five
//! authors taking turns. Each session loads the saved document as a new
//! peer, edits, and saves. Reports stored size against the document's ODT
//! size, load time, memory after load (resident set of a child process),
//! and for Loro the import of a snapshot against replaying the updates.
//!
//! `load <lib> <file>` is the child process for the memory measurement.

use std::path::{Path, PathBuf};
use std::time::Instant;

use skriv_crdt::CrdtDoc;
use skriv_crdt::automerge_backend::AutomergeDocument;
use skriv_crdt::codec;
use skriv_crdt::loro_backend::LoroDocument;
use skriv_crdt::loro_flow_backend::LoroFlowDocument;
use skriv_crdt::workload::{EditStats, Editor};

const AUTHORS: &[&str] = &["Anne", "Bo", "Cecilie", "Dorte", "Emil"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn out_dir() -> PathBuf {
    let d = root().join("target/spike3");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn letter(copies: usize) -> Vec<u8> {
    let src = std::fs::read(root().join("fidelity/corpus/generated/danish-municipal-letter.odt"))
        .unwrap();
    if copies == 0 {
        return src;
    }
    let mut odt = skriv_odf::read(src).unwrap();
    odt.doc = skriv_crdt::workload::scale(&odt.doc, copies);
    odt.write().unwrap().bytes
}

fn rss_kb() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (get("VmRSS:"), get("VmHWM:"))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

struct Report {
    lib: &'static str,
    doc: String,
    sessions: usize,
    ops: usize,
    seconds: f64,
    stored: usize,
    odt_start: usize,
    odt_end: usize,
    xml_end: usize,
    load_ms: f64,
    read_ms: f64,
    rss_kb: u64,
    rss_base_kb: u64,
    stats: EditStats,
    extra: Vec<String>,
}

fn history<D: CrdtDoc>(
    name: &str,
    copies: usize,
    steady: bool,
    sessions: usize,
    ops: usize,
) -> Report {
    let odt_bytes = letter(copies);
    let odt = skriv_odf::read(odt_bytes.clone()).unwrap();
    let (l, base) = codec::import(&odt.doc);
    let mut d = D::create(&l, 1);
    let mut saved = d.save();
    let mut total = EditStats::default();
    let t = Instant::now();
    for s in 0..sessions {
        let mut doc = D::load(&saved, 1000 + s as u64);
        doc.set_author(AUTHORS[s % AUTHORS.len()]);
        let cur = doc.read();
        let mut ed = Editor::new(s as u64, &cur, &format!("s{s}"));
        ed.steady = steady;
        for i in 0..ops {
            ed.step(&mut doc);
            if i % 20 == 19 {
                doc.commit();
            }
        }
        doc.commit();
        saved = doc.save();
        let st = &ed.stats;
        total.inserts += st.inserts;
        total.deletes += st.deletes;
        total.formats += st.formats;
        total.splits += st.splits;
        total.para_props += st.para_props;
        total.cell_edits += st.cell_edits;
        total.inserted_chars += st.inserted_chars;
        total.deleted_chars += st.deleted_chars;
        if s % 50 == 49 {
            eprintln!(
                "  {} {name}: session {} of {sessions}, {} bytes",
                D::NAME,
                s + 1,
                saved.len()
            );
        }
    }
    let seconds = t.elapsed().as_secs_f64();
    let slug: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let file = out_dir().join(format!("history-{}-{slug}.bin", D::NAME));
    std::fs::write(&file, &saved).unwrap();

    // Load time: median of five.
    let mut loads = Vec::new();
    let mut reads = Vec::new();
    let mut last = None;
    for _ in 0..5 {
        let t = Instant::now();
        let doc = D::load(&saved, 9);
        loads.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        let l = doc.read();
        reads.push(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(l);
    }
    let l = last.unwrap();
    // The document at the end of the year, as ODT.
    let mut out = skriv_odf::read(odt_bytes.clone()).unwrap();
    out.doc = codec::export(&l, &base).doc;
    let w = out.write().unwrap();
    std::fs::write(
        out_dir().join(format!("history-{}-{slug}.odt", D::NAME)),
        &w.bytes,
    )
    .unwrap();
    let xml_end = w.content_xml.len() + w.styles_xml.as_ref().map_or(0, |s| s.len());

    // Memory: a fresh process that only loads the file.
    let exe = std::env::current_exe().unwrap();
    let child = std::process::Command::new(exe)
        .args(["load", D::NAME, file.to_str().unwrap()])
        .output()
        .unwrap();
    let txt = String::from_utf8_lossy(&child.stdout);
    let nums: Vec<u64> = txt
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    let (rss_base_kb, rss_kb) = (
        nums.first().copied().unwrap_or(0),
        nums.get(1).copied().unwrap_or(0),
    );

    let mut extra = Vec::new();
    if D::NAME.starts_with("loro") {
        extra.extend(loro_extra(&saved));
    } else {
        extra.extend(automerge_extra(&saved));
    }
    Report {
        lib: D::NAME,
        doc: name.to_owned(),
        sessions,
        ops,
        seconds,
        stored: saved.len(),
        odt_start: odt_bytes.len(),
        odt_end: w.bytes.len(),
        xml_end,
        load_ms: median(loads),
        read_ms: median(reads),
        rss_kb,
        rss_base_kb,
        stats: total,
        extra,
    }
}

/// Loro: snapshot vs updates-only import (the report's "10–100× faster
/// import" claim), and the shallow snapshot that drops history.
fn loro_extra(saved: &[u8]) -> Vec<String> {
    use loro::{ExportMode, LoroDoc};
    let doc = LoroDoc::new();
    doc.import(saved).unwrap();
    let updates = doc.export(ExportMode::all_updates()).unwrap();
    let shallow = doc
        .export(ExportMode::shallow_snapshot(&doc.oplog_frontiers()))
        .unwrap();
    let state = doc.export(ExportMode::StateOnly(None)).unwrap();
    let time = |bytes: &[u8]| {
        let mut v = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            let d = LoroDoc::new();
            d.import(bytes).unwrap();
            let _ = d.get_deep_value();
            v.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        median(v)
    };
    let (ts, tu, tsh) = (time(saved), time(&updates), time(&shallow));
    vec![
        format!("snapshot {} bytes, import+state {ts:.1} ms", saved.len()),
        format!(
            "updates only {} bytes, import+state {tu:.1} ms ({:.1}× the snapshot)",
            updates.len(),
            tu / ts
        ),
        format!(
            "shallow snapshot (no history) {} bytes, import+state {tsh:.1} ms",
            shallow.len()
        ),
        format!("state only {} bytes", state.len()),
    ]
}

fn automerge_extra(saved: &[u8]) -> Vec<String> {
    let mut d = automerge::AutoCommit::load(saved).unwrap();
    let changes = d.get_changes(&[]).len();
    let uncompressed = d.save_nocompress().len();
    vec![format!(
        "{changes} changes; saved without compression {uncompressed} bytes"
    )]
}

fn print(r: &Report) {
    let kb = |b: usize| b as f64 / 1024.0;
    println!("### {} — {}", r.lib, r.doc);
    println!();
    println!("| Measure | Value |");
    println!("|---|---|");
    println!(
        "| Sessions × operations | {} × {} = {} |",
        r.sessions,
        r.ops,
        r.sessions * r.ops
    );
    println!(
        "| Edits | {} inserts ({} chars), {} deletes ({} chars), {} formats, {} splits, {} paragraph properties, {} cell edits |",
        r.stats.inserts,
        r.stats.inserted_chars,
        r.stats.deletes,
        r.stats.deleted_chars,
        r.stats.formats,
        r.stats.splits,
        r.stats.para_props,
        r.stats.cell_edits
    );
    println!("| Time to replay the year | {:.1} s |", r.seconds);
    println!("| ODT at the start | {:.1} KiB |", kb(r.odt_start));
    println!(
        "| ODT at the end | {:.1} KiB (content.xml + styles.xml {:.1} KiB) |",
        kb(r.odt_end),
        kb(r.xml_end)
    );
    println!(
        "| CRDT stored, history included | {:.1} KiB |",
        kb(r.stored)
    );
    println!(
        "| **Stored ÷ ODT at the end** | **{:.1}×** (÷ uncompressed XML: {:.2}×) |",
        r.stored as f64 / r.odt_end as f64,
        r.stored as f64 / r.xml_end as f64
    );
    println!("| Load (median of 5) | {:.1} ms |", r.load_ms);
    println!(
        "| Read the whole document after load | {:.1} ms |",
        r.read_ms
    );
    println!(
        "| Resident memory after load | {:.1} MiB (process baseline {:.1} MiB) |",
        r.rss_kb as f64 / 1024.0,
        r.rss_base_kb as f64 / 1024.0
    );
    for e in &r.extra {
        println!("| | {e} |");
    }
    println!();
}

fn load_child(lib: &str, file: &Path) {
    let bytes = std::fs::read(file).unwrap();
    let (base, _) = rss_kb();
    match lib {
        "loro" => {
            let d = LoroDocument::load(&bytes, 1);
            let _ = d.read();
            let (now, _) = rss_kb();
            println!("{base} {now}");
            std::hint::black_box(d);
        }
        "loro-flow" => {
            let d = LoroFlowDocument::load(&bytes, 1);
            let _ = d.read();
            let (now, _) = rss_kb();
            println!("{base} {now}");
            std::hint::black_box(d);
        }
        _ => {
            let d = AutomergeDocument::load(&bytes, 1);
            let _ = d.read();
            let (now, _) = rss_kb();
            println!("{base} {now}");
            std::hint::black_box(d);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("load") => load_child(&args[2], Path::new(&args[3])),
        Some("create") => {
            let copies = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(100);
            let bytes = letter(copies);
            let t = Instant::now();
            let odt = skriv_odf::read(bytes.clone()).unwrap();
            let (l, _) = codec::import(&odt.doc);
            println!(
                "read + import: {:.1} ms",
                t.elapsed().as_secs_f64() * 1000.0
            );
            fn go<D: CrdtDoc>(l: &skriv_crdt::logical::LDoc) {
                let t = Instant::now();
                let mut d = D::create(l, 1);
                let c = t.elapsed().as_secs_f64() * 1000.0;
                let t = Instant::now();
                let _ = d.read();
                let r = t.elapsed().as_secs_f64() * 1000.0;
                let t = Instant::now();
                let n = d.save().len();
                let s = t.elapsed().as_secs_f64() * 1000.0;
                let t = Instant::now();
                let _ = D::load(&d.save(), 2);
                let ld = t.elapsed().as_secs_f64() * 1000.0;
                println!(
                    "{}: create {c:.1} ms, read {r:.1} ms, save {s:.1} ms ({n} bytes), load {ld:.1} ms",
                    D::NAME
                );
            }
            go::<LoroDocument>(&l);
            go::<LoroFlowDocument>(&l);
            go::<AutomergeDocument>(&l);
        }
        Some("history") => {
            let sessions = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(250);
            let ops = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(400);
            let which = args.get(4).cloned().unwrap_or_else(|| "both".into());
            for (name, copies, steady) in [
                ("letter, growing (2 pages at the start)", 0usize, false),
                ("letter, steady size (2 pages)", 0, true),
                ("100 pages, steady size", 100, true),
            ] {
                if which != "both" && !name.contains(&which) {
                    continue;
                }
                let libs = std::env::var("SKRIV_LIBS")
                    .unwrap_or_else(|_| "loro,loro-flow,automerge".into());
                for lib in libs.split(',') {
                    let r = match lib {
                        "loro" => history::<LoroDocument>(name, copies, steady, sessions, ops),
                        "loro-flow" => {
                            history::<LoroFlowDocument>(name, copies, steady, sessions, ops)
                        }
                        _ => history::<AutomergeDocument>(name, copies, steady, sessions, ops),
                    };
                    print(&r);
                }
            }
        }
        _ => eprintln!(
            "usage: spike3 history [sessions] [ops] [growing|steady|100] | create [copies] | load <lib> <file>"
        ),
    }
}
