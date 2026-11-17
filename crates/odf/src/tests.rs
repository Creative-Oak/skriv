// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use super::*;
use skriv_model::*;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:loext="urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0" xmlns:x="urn:example:foreign" office:version="1.4""#;

/// A content.xml in LibreOffice's style (no pretty-printing) touching every
/// Spike 1 feature, plus a foreign element and attribute.
fn content_xml() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<office:document-content {ns}><office:scripts/><office:automatic-styles>"#,
            r#"<style:style style:name="P1" style:family="paragraph" style:parent-style-name="Heading_20_2"><style:text-properties fo:font-weight="bold" officeooo:rsid="00a1"/></style:style>"#,
            r#"<style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="4cm"/></style:style>"#,
            r#"<text:list-style style:name="L1"><text:list-level-style-number text:level="1" style:num-format="1"><style:list-level-properties/></text:list-level-style-number><text:list-level-style-number text:level="2" style:num-format="a" text:display-levels="2"/></text:list-style>"#,
            r#"</office:automatic-styles><office:body><office:text>"#,
            r#"<text:sequence-decls><text:sequence-decl text:display-outline-level="0" text:name="Figure"/></text:sequence-decls>"#,
            r#"<text:tracked-changes text:track-changes="false"><text:changed-region xml:id="ct1" text:id="ct1"><text:deletion><office:change-info><dc:creator>Anna</dc:creator><dc:date>2026-01-01T10:00:00</dc:date></office:change-info><text:p>gone &amp; deleted</text:p></text:deletion></text:changed-region></text:tracked-changes>"#,
            r#"<text:h text:style-name="P1" text:outline-level="2" xml:id="id1">Overskrift <text:bookmark text:name="b1"/>én</text:h>"#,
            r#"<text:p text:style-name="Standard">A<text:s text:c="3"/>B<text:tab/>C<text:change text:change-id="ct1"/> &lt;x&gt; <text:span text:style-name="T1">span</text:span> <text:a xlink:type="simple" xlink:href="https://example.org/?a=1&amp;b=2">link</text:a> <text:page-number text:select-page="current">7</text:page-number> <text:bookmark-ref text:reference-format="page" text:ref-name="b1">1</text:bookmark-ref><office:annotation office:name="c1"><dc:creator>Bo</dc:creator><dc:date>2026-02-02T00:00:00</dc:date><text:p>a comment</text:p></office:annotation>commented<office:annotation-end office:name="c1"/><x:foreign x:a="1">foreign</x:foreign></text:p>"#,
            r#"<text:list text:style-name="L1"><text:list-item><text:p>one</text:p><text:list><text:list-item><text:p>one.a</text:p></text:list-item></text:list></text:list-item><text:list-header><text:p>header</text:p></text:list-header></text:list>"#,
            r#"<text:section text:style-name="Sect1" text:name="Section1"><text:p x:extra="kept">in section</text:p></text:section>"#,
            r#"<table:table table:name="Table1" table:style-name="Table1"><table:table-column table:style-name="co1" table:number-columns-repeated="2"/><table:table-header-rows><table:table-row><table:table-cell office:value-type="string"><text:p>h1</text:p></table:table-cell><table:table-cell><text:p>h2</text:p></table:table-cell></table:table-row></table:table-header-rows><table:table-row><table:table-cell table:number-columns-spanned="2"><text:p>wide</text:p></table:table-cell><table:covered-table-cell/></table:table-row></table:table>"#,
            r#"<text:p><draw:frame draw:name="Image1" text:anchor-type="as-char" svg:width="2cm" svg:height="1cm"><draw:image xlink:href="Pictures/a.png" xlink:type="simple"/><svg:title>Map of Aarhus</svg:title></draw:frame><text:sequence text:ref-name="refFigure0" text:name="Figure" text:formula="ooow:Figure+1" style:num-format="1">1</text:sequence></text:p>"#,
            r#"<text:p/>"#,
            r#"</office:text></office:body></office:document-content>"#,
        ),
        ns = NS
    )
}

fn styles_xml() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<office:document-styles {ns}><office:styles>"#,
            r#"<style:default-style style:family="paragraph"><style:text-properties fo:language="da" fo:country="DK"/></style:default-style>"#,
            r#"<style:style style:name="Standard" style:family="paragraph" style:class="text"/>"#,
            r#"<style:style style:name="Heading" style:family="paragraph" style:parent-style-name="Standard"><style:text-properties style:font-name="Inter" fo:font-size="14pt"/></style:style>"#,
            r#"<style:style style:name="Heading_20_2" style:display-name="Heading 2" style:family="paragraph" style:parent-style-name="Heading" style:default-outline-level="2"/>"#,
            r#"<text:outline-style style:name="Outline"><text:outline-level-style text:level="1" style:num-format=""/></text:outline-style>"#,
            r#"</office:styles><office:automatic-styles><style:page-layout style:name="pm1"/></office:automatic-styles><office:master-styles><style:master-page style:name="Standard" style:page-layout-name="pm1"/></office:master-styles></office:document-styles>"#,
        ),
        ns = NS
    )
}

fn meta_xml() -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<office:document-meta {ns}><office:meta><dc:title>Prøve</dc:title><meta:user-defined meta:name="Afdeling">Teknik</meta:user-defined><meta:document-statistic meta:page-count="1"/></office:meta></office:document-meta>"#,
        ),
        ns = NS
    )
}

fn build_odt(content: &str, styles: &str, meta: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    w.start_file("mimetype", stored).unwrap();
    w.write_all(b"application/vnd.oasis.opendocument.text")
        .unwrap();
    for (name, data) in [
        ("content.xml", content.as_bytes()),
        ("styles.xml", styles.as_bytes()),
        ("meta.xml", meta.as_bytes()),
    ] {
        w.start_file(name, deflated).unwrap();
        w.write_all(data).unwrap();
    }
    for (name, data) in extra {
        w.start_file(*name, deflated).unwrap();
        w.write_all(data).unwrap();
    }
    w.start_file("META-INF/manifest.xml", deflated).unwrap();
    w.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/></manifest:manifest>"#).unwrap();
    w.finish().unwrap().into_inner()
}

fn sample() -> Vec<u8> {
    build_odt(
        &content_xml(),
        &styles_xml(),
        &meta_xml(),
        &[("skriv/extra.bin", b"\x00\x01opaque")],
    )
}

fn entry(bytes: &[u8], name: &str) -> Vec<u8> {
    package::Package::open(bytes.to_vec())
        .unwrap()
        .read(name)
        .unwrap()
        .unwrap()
}

#[test]
fn identity_round_trip_is_byte_identical_per_part() {
    let mut odt = read(sample()).unwrap();
    let w = odt.write().unwrap();
    assert_eq!(w.content_xml, content_xml());
    assert_eq!(w.styles_xml.as_deref(), Some(styles_xml().as_str()));
    assert_eq!(w.meta_xml.as_deref(), Some(meta_xml().as_str()));
    assert_eq!(w.stats.regenerated_nodes, 0);
    let orig = sample();
    for name in odt.part_names() {
        assert_eq!(entry(&w.bytes, name), entry(&orig, name), "{name}");
    }
    assert_eq!(odt.part_names()[0], "mimetype");
}

#[test]
fn regenerating_everything_reproduces_libreoffice_style_xml() {
    let mut odt = read(sample()).unwrap();
    touch_all(&mut odt.doc);
    let w = odt
        .write_with(WriteOptions {
            regenerate_text: true,
        })
        .unwrap();
    assert!(w.stats.regenerated_nodes > 50);
    assert_eq!(w.content_xml, content_xml());
    assert_eq!(w.styles_xml.as_deref(), Some(styles_xml().as_str()));
    assert_eq!(w.meta_xml.as_deref(), Some(meta_xml().as_str()));
}

#[test]
fn model_understands_the_core_subset() {
    let mut odt = read(sample()).unwrap();
    let doc = &odt.doc;
    // tracked changes, heading, paragraph, list, section, table, frame para, empty para + sequence-decls
    assert_eq!(doc.body.len(), 9);
    let Block::Paragraph(h) = &doc.body[2] else {
        panic!()
    };
    assert_eq!(h.outline_level(), Some(2));
    assert_eq!(h.meta.id.as_str(), "id1");
    assert_eq!(h.plain_text(), "Overskrift én");
    assert_eq!(
        doc.styles.explain("paragraph", "P1").sentence(),
        "Heading 2 + bold added by hand"
    );
    let Block::Paragraph(p) = &doc.body[3] else {
        panic!()
    };
    assert!(p.plain_text().starts_with("A   B\tC <x> span link 7 1"));
    let field = p
        .content
        .iter()
        .find_map(|i| match i {
            Inline::Field(f) if f.kind == FieldKind::Reference => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!((field.target(), field.format()), (Some("b1"), Some("page")));
    let ann = p
        .content
        .iter()
        .find_map(|i| match i {
            Inline::Annotation(a) => Some(a),
            _ => None,
        })
        .unwrap();
    assert_eq!(ann.author.as_deref(), Some("Bo"));
    assert!(
        p.content
            .iter()
            .any(|i| matches!(i, Inline::Opaque(s) if s.name.as_deref() == Some("x:foreign")))
    );
    let Block::Table(t) = &doc.body[6] else {
        panic!()
    };
    assert_eq!(t.columns().len(), 2);
    assert_eq!(t.rows().len(), 2);
    let width = doc.styles.resolve(
        "table-column",
        t.columns()[0].style_name().unwrap(),
        "style:table-column-properties",
        "style:column-width",
    );
    assert_eq!(width.unwrap().value, "4cm");
    let Block::TrackedChanges(tc) = &doc.body[1] else {
        panic!()
    };
    let RegionChild::Region(r) = &tc.children[0] else {
        panic!()
    };
    assert_eq!(r.change.kind, ChangeKind::Deletion);
    assert_eq!(r.change.info.author.as_deref(), Some("Anna"));
    assert_eq!(r.change_id(), Some("ct1"));
    let l = doc.styles.list_style("L1").unwrap();
    assert_eq!(l.level(2).unwrap().num_format(), Some("a"));
    assert_eq!(doc.meta.user_defined("Afdeling"), Some("Teknik"));
    let Block::Paragraph(fp) = &doc.body[7] else {
        panic!()
    };
    let Inline::Frame(f) = &fp.content[0] else {
        panic!()
    };
    assert_eq!(f.alt_text(), Some("Map of Aarhus"));
    let counts = feature_counts(&mut odt.doc);
    for f in [
        Feature::Heading,
        Feature::List,
        Feature::Table,
        Feature::Section,
        Feature::Frame,
        Feature::Field,
        Feature::Comment,
        Feature::TrackedChange,
        Feature::Bookmark,
        Feature::Style,
        Feature::ListStyle,
        Feature::Meta,
    ] {
        assert!(counts.get(&f).copied().unwrap_or(0) > 0, "{f:?}");
    }
}

#[test]
fn editing_one_paragraph_leaves_everything_else_verbatim() {
    let mut odt = read(sample()).unwrap();
    // Edit the list's first paragraph; the list itself stays clean.
    let paras = paragraphs_mut(&mut odt.doc);
    let target = paras.into_iter().find(|p| p.plain_text() == "one").unwrap();
    target.content.push(Inline::Text(Text::new(" & two")));
    target.meta.dirty = true;
    let w = odt.write().unwrap();
    let orig = content_xml();
    assert_eq!(
        w.content_xml,
        orig.replace("<text:p>one</text:p>", "<text:p>one &amp; two</text:p>")
    );
    assert_eq!(w.styles_xml.as_deref(), Some(styles_xml().as_str()));
    // styles.xml untouched → copied raw
    let orig_bytes = sample();
    assert_eq!(
        entry(&w.bytes, "styles.xml"),
        entry(&orig_bytes, "styles.xml")
    );
}

#[test]
fn stamping_ids_patches_only_start_tags() {
    let mut odt = read(sample()).unwrap();
    stamp_ids(&mut odt.doc);
    let w = odt.write().unwrap();
    let tree = xml::parse(&w.content_xml).unwrap();
    let ids: Vec<_> = tree
        .descendants(tree.root)
        .into_iter()
        .filter(|&i| matches!(tree.node(i).name(), Some("text:p" | "text:h")))
        .map(|i| tree.node(i).attr("xml:id").map(str::to_owned))
        .collect();
    assert!(ids.iter().all(Option::is_some));
    assert!(ids.contains(&Some("id1".to_owned())), "existing IDs kept");
    // Removing the stamped attributes gives the original back.
    let mut stripped = w.content_xml.clone();
    while let Some(i) = stripped.find(" xml:id=\"sk-") {
        let j = stripped[i + 1..].find('"').unwrap() + i + 1;
        let k = stripped[j + 1..].find('"').unwrap() + j + 2;
        stripped.replace_range(i..k, "");
    }
    assert_eq!(stripped, content_xml());
}

#[test]
fn adding_user_defined_metadata_regenerates_only_meta() {
    let mut odt = read(sample()).unwrap();
    odt.doc.meta.set_user_defined("skriv:doc-id", "sk-123");
    let w = odt.write().unwrap();
    assert!(w.meta_xml.unwrap().contains(
        r#"<meta:user-defined meta:name="skriv:doc-id">sk-123</meta:user-defined></office:meta>"#
    ));
    assert_eq!(w.content_xml, content_xml());
}

#[test]
fn unknown_parts_survive_raw() {
    let mut odt = read(sample()).unwrap();
    let w = odt.write().unwrap();
    assert_eq!(entry(&w.bytes, "skriv/extra.bin"), b"\x00\x01opaque");
}

#[test]
fn wrong_namespace_binding_is_refused() {
    let bad = content_xml().replace(
        "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
        "urn:example:not-text",
    );
    let odt = build_odt(&bad, &styles_xml(), &meta_xml(), &[]);
    assert!(matches!(read(odt), Err(Error::Namespace { .. })));
}

#[test]
fn spreadsheet_is_not_text() {
    let ods = content_xml()
        .replace("<office:text>", "<office:spreadsheet>")
        .replace("</office:text>", "</office:spreadsheet>");
    let odt = build_odt(&ods, &styles_xml(), &meta_xml(), &[]);
    assert!(matches!(read(odt), Err(Error::NotText(_))));
}

mod prop {
    use super::*;
    use proptest::prelude::*;

    fn arb_text() -> impl Strategy<Value = String> {
        proptest::string::string_regex("[a-zæøåA-Z0-9 <>&\"'.]{0,20}").unwrap()
    }

    fn arb_para() -> impl Strategy<Value = String> {
        (arb_text(), arb_text(), 0usize..4, any::<bool>()).prop_map(|(a, b, sp, span)| {
            let esc = |s: &str| {
                let mut o = String::new();
                xml::escape_text(s, &mut o);
                o
            };
            let mut p = String::from("<text:p text:style-name=\"Standard\">");
            p.push_str(&esc(&a));
            if sp > 0 {
                p.push_str(&format!("<text:s text:c=\"{sp}\"/>"));
            }
            if span {
                p.push_str(&format!(
                    "<text:span text:style-name=\"T1\">{}</text:span>",
                    esc(&b)
                ));
            } else {
                p.push_str(&esc(&b));
            }
            p.push_str("</text:p>");
            p
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// For LibreOffice-style XML, identity and full regeneration both give
        /// the exact original bytes.
        #[test]
        fn identity_and_regeneration_agree(paras in proptest::collection::vec(arb_para(), 0..12)) {
            let content = content_xml().replace(
                "<text:p/>",
                &format!("<text:p/>{}", paras.concat()),
            );
            let bytes = build_odt(&content, &styles_xml(), &meta_xml(), &[]);
            let mut odt = read(bytes.clone()).unwrap();
            prop_assert_eq!(&odt.write().unwrap().content_xml, &content);
            let mut odt = read(bytes).unwrap();
            touch_all(&mut odt.doc);
            let w = odt.write_with(WriteOptions { regenerate_text: true }).unwrap();
            prop_assert_eq!(&w.content_xml, &content);
        }
    }
}
