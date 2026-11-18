// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 4's experiment driver.
//!
//! ```text
//! spike4 prepare <corpus dir> <work dir>   # files with skriv state + jobs.json
//! python3 fidelity/scripts/spike4_lo.py <work dir>   # LibreOffice edits and saves them
//! spike4 evaluate <work dir>               # import every returned file, measure
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use skriv_crdt::CrdtDoc;
use skriv_crdt::codec;
use skriv_crdt::logical::{LDoc, LNode};
use skriv_crdt::loro_flow_backend::LoroFlowDocument;
use skriv_filemode::scenario::{self, Scenario};
use skriv_filemode::state::{self, SnapshotKind, State};
use skriv_filemode::units::{Detail, Units, units, units_detail};
use skriv_filemode::{align, apply, noise, tracked};

const SKRIV_AUTHOR: &str = "Skriv-bruger";
/// The name the LibreOffice profile is given (`spike4_lo.py`).
const LO_AUTHOR: &str = "Lis Larsen";
const SECRET: &str = "HEMMELIGT UDKAST ";
const CONCURRENT: &str = "Samtidig rettelse i skriv. ";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    doc: String,
    scenario: Scenario,
    /// `store` (UNO edit and save), `convert` (soffice --convert-to),
    /// `docx` (edit, save as DOCX, reopen, save as ODT).
    save: String,
    #[serde(rename = "in")]
    input: String,
    out: String,
}

fn job_name(j: &Job) -> String {
    format!("{}__{}__{}", j.doc, j.scenario.name, j.save)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("prepare") if args.len() >= 4 => {
            let dirs: Vec<&Path> = args[2..args.len() - 1].iter().map(Path::new).collect();
            prepare(&dirs, Path::new(&args[args.len() - 1]))
        }
        Some("evaluate") if args.len() >= 3 => evaluate(Path::new(&args[2])),
        Some("dump") if args.len() >= 3 => {
            // The logical document of a file, as JSON-ish debug output.
            let odt = skriv_odf::read(std::fs::read(&args[2]).expect("read")).expect("odt");
            let (mut l, _) = codec::import(&odt.doc);
            if args.get(3).is_some_and(|a| a == "--clean") {
                noise::clean(&mut l);
            }
            for t in l.tables() {
                println!(
                    "table {} cols {:?}",
                    t.id,
                    t.cols.iter().map(|c| &c.1).collect::<Vec<_>>()
                );
            }
            fn embeds(v: &[LNode]) {
                for n in v {
                    match n {
                        LNode::Embed { json, .. } => {
                            println!("embed {}", &json[..json.len().min(3000)])
                        }
                        LNode::Container { children, .. } => embeds(children),
                        _ => {}
                    }
                }
            }
            embeds(&l.body);
        }
        Some("scale") if args.len() >= 5 => {
            // A long document: `copies` copies of a file, one section each.
            let src = std::fs::read(&args[2]).expect("read");
            let mut odt = skriv_odf::read(src).expect("odt");
            odt.doc = skriv_crdt::workload::scale(&odt.doc, args[3].parse().expect("copies"));
            std::fs::write(&args[4], odt.write().expect("write").bytes).expect("write");
        }
        _ => {
            eprintln!(
                "usage: spike4 prepare <corpus dir>... <work dir> | spike4 evaluate <work dir> | spike4 scale <in.odt> <copies> <out.odt>"
            );
            std::process::exit(2);
        }
    }
}

/// The first plain body paragraph: where the history test types its secret.
fn first_body_para(l: &LDoc) -> Option<String> {
    fn walk(v: &[LNode]) -> Option<String> {
        v.iter().find_map(|n| match n {
            LNode::Para(p) if p.text().chars().count() >= 20 => Some(p.id.clone()),
            LNode::Container { children, .. } => walk(children),
            _ => None,
        })
    }
    walk(&l.body)
}

fn sources(corpus: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(corpus)
        .expect("corpus dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "odt"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

#[derive(Serialize)]
struct Sizes {
    doc: String,
    odt_plain: usize,
    odt_none: usize,
    odt_shallow: usize,
    odt_full: usize,
    shallow_has_secret: bool,
    full_has_secret: bool,
}

fn has_secret(snapshot: &[u8]) -> bool {
    let doc = loro::LoroDoc::new();
    if doc.import(snapshot).is_err() {
        return false;
    }
    let Ok(json) = doc.export_json_updates(&Default::default(), &doc.oplog_vv()) else {
        return false;
    };
    serde_json::to_string(&json)
        .unwrap_or_default()
        .contains(SECRET.trim())
}

fn prepare(corpora: &[&Path], work: &Path) {
    let _ = std::fs::remove_dir_all(work);
    for d in ["prepared", "server", "returned"] {
        std::fs::create_dir_all(work.join(d)).unwrap();
    }
    let mut jobs = Vec::new();
    let mut sizes = Vec::new();
    for (name, src) in corpora.iter().flat_map(|c| sources(c)) {
        let odt = skriv_odf::read(src.clone()).expect("read odt");
        let (l, baseline) = codec::import(&odt.doc);
        let mut d = LoroFlowDocument::create(&l, 1);
        d.set_author(SKRIV_AUTHOR);
        // History: a draft sentence typed and deleted again before the file is sent.
        if let Some(pid) = first_body_para(&l) {
            d.insert_text(&pid, 0, SECRET);
            d.commit();
            d.delete_text(&pid, 0, SECRET.chars().count());
            d.commit();
        }
        let version = d.version();
        let server = d.save();
        std::fs::write(work.join("server").join(format!("{name}.loro")), &server).unwrap();
        let cur = d.read();
        let doc_id = format!("sk-doc-{name}");
        let write = |kind: SnapshotKind, snapshot: Option<Vec<u8>>| {
            let st = State {
                doc_id: doc_id.clone(),
                version: version.clone(),
                kind,
                snapshot,
            };
            state::write(&src, &cur, &baseline, &st).expect("write")
        };
        let shallow = d.save_shallow();
        let full = d.save();
        let f_none = write(SnapshotKind::None, None);
        let f_shallow = write(SnapshotKind::Shallow, Some(shallow.clone()));
        let f_full = write(SnapshotKind::Full, Some(full.clone()));
        let mut plain = skriv_odf::read(src.clone()).unwrap();
        plain.doc = codec::export(&cur, &baseline).doc;
        let plain_len = plain.write().unwrap().bytes.len();
        sizes.push(Sizes {
            doc: name.clone(),
            odt_plain: plain_len,
            odt_none: f_none.len(),
            odt_shallow: f_shallow.len(),
            odt_full: f_full.len(),
            shallow_has_secret: has_secret(&shallow),
            full_has_secret: has_secret(&full),
        });
        let prepared = work.join("prepared").join(format!("{name}.odt"));
        std::fs::write(&prepared, &f_shallow).unwrap();
        std::fs::write(
            work.join("prepared").join(format!("{name}.full.odt")),
            &f_full,
        )
        .unwrap();

        let mut add = |sc: Scenario, save: &str| {
            let mut j = Job {
                doc: name.clone(),
                scenario: sc,
                save: save.into(),
                input: abs(&prepared),
                out: String::new(),
            };
            j.out = abs(&work.join("returned").join(format!("{}.odt", job_name(&j))));
            jobs.push(j);
        };
        let noop = Scenario {
            name: "no-edit".into(),
            ops: vec![],
            track: false,
        };
        add(noop.clone(), "convert");
        add(noop.clone(), "store");
        add(noop, "docx");
        let scs = scenario::scenarios(&cur);
        for sc in scs {
            if sc.name == "type" {
                add(sc.clone(), "docx");
            }
            add(sc, "store");
        }
    }
    std::fs::write(
        work.join("jobs.json"),
        serde_json::to_string_pretty(&jobs).unwrap(),
    )
    .unwrap();
    std::fs::write(
        work.join("sizes.json"),
        serde_json::to_string_pretty(&sizes).unwrap(),
    )
    .unwrap();
    println!("{} jobs for {} documents", jobs.len(), sizes.len());
}

fn abs(p: &Path) -> String {
    std::fs::canonicalize(p.parent().unwrap())
        .unwrap()
        .join(p.file_name().unwrap())
        .to_string_lossy()
        .into_owned()
}

#[derive(Serialize, Default)]
struct Outcome {
    job: String,
    doc: String,
    scenario: String,
    save: String,
    returned: bool,
    doc_id_in_meta: bool,
    version_in_meta: bool,
    state_part: bool,
    snapshot: bool,
    /// Where the starting state came from.
    base_from: String,
    noise: BTreeMap<String, usize>,
    tracked: tracked::TrackedStats,
    change_ids_restored: usize,
    align: Option<align::AlignStats>,
    apply: Option<apply::ApplyStats>,
    reenact_failed: Vec<String>,
    intended: Units,
    got: Units,
    residual: Units,
    residual_detail: Vec<String>,
    /// The branch against the returned file (import errors only).
    file_residual: Units,
    file_residual_detail: Vec<String>,
    /// Characters whose author in the branch matches the same edit made in skriv.
    attribution_ok: usize,
    attribution_total: usize,
    /// The branch merged into skriv's document after a concurrent edit there.
    merge_residual: Option<Units>,
    merge_has_concurrent: bool,
    import_ms: f64,
    /// Exporting the branch over the returned file (re-baselined): nodes
    /// copied verbatim from LibreOffice's file, and nodes regenerated.
    rebase_reused: usize,
    rebase_regenerated: usize,
    /// The same without adopting skriv's spelling of equivalent nodes.
    rebase_plain: (usize, usize),
    rebase_adopted: usize,
    error: Option<String>,
}

fn clean(l: &LDoc) -> LDoc {
    let mut l = l.clone();
    noise::clean(&mut l);
    l.normalize();
    l
}

/// Units between `a` and `b`, after aligning `b` to `a`.
fn distance(a: &LDoc, b: &LDoc, detail: &mut Detail) -> Units {
    let a = clean(a);
    let mut b = clean(b);
    align::align(&a, &mut b);
    units_detail(&a, &b, detail)
}

/// Compare authors character by character, paragraph by paragraph (by
/// aligned ID), on characters both documents have.
fn attribution(
    expected: &LoroFlowDocument,
    branch: &LoroFlowDocument,
    el: &LDoc,
    bl: &LDoc,
) -> (usize, usize) {
    let e = clean(el);
    let mut b = clean(bl);
    // Map aligned IDs back to the branch's own IDs by position in the walk.
    let before: Vec<String> = b.paras().iter().map(|p| p.id.clone()).collect();
    align::align(&e, &mut b);
    let after: Vec<String> = b.paras().iter().map(|p| p.id.clone()).collect();
    let own: BTreeMap<&str, &str> = after
        .iter()
        .map(String::as_str)
        .zip(before.iter().map(String::as_str))
        .collect();
    let (mut ok, mut total) = (0, 0);
    let mut sample: Vec<String> = Vec::new();
    let eparas: BTreeMap<String, String> =
        e.paras().iter().map(|p| (p.id.clone(), p.text())).collect();
    for p in b.paras() {
        let Some(et) = eparas.get(&p.id) else {
            continue;
        };
        let Some(bid) = own.get(p.id.as_str()) else {
            continue;
        };
        let ec: Vec<char> = et.chars().collect();
        let bc: Vec<char> = p.text().chars().collect();
        for op in similar::capture_diff_slices(similar::Algorithm::Myers, &ec, &bc) {
            if let similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } = op
            {
                for i in 0..len {
                    total += 1;
                    let ea = expected.author_at(&p.id, old_index + i);
                    let ba = branch.author_at(bid, new_index + i);
                    if ea == ba {
                        ok += 1;
                    } else if sample.len() < 6 {
                        sample.push(format!(
                            "{:?} in {:?}: expected {ea:?} branch {ba:?}",
                            ec[old_index + i],
                            et.chars().take(30).collect::<String>()
                        ));
                    }
                }
            }
        }
    }
    if !sample.is_empty() && std::env::var_os("SKRIV_DEBUG_ATTR").is_some() {
        eprintln!("ATTR {}", sample.join(" | "));
    }
    (ok, total)
}

fn evaluate(work: &Path) {
    let jobs: Vec<Job> =
        serde_json::from_str(&std::fs::read_to_string(work.join("jobs.json")).unwrap()).unwrap();
    let mut out = Vec::new();
    for (i, j) in jobs.iter().enumerate() {
        let o = evaluate_one(work, j, 1000 + i as u64);
        eprintln!(
            "{:<60} {:>4} intended {:>4} got {:>4} residual{}",
            o.job,
            o.intended.total(),
            o.got.total(),
            o.residual.total(),
            o.error
                .as_deref()
                .map(|e| format!("  ERROR {e}"))
                .unwrap_or_default()
        );
        out.push(o);
    }
    std::fs::write(
        work.join("results.json"),
        serde_json::to_string_pretty(&out).unwrap(),
    )
    .unwrap();
}

fn evaluate_one(work: &Path, j: &Job, peer: u64) -> Outcome {
    let mut o = Outcome {
        job: job_name(j),
        doc: j.doc.clone(),
        scenario: j.scenario.name.clone(),
        save: j.save.clone(),
        ..Default::default()
    };
    let Ok(ret) = std::fs::read(&j.out) else {
        o.error = Some("LibreOffice produced no file".into());
        return o;
    };
    o.returned = true;
    let found = match state::read(&ret) {
        Ok(f) => f,
        Err(e) => {
            o.error = Some(format!("not a package: {e}"));
            return o;
        }
    };
    o.doc_id_in_meta = found.meta_doc_id.is_some();
    o.version_in_meta = found.meta_version.is_some();
    o.state_part = found.state.is_some();
    o.snapshot = found.state.as_ref().is_some_and(|s| s.snapshot.is_some());

    let server_bytes = std::fs::read(work.join("server").join(format!("{}.loro", j.doc))).unwrap();
    let mut server = LoroFlowDocument::load(&server_bytes, 1);
    let version = match (&found.state, &found.meta_version) {
        (Some(s), _) => s.version.clone(),
        (None, Some(v)) => v.clone(),
        (None, None) => server.version(),
    };

    let t0 = Instant::now();
    // The starting state: the file's own snapshot when it has one, else
    // skriv's copy at the version the file names, else (the state part is
    // gone) skriv's copy as it is now, found by the document ID.
    let mut branch = match &found.state {
        Some(State {
            snapshot: Some(snap),
            ..
        }) => {
            o.base_from = "file snapshot".into();
            LoroFlowDocument::load(snap, peer)
        }
        Some(s) => {
            o.base_from = "skriv at the file's version".into();
            server.fork_at(&s.version, peer).expect("fork at version")
        }
        None if found.meta_doc_id.is_some() && found.meta_version.is_some() => {
            o.base_from = "skriv at the version in meta.xml".into();
            match server.fork_at(&version, peer) {
                Some(b) => b,
                None => {
                    o.error = Some("version in meta.xml unknown to skriv".into());
                    return o;
                }
            }
        }
        None if found.meta_doc_id.is_some() => {
            o.base_from = "skriv's current state (document ID only)".into();
            server.fork(peer)
        }
        None => {
            o.base_from = "none: unknown document".into();
            o.error = Some("no skriv identity left in the file".into());
            return o;
        }
    };
    let base = branch.read();
    branch.set_author(LO_AUTHOR);
    let rodt = match skriv_odf::read(ret.clone()) {
        Ok(x) => x,
        Err(e) => {
            o.error = Some(format!("returned file does not read: {e}"));
            return o;
        }
    };
    let (mut rl, _) = codec::import(&rodt.doc);
    o.tracked = tracked::accept_new(&base, &mut rl);
    o.change_ids_restored = tracked::restore_ids(&base, &mut rl);
    o.noise = noise::strip_layout(&mut rl);
    for (k, v) in noise::clean(&mut rl.clone()) {
        *o.noise.entry(k).or_insert(0) += v;
    }
    let file_l = rl.clone();
    let al = align::align(&base, &mut rl);
    let ap = apply::apply(&mut branch, &base, &rl);
    // Re-baseline: the returned file becomes skriv's source file, with its
    // nodes renamed to skriv's IDs; how much of it would the next export
    // copy verbatim?
    let renames: BTreeMap<String, String> = all_ids(&file_l)
        .into_iter()
        .zip(all_ids(&rl))
        .filter(|(a, b)| a != b)
        .collect();
    let mut model = rodt.doc.clone();
    skriv_model::for_each_node_mut(&mut model, &mut |_, m| {
        if let Some(n) = renames.get(&m.id.0) {
            m.id = skriv_model::NodeId(n.clone());
        }
    });
    let (_, mut rebase) = codec::import(&model);
    o.import_ms = t0.elapsed().as_secs_f64() * 1000.0;
    o.align = Some(al);
    o.apply = Some(ap);
    let branch_l = branch.read();

    // The same edits made in skriv.
    let mut expected = server.fork_at(&version, peer + 100_000).expect("fork");
    expected.set_author(LO_AUTHOR);
    o.reenact_failed = scenario::reenact(&mut expected, &j.scenario);
    let expected_l = expected.read();

    o.intended = units(&clean(&base), &clean(&expected_l));
    o.got = units(&clean(&base), &clean(&branch_l));
    let mut det = Detail::default();
    o.residual = distance(&expected_l, &branch_l, &mut det);
    o.residual_detail = det.lines.into_iter().take(20).collect();
    let ex = codec::export(&branch_l, &rebase);
    o.rebase_plain = (ex.reused, ex.regenerated);
    o.rebase_adopted = rebase.adopt(&branch_l, &noise::equivalent);
    let ex = codec::export(&branch_l, &rebase);
    o.rebase_reused = ex.reused;
    o.rebase_regenerated = ex.regenerated;
    // Import fidelity: the branch against the returned file itself.
    let mut fdet = Detail::default();
    o.file_residual = distance(&file_l, &branch_l, &mut fdet);
    o.file_residual_detail = fdet.lines.into_iter().take(10).collect();
    let (ok, total) = attribution(&expected, &branch, &expected_l, &branch_l);
    o.attribution_ok = ok;
    o.attribution_total = total;

    // Merge into skriv's document after someone edited it there meanwhile.
    if let Some(pid) = last_body_para(&server.read()) {
        server.set_author(SKRIV_AUTHOR);
        server.insert_text(&pid, 0, CONCURRENT);
        server.commit();
        let mut want = server.fork(peer + 200_000);
        want.set_author(LO_AUTHOR);
        scenario::reenact(&mut want, &j.scenario);
        let want_l = want.read();
        if let Some(upd) = branch.updates_since(&version) {
            server.import_updates(&upd);
            let merged = server.read();
            o.merge_has_concurrent = merged
                .paras()
                .iter()
                .any(|p| p.text().contains(CONCURRENT.trim()));
            o.merge_residual = Some(distance(&want_l, &merged, &mut Detail::default()));
        }
    }
    o
}

/// Every node ID in walk order (same structure gives the same order).
fn all_ids(l: &LDoc) -> Vec<String> {
    fn walk(v: &[LNode], out: &mut Vec<String>) {
        for n in v {
            out.push(n.id().to_owned());
            match n {
                LNode::Container { children, .. } => walk(children, out),
                LNode::Table(t) => {
                    out.extend(t.rows.iter().map(|r| r.0.clone()));
                    out.extend(t.cols.iter().map(|c| c.0.clone()));
                    for row in &t.cells {
                        for c in row {
                            out.push(c.id.clone());
                            walk(&c.blocks, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&l.body, &mut out);
    out
}

fn last_body_para(l: &LDoc) -> Option<String> {
    l.body.iter().rev().find_map(|n| match n {
        LNode::Para(p) if p.text().chars().count() >= 10 => Some(p.id.clone()),
        _ => None,
    })
}
