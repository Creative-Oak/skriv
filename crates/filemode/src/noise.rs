// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What changes in every file LibreOffice saves, whether or not anyone
//! edited it. Every rule here is here because a LibreOffice round trip
//! without edits produced it (`docs/spikes/04-results.md`); none of them is
//! something a person can see or would want reported as their change.
//!
//! Two levels:
//!
//! - **Import** ([`strip_layout`], [`ignore`], [`same_value`],
//!   [`same_obj`], [`canon_para_props`]): the returned file only loses
//!   layout output (soft page breaks); everything else is compared through
//!   equivalences, so skriv's own representation of untouched content is
//!   never rewritten.
//! - **Comparison** ([`clean`]): both documents are canonicalised (spaces
//!   written as `<text:s/>` become spaces, lengths become millimetres,
//!   regenerated IDs are renumbered), for measuring.

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;
use skriv_crdt::logical::{LDoc, LNode, Para, Props, Run};

/// How often each rule fired.
pub type NoiseStats = BTreeMap<String, usize>;

fn bump(st: &mut NoiseStats, rule: &str) {
    *st.entry(rule.to_owned()).or_insert(0) += 1;
}

/// Where a property sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Para,
    Run,
    Section,
    Container,
    Table,
    Row,
    Column,
    Cell,
}

/// Properties that are bookkeeping or serialisation, not content: never
/// compared, never written into skriv's document by an import.
pub fn ignore(scope: Scope, k: &str) -> bool {
    // Revision-save IDs: every LibreOffice edit session stamps what it touched.
    k.ends_with("officeooo:rsid")
        || k.ends_with("officeooo:paragraph-rsid")
        // The node's identity, which skriv writes and LibreOffice keeps.
        || k == "xml:id"
        // The order attributes were written in.
        || k == "@order"
        || match scope {
            // LibreOffice gives every section an automatic style (`Sect1`),
            // and renames table automatic styles (`Table1.A1` →
            // `Table1.A2`). The codec does not resolve these styles into
            // properties (it does for paragraphs and text), so only the
            // names could be compared, and the names are noise.
            Scope::Section | Scope::Cell => k == "text:style-name" || k == "table:style-name",
            // Which column elements were written as one repeated element,
            // and their style names. The width itself is `@width`.
            Scope::Column => k == "@rep" || k == "table:style-name",
            _ => false,
        }
}

/// A length in millimetres, if `v` is one (`8.5cm`, `3.3465in`, `12pt`).
fn length_mm(v: &str) -> Option<f64> {
    let units = [
        ("mm", 1.0),
        ("cm", 10.0),
        ("in", 25.4),
        ("pt", 25.4 / 72.0),
        ("pc", 25.4 / 6.0),
        ("px", 25.4 / 96.0),
    ];
    for (u, f) in units {
        if let Some(n) = v.strip_suffix(u)
            && let Ok(x) = n.parse::<f64>()
        {
            return Some(x * f);
        }
    }
    None
}

/// LibreOffice writes lengths in its own unit (inches): compare to a tenth
/// of a millimetre.
fn canon_value(v: &str) -> String {
    match length_mm(v) {
        Some(mm) => format!("{mm:.1}mm"),
        None => v.to_owned(),
    }
}

/// Whether two values of property `k` (in `scope`) mean the same.
pub fn same_prop(scope: Scope, k: &str, a: Option<&str>, b: Option<&str>) -> bool {
    // LibreOffice's convert writes only relative column widths for tables it
    // has not laid out; a later save writes the absolute width too. The
    // relative width governs, so one side lacking `@width` is no change.
    if scope == Scope::Column && k == "@width" && (a.is_none() || b.is_none()) {
        return true;
    }
    same_value(a, b)
}

pub fn same_value(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x == y || canon_value(x) == canon_value(y),
        (None, None) => true,
        _ => false,
    }
}

/// A paragraph's named style is either `text:style-name` (the paragraph
/// uses the named style directly) or `@style` (it uses an automatic style
/// whose parent is the named one). LibreOffice switches between the two
/// whenever it touches a paragraph; with the direct formatting compared
/// separately, they mean the same.
pub fn canon_para_props(p: &Props) -> Props {
    let mut out = p.clone();
    if let Some(s) = out.remove("text:style-name")
        && !out.contains_key("@style")
    {
        out.insert("@style".into(), s);
    }
    out
}

/// Strip skriv's bookkeeping (`meta`: IDs, dirty flags, source offsets),
/// computed field results, and length spellings from a model node
/// serialised as JSON.
fn canon_json(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("meta");
            m.remove("origin");
            if let Some(Value::Object(f)) = m.get_mut("Field") {
                // What a field shows (a page number, a date) is computed.
                f.remove("display");
            }
            m.values_mut().for_each(canon_json);
            // LibreOffice writes the regions of a tracked-changes list in
            // its own order: sort them by content (IDs left out).
            if let Some(Value::Array(children)) = m
                .get_mut("TrackedChanges")
                .and_then(|t| t.get_mut("children"))
            {
                children.sort_by_cached_key(|c| {
                    let mut k = c.clone();
                    if let Some(r) = k.get_mut("Region").and_then(Value::as_object_mut) {
                        r.remove("attrs");
                    }
                    k.to_string()
                });
            }
        }
        Value::Array(a) => {
            // An `xml:id` is identity, not content (skriv stamps them,
            // LibreOffice drops some).
            a.retain(|x| x.get("name").and_then(Value::as_str) != Some("xml:id"));
            a.iter_mut().for_each(canon_json);
        }
        Value::String(s) if length_mm(s).is_some() => {
            *s = canon_value(s);
        }
        _ => {}
    }
}

fn is_soft_page_break(v: &Value) -> bool {
    v.get("Control")
        .and_then(|c| c.get("kind"))
        .and_then(Value::as_str)
        == Some("SoftPageBreak")
        || v.get("Opaque")
            .and_then(|c| c.get("name"))
            .and_then(Value::as_str)
            == Some("text:soft-page-break")
}

fn space_count(v: &Value) -> Option<usize> {
    let c = v.get("Control")?;
    if c.get("kind")?.as_str()? != "Space" {
        return None;
    }
    let n = c
        .get("attrs")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter().find_map(|x| {
                (x.get("name")?.as_str()? == "text:c")
                    .then(|| x.get("value")?.as_str()?.parse().ok())?
            })
        })
        .unwrap_or(1);
    Some(n)
}

/// Identifiers LibreOffice regenerates on every save: footnote and endnote
/// IDs (`ftn0` → `ftn1`) and tracked-change IDs. They only tie two places
/// in the file together; renumbered in order of first appearance, two
/// documents compare equal when the references match.
#[derive(Default)]
struct Renumber {
    map: HashMap<String, String>,
    notes: usize,
    /// Leave change IDs alone (see [`obj_key`]).
    keep_change_ids: bool,
}

impl Renumber {
    fn name(&mut self, old: &str) -> String {
        let n = self.map.len() + 1;
        self.map
            .entry(old.to_owned())
            .or_insert_with(|| format!("#ref{n}"))
            .clone()
    }

    fn json(&mut self, v: &mut Value) {
        match v {
            Value::Object(m) => {
                // Model attributes: [{"name": "...", "value": "..."}].
                if let (Some(Value::String(name)), Some(Value::String(value))) =
                    (m.get("name").cloned(), m.get("value").cloned())
                    && matches!(name.as_str(), "text:change-id" | "text:id")
                    && !self.keep_change_ids
                {
                    m.insert("value".into(), Value::String(self.name(&value)));
                }
                if let Some(Value::String(raw)) = m.get("raw").cloned() {
                    m.insert("raw".into(), Value::String(self.xml(&raw)));
                }
                m.values_mut().for_each(|x| self.json(x));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.json(x)),
            _ => {}
        }
    }

    /// Replace `text:id="…"` and `text:change-id="…"` values in raw XML.
    /// A footnote's own ID is numbered by occurrence: LibreOffice makes
    /// duplicate note IDs unique, and nothing outside the note refers to it.
    fn xml(&mut self, raw: &str) -> String {
        if let Some(rest) = raw.strip_prefix("<text:note ")
            && let Some(i) = rest.find("text:id=\"")
        {
            self.notes += 1;
            let after = &rest[i + "text:id=\"".len()..];
            let end = after.find('"').unwrap_or(after.len());
            let tail = self.xml_ids(&after[end..]);
            return format!(
                "<text:note {}text:id=\"#note{}{}",
                &rest[..i],
                self.notes,
                tail
            );
        }
        self.xml_ids(raw)
    }

    fn xml_ids(&mut self, raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        loop {
            let hit = ["text:id=\"", "text:change-id=\""]
                .iter()
                .filter_map(|p| rest.find(p).map(|i| (i, p.len())))
                .min();
            let Some((i, plen)) = hit else {
                out.push_str(rest);
                return out;
            };
            out.push_str(&rest[..i + plen]);
            let after = &rest[i + plen..];
            let end = after.find('"').unwrap_or(after.len());
            out.push_str(&self.name(&after[..end]));
            rest = &after[end..];
        }
    }
}

fn canon_obj(json: &str, ids: &mut Renumber) -> String {
    match serde_json::from_str::<Value>(json) {
        Ok(mut v) => {
            canon_json(&mut v);
            ids.json(&mut v);
            v.to_string()
        }
        Err(_) => json.to_owned(),
    }
}

/// An inline object's (or embed's) canonical form on its own: two objects
/// are the same object when these are equal, whatever bookkeeping they
/// carry. Change IDs are kept: a change mark and its tracked-change region
/// refer to each other by ID, so an import must keep them in step
/// ([`crate::tracked::restore_ids`] maps LibreOffice's renumbered IDs back
/// first). Footnote IDs are renumbered: only the note itself uses them.
pub fn obj_key(json: &str) -> String {
    canon_obj(
        json,
        &mut Renumber {
            keep_change_ids: true,
            ..Default::default()
        },
    )
}

pub fn same_obj(a: &str, b: &str) -> bool {
    a == b || obj_key(a) == obj_key(b)
}

// ---------------------------------------------------------------------------
// Import level: remove what is layout output.

/// Remove soft page breaks (where LibreOffice's layout broke the page) from
/// a returned document.
pub fn strip_layout(doc: &mut LDoc) -> NoiseStats {
    let mut st = NoiseStats::new();
    fn para(p: &mut Para, st: &mut NoiseStats) {
        let before = p.runs.len();
        p.runs.retain(|r| {
            let spb = r.marks.get("obj").is_some_and(|o| {
                serde_json::from_str::<Value>(o).is_ok_and(|v| is_soft_page_break(&v))
            });
            if spb {
                bump(st, "soft page break");
            }
            !spb
        });
        if p.runs.len() != before {
            p.normalize();
        }
    }
    fn walk(v: &mut Vec<LNode>, st: &mut NoiseStats) {
        v.retain_mut(|n| match n {
            LNode::Para(p) => {
                para(p, st);
                true
            }
            LNode::Container { children, .. } => {
                walk(children, st);
                true
            }
            LNode::Table(t) => {
                for row in &mut t.cells {
                    for c in row {
                        walk(&mut c.blocks, st);
                    }
                }
                true
            }
            LNode::Embed { json, .. } => {
                let spb = serde_json::from_str::<Value>(json).is_ok_and(|v| is_soft_page_break(&v));
                if spb {
                    bump(st, "soft page break");
                }
                !spb
            }
        });
    }
    walk(&mut doc.body, &mut st);
    st
}

// ---------------------------------------------------------------------------
// Comparison level.

fn clean_props(scope: Scope, p: &mut Props, st: &mut NoiseStats) {
    p.retain(|k, _| {
        let drop = ignore(scope, k);
        if drop {
            bump(st, &format!("ignore {k}"));
        }
        !drop
    });
    for v in p.values_mut() {
        *v = canon_value(v);
    }
}

fn clean_para(p: &mut Para, ids: &mut Renumber, st: &mut NoiseStats) {
    clean_props(Scope::Para, &mut p.props, st);
    p.props = canon_para_props(&p.props);
    let mut runs: Vec<Run> = Vec::with_capacity(p.runs.len());
    for mut r in p.runs.drain(..) {
        clean_props(Scope::Run, &mut r.marks, st);
        if let Some(obj) = r.marks.get("obj").cloned()
            && let Ok(v) = serde_json::from_str::<Value>(&obj)
        {
            if is_soft_page_break(&v) {
                bump(st, "soft page break");
                continue;
            }
            if let Some(n) = space_count(&v) {
                // `<text:s/>` is how ODF spells repeated spaces.
                r.marks.remove("obj");
                r.text = " ".repeat(n * r.text.chars().count());
                bump(st, "spaces");
            } else {
                r.marks.insert("obj".into(), canon_obj(&obj, ids));
            }
        }
        runs.push(r);
    }
    p.runs = runs;
    p.normalize();
}

/// Canonicalise a document for comparison. Apply to both sides.
pub fn clean(doc: &mut LDoc) -> NoiseStats {
    let mut st = NoiseStats::new();
    let mut ids = Renumber::default();
    // Number change IDs in the order their marks appear in the text, so the
    // numbering doesn't depend on the order of the tracked-changes list.
    for p in doc.paras() {
        for r in &p.runs {
            if let Some(o) = r.marks.get("obj")
                && let Ok(mut v) = serde_json::from_str::<Value>(o)
                && v.get("ChangeMark").is_some()
            {
                ids.json(&mut v);
            }
        }
    }
    walk(&mut doc.body, &mut ids, &mut st);
    st
}

fn walk(v: &mut Vec<LNode>, ids: &mut Renumber, st: &mut NoiseStats) {
    v.retain_mut(|n| match n {
        LNode::Para(p) => {
            clean_para(p, ids, st);
            true
        }
        LNode::Container {
            kind,
            props,
            children,
            ..
        } => {
            let scope = match kind.as_str() {
                "section" => Scope::Section,
                // Rows, columns and cells as `codec::Baseline::adopt` passes them.
                "@row" => Scope::Row,
                "@col" => Scope::Column,
                "@cell" => Scope::Cell,
                _ => Scope::Container,
            };
            clean_props(scope, props, st);
            walk(children, ids, st);
            true
        }
        LNode::Table(t) => {
            clean_props(Scope::Table, &mut t.props, st);
            for (_, p) in &mut t.rows {
                clean_props(Scope::Row, p, st);
            }
            for (_, p) in &mut t.cols {
                clean_props(Scope::Column, p, st);
            }
            for row in &mut t.cells {
                for c in row {
                    clean_props(Scope::Cell, &mut c.props, st);
                    walk(&mut c.blocks, ids, st);
                }
            }
            true
        }
        LNode::Embed { json, .. } => {
            let spb = serde_json::from_str::<Value>(json).is_ok_and(|v| is_soft_page_break(&v));
            if spb {
                bump(st, "soft page break");
                return false;
            }
            *json = canon_obj(json, ids);
            true
        }
    });
}

/// Whether two nodes are the same content, by the rules above (for
/// `codec::Baseline::adopt`).
pub fn equivalent(a: &LNode, b: &LNode) -> bool {
    let mut x = LDoc {
        body: vec![a.clone()],
        ..Default::default()
    };
    let mut y = LDoc {
        body: vec![b.clone()],
        ..Default::default()
    };
    clean(&mut x);
    clean(&mut y);
    x.normalize();
    y.normalize();
    if let (
        Some(LNode::Container {
            kind, props: pa, ..
        }),
        Some(LNode::Container { props: pb, .. }),
    ) = (x.body.first(), y.body.first())
        && kind == "@col"
    {
        let keys: std::collections::BTreeSet<&String> = pa.keys().chain(pb.keys()).collect();
        return keys.into_iter().all(|k| {
            same_prop(
                Scope::Column,
                k,
                pa.get(k).map(String::as_str),
                pb.get(k).map(String::as_str),
            )
        });
    }
    x.body == y.body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_compare_in_millimetres() {
        assert!(same_value(Some("8.5cm"), Some("3.3465in")));
        assert!(!same_value(Some("8.5cm"), Some("3in")));
    }

    #[test]
    fn note_ids_renumber() {
        let mut r = Renumber::default();
        let a = r.xml(r#"<text:note text:id="ftn0"><x text:change-id="ct7"/>"#);
        assert_eq!(
            a,
            r##"<text:note text:id="#note1"><x text:change-id="#ref1"/>"##
        );
        assert!(same_obj(
            r#"{"Opaque":{"raw":"<text:note text:id=\"ftn0\"/>"}}"#,
            r#"{"Opaque":{"raw":"<text:note text:id=\"ftn1\"/>"}}"#
        ));
    }

    #[test]
    fn named_style_two_ways() {
        let a = Props::from([("text:style-name".to_owned(), "Text_20_body".to_owned())]);
        let b = Props::from([("@style".to_owned(), "Text_20_body".to_owned())]);
        assert_eq!(canon_para_props(&a), canon_para_props(&b));
    }
}
