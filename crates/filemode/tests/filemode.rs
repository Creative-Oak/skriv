// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! File mode without LibreOffice: state in the file, shallow snapshots, and
//! the import pipeline on files that lost what LibreOffice drops.
//! (`fidelity/scripts/spike4.sh` runs the same pipeline through LibreOffice.)

use std::collections::HashMap;
use std::path::PathBuf;

use skriv_crdt::CrdtDoc;
use skriv_crdt::codec;
use skriv_crdt::logical::{LDoc, LNode};
use skriv_crdt::loro_flow_backend::LoroFlowDocument;
use skriv_filemode::scenario::{self, Op, Scenario};
use skriv_filemode::state::{self, SnapshotKind, State};
use skriv_filemode::units::units;
use skriv_filemode::{align, apply, noise};

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fidelity/corpus/generated");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "odt"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(&p).unwrap(),
            )
        })
        .collect();
    v.sort();
    v
}

fn clean(l: &LDoc) -> LDoc {
    let mut l = l.clone();
    noise::clean(&mut l);
    l.normalize();
    l
}

#[test]
fn state_survives_our_own_round_trip() {
    for (name, src) in corpus() {
        let odt = skriv_odf::read(src.clone()).unwrap();
        let (l, base) = codec::import(&odt.doc);
        let mut d = LoroFlowDocument::create(&l, 1);
        let version = d.version();
        let snap = d.save_shallow();
        let st = State {
            doc_id: "sk-doc-test".into(),
            version: version.clone(),
            kind: SnapshotKind::Shallow,
            snapshot: Some(snap),
        };
        let bytes = state::write(&src, &d.read(), &base, &st).unwrap();
        // Still an ODT we read, with the part listed in the manifest.
        let back = skriv_odf::read(bytes.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let found = state::read(&bytes).unwrap();
        assert_eq!(found.meta_doc_id.as_deref(), Some("sk-doc-test"), "{name}");
        assert_eq!(
            found.meta_version.as_deref(),
            Some(version.as_slice()),
            "{name}"
        );
        assert_eq!(found.state.as_ref(), Some(&st), "{name}");
        // The snapshot is the document.
        let from_file = LoroFlowDocument::load(found.state.unwrap().snapshot.as_ref().unwrap(), 2);
        assert_eq!(from_file.read(), d.read(), "{name}");
        // Every paragraph in the file carries its skriv ID.
        let (rl, _) = codec::import(&back.doc);
        let want: Vec<String> = d.read().paras().iter().map(|p| p.id.clone()).collect();
        let got: Vec<String> = rl.paras().iter().map(|p| p.id.clone()).collect();
        assert_eq!(got, want, "{name}");
        // Writing the state again replaces it rather than adding a second part.
        let again = state::embed(&bytes, &st).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&again[..])).unwrap();
        let n = (0..z.len())
            .filter(|i| z.by_index(*i).unwrap().name().unwrap() == state::PART)
            .count();
        assert_eq!(n, 1, "{name}");
    }
}

#[test]
fn shallow_snapshot_keeps_history_out_and_still_merges() {
    let (_, src) = corpus()
        .into_iter()
        .find(|(n, _)| n.starts_with("danish"))
        .unwrap();
    let (l, _) = codec::import(&skriv_odf::read(src).unwrap().doc);
    let mut d = LoroFlowDocument::create(&l, 1);
    let pid = l.paras().iter().find(|p| p.len() > 20).unwrap().id.clone();
    d.insert_text(&pid, 0, "HEMMELIGT ");
    d.commit();
    d.delete_text(&pid, 0, "HEMMELIGT ".len());
    d.commit();
    let version = d.version();
    let shallow = d.save_shallow();
    let full = d.save();
    let has = |bytes: &[u8]| {
        let doc = loro::LoroDoc::new();
        doc.import(bytes).unwrap();
        let json = doc
            .export_json_updates(&Default::default(), &doc.oplog_vv())
            .unwrap();
        serde_json::to_string(&json).unwrap().contains("HEMMELIGT")
    };
    assert!(has(&full), "the full snapshot carries deleted text");
    assert!(!has(&shallow), "the shallow snapshot does not");

    // A branch made from the shallow snapshot merges into the full document,
    // after an edit there in the meantime.
    let mut branch = LoroFlowDocument::load(&shallow, 2);
    branch.set_author("Lis");
    branch.insert_text(&pid, 0, "Fra filen. ");
    branch.commit();
    d.insert_text(&pid, 0, "Samtidig. ");
    d.commit();
    let upd = branch.updates_since(&version).unwrap();
    assert!(d.import_updates(&upd));
    let text = d.para(&pid).unwrap().text();
    assert!(
        text.contains("Fra filen. ") && text.contains("Samtidig. "),
        "{text}"
    );
    assert_eq!(
        d.author_at(&pid, text.find("Fra").unwrap()).as_deref(),
        Some("Lis")
    );
}

/// What LibreOffice does to IDs: tables, rows, columns, cells, lists and
/// list items lose theirs (the codec mints new ones); paragraphs and
/// sections keep theirs. Paragraphs new to the file get fresh IDs.
fn lose_ids(l: &LDoc, known: &std::collections::HashSet<String>) -> LDoc {
    let mut n = 0;
    let mut fresh = |_: &str| {
        n += 1;
        format!("lo-{n}")
    };
    fn walk(
        v: &mut [LNode],
        known: &std::collections::HashSet<String>,
        fresh: &mut dyn FnMut(&str) -> String,
    ) {
        for node in v {
            match node {
                LNode::Para(p) => {
                    if !known.contains(&p.id) {
                        p.id = fresh("p");
                    }
                }
                LNode::Embed { .. } => {}
                LNode::Container {
                    id, kind, children, ..
                } => {
                    if kind != "section" || !known.contains(id.as_str()) {
                        *id = fresh("c");
                    }
                    walk(children, known, fresh);
                }
                LNode::Table(t) => {
                    t.id = fresh("t");
                    let mut rows: HashMap<String, String> = HashMap::new();
                    for (r, _) in &mut t.rows {
                        let f = fresh("r");
                        rows.insert(r.clone(), f.clone());
                        *r = f;
                    }
                    for (c, _) in &mut t.cols {
                        *c = fresh("c");
                    }
                    for row in &mut t.cells {
                        for c in row {
                            c.id = fresh("cell");
                            walk(&mut c.blocks, known, fresh);
                        }
                    }
                }
            }
        }
    }
    let mut out = l.clone();
    walk(&mut out.body, known, &mut fresh);
    out
}

#[test]
fn returned_files_become_exact_branches() {
    let mut checked = 0;
    for (name, src) in corpus() {
        let (l, _) = codec::import(&skriv_odf::read(src).unwrap().doc);
        let mut server = LoroFlowDocument::create(&l, 1);
        server.set_author("skriv");
        let version = server.version();
        let base = server.read();
        let known: std::collections::HashSet<String> = base
            .paras()
            .iter()
            .map(|p| p.id.clone())
            .chain(sections(&base))
            .collect();
        let mut scs = scenario::scenarios(&base);
        scs.push(Scenario {
            name: "no-edit".into(),
            ops: vec![],
            track: false,
        });
        for sc in scs {
            // The person's edits, made in skriv: what the branch must become.
            let mut expected = server.fork_at(&version, 7).unwrap();
            expected.set_author("Lis");
            let failed = scenario::reenact(&mut expected, &sc);
            assert!(failed.is_empty(), "{name} {}: {failed:?}", sc.name);
            let want = expected.read();
            // The returned file: the same document without the IDs
            // LibreOffice drops.
            let mut ret = lose_ids(&want, &known);
            let mut branch = server.fork_at(&version, 8).unwrap();
            branch.set_author("Lis");
            align::align(&base, &mut ret);
            let st = apply::apply(&mut branch, &base, &ret);
            assert!(
                st.unsupported.is_empty(),
                "{name} {}: {:?}",
                sc.name,
                st.unsupported
            );
            let got = branch.read();
            let mut aligned = clean(&got);
            align::align(&clean(&want), &mut aligned);
            let u = units(&clean(&want), &aligned);
            assert_eq!(u.total(), 0, "{name} {}: {u:?}", sc.name);
            // Text the person typed is theirs; nobody else's text became theirs.
            for op in &sc.ops {
                if let Op::Insert { text, .. } = op {
                    let p = got
                        .paras()
                        .into_iter()
                        .find(|p| p.text().contains(text.as_str()))
                        .unwrap();
                    let at = p.text().find(text.as_str()).unwrap();
                    let at = p.text()[..at].chars().count();
                    // Every letter; a space or comma at the edge of an edit
                    // can match the original's equally well (a tie).
                    for (i, c) in text.chars().enumerate() {
                        if c.is_alphanumeric() {
                            assert_eq!(
                                branch.author_at(&p.id, at + i).as_deref(),
                                Some("Lis"),
                                "{name} {}",
                                sc.name
                            );
                        }
                    }
                }
            }
            checked += 1;
        }
    }
    assert!(checked >= 40, "only {checked} scenarios ran");
}

fn sections(l: &LDoc) -> Vec<String> {
    fn walk(v: &[LNode], out: &mut Vec<String>) {
        for n in v {
            if let LNode::Container {
                id, kind, children, ..
            } = n
            {
                if kind == "section" {
                    out.push(id.clone());
                }
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&l.body, &mut out);
    out
}
