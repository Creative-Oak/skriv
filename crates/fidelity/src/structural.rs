// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Structural round-trip measurement for one file.
//!
//! Three modes:
//!
//! - **identity**: read and write with nothing changed.
//! - **touch**: mark every 5th modelled node dirty (no content change) and
//!   write, with regenerated start tags' attributes reversed so every
//!   touched element really differs in bytes. Measures the kill criterion:
//!   are *untouched* elements preserved byte for byte, and are the touched
//!   ones regenerated faithfully (canonically)?
//! - **regen**: mark everything dirty and re-escape all text. Measures how
//!   much of the document the model genuinely understands.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use serde::Serialize;
use skriv_model::{Feature, Origin, PartKind};
use skriv_odf::{WriteOptions, Written};

use crate::canon::{Canon, auto_styles, canon};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Identity,
    Touch,
    Regen,
    /// Negative control: deliberately empties every third non-empty
    /// paragraph. The harness must report this as loss; if it doesn't, the
    /// harness is broken. Never counts towards the kill criteria.
    Canary,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Identity, Mode::Touch, Mode::Regen];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Identity => "identity",
            Mode::Touch => "touch",
            Mode::Regen => "regen",
            Mode::Canary => "canary",
        }
    }
}

/// (preserved, total).
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Tally {
    pub ok: usize,
    pub total: usize,
}

impl Tally {
    pub fn add(&mut self, ok: bool) {
        self.total += 1;
        if ok {
            self.ok += 1;
        }
    }

    pub fn merge(&mut self, o: Tally) {
        self.ok += o.ok;
        self.total += o.total;
    }

    pub fn pct(&self) -> Option<f64> {
        (self.total > 0).then(|| 100.0 * self.ok as f64 / self.total as f64)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ModeResult {
    /// Which of content.xml / styles.xml / meta.xml came out byte-identical.
    pub identical_parts: BTreeMap<String, bool>,
    /// Elements outside any touched node: preserved byte for byte?
    pub untouched: BTreeMap<&'static str, Tally>,
    /// Every element: does its canonical fingerprint (whole subtree) survive?
    pub canonical: BTreeMap<&'static str, Tally>,
    /// Elements the writer regenerated: canonical fingerprint survives?
    pub touched: BTreeMap<&'static str, Tally>,
    pub regenerated_nodes: usize,
    pub passthrough_nodes: usize,
    pub verbatim_nodes: usize,
    pub write_ms: f64,
    /// Things worth a human look, capped.
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FileResult {
    pub name: String,
    pub bytes: usize,
    pub error: Option<String>,
    pub read_ms: f64,
    /// Count of modelled nodes per feature.
    pub model_nodes: BTreeMap<&'static str, usize>,
    /// Unknown elements carried in slots, by name.
    pub slots: BTreeMap<String, usize>,
    pub slot_bytes: usize,
    pub part_bytes: usize,
    pub modes: BTreeMap<Mode, ModeResult>,
}

pub struct Outputs {
    pub result: FileResult,
    /// Written packages per mode, for the visual pass.
    pub packages: Vec<(Mode, Vec<u8>)>,
}

const PARTS: [(PartKind, &str); 3] = [
    (PartKind::Content, "content.xml"),
    (PartKind::Styles, "styles.xml"),
    (PartKind::Meta, "meta.xml"),
];

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

pub fn run_file(name: &str, bytes: &[u8], modes: &[Mode]) -> Outputs {
    let mut result = FileResult {
        name: name.to_owned(),
        bytes: bytes.len(),
        ..Default::default()
    };
    let mut packages = Vec::new();
    let t = Instant::now();
    let odt = match skriv_odf::read(bytes.to_vec()) {
        Ok(o) => o,
        Err(e) => {
            result.error = Some(e.to_string());
            return Outputs { result, packages };
        }
    };
    result.read_ms = ms(t);
    drop(odt);

    for &mode in modes {
        let mut odt = skriv_odf::read(bytes.to_vec()).expect("read twice");
        let originals: Vec<(PartKind, String)> = PARTS
            .iter()
            .filter_map(|(k, _)| odt.source(*k).map(|s| (*k, s.to_owned())))
            .collect();
        if result.model_nodes.is_empty() {
            for (f, n) in skriv_model::feature_counts(&mut odt.doc) {
                result.model_nodes.insert(f.label(), n);
            }
            result.part_bytes = originals.iter().map(|(_, s)| s.len()).sum();
        }
        let touched = apply_mode(&mut odt.doc, mode);
        let t = Instant::now();
        let written = match odt.write_with(WriteOptions {
            regenerate_text: mode == Mode::Regen,
            // In touch mode, make regenerated elements differ in bytes from
            // the source (attribute order), so the untouched-element metric
            // tests preservation rather than serialisation coincidence.
            scramble_regenerated: mode == Mode::Touch,
        }) {
            Ok(w) => w,
            Err(e) => {
                result.error = Some(format!("{}: {e}", mode.label()));
                continue;
            }
        };
        let write_ms = ms(t);
        // Count slots from the regenerate-all pass: in other modes a slot inside
        // an untouched element is copied with its parent and never visited.
        if mode == Mode::Regen {
            result.slots = written.stats.slot_names.clone();
            result.slot_bytes = written.stats.slot_bytes;
        }
        let mut mr = compare(&originals, &written, &touched);
        mr.write_ms = write_ms;
        mr.regenerated_nodes = written.stats.regenerated_nodes;
        mr.passthrough_nodes = written.stats.passthrough_nodes;
        mr.verbatim_nodes = written.stats.verbatim_nodes;
        packages.push((mode, written.bytes));
        result.modes.insert(mode, mr);
    }
    Outputs { result, packages }
}

/// Apply a mode's changes; return the origins of nodes now marked dirty.
fn apply_mode(doc: &mut skriv_model::Document, mode: Mode) -> Vec<Origin> {
    let mut touched = Vec::new();
    match mode {
        Mode::Identity => {}
        Mode::Canary => {
            let mut n = 0usize;
            for p in skriv_model::paragraphs_mut(doc) {
                if p.plain_text().trim().is_empty() {
                    continue;
                }
                n += 1;
                if n % 3 == 1 {
                    p.content.clear();
                    p.meta.dirty = true;
                    if let Some(o) = p.meta.origin {
                        touched.push(o);
                    }
                }
            }
        }
        Mode::Regen => skriv_model::for_each_node_mut(doc, &mut |_, m| {
            m.dirty = true;
            if let Some(o) = m.origin {
                touched.push(o);
            }
        }),
        Mode::Touch => {
            let mut n = 0usize;
            skriv_model::for_each_node_mut(doc, &mut |f, m| {
                let eligible = !matches!(f, Feature::Control | Feature::Meta);
                if eligible && let Some(o) = m.origin {
                    n += 1;
                    if n.is_multiple_of(5) {
                        m.dirty = true;
                        touched.push(o);
                    }
                }
            });
        }
    }
    touched
}

fn compare(originals: &[(PartKind, String)], w: &Written, touched: &[Origin]) -> ModeResult {
    let mut mr = ModeResult::default();
    let orig_auto = originals
        .iter()
        .find(|(k, _)| *k == PartKind::Content)
        .map(|(_, s)| auto_styles(s))
        .unwrap_or_default();
    let out_auto = auto_styles(&w.content_xml);
    for (kind, orig) in originals {
        let (label, out) = match kind {
            PartKind::Content => ("content.xml", Some(&w.content_xml)),
            PartKind::Styles => ("styles.xml", w.styles_xml.as_ref()),
            PartKind::Meta => ("meta.xml", w.meta_xml.as_ref()),
        };
        let Some(out) = out else { continue };
        mr.identical_parts.insert(label.to_owned(), orig == out);
        // styles.xml references content's automatic styles never, and its own
        // automatic styles are for page layout; only content gets the map.
        let empty = HashMap::new();
        let (ao, aw) = if *kind == PartKind::Content {
            (&orig_auto, &out_auto)
        } else {
            (&empty, &empty)
        };
        let (Ok(co), Ok(cw)) = (canon(orig, ao), canon(out, aw)) else {
            mr.notes.push(format!("{label}: output does not parse"));
            continue;
        };
        let mut starts: Vec<usize> = touched
            .iter()
            .filter(|o| o.part == *kind)
            .map(|o| o.start)
            .collect();
        starts.sort_unstable();
        tally_part(label, orig, out, &co, &cw, &starts, &mut mr);
    }
    mr
}

fn tally_part(
    label: &str,
    orig: &str,
    out: &str,
    co: &Canon,
    cw: &Canon,
    touched_starts: &[usize],
    mr: &mut ModeResult,
) {
    let by_path: HashMap<&[u32], &crate::canon::Elem> =
        cw.elems.iter().map(|e| (e.path.as_slice(), e)).collect();
    // Multiset of output fingerprints, for path-independent canonical matching.
    let mut pool: HashMap<(&str, u64), usize> = HashMap::new();
    for e in &cw.elems {
        *pool.entry((e.feature, e.hash)).or_insert(0) += 1;
    }
    for e in &co.elems {
        // Canonical: does an identical subtree exist in the output?
        let key = (e.feature, e.hash);
        let found = match pool.get_mut(&key) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        };
        mr.canonical.entry(e.feature).or_default().add(found);

        let lo = touched_starts.partition_point(|&s| s < e.start);
        let contains_touched = lo < touched_starts.len() && touched_starts[lo] < e.end;
        let is_touched = touched_starts.binary_search(&e.start).is_ok();
        let peer = by_path.get(e.path.as_slice());
        if !contains_touched {
            let same = peer.is_some_and(|p| orig[e.start..e.end] == out[p.start..p.end]);
            mr.untouched.entry(e.feature).or_default().add(same);
            if !same && mr.notes.len() < 12 {
                mr.notes.push(format!(
                    "{label}: untouched <{}> at {:?} changed",
                    e.name, e.path
                ));
            }
        }
        if is_touched {
            let same = peer.is_some_and(|p| p.hash == e.hash);
            mr.touched.entry(e.feature).or_default().add(same);
            if !same && mr.notes.len() < 12 {
                let shown = peer
                    .map(|p| snippet(&out[p.start..p.end]))
                    .unwrap_or_default();
                mr.notes.push(format!(
                    "{label}: regenerated <{}> differs: {} → {}",
                    e.name,
                    snippet(&orig[e.start..e.end]),
                    shown
                ));
            }
        }
    }
}

fn snippet(s: &str) -> String {
    let mut t: String = s.chars().take(160).collect();
    if s.len() > t.len() {
        t.push('…');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tally_percentages() {
        let mut t = Tally::default();
        assert_eq!(t.pct(), None);
        t.add(true);
        t.add(false);
        assert_eq!(t.pct(), Some(50.0));
    }
}
