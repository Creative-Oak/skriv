// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Tracked changes made in LibreOffice become the branch's own edits.
//!
//! When someone edits a returned file with Track Changes on, LibreOffice
//! keeps inserted text in the body between change marks, moves deleted text
//! into `text:tracked-changes`, and records author and date. In skriv a
//! suggestion is a branch, so the file's new tracked changes are accepted
//! into the branch: the marks and regions go, the text stays as the person
//! left it, and the branch's diff shows the suggestion.
//!
//! Tracked changes the document already had in skriv are kept as they are.
//! They are told apart by kind, author and date, because LibreOffice
//! renumbers change IDs on every save.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;
use skriv_crdt::logical::{LDoc, LNode, Para};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct TrackedStats {
    /// New changes accepted into the branch, by kind.
    pub accepted: BTreeMap<String, usize>,
    /// Their authors.
    pub authors: BTreeMap<String, usize>,
    /// Changes the document already had.
    pub kept: usize,
}

type Key = (String, String, String);

fn regions(v: &Value) -> Vec<&Value> {
    v.get("TrackedChanges")
        .and_then(|t| t.get("children"))
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|c| c.get("Region")).collect())
        .unwrap_or_default()
}

fn attr<'a>(v: &'a Value, name: &str) -> Option<&'a str> {
    v.get("attrs")?
        .as_array()?
        .iter()
        .find_map(|a| (a.get("name")?.as_str()? == name).then(|| a.get("value")?.as_str())?)
}

fn key(region: &Value) -> Key {
    let change = region.get("change");
    let kind = change
        .and_then(|c| c.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let info = change.and_then(|c| c.get("info"));
    let s = |k: &str| {
        info.and_then(|i| i.get(k))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    (kind, s("author"), s("date"))
}

fn embeds(v: &[LNode], out: &mut Vec<String>) {
    for n in v {
        match n {
            LNode::Embed { json, .. } => out.push(json.clone()),
            LNode::Container { children, .. } => embeds(children, out),
            _ => {}
        }
    }
}

/// Accept the returned file's new tracked changes. `base` is skriv's document.
pub fn accept_new(base: &LDoc, ret: &mut LDoc) -> TrackedStats {
    let mut st = TrackedStats::default();
    let mut known: HashSet<Key> = HashSet::new();
    let mut base_embeds = Vec::new();
    embeds(&base.body, &mut base_embeds);
    for j in &base_embeds {
        if let Ok(v) = serde_json::from_str::<Value>(j) {
            for r in regions(&v) {
                known.insert(key(r));
            }
        }
    }

    // Drop new regions from the tracked-changes blocks, remembering their IDs.
    let mut new_ids: HashSet<String> = HashSet::new();
    fn blocks(
        v: &mut Vec<LNode>,
        known: &HashSet<Key>,
        new_ids: &mut HashSet<String>,
        st: &mut TrackedStats,
    ) {
        v.retain_mut(|n| match n {
            LNode::Embed { json, .. } => {
                let Ok(mut val) = serde_json::from_str::<Value>(json) else {
                    return true;
                };
                let Some(children) = val
                    .get_mut("TrackedChanges")
                    .and_then(|t| t.get_mut("children"))
                    .and_then(Value::as_array_mut)
                else {
                    return true;
                };
                let before = children.len();
                children.retain(|c| {
                    let Some(r) = c.get("Region") else {
                        return true;
                    };
                    let k = key(r);
                    if known.contains(&k) {
                        st.kept += 1;
                        return true;
                    }
                    if let Some(id) = attr(r, "text:id").or_else(|| attr(r, "xml:id")) {
                        new_ids.insert(id.to_owned());
                    }
                    *st.accepted.entry(k.0.clone()).or_insert(0) += 1;
                    *st.authors.entry(k.1.clone()).or_insert(0) += 1;
                    false
                });
                if children.len() == before {
                    return true;
                }
                if children.is_empty() {
                    return false;
                }
                *json = val.to_string();
                true
            }
            LNode::Container { children, .. } => {
                blocks(children, known, new_ids, st);
                true
            }
            _ => true,
        });
    }
    blocks(&mut ret.body, &known, &mut new_ids, &mut st);
    if new_ids.is_empty() {
        return st;
    }

    // Remove the marks that point at them.
    fn para(p: &mut Para, new_ids: &HashSet<String>) {
        let before = p.runs.len();
        p.runs.retain(|r| {
            let Some(o) = r.marks.get("obj") else {
                return true;
            };
            let Ok(v) = serde_json::from_str::<Value>(o) else {
                return true;
            };
            match v.get("ChangeMark") {
                Some(m) => !attr(m, "text:change-id").is_some_and(|id| new_ids.contains(id)),
                None => true,
            }
        });
        if p.runs.len() != before {
            p.normalize();
        }
    }
    fn walk(v: &mut [LNode], new_ids: &HashSet<String>) {
        for n in v {
            match n {
                LNode::Para(p) => para(p, new_ids),
                LNode::Container { children, .. } => walk(children, new_ids),
                LNode::Table(t) => {
                    for row in &mut t.cells {
                        for c in row {
                            walk(&mut c.blocks, new_ids);
                        }
                    }
                }
                LNode::Embed { .. } => {}
            }
        }
    }
    walk(&mut ret.body, &new_ids);
    st
}

/// The content of a region, without its IDs or bookkeeping.
fn content_key(region: &Value) -> String {
    let mut r = region.clone();
    fn strip(v: &mut Value) {
        match v {
            Value::Object(m) => {
                m.remove("meta");
                m.remove("origin");
                m.remove("attrs");
                m.values_mut().for_each(strip);
            }
            Value::Array(a) => a.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut r);
    r.to_string()
}

fn region_ids(v: &[LNode]) -> Vec<(String, String)> {
    let mut js = Vec::new();
    embeds(v, &mut js);
    let mut out = Vec::new();
    for j in js {
        if let Ok(val) = serde_json::from_str::<Value>(&j) {
            for r in regions(&val) {
                if let Some(id) = attr(r, "text:id").or_else(|| attr(r, "xml:id")) {
                    out.push((content_key(r), id.to_owned()));
                }
            }
        }
    }
    out
}

/// LibreOffice renumbers every tracked change on save. Give the returned
/// file's changes skriv's IDs back where the change itself is unchanged
/// (same kind, author, date and content), so the marks in the text and the
/// regions they point at stay skriv's. Returns how many were mapped.
pub fn restore_ids(base: &LDoc, ret: &mut LDoc) -> usize {
    let mut by_content: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (k, id) in region_ids(&base.body) {
        by_content.entry(k).or_default().push(id);
    }
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (k, id) in region_ids(&ret.body) {
        if let Some(ids) = by_content.get_mut(&k)
            && !ids.is_empty()
        {
            map.insert(id, ids.remove(0));
        }
    }
    if map.is_empty() {
        return 0;
    }
    fn fix(v: &mut Value, map: &BTreeMap<String, String>) {
        match v {
            Value::Object(m) => {
                if let (Some(Value::String(name)), Some(Value::String(value))) =
                    (m.get("name").cloned(), m.get("value").cloned())
                    && matches!(name.as_str(), "text:change-id" | "text:id" | "xml:id")
                    && let Some(to) = map.get(&value)
                {
                    m.insert("value".into(), Value::String(to.clone()));
                }
                m.values_mut().for_each(|x| fix(x, map));
            }
            Value::Array(a) => a.iter_mut().for_each(|x| fix(x, map)),
            _ => {}
        }
    }
    let rewrite = |json: &mut String| {
        if let Ok(mut v) = serde_json::from_str::<Value>(json) {
            fix(&mut v, &map);
            *json = v.to_string();
        }
    };
    fn walk(v: &mut [LNode], rewrite: &dyn Fn(&mut String)) {
        for n in v {
            match n {
                LNode::Para(p) => {
                    for r in &mut p.runs {
                        if let Some(o) = r.marks.get_mut("obj")
                            && o.contains("ChangeMark")
                        {
                            rewrite(o);
                        }
                    }
                }
                LNode::Embed { json, .. } => {
                    if json.contains("TrackedChanges") {
                        rewrite(json);
                    }
                }
                LNode::Container { children, .. } => walk(children, rewrite),
                LNode::Table(t) => {
                    for row in &mut t.cells {
                        for c in row {
                            walk(&mut c.blocks, rewrite);
                        }
                    }
                }
            }
        }
    }
    walk(&mut ret.body, &rewrite);
    map.len()
}
