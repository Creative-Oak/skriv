#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""What does LibreOffice keep when it re-saves an ODT we wrote?

Spike 4 pre-work. Builds small ODT files carrying our identifiers and private
data in every place the report suggested (xml:id, text:meta, manifest.rdf,
extra package parts, user-defined metadata, foreign attributes and elements),
re-saves each one through LibreOffice three ways, and records exactly what
survives.

Save paths:
  convert  soffice --convert-to odt (no UI, no edit)
  store    UNO loadComponentFromURL + storeToURL (what an integration would do)
  edit     UNO load, type a word at the start of the document, store
           (a person opening the file in LibreOffice, editing, saving)

Usage: python3 fidelity/scripts/lo_preservation.py [--soffice PATH] [--out DIR]
Writes DIR/findings.json and DIR/findings.md (default fidelity/work/preservation).
Standard library only.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
import xml.etree.ElementTree as ET

NS = {
    "office": "urn:oasis:names:tc:opendocument:xmlns:office:1.0",
    "style": "urn:oasis:names:tc:opendocument:xmlns:style:1.0",
    "text": "urn:oasis:names:tc:opendocument:xmlns:text:1.0",
    "table": "urn:oasis:names:tc:opendocument:xmlns:table:1.0",
    "draw": "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0",
    "fo": "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0",
    "svg": "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0",
    "meta": "urn:oasis:names:tc:opendocument:xmlns:meta:1.0",
    "dc": "http://purl.org/dc/elements/1.1/",
    "xlink": "http://www.w3.org/1999/xlink",
    "manifest": "urn:oasis:names:tc:opendocument:xmlns:manifest:1.0",
    "sk": "urn:skriv:names:experimental:1.0",
    "xml": "http://www.w3.org/XML/1998/namespace",
    "rdf": "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    "pkg": "http://docs.oasis-open.org/ns/office/1.2/meta/pkg#",
}
XMLNS = " ".join(f'xmlns:{k}="{v}"' for k, v in NS.items() if k not in ("xml", "rdf", "pkg", "manifest"))


def q(tag):
    p, local = tag.split(":")
    return f"{{{NS[p]}}}{local}"


XML_ID = q("xml:id")

# ---------------------------------------------------------------- probes ---
# Each probe contributes content and says how to check the result. Content
# probes carry a unique marker word so the element can be found again even if
# LibreOffice drops its ID.

BODY = {
    "id-paragraph": '<text:p text:style-name="Standard" xml:id="sk-p1">marker-p1 a plain paragraph</text:p>',
    "id-paragraph-rdf": '<text:p text:style-name="Standard" xml:id="sk-p2">marker-p2 a paragraph RDF talks about</text:p>',
    "id-heading": '<text:h text:outline-level="1" xml:id="sk-h1">marker-h1 heading</text:h>',
    "id-section": '<text:section text:name="SkSection" xml:id="sk-sec1"><text:p>marker-sec1 in a section</text:p></text:section>',
    "id-list": '<text:list xml:id="sk-list1"><text:list-item xml:id="sk-li1"><text:p>marker-li1 list item</text:p></text:list-item></text:list>',
    "id-table": (
        '<table:table table:name="SkTable" xml:id="sk-tbl1"><table:table-column/>'
        '<table:table-row xml:id="sk-row1"><table:table-cell office:value-type="string" xml:id="sk-cell1">'
        "<text:p>marker-cell1 cell</text:p></table:table-cell></table:table-row></table:table>"
    ),
    "id-frame": (
        '<text:p>marker-fig1 <draw:frame draw:name="SkFigure" xml:id="sk-fig1" text:anchor-type="as-char" '
        'svg:width="3cm" svg:height="1cm"><draw:text-box><text:p>in a frame</text:p></draw:text-box>'
        "<svg:title>alt text</svg:title></draw:frame></text:p>"
    ),
    "bookmark-passage": (
        '<text:p>marker-bm1 <text:bookmark-start text:name="sk-passage-1" xml:id="sk-bm1"/>a passage that '
        "spans</text:p><text:p>two paragraphs<text:bookmark-end text:name=\"sk-passage-1\"/> end</text:p>"
    ),
    "text-meta": '<text:p>marker-tm1 <text:meta xml:id="sk-meta1">annotated words</text:meta> after</text:p>',
    "text-meta-field": '<text:p>marker-mf1 <text:meta-field xml:id="sk-mf1">field text</text:meta-field> after</text:p>',
    "foreign-attr-paragraph": '<text:p sk:id="fa-1" sk:extra="kept?">marker-fa1 foreign attribute</text:p>',
    "foreign-attr-cell": (
        '<table:table table:name="SkTable2"><table:table-column/><table:table-row>'
        '<table:table-cell office:value-type="string" sk:id="fa-cell"><text:p>marker-fa2 cell</text:p>'
        "</table:table-cell></table:table-row></table:table>"
    ),
    "foreign-element-block": '<sk:block sk:id="fe-1"><text:p>marker-fe1 inside foreign block</text:p></sk:block>',
    "foreign-element-inline": '<text:p>marker-fe2 before <sk:anchor sk:id="fe-2"/> after</text:p>',
    "foreign-element-inline-text": '<text:p>marker-fe3 before <sk:note sk:id="fe-3">foreign words</sk:note> after</text:p>',
}

RDF = """<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:pkg="http://docs.oasis-open.org/ns/office/1.2/meta/pkg#" xmlns:sk="urn:skriv:names:experimental:1.0">
 <rdf:Description rdf:about="content.xml"><rdf:type rdf:resource="http://docs.oasis-open.org/ns/office/1.2/meta/pkg#ContentFile"/></rdf:Description>
 <rdf:Description rdf:about="skriv/meta.rdf"><rdf:type rdf:resource="http://docs.oasis-open.org/ns/office/1.2/meta/pkg#MetadataFile"/></rdf:Description>
 <rdf:Description rdf:about=""><pkg:hasPart rdf:resource="content.xml"/><pkg:hasPart rdf:resource="skriv/meta.rdf"/></rdf:Description>
</rdf:RDF>
"""

SKRIV_RDF = """<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:sk="urn:skriv:names:experimental:1.0">
 <rdf:Description rdf:about="../content.xml#sk-p2"><sk:node>8f0c2b4e-skriv-node</sk:node></rdf:Description>
 <rdf:Description rdf:about="../content.xml#sk-meta1"><sk:comment>about annotated words</sk:comment></rdf:Description>
</rdf:RDF>
"""


# A stand-in for an embedded CRDT snapshot: ~256 KB of base64 as one RDF literal.
SNAPSHOT_PAYLOAD = __import__("base64").b64encode(bytes((i * 7919) % 251 for i in range(192 * 1024))).decode()
SNAPSHOT_RDF = f"""<?xml version="1.0" encoding="utf-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:sk="urn:skriv:names:experimental:1.0">
 <rdf:Description rdf:about="../content.xml"><sk:snapshot>{SNAPSHOT_PAYLOAD}</sk:snapshot></rdf:Description>
</rdf:RDF>
"""


def content_xml(probes):
    body = "".join(BODY[p] for p in probes if p in BODY)
    root_attr = ' sk:root="kept?"' if "foreign-attr-root" in probes else ""
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<office:document-content {XMLNS} office:version="1.4"{root_attr}>'
        "<office:automatic-styles/><office:body><office:text>"
        "<text:p text:style-name=\"Standard\">first paragraph</text:p>"
        f"{body}"
        "</office:text></office:body></office:document-content>"
    )


def styles_xml(probes):
    extra = ""
    if "foreign-attr-props" in probes:
        extra += '<style:style style:name="SkPropsStyle" style:family="paragraph"><style:text-properties fo:font-style="italic" sk:id="fp-1"/></style:style>'
    if "foreign-attr-style" in probes:
        extra += '<style:style style:name="SkStyle" style:family="paragraph" sk:id="fs-1"><style:text-properties fo:font-weight="bold"/></style:style>'
    if "foreign-element-styles" in probes:
        extra += '<sk:style-note sk:id="fes-1">a note among styles</sk:style-note>'
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<office:document-styles {XMLNS} office:version="1.4"><office:styles>'
        '<style:style style:name="Standard" style:family="paragraph" style:class="text"/>'
        f"{extra}</office:styles></office:document-styles>"
    )


def meta_xml(probes):
    extra = ""
    if "user-defined-meta" in probes:
        extra += '<meta:user-defined meta:name="skriv:doc-id">sk-doc-0193</meta:user-defined>'
        extra += '<meta:user-defined meta:name="skriv:revision" meta:value-type="float">42</meta:user-defined>'
    if "foreign-element-meta" in probes:
        extra += '<sk:history sk:id="fem-1">opaque history pointer</sk:history>'
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<office:document-meta {XMLNS} office:version="1.4"><office:meta>'
        f"<dc:title>probe</dc:title>{extra}</office:meta></office:document-meta>"
    )


def manifest_xml(entries):
    rows = "".join(
        f'<manifest:file-entry manifest:full-path="{p}" manifest:media-type="{t}"/>' for p, t in entries
    )
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4">'
        '<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>'
        f"{rows}</manifest:manifest>"
    )


def build(path, probes):
    parts = {
        "content.xml": content_xml(probes),
        "styles.xml": styles_xml(probes),
        "meta.xml": meta_xml(probes),
    }
    listed = [("content.xml", "text/xml"), ("styles.xml", "text/xml"), ("meta.xml", "text/xml")]
    if "id-paragraph-rdf" in probes or "text-meta" in probes or "manifest-rdf" in probes:
        parts["manifest.rdf"] = RDF
        parts["skriv/meta.rdf"] = SKRIV_RDF
        listed += [("manifest.rdf", "application/rdf+xml"), ("skriv/meta.rdf", "application/rdf+xml")]
    if "rdf-snapshot" in probes:
        parts.setdefault("manifest.rdf", RDF.replace("skriv/meta.rdf", "skriv/snapshot.rdf"))
        parts["skriv/snapshot.rdf"] = SNAPSHOT_RDF
        listed += [("skriv/snapshot.rdf", "application/rdf+xml")]
        if ("manifest.rdf", "application/rdf+xml") not in listed:
            listed.append(("manifest.rdf", "application/rdf+xml"))
        if "skriv/snapshot.rdf" not in parts["manifest.rdf"]:
            parts["manifest.rdf"] = parts["manifest.rdf"].replace(
                '<rdf:Description rdf:about="">',
                '<rdf:Description rdf:about="skriv/snapshot.rdf"><rdf:type rdf:resource="http://docs.oasis-open.org/ns/office/1.2/meta/pkg#MetadataFile"/></rdf:Description>\n <rdf:Description rdf:about=""><pkg:hasPart rdf:resource="skriv/snapshot.rdf"/>',
            )
    if "extra-part-listed" in probes:
        parts["skriv/state.bin"] = "SKRIV-CRDT-SNAPSHOT\x00\x01\x02"
        listed.append(("skriv/state.bin", "application/octet-stream"))
    if "extra-part-unlisted" in probes:
        parts["skriv/unlisted.bin"] = "UNLISTED"
    if "extra-part-root-listed" in probes:
        parts["skriv-state.json"] = '{"skriv": 1}'
        listed.append(("skriv-state.json", "application/json"))
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/vnd.oasis.opendocument.text", zipfile.ZIP_STORED)
        for name, data in parts.items():
            z.writestr(name, data, zipfile.ZIP_DEFLATED)
        z.writestr("META-INF/manifest.xml", manifest_xml(listed), zipfile.ZIP_DEFLATED)


# ---------------------------------------------------------------- checks ---


def read(z, name):
    try:
        return z.read(name).decode("utf-8")
    except KeyError:
        return None


def parse(s):
    return ET.fromstring(s.encode("utf-8")) if s else None


def by_marker(root, tag, marker):
    """The element with this tag whose text (or a descendant's) contains the marker."""
    for el in root.iter(q(tag)):
        if marker in "".join(el.itertext()):
            return el
    return None


def check_id(root, tag, marker, expected):
    el = by_marker(root, tag, marker)
    if el is None:
        # Some markers sit in a child paragraph: look for an ancestor of that.
        for cand in root.iter(q(tag)):
            if marker in "".join(cand.itertext()):
                el = cand
                break
    if el is None:
        return "dropped", f"no <{tag}> with '{marker}' found"
    got = el.get(XML_ID)
    if got == expected:
        return "kept", f'xml:id="{got}"'
    if got:
        return "changed", f'xml:id="{expected}" became "{got}"'
    return "dropped", f"<{tag}> survived but xml:id is gone"


def element_attr(el):
    return {k: v for k, v in el.attrib.items()}


def checks_for(probe):
    """Return a list of (item, function(z, content, styles, meta) -> (verdict, detail))."""
    C = []

    def add(item, fn):
        C.append((item, fn))

    simple_ids = {
        "id-paragraph": ("text:p", "marker-p1", "sk-p1"),
        "id-paragraph-rdf": ("text:p", "marker-p2", "sk-p2"),
        "id-heading": ("text:h", "marker-h1", "sk-h1"),
        "id-section": ("text:section", "marker-sec1", "sk-sec1"),
        "id-frame": ("draw:frame", "in a frame", "sk-fig1"),
    }
    if probe in simple_ids:
        tag, marker, exp = simple_ids[probe]
        add(f"xml:id on <{tag}>", lambda z, c, s, m: check_id(c, tag, marker, exp))
    if probe == "id-list":
        add("xml:id on <text:list>", lambda z, c, s, m: check_id(c, "text:list", "marker-li1", "sk-list1"))
        add("xml:id on <text:list-item>", lambda z, c, s, m: check_id(c, "text:list-item", "marker-li1", "sk-li1"))
    if probe == "id-table":
        add("xml:id on <table:table>", lambda z, c, s, m: check_id(c, "table:table", "marker-cell1", "sk-tbl1"))
        add("xml:id on <table:table-row>", lambda z, c, s, m: check_id(c, "table:table-row", "marker-cell1", "sk-row1"))
        add("xml:id on <table:table-cell>", lambda z, c, s, m: check_id(c, "table:table-cell", "marker-cell1", "sk-cell1"))
    if probe == "bookmark-passage":

        def bm(z, c, s, m):
            starts = [e for e in c.iter(q("text:bookmark-start")) if e.get(q("text:name")) == "sk-passage-1"]
            ends = [e for e in c.iter(q("text:bookmark-end")) if e.get(q("text:name")) == "sk-passage-1"]
            if not starts or not ends:
                return "dropped", "bookmark sk-passage-1 not found"
            return "kept", "start and end present, name unchanged"

        add("cross-paragraph bookmark name", bm)
        add(
            "xml:id on <text:bookmark-start>",
            lambda z, c, s, m: (
                ("kept", 'xml:id="sk-bm1"')
                if any(e.get(XML_ID) == "sk-bm1" for e in c.iter(q("text:bookmark-start")))
                else (
                    ("changed", "bookmark kept, xml:id=" + repr([e.get(XML_ID) for e in c.iter(q("text:bookmark-start"))]))
                    if any(True for _ in c.iter(q("text:bookmark-start")))
                    else ("dropped", "no bookmark-start")
                )
            ),
        )
    if probe == "text-meta":

        def tm(z, c, s, m):
            els = list(c.iter(q("text:meta")))
            if not els:
                return "dropped", "no <text:meta>"
            ids = [e.get(XML_ID) for e in els]
            if "sk-meta1" in ids:
                return "kept", 'text:meta xml:id="sk-meta1"'
            return "changed", f"text:meta present, xml:id={ids}"

        add("<text:meta> with xml:id", tm)
    if probe == "text-meta-field":

        def mf(z, c, s, m):
            els = list(c.iter(q("text:meta-field")))
            if not els:
                return "dropped", "no <text:meta-field>"
            ids = [e.get(XML_ID) for e in els]
            return ("kept", "xml:id kept") if "sk-mf1" in ids else ("changed", f"xml:id={ids}")

        add("<text:meta-field> with xml:id", mf)
    if probe in ("id-paragraph-rdf", "text-meta", "manifest-rdf"):

        def rdf(z, c, s, m):
            mr = read(z, "manifest.rdf")
            sk = read(z, "skriv/meta.rdf")
            if mr is None:
                return "dropped", "manifest.rdf gone"
            mentions = "skriv/meta.rdf" in mr
            if sk is None:
                return ("changed" if mentions else "dropped"), "skriv/meta.rdf part gone" + (
                    " (still named in manifest.rdf)" if mentions else ""
                )
            kept_stmt = "8f0c2b4e-skriv-node" in sk
            return ("kept" if kept_stmt and mentions else "changed"), (
                f"manifest.rdf names our metadata file: {mentions}; our statement survived: {kept_stmt}"
            )

        add("RDF metadata file (manifest.rdf + skriv/meta.rdf)", rdf)
    if probe.startswith("extra-part"):
        name = {
            "extra-part-listed": "skriv/state.bin",
            "extra-part-unlisted": "skriv/unlisted.bin",
            "extra-part-root-listed": "skriv-state.json",
        }[probe]

        def part(z, c, s, m, name=name):
            if name not in z.namelist():
                return "dropped", f"{name} not in package"
            man = read(z, "META-INF/manifest.xml") or ""
            return "kept", f"{name} present, listed in manifest: {name in man}"

        add(f"package part {name}", part)
    if probe == "user-defined-meta":

        def ud(z, c, s, m):
            found = {
                e.get(q("meta:name")): (e.text, e.get(q("meta:value-type")))
                for e in m.iter(q("meta:user-defined"))
            }
            a = found.get("skriv:doc-id")
            b = found.get("skriv:revision")
            if a and a[0] == "sk-doc-0193" and b and b[0] in ("42", "42.0"):
                return "kept", f"doc-id={a}, revision={b}"
            return ("changed" if found else "dropped"), f"found {found}"

        add("meta:user-defined (doc ID, typed value)", ud)
    if probe == "foreign-attr-paragraph":

        def fa(z, c, s, m):
            el = by_marker(c, "text:p", "marker-fa1")
            if el is None:
                return "dropped", "paragraph gone"
            sk = {k: v for k, v in el.attrib.items() if NS["sk"] in k}
            if sk.get(q("sk:id")) == "fa-1" and sk.get(q("sk:extra")) == "kept?":
                return "kept", "both foreign attributes on <text:p>"
            return ("changed" if sk else "dropped"), f"foreign attributes now {sk or 'none'}"

        add("foreign attribute on <text:p>", fa)
    if probe == "foreign-attr-cell":

        def fc(z, c, s, m):
            el = by_marker(c, "table:table-cell", "marker-fa2")
            if el is None:
                return "dropped", "cell gone"
            v = el.get(q("sk:id"))
            return ("kept", "sk:id kept") if v == "fa-cell" else ("dropped", f"sk:id={v}")

        add("foreign attribute on <table:table-cell>", fc)
    if probe == "foreign-attr-props":

        def fp(z, c, s, m):
            for el in s.iter(q("style:style")):
                if el.get(q("style:name")) == "SkPropsStyle":
                    tp = el.find(q("style:text-properties"))
                    if tp is None:
                        return "dropped", "text-properties gone"
                    v = tp.get(q("sk:id"))
                    return ("kept", "sk:id kept on text-properties") if v == "fp-1" else ("dropped", f"attribute gone; now {tp.attrib}")
            return "dropped", "style SkPropsStyle gone"

        add("foreign attribute on <style:text-properties>", fp)
    if probe == "rdf-snapshot":

        def snap(z, c, s, m):
            data = read(z, "skriv/snapshot.rdf")
            if data is None:
                return "dropped", "skriv/snapshot.rdf gone"
            if SNAPSHOT_PAYLOAD in data:
                same = data == SNAPSHOT_RDF
                return "kept", f"256 KB literal intact ({'byte-identical file' if same else 'file re-serialised'})"
            return "changed", f"file present ({len(data)} bytes) but literal altered"

        add("~256 KB snapshot as an RDF literal", snap)
    if probe == "foreign-attr-style":

        def fs(z, c, s, m):
            for el in s.iter(q("style:style")):
                if el.get(q("style:name")) == "SkStyle":
                    v = el.get(q("sk:id"))
                    return ("kept", "sk:id kept on style") if v == "fs-1" else ("dropped", "style kept, attribute gone")
            return "dropped", "style SkStyle gone"

        add("foreign attribute on <style:style>", fs)
    if probe == "foreign-attr-root":
        add(
            "foreign attribute on root element",
            lambda z, c, s, m: ("kept", "kept") if c.get(q("sk:root")) == "kept?" else ("dropped", "gone"),
        )

    def foreign_el(part_getter, tag, marker_text=None, keep_text_marker=None):
        def fn(z, c, s, m):
            root = part_getter(c, s, m)
            els = list(root.iter(q(tag)))
            text_survived = keep_text_marker and keep_text_marker in "".join(root.itertext())
            if els:
                return "kept", f"<{tag}> present"
            if text_survived:
                return "changed", f"<{tag}> removed but its text content was kept"
            return "dropped", f"<{tag}> and its content are gone"

        return fn

    if probe == "foreign-element-block":
        add("foreign block element (with a paragraph inside)", foreign_el(lambda c, s, m: c, "sk:block", keep_text_marker="marker-fe1"))
    if probe == "foreign-element-inline":
        add("foreign empty inline element", foreign_el(lambda c, s, m: c, "sk:anchor"))
    if probe == "foreign-element-inline-text":
        add("foreign inline element with text", foreign_el(lambda c, s, m: c, "sk:note", keep_text_marker="foreign words"))
    if probe == "foreign-element-meta":
        add("foreign element in office:meta", foreign_el(lambda c, s, m: m, "sk:history"))
    if probe == "foreign-element-styles":
        add("foreign element in office:styles", foreign_el(lambda c, s, m: s, "sk:style-note"))
    return C


PROBES = list(BODY) + [
    "manifest-rdf",
    "extra-part-listed",
    "extra-part-unlisted",
    "extra-part-root-listed",
    "user-defined-meta",
    "foreign-attr-style",
    "foreign-attr-props",
    "foreign-attr-root",
    "rdf-snapshot",
    "foreign-element-meta",
    "foreign-element-styles",
]

# ------------------------------------------------------------------ runs ---


def soffice_version(soffice):
    try:
        return subprocess.run([soffice, "--version"], capture_output=True, text=True, timeout=60).stdout.strip()
    except Exception as e:  # noqa: BLE001
        return f"unknown ({e})"


def convert(soffice, profile, files, outdir):
    os.makedirs(outdir, exist_ok=True)
    subprocess.run(
        [soffice, f"-env:UserInstallation=file://{profile}", "--headless", "--norestore", "--convert-to", "odt", "--outdir", outdir, *files],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=600,
        check=False,
    )


def uno_store(soffice, profile, files, outdir, edit):
    os.makedirs(outdir, exist_ok=True)
    scripts = os.path.join(profile, "user", "Scripts", "python")
    os.makedirs(scripts, exist_ok=True)
    shutil.copy(os.path.join(os.path.dirname(__file__), "uno", "skriv_uno.py"), scripts)
    job = os.path.join(outdir, "job.json")
    with open(job, "w") as f:
        json.dump([{"in": p, "out": os.path.join(outdir, os.path.basename(p)), "edit": edit} for p in files], f)
    env = dict(os.environ, SKRIV_JOB=job)
    subprocess.run(
        [soffice, f"-env:UserInstallation=file://{profile}", "--headless", "--invisible", "--norestore",
         "vnd.sun.star.script:skriv_uno.py$run?language=Python&location=user"],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=600,
        check=False,
    )
    log_path = job + ".log"
    return json.load(open(log_path)) if os.path.exists(log_path) else [{"ok": False, "error": "macro produced no log"}]


def evaluate(path, checks):
    out = []
    if not os.path.exists(path):
        return [(item, "refused", "LibreOffice did not open the file (no document loaded)") for item, _ in checks]
    with zipfile.ZipFile(path) as z:
        try:
            c, s, m = parse(read(z, "content.xml")), parse(read(z, "styles.xml")), parse(read(z, "meta.xml"))
        except ET.ParseError as e:
            return [(item, "error", f"unparseable output: {e}") for item, _ in checks]
        for item, fn in checks:
            try:
                verdict, detail = fn(z, c, s, m)
            except Exception as e:  # noqa: BLE001
                verdict, detail = "error", repr(e)
            out.append((item, verdict, detail))
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--soffice", default=os.environ.get("SKRIV_SOFFICE", "soffice"))
    ap.add_argument("--out", default="fidelity/work/preservation")
    args = ap.parse_args()
    out = os.path.abspath(args.out)
    if os.path.exists(out):
        shutil.rmtree(out)
    src = os.path.join(out, "input")
    os.makedirs(src)
    profile = tempfile.mkdtemp(prefix="skriv-lo-profile-")

    files = []
    for p in PROBES:
        path = os.path.join(src, f"{p}.odt")
        build(path, [p])
        files.append(path)
    # Every probe in one file, except the unlisted part (LibreOffice refuses
    # to open a package with one, which would hide every other result).
    combined = os.path.join(src, "all-probes.odt")
    build(combined, [p for p in PROBES if p != "extra-part-unlisted"])
    files.append(combined)

    # Sanity: our own inputs must pass every check before LibreOffice touches them.
    baseline = {}
    for p in PROBES:
        baseline[p] = evaluate(os.path.join(src, f"{p}.odt"), checks_for(p))

    convert(args.soffice, profile, files, os.path.join(out, "convert"))
    logs = {
        "store": uno_store(args.soffice, profile, files, os.path.join(out, "store"), edit=False),
        "edit": uno_store(args.soffice, profile, files, os.path.join(out, "edit"), edit=True),
    }

    results = {"libreoffice": soffice_version(args.soffice), "probes": {}, "uno_logs": logs}
    for p in PROBES:
        checks = checks_for(p)
        row = {"baseline": baseline[p]}
        for path_kind in ("convert", "store", "edit"):
            row[path_kind] = evaluate(os.path.join(out, path_kind, f"{p}.odt"), checks)
            row[path_kind + "_combined"] = evaluate(os.path.join(out, path_kind, "all-probes.odt"), checks)
        results["probes"][p] = row
    # Did the edit actually happen?
    edited = os.path.join(out, "edit", "id-paragraph.odt")
    if os.path.exists(edited):
        with zipfile.ZipFile(edited) as z:
            results["edit_applied"] = "Edited in LibreOffice" in read(z, "content.xml")

    with open(os.path.join(out, "findings.json"), "w") as f:
        json.dump(results, f, indent=1)
    md = ["| Item | Our file | convert | store | edit | edit (all probes in one file) |", "|---|---|---|---|---|---|"]
    mark = {"kept": "kept", "changed": "**changed**", "dropped": "**dropped**", "error": "error", "refused": "**refused**"}
    for p, row in results["probes"].items():
        for i, (item, verdict, _) in enumerate(row["baseline"]):
            cells = [mark[verdict]]
            for k in ("convert", "store", "edit", "edit_combined"):
                v = row[k][i]
                cells.append(f"{mark[v[1]]}<br><sub>{v[2]}</sub>" if v[1] != "kept" else "kept")
            md.append(f"| {item} | " + " | ".join(cells) + " |")
    with open(os.path.join(out, "findings.md"), "w") as f:
        f.write(f"LibreOffice: {results['libreoffice']}\n\nEdit applied: {results.get('edit_applied')}\n\n")
        f.write("\n".join(md) + "\n")
    print(open(os.path.join(out, "findings.md")).read())
    bad_baseline = [p for p, r in results["probes"].items() if any(v != "kept" for _, v, _ in r["baseline"])]
    if bad_baseline:
        print("WARNING: our own inputs fail checks for:", bad_baseline, file=sys.stderr)
    shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    main()
