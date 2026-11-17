// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Canonical XML: a fingerprint per element that ignores what doesn't matter.
//!
//! - attribute order;
//! - whitespace-only text between block elements;
//! - the names LibreOffice invents for automatic styles (`P1`, `T3`): every
//!   reference to an automatic style is replaced by a hash of that style's
//!   properties, so a renamed but identical style compares equal.
//!
//! Each element's fingerprint covers its whole subtree, so "this table's
//! fingerprint survived" means the table and everything in it did.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use skriv_odf::xml::{Kind, Tree, parse, unescape};

#[derive(Clone, Debug)]
pub struct Elem {
    /// Child-element indices from the root, e.g. `[3, 0, 12]`.
    pub path: Vec<u32>,
    pub name: String,
    pub feature: &'static str,
    pub hash: u64,
    pub start: usize,
    pub end: usize,
}

pub struct Canon {
    pub elems: Vec<Elem>,
}

/// Which feature bucket an element counts towards in reports. Labels match
/// `skriv_model::Feature::label`.
pub fn feature_of(name: &str) -> &'static str {
    if skriv_model::Field::kind_of(name).is_some() {
        return "fields";
    }
    match name {
        "text:p" => "paragraphs",
        "text:h" => "headings",
        "text:s" | "text:tab" | "text:line-break" | "text:soft-page-break" => "spaces/tabs/breaks",
        "text:list" | "text:list-item" | "text:list-header" => "lists",
        "text:section" => "sections",
        "office:annotation" | "office:annotation-end" => "comments",
        "text:tracked-changes"
        | "text:changed-region"
        | "text:insertion"
        | "text:deletion"
        | "text:format-change"
        | "office:change-info"
        | "text:change"
        | "text:change-start"
        | "text:change-end" => "tracked changes",
        "text:bookmark"
        | "text:bookmark-start"
        | "text:bookmark-end"
        | "text:reference-mark"
        | "text:reference-mark-start"
        | "text:reference-mark-end" => "bookmarks",
        "text:span" | "text:a" => "spans/links",
        "style:style" | "style:default-style" => "styles",
        "text:list-style" | "text:outline-style" => "list styles",
        n if n.starts_with("text:list-level-style") || n == "text:outline-level-style" => {
            "list styles"
        }
        n if n.starts_with("style:") && n.ends_with("-properties") => "styles",
        n if n.starts_with("table:") => "tables",
        n if n.starts_with("draw:") || n == "svg:title" || n == "svg:desc" => "images/frames",
        n if n.starts_with("dc:") || n.starts_with("meta:") => "metadata",
        _ => "other",
    }
}

const TEXTUAL: &[&str] = &[
    "text:p",
    "text:h",
    "text:span",
    "text:a",
    "dc:creator",
    "dc:date",
];

fn is_style_ref(attr: &str) -> bool {
    attr.ends_with("style-name") || attr == "style:name"
}

struct Ctx<'a> {
    src: &'a str,
    tree: &'a Tree,
    auto: &'a HashMap<String, u64>,
}

fn hash_elem(cx: &Ctx, i: usize, skip_name_attr: bool) -> u64 {
    let n = cx.tree.node(i);
    let mut h = DefaultHasher::new();
    let Kind::Element { name, attrs, .. } = &n.kind else {
        return 0;
    };
    name.hash(&mut h);
    let mut attrs: Vec<(&str, String)> = attrs
        .iter()
        .filter(|(k, _)| !(skip_name_attr && k == "style:name"))
        .map(|(k, v)| {
            let v = if is_style_ref(k) {
                match cx.auto.get(v) {
                    Some(hash) => format!("auto#{hash:x}"),
                    None => v.clone(),
                }
            } else {
                v.clone()
            };
            (k.as_str(), v)
        })
        .collect();
    attrs.sort();
    attrs.hash(&mut h);
    let textual = TEXTUAL.contains(&name.as_str()) || feature_of(name) == "fields";
    for &c in n.children() {
        let cn = cx.tree.node(c);
        match cn.kind {
            Kind::Element { .. } => {
                1u8.hash(&mut h);
                hash_elem(cx, c, false).hash(&mut h);
            }
            Kind::Text => {
                let t = unescape(&cx.src[cn.start..cn.end]);
                if textual || !t.trim().is_empty() {
                    2u8.hash(&mut h);
                    t.hash(&mut h);
                }
            }
            Kind::Other => {}
        }
    }
    h.finish()
}

/// Fingerprints of a content.xml's automatic styles, by name.
pub fn auto_styles(src: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    let Ok(tree) = parse(src) else { return map };
    let Some(auto) = tree.child(tree.root, "office:automatic-styles") else {
        return map;
    };
    let empty = HashMap::new();
    let cx = Ctx {
        src,
        tree: &tree,
        auto: &empty,
    };
    for &c in tree.node(auto).children() {
        if let Some(name) = tree.node(c).attr("style:name") {
            map.insert(name.to_owned(), hash_elem(&cx, c, true));
        }
    }
    map
}

pub fn canon(src: &str, auto: &HashMap<String, u64>) -> Result<Canon, skriv_odf::Error> {
    let tree = parse(src)?;
    let cx = Ctx {
        src,
        tree: &tree,
        auto,
    };
    let mut elems = Vec::new();
    let mut stack: Vec<(usize, Vec<u32>)> = vec![(tree.root, Vec::new())];
    while let Some((i, path)) = stack.pop() {
        let n = tree.node(i);
        let Kind::Element { name, .. } = &n.kind else {
            continue;
        };
        elems.push(Elem {
            path: path.clone(),
            name: name.clone(),
            feature: feature_of(name),
            hash: hash_elem(&cx, i, false),
            start: n.start,
            end: n.end,
        });
        let mut k = 0u32;
        let mut kids = Vec::new();
        for &c in n.children() {
            if matches!(tree.node(c).kind, Kind::Element { .. }) {
                let mut p = path.clone();
                p.push(k);
                kids.push((c, p));
                k += 1;
            }
        }
        for kid in kids.into_iter().rev() {
            stack.push(kid);
        }
    }
    Ok(Canon { elems })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<r xmlns:style="s" xmlns:text="t"><office:automatic-styles><style:style style:name="P1" style:family="paragraph"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles><text:p text:style-name="P1">Hi <text:span>you</text:span></text:p></r>"#;

    #[test]
    fn renamed_automatic_styles_compare_equal() {
        let renamed = DOC.replace("P1", "P7");
        let a = canon(DOC, &auto_styles(DOC)).unwrap();
        let b = canon(&renamed, &auto_styles(&renamed)).unwrap();
        let ha: Vec<_> = a.elems.iter().map(|e| e.hash).collect();
        let hb: Vec<_> = b.elems.iter().map(|e| e.hash).collect();
        assert_eq!(ha, hb);
    }

    #[test]
    fn attribute_order_is_ignored_but_values_are_not() {
        let a = r#"<x a="1" b="2"><text:p>t</text:p></x>"#;
        let b = r#"<x b="2" a="1"><text:p>t</text:p></x>"#;
        let c = r#"<x b="2" a="1"><text:p>u</text:p></x>"#;
        let h = |s| canon(s, &HashMap::new()).unwrap().elems[0].hash;
        assert_eq!(h(a), h(b));
        assert_ne!(h(a), h(c));
    }

    #[test]
    fn paths_and_features() {
        let c = canon(DOC, &HashMap::new()).unwrap();
        let p = c.elems.iter().find(|e| e.name == "text:p").unwrap();
        assert_eq!(p.path, vec![1]);
        assert_eq!(p.feature, "paragraphs");
        assert_eq!(feature_of("table:table-cell"), "tables");
        assert_eq!(feature_of("text:bookmark-ref"), "fields");
    }
}
