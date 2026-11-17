// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::*;

fn style(family: &str, name: &str, parent: Option<&str>, props: &[(&str, &str, &str)]) -> Style {
    let mut s = Style {
        meta: NodeMeta::new(),
        default: false,
        attrs: Attrs::from([("style:name", name), ("style:family", family)]),
        children: Vec::new(),
    };
    if let Some(p) = parent {
        s.attrs.set("style:parent-style-name", p);
    }
    for (set, k, v) in props {
        s.props_mut(set).attrs.set(k, *v);
    }
    s
}

fn sample_styles() -> Styles {
    let mut heading = style(
        "paragraph",
        "Heading_20_2",
        Some("Heading"),
        &[("style:text-properties", "fo:font-size", "14pt")],
    );
    heading.attrs.set("style:display-name", "Heading 2");
    Styles {
        named: vec![
            StyleEntry::Style(Style {
                meta: NodeMeta::new(),
                default: true,
                attrs: Attrs::from([("style:family", "paragraph")]),
                children: vec![StyleChild::Props(PropertySet {
                    meta: NodeMeta::new(),
                    name: "style:text-properties".into(),
                    attrs: Attrs::from([("fo:language", "da"), ("fo:font-size", "12pt")]),
                    children: vec![],
                })],
            }),
            StyleEntry::Style(style(
                "paragraph",
                "Heading",
                None,
                &[("style:text-properties", "style:font-name", "Inter")],
            )),
            StyleEntry::Style(heading),
        ],
        content_automatic: vec![StyleEntry::Style(style(
            "paragraph",
            "P1",
            Some("Heading_20_2"),
            &[
                ("style:text-properties", "fo:font-weight", "bold"),
                ("style:text-properties", "officeooo:rsid", "0012ab"),
            ],
        ))],
        styles_automatic: vec![],
    }
}

#[test]
fn minted_ids_are_ncnames_and_unique() {
    let a = NodeId::mint();
    let b = NodeId::mint();
    assert_ne!(a, b);
    assert!(a.is_minted());
    assert!(a.as_str().chars().next().unwrap().is_ascii_alphabetic());
}

#[test]
fn imported_ids_are_kept() {
    let o = Origin {
        part: PartKind::Content,
        start: 0,
        end: 10,
        start_tag_end: 5,
    };
    let m = NodeMeta::imported(o, Some("id42"));
    assert_eq!(m.id.as_str(), "id42");
    assert!(m.id_from_source);
    assert!(m.is_verbatim());
    let n = NodeMeta::imported(o, None);
    assert!(n.id.is_minted());
}

#[test]
fn attrs_keep_order_and_replace_in_place() {
    let mut a = Attrs::from([("a", "1"), ("b", "2"), ("c", "3")]);
    a.set("b", "20");
    a.set("d", "4");
    let names: Vec<_> = a.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["a", "b", "c", "d"]);
    assert_eq!(a.get("b"), Some("20"));
    assert_eq!(a.remove("a").as_deref(), Some("1"));
    assert_eq!(a.len(), 3);
}

#[test]
fn inheritance_walks_parents_then_default() {
    let s = sample_styles();
    let r = s
        .resolve("paragraph", "P1", "style:text-properties", "fo:font-weight")
        .unwrap();
    assert_eq!((r.value, r.from_style, r.automatic), ("bold", "P1", true));
    let r = s
        .resolve("paragraph", "P1", "style:text-properties", "fo:font-size")
        .unwrap();
    assert_eq!((r.value, r.from_style), ("14pt", "Heading_20_2"));
    let r = s
        .resolve(
            "paragraph",
            "P1",
            "style:text-properties",
            "style:font-name",
        )
        .unwrap();
    assert_eq!(r.from_style, "Heading");
    let r = s
        .resolve("paragraph", "P1", "style:text-properties", "fo:language")
        .unwrap();
    assert_eq!((r.value, r.from_style), ("da", "(default)"));
    assert!(
        s.resolve("paragraph", "P1", "style:text-properties", "fo:color")
            .is_none()
    );
}

#[test]
fn explanation_reads_like_principle_one() {
    let s = sample_styles();
    assert_eq!(
        s.explain("paragraph", "P1").sentence(),
        "Heading 2 + bold added by hand"
    );
    assert_eq!(
        s.explain("paragraph", "Heading_20_2").sentence(),
        "Heading 2"
    );
}

#[test]
fn inheritance_cycle_does_not_hang() {
    let mut s = Styles::default();
    s.named
        .push(StyleEntry::Style(style("paragraph", "A", Some("B"), &[])));
    s.named
        .push(StyleEntry::Style(style("paragraph", "B", Some("A"), &[])));
    assert!(
        s.resolve("paragraph", "A", "style:text-properties", "x")
            .is_none()
    );
}

#[test]
fn multilevel_numbering_comes_from_list_style_linked_by_paragraph_style() {
    let mut s = Styles::default();
    let mut numbered = style("paragraph", "Numbered", None, &[]);
    numbered.attrs.set("style:list-style-name", "Outline");
    s.named.push(StyleEntry::Style(numbered));
    s.named.push(StyleEntry::Style(style(
        "paragraph",
        "Child",
        Some("Numbered"),
        &[],
    )));
    let levels = (1..=3)
        .map(|n| {
            ListLevelChild::Level(ListLevel {
                meta: NodeMeta::new(),
                kind: ListLevelKind::Number,
                attrs: Attrs(vec![
                    Attr {
                        name: "text:level".into(),
                        value: n.to_string(),
                    },
                    Attr {
                        name: "style:num-format".into(),
                        value: "1".into(),
                    },
                    Attr {
                        name: "text:display-levels".into(),
                        value: n.to_string(),
                    },
                ]),
                children: vec![],
            })
        })
        .collect();
    s.named.push(StyleEntry::List(ListStyle {
        meta: NodeMeta::new(),
        outline: false,
        attrs: Attrs::from([("style:name", "Outline")]),
        levels,
    }));
    let l = s.list_style_for_paragraph_style("Child").unwrap();
    assert_eq!(l.level(3).unwrap().display_levels(), 3);
    assert_eq!(l.level(2).unwrap().num_format(), Some("1"));
}

#[test]
fn table_columns_expand_repeats() {
    let col = |name: &str, rep: Option<&str>| {
        let mut attrs = Attrs::from([("table:style-name", name)]);
        if let Some(r) = rep {
            attrs.set("table:number-columns-repeated", r);
        }
        TableChild::Column(TableColumn {
            meta: NodeMeta::new(),
            attrs,
        })
    };
    let t = Table {
        meta: NodeMeta::new(),
        attrs: Attrs::from([("table:name", "T1")]),
        children: vec![col("A", None), col("B", Some("3"))],
    };
    let names: Vec<_> = t
        .columns()
        .iter()
        .map(|c| c.style_name().unwrap())
        .collect();
    assert_eq!(names, ["A", "B", "B", "B"]);
}

#[test]
fn fields_store_target_and_format() {
    let f = Field {
        meta: NodeMeta::new(),
        name: "text:bookmark-ref".into(),
        kind: Field::kind_of("text:bookmark-ref").unwrap(),
        attrs: Attrs::from([
            ("text:reference-format", "page"),
            ("text:ref-name", "fig-map"),
        ]),
        display: vec![Inline::Text(Text::new("3"))],
    };
    assert_eq!(f.kind, FieldKind::Reference);
    assert_eq!(f.target(), Some("fig-map"));
    assert_eq!(f.format(), Some("page"));
}

#[test]
fn user_defined_meta_round_trips_through_api() {
    let mut m = Meta::default();
    assert_eq!(m.user_defined("skriv:doc-id"), None);
    m.set_user_defined("skriv:doc-id", "abc");
    m.set_user_defined("skriv:doc-id", "def");
    assert_eq!(m.user_defined("skriv:doc-id"), Some("def"));
    assert_eq!(m.entries.len(), 1);
}

#[test]
fn walking_reaches_nested_paragraphs() {
    let para = |t: &str| {
        Block::Paragraph(Paragraph {
            meta: NodeMeta::new(),
            heading: false,
            attrs: Attrs::default(),
            content: vec![Inline::Text(Text::new(t))],
        })
    };
    let mut doc = Document {
        body: vec![
            para("a"),
            Block::Section(Section {
                meta: NodeMeta::new(),
                attrs: Attrs::from([("text:name", "S")]),
                blocks: vec![Block::Table(Table {
                    meta: NodeMeta::new(),
                    attrs: Attrs::default(),
                    children: vec![TableChild::Row(TableRow {
                        meta: NodeMeta::new(),
                        attrs: Attrs::default(),
                        cells: vec![CellChild::Cell(TableCell {
                            meta: NodeMeta::new(),
                            covered: false,
                            attrs: Attrs::default(),
                            blocks: vec![para("b")],
                        })],
                    })],
                })],
            }),
        ],
        ..Default::default()
    };
    let texts: Vec<_> = paragraphs_mut(&mut doc)
        .iter()
        .map(|p| p.plain_text())
        .collect();
    assert_eq!(texts, ["a", "b"]);
    let counts = feature_counts(&mut doc);
    assert_eq!(counts[&Feature::Section], 1);
    assert_eq!(counts[&Feature::Table], 3); // table, row, cell
}

#[test]
fn new_attributes_respect_libreoffice_order_sensitivity() {
    let mut a = Attrs::from([("fo:country", "DK"), ("officeooo:rsid", "1")]);
    a.set("fo:language", "da");
    let names: Vec<_> = a.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["fo:language", "fo:country", "officeooo:rsid"]);
    let mut b = Attrs::from([("style:text-rotation-scale", "line-height")]);
    b.set("style:text-rotation-angle", "0");
    assert_eq!(b.0[0].name, "style:text-rotation-angle");
    // Updating keeps position.
    a.set("fo:country", "SE");
    assert_eq!(a.0[1].value, "SE");
}
