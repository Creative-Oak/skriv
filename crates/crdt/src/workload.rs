// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Seeded, deterministic workloads shared by the tests and the measurement
//! example: a small RNG, the 100-page document scaler, the concurrent table
//! fuzzer and the one-year history trace.

use std::collections::{BTreeMap, BTreeSet};

use skriv_model::{Block, Document, NodeId};

use crate::logical::{LDoc, LNode, LTable, Props};
use crate::{CrdtDoc, Peer};

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

    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + self.below(hi - lo + 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() as f64 / u64::MAX as f64) < p
    }

    pub fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        (!v.is_empty()).then(|| &v[self.below(v.len())])
    }
}

// ---------------------------------------------------------------------------
// Scaling a document.

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
        let blocks: Vec<Block> = serde_json::from_str(&json).expect("deserialise");
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

// ---------------------------------------------------------------------------
// Concurrent table fuzzing.

#[derive(Clone, Debug, Default)]
pub struct FuzzStats {
    pub runs: usize,
    pub ops: usize,
    pub row_inserts: usize,
    pub row_deletes: usize,
    pub col_inserts: usize,
    pub col_deletes: usize,
    pub cell_edits: usize,
    pub merges: usize,
    /// Cells two peers created concurrently (a new row meeting a new column).
    pub lazily_created_cells: usize,
    /// Runs where concurrent deletes removed every row or every column.
    pub emptied_tables: usize,
    pub failures: Vec<String>,
}

impl FuzzStats {
    pub fn add(&mut self, o: &FuzzStats) {
        self.runs += o.runs;
        self.ops += o.ops;
        self.row_inserts += o.row_inserts;
        self.row_deletes += o.row_deletes;
        self.col_inserts += o.col_inserts;
        self.col_deletes += o.col_deletes;
        self.cell_edits += o.cell_edits;
        self.merges += o.merges;
        self.lazily_created_cells += o.lazily_created_cells;
        self.emptied_tables += o.emptied_tables;
        self.failures.extend(o.failures.iter().cloned());
    }
}

/// One fuzz run: 2–3 peers fork from `base`, edit table `tid` concurrently
/// in two rounds with partial syncs in between, then everyone syncs.
/// Checks convergence, a rectangular grid, and that every piece of text
/// typed into a cell whose row and column survive is still in that cell.
/// Returns the merged state for further checks (writing ODT).
pub fn table_fuzz_run<D: CrdtDoc>(seed: u64, base: &mut D, tid: &str) -> (LDoc, FuzzStats) {
    let mut rng = Rng::new(seed);
    let mut st = FuzzStats {
        runs: 1,
        ..Default::default()
    };
    let start = base.table(tid).expect("table");
    let n = rng.range(2, 3);
    let mut peers: Vec<D> = (0..n).map(|p| base.fork(10 + p as Peer)).collect();
    // Every token typed, with the cell it went into.
    let mut typed: Vec<(String, String, String)> = Vec::new();
    let mut counter = 0usize;
    for round in 0..2 {
        for (p, peer) in peers.iter_mut().enumerate() {
            let ops = rng.range(1, 6);
            for _ in 0..ops {
                counter += 1;
                st.ops += 1;
                let Some(t) = peer.table(tid) else { continue };
                let tag = format!("s{seed}p{p}n{counter}");
                match rng.below(10) {
                    0 | 1 => {
                        let i = rng.below(t.rows.len() + 1);
                        if peer.insert_row(tid, i, &format!("r-{tag}")) {
                            st.row_inserts += 1;
                        }
                    }
                    2 if t.rows.len() > 1 => {
                        let r = &t.rows[rng.below(t.rows.len())].0;
                        if peer.delete_row(tid, r) {
                            st.row_deletes += 1;
                        }
                    }
                    3 => {
                        let i = rng.below(t.cols.len() + 1);
                        if peer.insert_col(tid, i, &format!("c-{tag}"), "2.5cm") {
                            st.col_inserts += 1;
                        }
                    }
                    4 if t.cols.len() > 1 => {
                        let c = &t.cols[rng.below(t.cols.len())].0;
                        if peer.delete_col(tid, c) {
                            st.col_deletes += 1;
                        }
                    }
                    _ if t.rows.is_empty() || t.cols.is_empty() => {}
                    _ => {
                        let ri = rng.below(t.rows.len());
                        let ci = rng.below(t.cols.len());
                        let (r, c) = (t.rows[ri].0.clone(), t.cols[ci].0.clone());
                        let cell = &t.cells[ri][ci];
                        let len = match cell.blocks.first() {
                            Some(LNode::Para(p)) => p.len(),
                            _ => 0,
                        };
                        if cell.blocks.is_empty() {
                            st.lazily_created_cells += 1;
                        }
                        let token = format!("⟨{tag}⟩");
                        if peer.cell_insert_text(tid, &r, &c, rng.below(len + 1), &token) {
                            st.cell_edits += 1;
                            typed.push((token, r, c));
                        }
                    }
                }
            }
            peer.commit();
        }
        if round == 0 {
            // Partial sync: one random peer pulls from another.
            let a = rng.below(n);
            let b = (a + 1 + rng.below(n - 1)) % n;
            let (pa, pb) = two(&mut peers, a, b);
            pa.merge(pb);
            st.merges += 1;
        }
    }
    // Full sync through peer 0, then back out.
    for i in 1..n {
        let (a, b) = two(&mut peers, 0, i);
        a.merge(b);
        st.merges += 1;
    }
    for i in 1..n {
        let (a, b) = two(&mut peers, i, 0);
        a.merge(b);
        st.merges += 1;
    }
    let states: Vec<LDoc> = peers
        .iter()
        .map(|p| {
            let mut l = p.read();
            l.normalize();
            l
        })
        .collect();
    for (i, s) in states.iter().enumerate().skip(1) {
        if *s != states[0] {
            st.failures.push(format!(
                "seed {seed}: peer {i} did not converge with peer 0"
            ));
        }
    }
    let t = peers[0].table(tid).expect("table survives");
    if t.rows.is_empty() || t.cols.is_empty() {
        st.emptied_tables += 1;
    }
    check_table(seed, &start, &t, &typed, &mut st);
    (states.into_iter().next().unwrap(), st)
}

fn two<T>(v: &mut [T], a: usize, b: usize) -> (&mut T, &mut T) {
    assert_ne!(a, b);
    if a < b {
        let (x, y) = v.split_at_mut(b);
        (&mut x[a], &mut y[0])
    } else {
        let (x, y) = v.split_at_mut(a);
        (&mut y[0], &mut x[b])
    }
}

/// Remove every `⟨…⟩` token, innermost first (later typing can land inside
/// an earlier token). Returns the tokens and what is left.
pub fn strip_tokens(s: &str) -> (Vec<String>, String) {
    let mut chars: Vec<char> = s.chars().collect();
    let mut tokens = Vec::new();
    loop {
        let mut found = None;
        let mut open = None;
        for (i, c) in chars.iter().enumerate() {
            match c {
                '⟨' => open = Some(i),
                '⟩' => {
                    if let Some(o) = open {
                        found = Some((o, i));
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some((o, e)) = found else { break };
        tokens.push(chars[o..=e].iter().collect());
        chars.drain(o..=e);
    }
    (tokens, chars.into_iter().collect())
}

fn check_table(
    seed: u64,
    start: &LTable,
    t: &LTable,
    typed: &[(String, String, String)],
    st: &mut FuzzStats,
) {
    let rows: BTreeSet<&str> = t.rows.iter().map(|r| r.0.as_str()).collect();
    let cols: BTreeSet<&str> = t.cols.iter().map(|c| c.0.as_str()).collect();
    if rows.len() != t.rows.len() || cols.len() != t.cols.len() {
        st.failures
            .push(format!("seed {seed}: a row or column appears twice"));
    }
    if t.cells.len() != t.rows.len() || t.cells.iter().any(|r| r.len() != t.cols.len()) {
        st.failures
            .push(format!("seed {seed}: table is not rectangular"));
    }
    let original = |r: &str, c: &str| -> String {
        let ri = start.rows.iter().position(|x| x.0 == r);
        let ci = start.cols.iter().position(|x| x.0 == c);
        match (ri, ci) {
            (Some(ri), Some(ci)) => start.cells[ri][ci].text(),
            _ => String::new(),
        }
    };
    let mut expected: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for (token, r, c) in typed {
        if rows.contains(r.as_str()) && cols.contains(c.as_str()) {
            expected.entry((r, c)).or_default().push(token);
        }
    }
    for (ri, (r, _)) in t.rows.iter().enumerate() {
        for (ci, (c, _)) in t.cols.iter().enumerate() {
            let text = t.cells[ri][ci].text();
            let (mut found, rest) = strip_tokens(&text);
            let mut want: Vec<String> = expected
                .get(&(r.as_str(), c.as_str()))
                .map(|v| v.iter().map(|s| s.to_string()).collect())
                .unwrap_or_default();
            found.sort();
            want.sort();
            if found != want {
                st.failures.push(format!(
                    "seed {seed}: cell {r}|{c} holds tokens {found:?}, expected {want:?}"
                ));
            }
            let flat = |s: &str| s.replace('\n', "");
            if flat(&rest) != flat(&original(r, c)) {
                st.failures.push(format!(
                    "seed {seed}: cell {r}|{c} reads {rest:?} without the typing, was {:?}",
                    original(r, c)
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Random document edits: for branch-and-merge and the history trace.

/// Words to type: Danish, so non-ASCII text is exercised everywhere.
const WORDS: &[&str] = &[
    "kommunen",
    "afgørelse",
    "ansøgning",
    "byggetilladelse",
    "ejendommen",
    "frist",
    "klage",
    "nævnet",
    "gården",
    "færge",
    "ålens",
    "sagsbehandler",
    "tilbygning",
    "Æbleøen",
    "på",
    "og",
    "er",
    "den",
    "vi",
    "har",
];

/// Character formatting a user applies by hand: one ODF property each.
const FORMATS: &[(&str, &str)] = &[
    ("Δstyle:text-properties|fo:font-weight", "bold"),
    ("Δstyle:text-properties|fo:font-style", "italic"),
    ("Δstyle:text-properties|style:text-underline-style", "solid"),
    ("Δstyle:text-properties|fo:color", "#c9211e"),
];

#[derive(Clone, Debug, Default)]
pub struct EditStats {
    pub inserts: usize,
    pub deletes: usize,
    pub formats: usize,
    pub splits: usize,
    pub para_props: usize,
    pub cell_edits: usize,
    pub inserted_chars: usize,
    pub deleted_chars: usize,
}

/// The IDs of paragraphs a user can type into, from a read of the document.
/// Taken once and kept up to date by [`Editor`], so editing doesn't read the
/// whole document per keystroke.
pub struct Editor {
    pub rng: Rng,
    pub paras: Vec<String>,
    pub tables: Vec<String>,
    pub minted: usize,
    pub prefix: String,
    pub stats: EditStats,
    /// Every id this editor touched, for the fidelity check.
    pub touched: BTreeSet<String>,
    /// Delete about as much as is typed, so the document keeps its size
    /// (a year of revisions rather than a year of writing).
    pub steady: bool,
}

impl Editor {
    pub fn new(seed: u64, l: &LDoc, prefix: &str) -> Self {
        Editor {
            rng: Rng::new(seed),
            paras: top_paras(l),
            tables: l.tables().iter().map(|t| t.id.clone()).collect(),
            minted: 0,
            prefix: prefix.to_owned(),
            stats: EditStats::default(),
            touched: BTreeSet::new(),
            steady: false,
        }
    }

    fn words(&mut self) -> String {
        let n = self.rng.range(1, 4);
        let mut s = String::new();
        for _ in 0..n {
            s.push(' ');
            s.push_str(WORDS[self.rng.below(WORDS.len())]);
        }
        s
    }

    /// One user-level edit: type a few words, delete a few characters,
    /// format a range, press Enter, change a paragraph property, or type
    /// into a table cell.
    pub fn step<D: CrdtDoc>(&mut self, d: &mut D) {
        let roll = self.rng.below(100);
        if roll < 4 && !self.tables.is_empty() {
            let tid = self.tables[self.rng.below(self.tables.len())].clone();
            if let Some(t) = d.table(&tid)
                && !t.rows.is_empty()
            {
                let ri = self.rng.below(t.rows.len());
                let ci = self.rng.below(t.cols.len());
                let w = self.words();
                if d.cell_insert_text(&tid, &t.rows[ri].0, &t.cols[ci].0, 0, &w) {
                    self.stats.cell_edits += 1;
                    self.touched.insert(tid);
                }
            }
            return;
        }
        let Some(pid) = self.rng.pick(&self.paras).cloned() else {
            return;
        };
        let Some(len) = d.para_len(&pid) else {
            self.paras.retain(|p| *p != pid);
            return;
        };
        self.touched.insert(pid.clone());
        match roll {
            0..=54 => {
                let w = self.words();
                let pos = self.rng.below(len + 1);
                if d.insert_text(&pid, pos, &w) {
                    self.stats.inserts += 1;
                    self.stats.inserted_chars += w.chars().count();
                }
            }
            55..=79 if len > 0 => {
                let pos = self.rng.below(len);
                let n = if self.steady {
                    self.rng.range(1, 60)
                } else {
                    self.rng.range(1, 12)
                };
                let n = n.min(len - pos);
                if d.delete_text(&pid, pos, n) {
                    self.stats.deletes += 1;
                    self.stats.deleted_chars += n;
                }
            }
            80..=91 if len > 1 => {
                let a = self.rng.below(len - 1);
                let b = self.rng.range(a + 1, len.min(a + 40));
                let (k, v) = FORMATS[self.rng.below(FORMATS.len())];
                let v = if self.rng.chance(0.2) { None } else { Some(v) };
                if d.mark(&pid, a, b, k, v) {
                    self.stats.formats += 1;
                }
            }
            92..=95 => {
                self.minted += 1;
                let id = format!("{}-{}", self.prefix, self.minted);
                let pos = self.rng.below(len + 1);
                if d.split_para(&pid, pos, &id) {
                    self.stats.splits += 1;
                    self.paras.push(id.clone());
                    self.touched.insert(id);
                }
            }
            _ => {
                let v = ["0.5cm", "1cm", "0cm"][self.rng.below(3)];
                if d.set_para_prop(&pid, "Δstyle:paragraph-properties|fo:margin-left", Some(v)) {
                    self.stats.para_props += 1;
                }
            }
        }
    }
}

/// Paragraphs outside tables, in document order.
pub fn top_paras(l: &LDoc) -> Vec<String> {
    fn walk(v: &[LNode], out: &mut Vec<String>) {
        for n in v {
            match n {
                LNode::Para(p) => out.push(p.id.clone()),
                LNode::Container { children, .. } => walk(children, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(&l.body, &mut out);
    out
}

/// All section IDs, in document order.
pub fn sections(l: &LDoc) -> Vec<(String, Props)> {
    fn walk(v: &[LNode], out: &mut Vec<(String, Props)>) {
        for n in v {
            if let LNode::Container {
                id,
                kind,
                props,
                children,
            } = n
            {
                if kind == "section" {
                    out.push((id.clone(), props.clone()));
                }
                walk(children, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(&l.body, &mut out);
    out
}

/// Plain text per paragraph ID, for comparing documents.
pub fn texts(l: &LDoc) -> BTreeMap<String, String> {
    l.paras()
        .into_iter()
        .map(|p| (p.id.clone(), p.text()))
        .collect()
}
