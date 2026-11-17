// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared test helpers: corpus access, and writing ODT from CRDT state with
//! the fidelity check that nothing unedited was damaged.

#![allow(dead_code)]

use std::path::PathBuf;

use skriv_crdt::codec::{self, Baseline};
use skriv_crdt::logical::LDoc;
use skriv_fidelity::structural::{ModeResult, compare_edited};
use skriv_model::PartKind;

/// Run a generic test body against every enabled backend.
#[macro_export]
macro_rules! each_backend {
    ($($name:ident),* $(,)?) => {
        $(
            mod $name {
                #[cfg(feature = "loro")]
                #[test]
                fn loro() {
                    super::$name::<skriv_crdt::loro_backend::LoroDocument>()
                }
                #[cfg(feature = "automerge")]
                #[test]
                fn automerge() {
                    super::$name::<skriv_crdt::automerge_backend::AutomergeDocument>()
                }
            }
        )*
    };
}

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn corpus(name: &str) -> Vec<u8> {
    std::fs::read(root().join("fidelity/corpus/generated").join(name)).expect("corpus file")
}

pub fn corpus_names() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root().join("fidelity/corpus/generated"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".odt"))
        .collect();
    v.sort();
    v
}

pub fn import(bytes: &[u8]) -> (LDoc, Baseline) {
    let odt = skriv_odf::read(bytes.to_vec()).expect("read odt");
    codec::import(&odt.doc)
}

pub struct Checked {
    pub result: ModeResult,
    pub bytes: Vec<u8>,
    pub touched: usize,
    pub regenerated: usize,
    pub reused: usize,
}

impl Checked {
    pub fn untouched(&self) -> (usize, usize) {
        let mut ok = 0;
        let mut total = 0;
        for t in self.result.untouched.values() {
            ok += t.ok;
            total += t.total;
        }
        (ok, total)
    }

    pub fn canonical(&self) -> (usize, usize) {
        let mut ok = 0;
        let mut total = 0;
        for t in self.result.canonical.values() {
            ok += t.ok;
            total += t.total;
        }
        (ok, total)
    }

    /// Nothing that wasn't edited changed, byte for byte or canonically.
    pub fn assert_undamaged(&self, what: &str) {
        let (ok, total) = self.untouched();
        let (cok, ctotal) = self.canonical();
        assert!(
            ok == total && cok == ctotal && total > 0,
            "{what}: untouched {ok}/{total}, canonical {cok}/{ctotal}; notes: {:#?}",
            self.result.notes
        );
    }
}

/// Write `l` (the CRDT's state) as ODT over the source file, and check that
/// every element the edits didn't touch survived.
pub fn write_and_check(source: &[u8], l: &LDoc, base: &Baseline) -> Checked {
    let mut odt = skriv_odf::read(source.to_vec()).expect("read odt");
    let originals: Vec<(PartKind, String)> = [PartKind::Content, PartKind::Styles, PartKind::Meta]
        .into_iter()
        .filter_map(|k| odt.source(k).map(|s| (k, s.to_owned())))
        .collect();
    let ex = codec::export(l, base);
    let mut touched = ex.touched.clone();
    if ex.styles_added > 0 {
        // New automatic styles were appended: their container changed.
        let src = &originals[0].1;
        if let Some(start) = src.find("<office:automatic-styles") {
            touched.push(skriv_model::Origin {
                part: PartKind::Content,
                start,
                end: start + 1,
                start_tag_end: start + 1,
            });
        }
    }
    odt.doc = ex.doc;
    let w = odt.write().expect("write odt");
    // The result must itself read back.
    skriv_odf::read(w.bytes.clone()).expect("written file reads back");
    let result = compare_edited(&originals, &w, &touched);
    Checked {
        result,
        bytes: w.bytes,
        touched: touched.len(),
        regenerated: ex.regenerated,
        reused: ex.reused,
    }
}

/// Keep written files for the CLI harness and for people to open.
pub fn keep(name: &str, bytes: &[u8]) {
    let dir = root().join("target/spike3-odt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
}

/// Every element of the original body, canonically, found in `out`'s body?
/// Returns (found, total) per part, ignoring automatic-style names.
pub fn canonical_survival(orig: &str, out: &str) -> (usize, usize, Vec<String>) {
    use skriv_fidelity::canon::{auto_styles, canon};
    use std::collections::HashMap;
    let co = canon(orig, &auto_styles(orig)).unwrap();
    let cw = canon(out, &auto_styles(out)).unwrap();
    let mut pool: HashMap<u64, usize> = HashMap::new();
    for e in &cw.elems {
        *pool.entry(e.hash).or_insert(0) += 1;
    }
    let (mut ok, mut total, mut lost) = (0, 0, Vec::new());
    // Only body content: inside office:text.
    let body_start = orig.find("<office:text").unwrap_or(0);
    for e in &co.elems {
        if e.start < body_start {
            continue;
        }
        total += 1;
        match pool.get_mut(&e.hash) {
            Some(n) if *n > 0 => {
                *n -= 1;
                ok += 1;
            }
            _ => {
                if lost.len() < 8 {
                    lost.push(orig[e.start..e.end].chars().take(200).collect());
                }
            }
        }
    }
    (ok, total, lost)
}
