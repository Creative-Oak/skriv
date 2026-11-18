// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The 100-page test document: a copy of `skriv_crdt::workload::scale`
//! (Spike 3), so this spike doesn't depend on that unmerged branch.

use skriv_model::{Block, Document, NodeId};

/// SplitMix64: small, fast, and the same everywhere.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }
}

/// Repeat a document's body `copies` times, each copy in its own named
/// section, with fresh IDs and unique table, note and bookmark names. The
/// first block (LibreOffice's sequence declarations) and the trailing empty
/// paragraph are kept once.
pub fn scale(doc: &Document, copies: usize) -> Document {
    let mut out = doc.clone();
    let (head, rest) = doc.body.split_at(1.min(doc.body.len()));
    let (middle, tail) = match rest.last() {
        Some(Block::Paragraph(p)) if p.content.is_empty() => rest.split_at(rest.len() - 1),
        _ => (rest, &[][..]),
    };
    let template = serde_json::to_string(middle).expect("serialise");
    let mut body = head.to_vec();
    for i in 0..copies {
        let json = template
            .replace("\"Table1\"", &format!("\"Table{}\"", i + 1))
            .replace("ftn0", &format!("ftn{i}"));
        // Text runs carry their own source origin (outside NodeMeta); clear
        // every origin, or the writer copies the source spelling and the
        // shuffle below has no effect. (This was the case in Spike 3's copy
        // of this function: its 100 copies are identical.)
        let mut value: serde_json::Value = serde_json::from_str(&json).expect("json");
        fn clear(v: &mut serde_json::Value) {
            match v {
                serde_json::Value::Object(m) => {
                    if let Some(o) = m.get_mut("origin") {
                        *o = serde_json::Value::Null;
                    }
                    m.values_mut().for_each(clear);
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(clear),
                _ => {}
            }
        }
        clear(&mut value);
        let blocks: Vec<Block> = serde_json::from_value(value).expect("deserialise");
        let mut section = Document {
            body: blocks,
            ..Default::default()
        };
        // Vary the wording per copy (a seeded shuffle of each text run's
        // words), so the copies don't compress to nothing in the zip.
        let mut rng = Rng::new(i as u64);
        for p in skriv_model::paragraphs_mut(&mut section) {
            for inline in &mut p.content {
                if let skriv_model::Inline::Text(t) = inline {
                    let mut words: Vec<&str> = t.text.split(' ').collect();
                    for k in (1..words.len()).rev() {
                        words.swap(k, rng.below(k + 1));
                    }
                    t.text = words.join(" ");
                }
            }
        }
        skriv_model::for_each_node_mut(&mut section, &mut |_, m| {
            m.id = NodeId::mint();
            m.id_from_source = false;
            m.origin = None;
            m.dirty = true;
        });
        let mut attrs = skriv_model::Attrs::default();
        attrs.set("text:name", format!("Afsnit {}", i + 1));
        body.push(Block::Section(skriv_model::Section {
            meta: skriv_model::NodeMeta::new(),
            attrs,
            blocks: section.body,
        }));
    }
    body.extend(tail.iter().cloned());
    out.body = body;
    out
}
