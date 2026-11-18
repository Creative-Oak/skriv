// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Model ⇄ logical document, shared by both backends.
//!
//! [`import`] turns a [`Document`] into an [`LDoc`] for the CRDT, plus a
//! [`Baseline`]: what each node looked like at import, with its origin. The
//! baseline never enters the CRDT. [`export`] turns the CRDT's current
//! [`LDoc`] back into a [`Document`]. A node whose logical form equals its
//! baseline form gets its original model node back, origins and all, so the
//! ODT writer copies its source bytes. Everything else is marked dirty and
//! regenerated. This is how origins and slots live with a CRDT (ADR 0002,
//! ADR 0006): slots ride inside the CRDT as atomic JSON, origins stay outside
//! it, keyed by node ID against one source file, and "dirty" is re-derived at
//! export by comparison instead of being tracked.
//!
//! Direct formatting is decomposed: an automatic style becomes its parent
//! (named) style plus one property per entry (`Δ<property-set>|<attr>`), so
//! concurrent "bold" and "italic" on the same text both survive a merge.
//! Export re-derives automatic styles, reusing the original name when the
//! property set matches, and minting `P…`/`T…` names otherwise.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;
use skriv_model::*;

use crate::logical::*;

const ORDER: &str = "@order";

// ---------------------------------------------------------------------------
// Styles: decomposition of automatic styles into parent + property delta.

/// A decomposed automatic style: its family, parent and own properties.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Sig {
    family: String,
    parent: Option<String>,
    delta: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
struct StyleIndex {
    by_name: HashMap<String, Sig>,
    by_sig: HashMap<Sig, String>,
    names: HashSet<String>,
}

fn decompose(s: &Style) -> Option<Sig> {
    if s.default {
        return None;
    }
    let family = s.family()?.to_owned();
    let mut delta = Vec::new();
    for a in s.attrs.iter() {
        match a.name.as_str() {
            "style:name" | "style:family" | "style:parent-style-name" => {}
            n => delta.push((format!("Δ@{n}"), a.value.clone())),
        }
    }
    for c in &s.children {
        match c {
            StyleChild::Props(p) if p.children.is_empty() => {
                for a in p.attrs.iter() {
                    delta.push((format!("Δ{}|{}", p.name, a.name), a.value.clone()));
                }
            }
            _ => return None,
        }
    }
    delta.sort();
    Some(Sig {
        family,
        parent: s.parent().map(str::to_owned),
        delta,
    })
}

impl StyleIndex {
    fn new(styles: &Styles) -> Self {
        let mut ix = StyleIndex::default();
        for list in [
            &styles.named,
            &styles.content_automatic,
            &styles.styles_automatic,
        ] {
            for e in list {
                if let StyleEntry::Style(s) = e
                    && let Some(n) = s.name()
                {
                    ix.names.insert(n.to_owned());
                }
            }
        }
        for e in &styles.content_automatic {
            if let StyleEntry::Style(s) = e
                && let (Some(name), Some(sig)) = (s.name(), decompose(s))
            {
                ix.by_sig
                    .entry(sig.clone())
                    .or_insert_with(|| name.to_owned());
                ix.by_name.insert(name.to_owned(), sig);
            }
        }
        ix
    }
}

fn prop_rank(set: &str) -> u8 {
    match set {
        "style:graphic-properties" => 0,
        "style:table-properties" | "style:table-column-properties" => 1,
        "style:table-row-properties" | "style:table-cell-properties" => 2,
        "style:paragraph-properties" => 3,
        "style:text-properties" => 4,
        _ => 5,
    }
}

/// Automatic styles created at export, for direct formatting the source file
/// had no style for.
struct Minter {
    ix: StyleIndex,
    created: Vec<StyleEntry>,
}

impl Minter {
    fn resolve(
        &mut self,
        family: &str,
        parent: Option<&str>,
        delta: &BTreeMap<&str, &str>,
    ) -> String {
        let sig = Sig {
            family: family.to_owned(),
            parent: parent.map(str::to_owned),
            delta: delta
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        if let Some(n) = self.ix.by_sig.get(&sig) {
            return n.clone();
        }
        let prefix = match family {
            "paragraph" => "P",
            "text" => "T",
            "table-column" => "Co",
            _ => "X",
        };
        let name = (1..)
            .map(|i| format!("{prefix}{i}"))
            .find(|n| !self.ix.names.contains(n))
            .expect("a free name");
        let mut attrs = Attrs::default();
        attrs.set("style:name", name.clone());
        attrs.set("style:family", family);
        if let Some(p) = parent {
            attrs.set("style:parent-style-name", p);
        }
        let mut sets: BTreeMap<(u8, &str), Attrs> = BTreeMap::new();
        for (k, v) in delta {
            let k = &k["Δ".len()..];
            if let Some(a) = k.strip_prefix('@') {
                attrs.set(a, *v);
            } else if let Some((set, a)) = k.split_once('|') {
                sets.entry((prop_rank(set), set)).or_default().set(a, *v);
            }
        }
        let children = sets
            .into_iter()
            .map(|((_, set), attrs)| {
                StyleChild::Props(PropertySet {
                    meta: NodeMeta::new(),
                    name: set.to_owned(),
                    attrs,
                    children: Vec::new(),
                })
            })
            .collect();
        self.created.push(StyleEntry::Style(Style {
            meta: NodeMeta::new(),
            default: false,
            attrs,
            children,
        }));
        self.ix.names.insert(name.clone());
        self.ix.by_sig.insert(sig, name.clone());
        name
    }
}

// ---------------------------------------------------------------------------
// JSON for things the CRDT carries but doesn't edit.

/// Origins are byte offsets into one source file; they must not enter the
/// CRDT. Strip them (and the dirty flags) from anything serialised into it.
fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                match k.as_str() {
                    "origin" => *x = Value::Null,
                    // No origin: everything is written from the model.
                    "dirty" => *x = Value::Bool(true),
                    "descendants_dirty" | "stamp_id" | "children_changed" => {
                        *x = Value::Bool(false)
                    }
                    _ => strip(x),
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

fn to_json<T: serde::Serialize>(t: &T) -> String {
    let mut v = serde_json::to_value(t).expect("model serialises");
    strip(&mut v);
    v.to_string()
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

// ---------------------------------------------------------------------------
// Properties.

fn attrs_to_props(attrs: &Attrs) -> Props {
    let mut p = Props::new();
    let mut order = Vec::new();
    for a in attrs.iter() {
        p.insert(a.name.clone(), a.value.clone());
        order.push(a.name.as_str());
    }
    if !order.is_empty() {
        p.insert(ORDER.into(), order.join(" "));
    }
    p
}

/// ODF attributes from props: source order first, then new ones.
fn props_to_attrs(p: &Props, overrides: &[(&str, Option<String>)]) -> Attrs {
    let mut out = Attrs::default();
    let over = |name: &str| {
        overrides
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
    };
    let plain = |k: &str| !k.starts_with('@') && !k.starts_with('Δ');
    if let Some(order) = p.get(ORDER) {
        for name in order.split(' ').filter(|n| !n.is_empty()) {
            let v = match over(name) {
                Some(v) => v,
                None => p.get(name).cloned(),
            };
            if let Some(v) = v {
                out.0.push(Attr {
                    name: name.to_owned(),
                    value: v,
                });
            }
        }
    }
    for (k, v) in p {
        if plain(k) && out.get(k).is_none() && over(k).is_none() {
            out.set(k, v.clone());
        }
    }
    for (k, v) in overrides {
        if let Some(v) = v
            && out.get(k).is_none()
        {
            out.set(k, v.clone());
        }
    }
    out
}

fn delta_of(p: &BTreeMap<String, String>) -> BTreeMap<&str, &str> {
    p.iter()
        .filter(|(k, _)| k.starts_with('Δ'))
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------
// The baseline: what each node was at import, and the node itself.

#[derive(Clone, Debug, Default)]
pub struct Baseline {
    paras: HashMap<String, (Para, Paragraph)>,
    embeds: HashMap<String, (String, Block)>,
    containers: HashMap<String, (Props, NodeMeta, Vec<Sig1>)>,
    tables: HashMap<String, (LTable, Table)>,
    rows: HashMap<String, (Props, Vec<LCell>, TableRow)>,
    cells: HashMap<String, (LCell, TableCell)>,
    /// Column groups by `(column props)`: the original column element.
    cols: HashMap<String, (Props, usize, TableColumn)>,
    groups: HashMap<(String, String), TableGroup>,
    styles: BTreeMap<String, (String, StyleEntry)>,
    meta: (String, Meta),
}

impl Baseline {
    fn origins(&self) -> HashMap<&str, Origin> {
        let mut m = HashMap::new();
        for (id, (_, p)) in &self.paras {
            if let Some(o) = p.meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, b)) in &self.embeds {
            if let Some(o) = block_origin(b) {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, meta, _)) in &self.containers {
            if let Some(o) = meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, t)) in &self.tables {
            if let Some(o) = t.meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, _, r)) in &self.rows {
            if let Some(o) = r.meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, c)) in &self.cells {
            if let Some(o) = c.meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        for (id, (_, _, c)) in &self.cols {
            if let Some(o) = c.meta.origin {
                m.insert(id.as_str(), o);
            }
        }
        m
    }
}

fn block_origin(b: &Block) -> Option<Origin> {
    match b {
        Block::Paragraph(p) => p.meta.origin,
        Block::List(l) => l.meta.origin,
        Block::Table(t) => t.meta.origin,
        Block::Section(s) => s.meta.origin,
        Block::TrackedChanges(t) => t.meta.origin,
        Block::Opaque(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Import.

struct Importer<'a> {
    ix: &'a StyleIndex,
    base: Baseline,
    embeds: usize,
}

/// Turn a model document into the logical document a CRDT stores.
pub fn import(doc: &Document) -> (LDoc, Baseline) {
    let ix = StyleIndex::new(&doc.styles);
    let mut imp = Importer {
        ix: &ix,
        base: Baseline::default(),
        embeds: 0,
    };
    let body = doc.body.iter().map(|b| imp.block(b)).collect();
    let mut styles = BTreeMap::new();
    for (scope, list) in [
        ("named", &doc.styles.named),
        ("auto", &doc.styles.content_automatic),
        ("sauto", &doc.styles.styles_automatic),
    ] {
        for (i, e) in list.iter().enumerate() {
            let key = format!("{scope}:{i:05}");
            let json = to_json(e);
            imp.base
                .styles
                .insert(key.clone(), (json.clone(), e.clone()));
            styles.insert(key, json);
        }
    }
    let meta = to_json(&doc.meta);
    imp.base.meta = (meta.clone(), doc.meta.clone());
    let mut l = LDoc { body, styles, meta };
    l.normalize();
    (l, imp.base)
}

impl Importer<'_> {
    fn embed(&mut self, b: &Block) -> LNode {
        self.embeds += 1;
        let id = match b.id() {
            Some(id) => id.0.clone(),
            None => format!("e{}", self.embeds),
        };
        let json = to_json(b);
        self.base
            .embeds
            .insert(id.clone(), (json.clone(), b.clone()));
        LNode::Embed { id, json }
    }

    fn container(
        &mut self,
        meta: &NodeMeta,
        kind: &str,
        attrs: &Attrs,
        children: Vec<LNode>,
        sig: Vec<Sig1>,
    ) -> LNode {
        let props = attrs_to_props(attrs);
        self.base
            .containers
            .insert(meta.id.0.clone(), (props.clone(), meta.clone(), sig));
        LNode::Container {
            id: meta.id.0.clone(),
            kind: kind.into(),
            props,
            children,
        }
    }

    fn block(&mut self, b: &Block) -> LNode {
        match b {
            Block::Paragraph(p) => LNode::Para(self.para(p)),
            Block::Section(s) => {
                let children = s.blocks.iter().map(|b| self.block(b)).collect();
                self.container(
                    &s.meta,
                    "section",
                    &s.attrs,
                    children,
                    blocks_sig(&s.blocks),
                )
            }
            Block::List(l) => {
                let mut children = Vec::new();
                for c in &l.children {
                    match c {
                        ListChild::Item(i) => {
                            let kids = i.blocks.iter().map(|b| self.block(b)).collect();
                            let kind = if i.header { "header" } else { "item" };
                            children.push(self.container(
                                &i.meta,
                                kind,
                                &i.attrs,
                                kids,
                                blocks_sig(&i.blocks),
                            ));
                        }
                        ListChild::Opaque(s) => {
                            children.push(self.embed(&Block::Opaque(s.clone())))
                        }
                    }
                }
                self.container(&l.meta, "list", &l.attrs, children, list_sig(&l.children))
            }
            Block::Table(t) => match self.table(t) {
                Some(lt) => LNode::Table(lt),
                None => self.embed(b),
            },
            Block::TrackedChanges(_) | Block::Opaque(_) => self.embed(b),
        }
    }

    fn para(&mut self, p: &Paragraph) -> Para {
        let mut props = attrs_to_props(&p.attrs);
        if p.heading {
            props.insert("@h".into(), "1".into());
        }
        if let Some(sn) = p.style_name()
            && let Some(sig) = self.ix.by_name.get(sn)
            && sig.family == "paragraph"
        {
            props.remove("text:style-name");
            if let Some(parent) = &sig.parent {
                props.insert("@style".into(), parent.clone());
            }
            for (k, v) in &sig.delta {
                props.insert(k.clone(), v.clone());
            }
        }
        let mut runs = Vec::new();
        self.inlines(&p.content, &BTreeMap::new(), &mut runs);
        let mut para = Para {
            id: p.meta.id.0.clone(),
            props,
            runs,
        };
        para.normalize();
        self.base
            .paras
            .insert(para.id.clone(), (para.clone(), p.clone()));
        para
    }

    fn inlines(&self, v: &[Inline], marks: &BTreeMap<String, String>, out: &mut Vec<Run>) {
        for i in v {
            match i {
                Inline::Text(t) if !t.text.contains(['\n', OBJ]) => out.push(Run {
                    text: t.text.clone(),
                    marks: marks.clone(),
                }),
                // A mark needs characters to sit on. An empty span (LibreOffice
                // writes them for the format of an empty paragraph) rides as
                // an inline object instead.
                Inline::Span(s) if !s.content.is_empty() => {
                    let mut m = marks.clone();
                    let mut extra = Attrs::default();
                    for a in s.attrs.iter() {
                        if a.name != "text:style-name" {
                            extra.0.push(a.clone());
                        }
                    }
                    if let Some(sn) = s.style_name() {
                        match self.ix.by_name.get(sn) {
                            Some(sig) if sig.family == "text" => {
                                if let Some(p) = &sig.parent {
                                    m.insert("cs".into(), p.clone());
                                }
                                for (k, v) in &sig.delta {
                                    m.insert(k.clone(), v.clone());
                                }
                            }
                            _ => {
                                m.insert("cs".into(), sn.to_owned());
                            }
                        }
                    }
                    if !extra.is_empty() || s.meta.id_from_source {
                        let id = s.meta.id_from_source.then(|| s.meta.id.0.clone());
                        m.insert(
                            "span".into(),
                            serde_json::json!({"id": id, "attrs": extra}).to_string(),
                        );
                    }
                    self.inlines(&s.content, &m, out);
                }
                Inline::Link(s) if !s.content.is_empty() => {
                    let mut m = marks.clone();
                    let id = s.meta.id_from_source.then(|| s.meta.id.0.clone());
                    m.insert(
                        "link".into(),
                        serde_json::json!({"id": id, "attrs": s.attrs}).to_string(),
                    );
                    self.inlines(&s.content, &m, out);
                }
                other => {
                    let mut m = marks.clone();
                    m.insert("obj".into(), to_json(other));
                    out.push(Run {
                        text: OBJ.to_string(),
                        marks: m,
                    });
                }
            }
        }
    }

    fn table(&mut self, t: &Table) -> Option<LTable> {
        let mut lt = LTable {
            id: t.meta.id.0.clone(),
            props: attrs_to_props(&t.attrs),
            ..Default::default()
        };
        let mut rows: Vec<(&TableRow, Option<&str>)> = Vec::new();
        let mut groups = Vec::new();
        for c in &t.children {
            match c {
                TableChild::Column(col) => self.column(&mut lt, col, None),
                TableChild::Row(r) => rows.push((r, None)),
                TableChild::Group(g) => match g.name.as_str() {
                    "table:table-columns" | "table:table-header-columns" => {
                        for gc in &g.children {
                            match gc {
                                TableChild::Column(col) => self.column(&mut lt, col, Some(&g.name)),
                                TableChild::Opaque(s) if s.is_whitespace() => {}
                                _ => return None,
                            }
                        }
                        groups.push(g);
                    }
                    "table:table-header-rows" | "table:table-rows" => {
                        for gc in &g.children {
                            match gc {
                                TableChild::Row(r) => rows.push((r, Some(&g.name))),
                                TableChild::Opaque(s) if s.is_whitespace() => {}
                                _ => return None,
                            }
                        }
                        groups.push(g);
                    }
                    _ => return None,
                },
                TableChild::Opaque(s) if s.is_whitespace() => {}
                TableChild::Opaque(_) => return None,
            }
        }
        let ncols = lt.cols.len();
        for (r, group) in rows {
            let mut props = attrs_to_props(&r.attrs);
            if let Some(g) = group {
                props.insert("@group".into(), g.into());
            }
            let mut cells = Vec::new();
            for c in &r.cells {
                match c {
                    CellChild::Cell(c) => {
                        if c.attrs.get("table:number-columns-repeated").is_some() {
                            return None;
                        }
                        let mut cp = attrs_to_props(&c.attrs);
                        if c.covered {
                            cp.insert("@covered".into(), "1".into());
                        }
                        let blocks = c
                            .blocks
                            .iter()
                            .map(|b| match b {
                                Block::Paragraph(p) => LNode::Para(self.para(p)),
                                other => self.embed(other),
                            })
                            .collect();
                        cells.push(LCell {
                            id: c.meta.id.0.clone(),
                            props: cp,
                            blocks,
                        });
                    }
                    CellChild::Opaque(s) if s.is_whitespace() => {}
                    CellChild::Opaque(_) => return None,
                }
            }
            if cells.len() != ncols {
                return None;
            }
            for c in &cells {
                let tc = r.cells.iter().find_map(|x| match x {
                    CellChild::Cell(tc) if tc.meta.id.0 == c.id => Some(tc.clone()),
                    _ => None,
                })?;
                self.base.cells.insert(c.id.clone(), (c.clone(), tc));
            }
            self.base.rows.insert(
                r.meta.id.0.clone(),
                (props.clone(), cells.clone(), r.clone()),
            );
            lt.rows.push((r.meta.id.0.clone(), props));
            lt.cells.push(cells);
        }
        for g in groups {
            self.base
                .groups
                .insert((lt.id.clone(), g.name.clone()), g.clone());
        }
        let mut lt_norm = lt.clone();
        for row in &mut lt_norm.cells {
            for c in row {
                for b in &mut c.blocks {
                    if let LNode::Para(p) = b {
                        p.normalize()
                    }
                }
            }
        }
        self.base.tables.insert(lt.id.clone(), (lt_norm, t.clone()));
        Some(lt)
    }

    fn column(&mut self, lt: &mut LTable, col: &TableColumn, group: Option<&str>) {
        let mut props = attrs_to_props(&col.attrs);
        props.remove("table:number-columns-repeated");
        if let Some(o) = props.get_mut(ORDER) {
            *o = o
                .split(' ')
                .filter(|n| *n != "table:number-columns-repeated")
                .collect::<Vec<_>>()
                .join(" ");
        }
        if let Some(g) = group {
            props.insert("@group".into(), g.into());
        }
        // The explicit width, for columns that lose their style.
        if let Some(sn) = col.style_name()
            && let Some(sig) = self.ix.by_name.get(sn)
            && let Some((_, w)) = sig
                .delta
                .iter()
                .find(|(k, _)| k == "Δstyle:table-column-properties|style:column-width")
        {
            props.insert("@width".into(), w.clone());
        }
        let n = col.repeat();
        if n > 1 {
            props.insert("@rep".into(), col.meta.id.0.clone());
        }
        self.base
            .cols
            .insert(col.meta.id.0.clone(), (props.clone(), n, col.clone()));
        for i in 0..n {
            let id = if n == 1 {
                col.meta.id.0.clone()
            } else {
                format!("{}.{}", col.meta.id.0, i)
            };
            lt.cols.push((id, props.clone()));
        }
    }
}

// ---------------------------------------------------------------------------
// Export.

/// The model rebuilt from the CRDT, and which source nodes changed.
pub struct Exported {
    pub doc: Document,
    /// Origins of every source node that was edited, regenerated or deleted:
    /// what the fidelity check must not hold to byte identity.
    pub touched: Vec<Origin>,
    pub regenerated: usize,
    pub reused: usize,
    /// Automatic styles minted for direct formatting the source never had.
    pub styles_added: usize,
}

struct Exporter<'a> {
    base: &'a Baseline,
    minter: Minter,
    seen: HashSet<String>,
    touched: Vec<Origin>,
    regenerated: usize,
    reused: usize,
}

fn fresh_meta(id: &str) -> NodeMeta {
    NodeMeta {
        id: NodeId(id.to_owned()),
        id_from_source: false,
        origin: None,
        dirty: true,
        stamp_id: false,
        descendants_dirty: false,
        children_changed: false,
    }
}

/// Turn the CRDT's logical document back into a model document.
pub fn export(l: &LDoc, base: &Baseline) -> Exported {
    let mut l = l.clone();
    l.normalize();
    let mut touched = Vec::new();
    let mut styles = Styles::default();
    for (key, json) in &l.styles {
        let entry = match base.styles.get(key) {
            Some((j, e)) if j == json => e.clone(),
            _ => {
                if let Some((_, e)) = base.styles.get(key)
                    && let Some(o) = style_origin(e)
                {
                    touched.push(o);
                }
                match from_json::<StyleEntry>(json) {
                    Some(e) => e,
                    None => continue,
                }
            }
        };
        match key.split_once(':').map(|(s, _)| s) {
            Some("named") => styles.named.push(entry),
            Some("auto") => styles.content_automatic.push(entry),
            _ => styles.styles_automatic.push(entry),
        }
    }
    let mut ex = Exporter {
        base,
        minter: Minter {
            ix: StyleIndex::new(&styles),
            created: Vec::new(),
        },
        seen: HashSet::new(),
        touched,
        regenerated: 0,
        reused: 0,
    };
    let body = ex.nodes(&l.body);
    let styles_added = ex.minter.created.len();
    styles.content_automatic.append(&mut ex.minter.created);
    let meta = if l.meta == base.meta.0 {
        base.meta.1.clone()
    } else {
        from_json(&l.meta).unwrap_or_else(|| base.meta.1.clone())
    };
    // Source nodes that no longer exist were deleted: touched.
    for (id, o) in base.origins() {
        if !ex.seen.contains(id) {
            ex.touched.push(o);
        }
    }
    Exported {
        doc: Document { body, styles, meta },
        touched: ex.touched,
        regenerated: ex.regenerated,
        reused: ex.reused,
        styles_added,
    }
}

fn style_origin(e: &StyleEntry) -> Option<Origin> {
    match e {
        StyleEntry::Style(s) => s.meta.origin,
        StyleEntry::List(l) => l.meta.origin,
        StyleEntry::Opaque(_) => None,
    }
}

impl Exporter<'_> {
    /// Meta for a node: its original (clean) if its own properties are
    /// unchanged, its original marked dirty if they changed, or fresh.
    fn meta(&mut self, id: &str, original: Option<(&Props, &NodeMeta)>, props: &Props) -> NodeMeta {
        self.seen.insert(id.to_owned());
        match original {
            Some((p, m)) if p == props => {
                self.reused += 1;
                m.clone()
            }
            Some((_, m)) => {
                self.regenerated += 1;
                if let Some(o) = m.origin {
                    self.touched.push(o);
                }
                NodeMeta {
                    dirty: true,
                    ..m.clone()
                }
            }
            None => {
                self.regenerated += 1;
                fresh_meta(id)
            }
        }
    }

    /// A clean node whose list of children changed (one added, deleted or
    /// moved) is written by passthrough: its own tags are copied, its
    /// children are visited. Flag it for the writer, and count it as touched
    /// because its bytes change even though nothing in it was edited.
    fn settle_children(&mut self, meta: &mut NodeMeta, before: &[Sig1], after: Vec<Sig1>) {
        if !meta.dirty
            && before != after.as_slice()
            && let Some(o) = meta.origin
        {
            meta.children_changed = true;
            self.touched.push(o);
        }
    }

    fn nodes(&mut self, v: &[LNode]) -> Vec<Block> {
        v.iter().filter_map(|n| self.node(n)).collect()
    }

    fn node(&mut self, n: &LNode) -> Option<Block> {
        Some(match n {
            LNode::Para(p) => Block::Paragraph(self.para(p)),
            LNode::Embed { id, json } => {
                self.seen.insert(id.clone());
                match self.base.embeds.get(id) {
                    Some((j, b)) if j == json => {
                        self.reused += 1;
                        b.clone()
                    }
                    other => {
                        self.regenerated += 1;
                        if let Some(o) = other.and_then(|(_, b)| block_origin(b)) {
                            self.touched.push(o);
                        }
                        from_json(json)?
                    }
                }
            }
            LNode::Container {
                id,
                kind,
                props,
                children,
            } => {
                let orig = self.base.containers.get(id).map(|(p, m, _)| (p, m));
                let mut meta = self.meta(id, orig, props);
                let before = self
                    .base
                    .containers
                    .get(id)
                    .map(|c| c.2.clone())
                    .unwrap_or_default();
                let attrs = props_to_attrs(props, &[]);
                match kind.as_str() {
                    "list" => {
                        let mut items = Vec::new();
                        for c in children {
                            match c {
                                LNode::Container {
                                    id,
                                    kind,
                                    props,
                                    children,
                                } if kind == "item" || kind == "header" => {
                                    let orig = self.base.containers.get(id).map(|(p, m, _)| (p, m));
                                    let mut meta = self.meta(id, orig, props);
                                    let blocks = self.nodes(children);
                                    let b4 = self
                                        .base
                                        .containers
                                        .get(id)
                                        .map(|c| c.2.clone())
                                        .unwrap_or_default();
                                    self.settle_children(&mut meta, &b4, blocks_sig(&blocks));
                                    items.push(ListChild::Item(ListItem {
                                        meta,
                                        header: kind == "header",
                                        attrs: props_to_attrs(props, &[]),
                                        blocks,
                                    }));
                                }
                                other => {
                                    if let Some(Block::Opaque(s)) = self.node(other) {
                                        items.push(ListChild::Opaque(s));
                                    }
                                }
                            }
                        }
                        self.settle_children(&mut meta, &before, list_sig(&items));
                        Block::List(List {
                            meta,
                            attrs,
                            children: items,
                        })
                    }
                    _ => {
                        let blocks = self.nodes(children);
                        self.settle_children(&mut meta, &before, blocks_sig(&blocks));
                        Block::Section(Section {
                            meta,
                            attrs,
                            blocks,
                        })
                    }
                }
            }
            LNode::Table(t) => Block::Table(self.table(t)),
        })
    }

    fn para(&mut self, p: &Para) -> Paragraph {
        self.seen.insert(p.id.clone());
        if let Some((bp, orig)) = self.base.paras.get(&p.id) {
            if bp == p {
                self.reused += 1;
                return orig.clone();
            }
            if let Some(o) = orig.meta.origin {
                self.touched.push(o);
            }
        }
        self.regenerated += 1;
        let parent = p
            .props
            .get("@style")
            .or_else(|| p.props.get("text:style-name"))
            .cloned();
        let delta = delta_of(&p.props);
        let style = if delta.is_empty() {
            parent
        } else {
            Some(self.minter.resolve("paragraph", parent.as_deref(), &delta))
        };
        let attrs = props_to_attrs(&p.props, &[("text:style-name", style)]);
        let meta = match self.base.paras.get(&p.id) {
            Some((_, orig)) => NodeMeta {
                dirty: true,
                ..orig.meta.clone()
            },
            None => fresh_meta(&p.id),
        };
        Paragraph {
            meta,
            heading: p.props.contains_key("@h"),
            attrs,
            content: self.inlines(&p.runs),
        }
    }

    fn inlines(&mut self, runs: &[Run]) -> Vec<Inline> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < runs.len() {
            let link = runs[i].marks.get("link");
            let mut j = i;
            while j < runs.len() && runs[j].marks.get("link") == link {
                j += 1;
            }
            let inner = self.spans(&runs[i..j]);
            match link.and_then(|l| serde_json::from_str::<Value>(l).ok()) {
                Some(v) => {
                    let attrs: Attrs =
                        serde_json::from_value(v["attrs"].clone()).unwrap_or_default();
                    let meta = match v["id"].as_str() {
                        Some(id) => NodeMeta {
                            id_from_source: true,
                            ..fresh_meta(id)
                        },
                        None => NodeMeta::new(),
                    };
                    out.push(Inline::Link(Span {
                        meta,
                        attrs,
                        content: inner,
                    }));
                }
                None => out.extend(inner),
            }
            i = j;
        }
        out
    }

    fn spans(&mut self, runs: &[Run]) -> Vec<Inline> {
        let key = |r: &Run| -> BTreeMap<String, String> {
            r.marks
                .iter()
                .filter(|(k, _)| *k != "link" && *k != "obj")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        let mut out = Vec::new();
        let mut i = 0;
        while i < runs.len() {
            let k = key(&runs[i]);
            let mut j = i;
            while j < runs.len() && key(&runs[j]) == k {
                j += 1;
            }
            let mut content = Vec::new();
            for r in &runs[i..j] {
                match r.marks.get("obj").and_then(|o| from_json::<Inline>(o)) {
                    Some(obj) => content.push(obj),
                    None => match content.last_mut() {
                        Some(Inline::Text(t)) => t.text.push_str(&r.text),
                        _ => content.push(Inline::Text(Text::new(r.text.clone()))),
                    },
                }
            }
            if k.is_empty() {
                out.extend(content);
            } else {
                let delta = delta_of(&k);
                let cs = k.get("cs").cloned();
                let style = if delta.is_empty() {
                    cs
                } else {
                    Some(self.minter.resolve("text", cs.as_deref(), &delta))
                };
                let extra = k
                    .get("span")
                    .and_then(|s| serde_json::from_str::<Value>(s).ok());
                let mut attrs = Attrs::default();
                if let Some(s) = style {
                    attrs.set("text:style-name", s);
                }
                let mut meta = NodeMeta::new();
                if let Some(v) = extra {
                    let more: Attrs =
                        serde_json::from_value(v["attrs"].clone()).unwrap_or_default();
                    for a in more.iter() {
                        attrs.set(&a.name, a.value.clone());
                    }
                    if let Some(id) = v["id"].as_str() {
                        meta = NodeMeta {
                            id_from_source: true,
                            ..fresh_meta(id)
                        };
                    }
                }
                out.push(Inline::Span(Span {
                    meta,
                    attrs,
                    content,
                }));
            }
            i = j;
        }
        out
    }

    fn table(&mut self, t: &LTable) -> Table {
        let t = &normalize_spans(t);
        self.seen.insert(t.id.clone());
        if let Some((bt, orig)) = self.base.tables.get(&t.id)
            && bt == t
        {
            self.reused += 1;
            // Every node inside is unchanged too.
            mark_seen_table(orig, &mut self.seen);
            return orig.clone();
        }
        let orig = self
            .base
            .tables
            .get(&t.id)
            .map(|(bt, o)| (&bt.props, &o.meta));
        let mut meta = self.meta(&t.id, orig, &t.props);
        let attrs = props_to_attrs(&t.props, &[]);
        let table_name = attrs.get("table:name").unwrap_or("Table").to_owned();

        // Columns: runs of identical properties compress back into one
        // repeated element, the way LibreOffice writes them.
        let mut cols: Vec<(Option<String>, TableChild)> = Vec::new();
        let mut i = 0;
        while i < t.cols.len() {
            let (id, props) = &t.cols[i];
            let mut j = i + 1;
            while j < t.cols.len() && t.cols[j].1 == *props && props.contains_key("@rep") {
                j += 1;
            }
            let n = j - i;
            let src_id = props.get("@rep").cloned().unwrap_or_else(|| id.clone());
            let col = match self.base.cols.get(&src_id) {
                Some((p, rep, c)) if p == props && *rep == n => {
                    self.seen.insert(src_id.clone());
                    self.reused += 1;
                    c.clone()
                }
                other => {
                    self.seen.insert(src_id.clone());
                    self.regenerated += 1;
                    let meta = match other {
                        Some((_, _, c)) => {
                            if let Some(o) = c.meta.origin {
                                self.touched.push(o);
                            }
                            NodeMeta {
                                dirty: true,
                                ..c.meta.clone()
                            }
                        }
                        None => fresh_meta(&src_id),
                    };
                    let mut style = props.get("table:style-name").cloned();
                    if style.is_none()
                        && let Some(w) = props.get("@width")
                    {
                        let mut d = BTreeMap::new();
                        d.insert(
                            "Δstyle:table-column-properties|style:column-width",
                            w.as_str(),
                        );
                        let _ = &table_name;
                        style = Some(self.minter.resolve("table-column", None, &d));
                    }
                    let mut attrs = props_to_attrs(props, &[("table:style-name", style)]);
                    if n > 1 {
                        attrs.set("table:number-columns-repeated", n.to_string());
                    }
                    TableColumn { meta, attrs }
                }
            };
            cols.push((props.get("@group").cloned(), TableChild::Column(col)));
            i = j;
        }

        let mut rows: Vec<(Option<String>, TableChild)> = Vec::new();
        for (ri, (rid, props)) in t.rows.iter().enumerate() {
            let cells = &t.cells[ri];
            self.seen.insert(rid.clone());
            if let Some((bp, bc, orig)) = self.base.rows.get(rid)
                && bp == props
                && bc == cells
            {
                self.reused += 1;
                for c in cells {
                    self.seen.insert(c.id.clone());
                    for b in &c.blocks {
                        self.seen.insert(b.id().to_owned());
                    }
                }
                rows.push((props.get("@group").cloned(), TableChild::Row(orig.clone())));
                continue;
            }
            let orig = self.base.rows.get(rid).map(|(p, _, r)| (p, &r.meta));
            let mut meta = self.meta(rid, orig, props);
            let before = self
                .base
                .rows
                .get(rid)
                .map(|r| cells_sig(&r.2.cells))
                .unwrap_or_default();
            let mut out_cells = Vec::new();
            for (ci, c) in cells.iter().enumerate() {
                out_cells.push(CellChild::Cell(self.cell(c, cells, ci)));
            }
            self.settle_children(&mut meta, &before, cells_sig(&out_cells));
            rows.push((
                props.get("@group").cloned(),
                TableChild::Row(TableRow {
                    meta,
                    attrs: props_to_attrs(props, &[]),
                    cells: out_cells,
                }),
            ));
        }

        let mut children = Vec::new();
        for (group, child) in cols.into_iter().chain(rows) {
            match group {
                None => children.push(child),
                Some(g) => {
                    if let Some(TableChild::Group(last)) = children.last_mut()
                        && last.name == g
                    {
                        last.children.push(child);
                        continue;
                    }
                    let mut grp = match self.base.groups.get(&(t.id.clone(), g.clone())) {
                        Some(o) => o.clone(),
                        None => TableGroup {
                            meta: NodeMeta::new(),
                            name: g.clone(),
                            attrs: Attrs::default(),
                            children: Vec::new(),
                        },
                    };
                    grp.children = vec![child];
                    children.push(TableChild::Group(grp));
                }
            }
        }
        if let Some((_, orig)) = self.base.tables.get(&t.id) {
            let orig = orig.clone();
            self.settle_children(&mut meta, &table_sig(&orig.children), table_sig(&children));
            for c in &mut children {
                if let TableChild::Group(g) = c {
                    let before = orig
                        .children
                        .iter()
                        .find_map(|o| match o {
                            TableChild::Group(og) if og.meta.id == g.meta.id => {
                                Some(table_sig(&og.children))
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    let after = table_sig(&g.children);
                    let mut m = g.meta.clone();
                    self.settle_children(&mut m, &before, after);
                    g.meta = m;
                }
            }
        }
        Table {
            meta,
            attrs,
            children,
        }
    }

    fn cell(&mut self, c: &LCell, row: &[LCell], ci: usize) -> TableCell {
        self.seen.insert(c.id.clone());
        if let Some((bc, orig)) = self.base.cells.get(&c.id)
            && bc == c
        {
            self.reused += 1;
            for b in &c.blocks {
                self.seen.insert(b.id().to_owned());
            }
            return orig.clone();
        }
        let orig = self
            .base
            .cells
            .get(&c.id)
            .map(|(bc, o)| (&bc.props, &o.meta));
        let mut meta = self.meta(&c.id, orig, &c.props);
        let before = self
            .base
            .cells
            .get(&c.id)
            .map(|(_, o)| blocks_sig(&o.blocks))
            .unwrap_or_default();
        let mut blocks = self.nodes(&c.blocks);
        if blocks.is_empty() && !c.props.contains_key("@covered") {
            // An empty cell still holds one paragraph; borrow a neighbour's style.
            let style = row
                .iter()
                .take(ci)
                .rev()
                .chain(row.iter().skip(ci + 1))
                .find_map(|n| {
                    n.blocks.iter().find_map(|b| match b {
                        LNode::Para(p) => p.props.get("text:style-name").cloned(),
                        _ => None,
                    })
                });
            let mut attrs = Attrs::default();
            if let Some(s) = style {
                attrs.set("text:style-name", s);
            }
            blocks.push(Block::Paragraph(Paragraph {
                meta: NodeMeta::new(),
                heading: false,
                attrs,
                content: Vec::new(),
            }));
        }
        self.settle_children(&mut meta, &before, blocks_sig(&blocks));
        TableCell {
            meta,
            covered: c.props.contains_key("@covered"),
            attrs: props_to_attrs(&c.props, &[]),
            blocks,
        }
    }
}

/// A child as the writer sees it: where its source bytes start, `None` for
/// a new node. Slots count as present (they are carried, never edited).
pub type Sig1 = Option<usize>;

const SLOT: Sig1 = Some(usize::MAX);

fn blocks_sig(v: &[Block]) -> Vec<Sig1> {
    v.iter()
        .map(|b| match b {
            Block::Opaque(_) => SLOT,
            b => block_origin(b).map(|o| o.start),
        })
        .collect()
}

fn list_sig(v: &[ListChild]) -> Vec<Sig1> {
    v.iter()
        .map(|c| match c {
            ListChild::Item(i) => i.meta.origin.map(|o| o.start),
            ListChild::Opaque(_) => SLOT,
        })
        .collect()
}

fn cells_sig(v: &[CellChild]) -> Vec<Sig1> {
    v.iter()
        .map(|c| match c {
            CellChild::Cell(c) => c.meta.origin.map(|o| o.start),
            CellChild::Opaque(_) => SLOT,
        })
        .collect()
}

fn table_sig(v: &[TableChild]) -> Vec<Sig1> {
    v.iter()
        .map(|c| match c {
            TableChild::Column(c) => c.meta.origin.map(|o| o.start),
            TableChild::Row(r) => r.meta.origin.map(|o| o.start),
            TableChild::Group(g) => g.meta.origin.map(|o| o.start),
            TableChild::Opaque(_) => SLOT,
        })
        .collect()
}

fn mark_seen_table(t: &Table, seen: &mut HashSet<String>) {
    fn blocks(v: &[Block], seen: &mut HashSet<String>) {
        for b in v {
            if let Some(id) = b.id() {
                seen.insert(id.0.clone());
            }
        }
    }
    fn walk(v: &[TableChild], seen: &mut HashSet<String>) {
        for c in v {
            match c {
                TableChild::Column(c) => {
                    seen.insert(c.meta.id.0.clone());
                }
                TableChild::Row(r) => {
                    seen.insert(r.meta.id.0.clone());
                    for c in &r.cells {
                        if let CellChild::Cell(c) = c {
                            seen.insert(c.meta.id.0.clone());
                            blocks(&c.blocks, seen);
                        }
                    }
                }
                TableChild::Group(g) => walk(&g.children, seen),
                TableChild::Opaque(_) => {}
            }
        }
    }
    walk(&t.children, seen);
}

/// After concurrent row and column deletions, merged cells can point past
/// the table or at cells that are no longer covered. Clamp spans to what
/// is actually covered, and uncover cells nothing covers.
pub fn normalize_spans(t: &LTable) -> LTable {
    let mut t = t.clone();
    let rows = t.cells.len();
    let cols = t.cols.len();
    let mut covered_by_span = vec![vec![false; cols]; rows];
    for r in 0..rows {
        let mut c = 0;
        while c < cols {
            let cell = &t.cells[r][c];
            if cell.props.contains_key("@covered") {
                c += 1;
                continue;
            }
            let span = |k: &str| {
                cell.props
                    .get(k)
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1)
            };
            let (cs, rs) = (
                span("table:number-columns-spanned"),
                span("table:number-rows-spanned"),
            );
            let mut ncs = 1;
            while ncs < cs && c + ncs < cols && t.cells[r][c + ncs].props.contains_key("@covered") {
                ncs += 1;
            }
            let mut nrs = 1;
            while nrs < rs && r + nrs < rows && t.cells[r + nrs][c].props.contains_key("@covered") {
                nrs += 1;
            }
            for (rr, row) in covered_by_span.iter_mut().enumerate().skip(r).take(nrs) {
                for (cc, flag) in row.iter_mut().enumerate().skip(c).take(ncs) {
                    if (rr, cc) != (r, c) {
                        *flag = true;
                    }
                }
            }
            let cell = &mut t.cells[r][c];
            for (k, n, orig) in [
                ("table:number-columns-spanned", ncs, cs),
                ("table:number-rows-spanned", nrs, rs),
            ] {
                if n != orig {
                    cell.props.insert(k.into(), n.to_string());
                }
            }
            c += 1;
        }
    }
    for (r, row) in t.cells.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            if cell.props.contains_key("@covered") && !covered_by_span[r][c] {
                cell.props.remove("@covered");
            }
        }
    }
    t
}
