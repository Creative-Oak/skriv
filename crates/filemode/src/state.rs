// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! skriv's state inside an ODT: the document ID in `meta.xml`, IDs as
//! `xml:id`, and `skriv/state.rdf`, an ODF 1.2 metadata file (registered in
//! `manifest.rdf` as a `pkg:MetadataFile` and listed in the manifest).
//!
//! The RDF part carries the CRDT version the file was written at and,
//! depending on [`SnapshotKind`], a snapshot. Without a snapshot the file is
//! only useful to a skriv that already has the document; with a shallow
//! snapshot any skriv can rebuild the document and make a branch that merges
//! into the original; with a full snapshot the whole history travels with
//! the file, deleted text included.

use std::io::{Cursor, Read, Write};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use skriv_crdt::codec::{self, Baseline};
use skriv_crdt::logical::LDoc;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const NS: &str = "urn:skriv:names:experimental:1.0";
pub const PART: &str = "skriv/state.rdf";
pub const DOC_ID_KEY: &str = "skriv:doc-id";
/// The version, also in `meta.xml`: unlike the RDF part, user-defined
/// metadata survives a trip through DOCX (as a custom document property).
pub const VERSION_KEY: &str = "skriv:version";
pub const FORMAT: &str = "loro-flow-1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotKind {
    /// Document ID and version only.
    None,
    /// The current state, no history (Loro shallow snapshot).
    Shallow,
    /// Everything, history included.
    Full,
}

impl SnapshotKind {
    pub fn name(self) -> &'static str {
        match self {
            SnapshotKind::None => "none",
            SnapshotKind::Shallow => "shallow",
            SnapshotKind::Full => "full",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "shallow" => SnapshotKind::Shallow,
            "full" => SnapshotKind::Full,
            _ => SnapshotKind::None,
        }
    }
}

/// What a file carries about its skriv document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub doc_id: String,
    /// The CRDT version (Loro frontiers) the file was written at.
    pub version: Vec<u8>,
    pub kind: SnapshotKind,
    pub snapshot: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}

/// Write `l` as an ODT over `source` (the file it was imported from, so
/// untouched elements are copied byte for byte), with `state` embedded.
pub fn write(source: &[u8], l: &LDoc, base: &Baseline, state: &State) -> Result<Vec<u8>, Error> {
    let mut odt = skriv_odf::read(source.to_vec()).map_err(err)?;
    let mut ex = codec::export(l, base);
    ex.doc.meta.set_user_defined(DOC_ID_KEY, &state.doc_id);
    ex.doc
        .meta
        .set_user_defined(VERSION_KEY, &B64.encode(&state.version));
    skriv_odf::stamp_ids(&mut ex.doc);
    odt.doc = ex.doc;
    let w = odt.write().map_err(err)?;
    embed(&w.bytes, state)
}

/// The RDF/XML of `skriv/state.rdf`.
pub fn rdf(state: &State) -> String {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    s.push_str(&format!(
        "<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:sk=\"{NS}\">\n"
    ));
    s.push_str(" <rdf:Description rdf:about=\"../content.xml\">\n");
    s.push_str(&format!(
        "  <sk:docId>{}</sk:docId>\n",
        xml_escape(&state.doc_id)
    ));
    s.push_str(&format!("  <sk:format>{FORMAT}</sk:format>\n"));
    s.push_str(&format!(
        "  <sk:version>{}</sk:version>\n",
        B64.encode(&state.version)
    ));
    s.push_str(&format!(
        "  <sk:snapshotKind>{}</sk:snapshotKind>\n",
        state.kind.name()
    ));
    if let Some(snap) = &state.snapshot {
        s.push_str(&format!(
            "  <sk:snapshot>{}</sk:snapshot>\n",
            B64.encode(snap)
        ));
    }
    s.push_str(" </rdf:Description>\n</rdf:RDF>\n");
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const MANIFEST: &str = "META-INF/manifest.xml";
const MANIFEST_RDF: &str = "manifest.rdf";
const PKG: &str = "http://docs.oasis-open.org/ns/office/1.2/meta/pkg#";

/// Add (or replace) `skriv/state.rdf` in an ODT package, registering it in
/// `manifest.rdf` and listing both in the manifest. Every other entry is
/// copied raw.
pub fn embed(odt: &[u8], state: &State) -> Result<Vec<u8>, Error> {
    let mut zin = ZipArchive::new(Cursor::new(odt)).map_err(err)?;
    let mut manifest = read_entry(&mut zin, MANIFEST)?.ok_or_else(|| err("no manifest"))?;
    let manifest_rdf = read_entry(&mut zin, MANIFEST_RDF)?;

    for (path, kind) in [
        (PART, "application/rdf+xml"),
        (MANIFEST_RDF, "application/rdf+xml"),
    ] {
        if !manifest.contains(&format!("manifest:full-path=\"{path}\"")) {
            let entry = format!(
                " <manifest:file-entry manifest:full-path=\"{path}\" manifest:media-type=\"{kind}\"/>\n"
            );
            let at = manifest
                .rfind("</manifest:manifest>")
                .ok_or_else(|| err("manifest has no end tag"))?;
            manifest.insert_str(at, &entry);
        }
    }
    let manifest_rdf = match manifest_rdf {
        Some(mut m) if !m.contains(PART) => {
            let at = m
                .rfind("</rdf:RDF>")
                .ok_or_else(|| err("manifest.rdf has no end tag"))?;
            m.insert_str(at, &registration());
            m
        }
        Some(m) => m,
        None => format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:pkg=\"{PKG}\">\n <rdf:Description rdf:about=\"content.xml\"><rdf:type rdf:resource=\"http://docs.oasis-open.org/ns/office/1.2/meta/odf#ContentFile\"/></rdf:Description>\n <rdf:Description rdf:about=\"\"><pkg:hasPart xmlns:pkg=\"{PKG}\" rdf:resource=\"content.xml\"/></rdf:Description>\n <rdf:Description rdf:about=\"\"><rdf:type rdf:resource=\"{PKG}Document\"/></rdf:Description>\n{}</rdf:RDF>\n",
            registration()
        ),
    };

    let mut w = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut wrote_rdf = false;
    for i in 0..zin.len() {
        let name = zin
            .by_index_raw(i)
            .map_err(err)?
            .name()
            .map_err(err)?
            .into_owned();
        match name.as_str() {
            MANIFEST => {
                w.start_file(MANIFEST, deflate).map_err(err)?;
                w.write_all(manifest.as_bytes()).map_err(err)?;
            }
            MANIFEST_RDF => {
                w.start_file(MANIFEST_RDF, deflate).map_err(err)?;
                w.write_all(manifest_rdf.as_bytes()).map_err(err)?;
                wrote_rdf = true;
            }
            PART => {}
            _ => {
                let f = zin.by_index_raw(i).map_err(err)?;
                w.raw_copy_file(f).map_err(err)?;
            }
        }
    }
    if !wrote_rdf {
        w.start_file(MANIFEST_RDF, deflate).map_err(err)?;
        w.write_all(manifest_rdf.as_bytes()).map_err(err)?;
    }
    w.start_file(PART, deflate).map_err(err)?;
    w.write_all(rdf(state).as_bytes()).map_err(err)?;
    Ok(w.finish().map_err(err)?.into_inner())
}

fn registration() -> String {
    format!(
        " <rdf:Description rdf:about=\"{PART}\"><rdf:type rdf:resource=\"{PKG}MetadataFile\"/></rdf:Description>\n <rdf:Description rdf:about=\"\"><pkg:hasPart xmlns:pkg=\"{PKG}\" rdf:resource=\"{PART}\"/></rdf:Description>\n"
    )
}

fn read_entry(z: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<String>, Error> {
    let mut f = match z.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let mut s = String::new();
    f.read_to_string(&mut s).map_err(err)?;
    Ok(Some(s))
}

/// What a returned file still knows about its skriv document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// From `meta:user-defined`.
    pub meta_doc_id: Option<String>,
    pub meta_version: Option<Vec<u8>>,
    /// From `skriv/state.rdf`.
    pub state: Option<State>,
}

/// Read skriv's state back from a file, whoever saved it last. LibreOffice
/// re-serialises the RDF part with its own prefixes (`ns0:snapshot`), so
/// elements are found by local name.
pub fn read(odt: &[u8]) -> Result<Found, Error> {
    let mut z = ZipArchive::new(Cursor::new(odt)).map_err(err)?;
    let meta = read_entry(&mut z, "meta.xml")?;
    let user_defined = |key: &str| {
        let m = meta.as_deref()?;
        let at = m.find(&format!("meta:name=\"{key}\""))?;
        let rest = &m[at..];
        let open = rest.find('>')? + 1;
        let close = rest.find("</")?;
        Some(rest[open..close].to_owned())
    };
    let meta_doc_id = user_defined(DOC_ID_KEY);
    let meta_version = user_defined(VERSION_KEY).and_then(|v| B64.decode(v.trim()).ok());
    let state = read_entry(&mut z, PART)?.and_then(|rdf| {
        let doc_id = literal(&rdf, "docId")?;
        let version = B64.decode(literal(&rdf, "version")?.trim()).ok()?;
        let kind = SnapshotKind::parse(literal(&rdf, "snapshotKind").as_deref().unwrap_or(""));
        let snapshot = literal(&rdf, "snapshot").and_then(|s| B64.decode(s.trim()).ok());
        Some(State {
            doc_id,
            version,
            kind,
            snapshot,
        })
    });
    Ok(Found {
        meta_doc_id,
        meta_version,
        state,
    })
}

/// The text of the first element whose local name is `local`.
fn literal(xml: &str, local: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = xml[from..].find('<') {
        let start = from + i + 1;
        let end = start + xml[start..].find(['>', ' ', '/', '\n', '\t'])?;
        let name = &xml[start..end];
        let lname = name.rsplit(':').next().unwrap_or(name);
        if lname == local && !name.starts_with('/') {
            let open = start + xml[start..].find('>')? + 1;
            let close = open + xml[open..].find("</")?;
            return Some(
                xml[open..close]
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&"),
            );
        }
        from = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_by_local_name() {
        let x = r#"<rdf:RDF><rdf:Description><ns0:snapshot xmlns:ns0="urn:x">QUJD</ns0:snapshot><sk:docId>a&amp;b</sk:docId></rdf:Description></rdf:RDF>"#;
        assert_eq!(literal(x, "snapshot").as_deref(), Some("QUJD"));
        assert_eq!(literal(x, "docId").as_deref(), Some("a&b"));
        assert_eq!(literal(x, "version"), None);
    }
}
