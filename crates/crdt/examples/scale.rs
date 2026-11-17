// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scale the Danish municipal letter: `cargo run --example scale -- <copies> <out.odt>`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let copies: usize = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(70);
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "letter-scaled.odt".into());
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let src =
        std::fs::read(root.join("fidelity/corpus/generated/danish-municipal-letter.odt")).unwrap();
    let mut odt = skriv_odf::read(src).unwrap();
    odt.doc = skriv_crdt::workload::scale(&odt.doc, copies);
    let w = odt.write().unwrap();
    std::fs::write(&out, &w.bytes).unwrap();
    println!(
        "{out}: {} bytes, content.xml {} bytes",
        w.bytes.len(),
        w.content_xml.len()
    );
}
