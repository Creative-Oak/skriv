# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 5: validate the WordprocessingML parts of a .docx against the
ECMA-376 Transitional schemas, after markup-compatibility processing.

A stand-in for the Open XML SDK validator (which needs .NET): every
namespace the ECMA-376 Transitional schemas define is "understood";
elements and attributes in namespaces the root declares `mc:Ignorable` are
removed, `mc:AlternateContent` resolves to the first `mc:Choice` whose
`Requires` are all understood (else its `mc:Fallback`), as ECMA-376 Part 3
specifies. What remains must validate against `wml.xsd`.

The schemas are not in the repository (ECMA's licence for them is not
checked); `fetch-ecma-schemas.sh` downloads them.
"""

import os
import re
import shutil
import tempfile
import zipfile

from lxml import etree

MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
XML_XSD = """<?xml version="1.0"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" targetNamespace="http://www.w3.org/XML/1998/namespace" xml:lang="en">
 <xs:attribute name="lang" type="xs:string"/>
 <xs:attribute name="space"><xs:simpleType><xs:restriction base="xs:NCName"><xs:enumeration value="default"/><xs:enumeration value="preserve"/></xs:restriction></xs:simpleType></xs:attribute>
</xs:schema>
"""

PARTS = re.compile(
    r"word/(document|styles|numbering|footnotes|endnotes|comments|settings|fontTable|webSettings|header\d*|footer\d*)\.xml"
)


class Validator:
    def __init__(self, xsd_dir):
        tmp = tempfile.mkdtemp(prefix="skriv-xsd-")
        for f in os.listdir(xsd_dir):
            src = open(os.path.join(xsd_dir, f), encoding="utf-8").read()
            src = src.replace(
                '<xsd:import namespace="http://www.w3.org/XML/1998/namespace"/>',
                '<xsd:import namespace="http://www.w3.org/XML/1998/namespace" schemaLocation="xml.xsd"/>',
            )
            open(os.path.join(tmp, f), "w", encoding="utf-8").write(src)
        open(os.path.join(tmp, "xml.xsd"), "w").write(XML_XSD)
        self.understood = set()
        for f in os.listdir(tmp):
            m = re.search(r'targetNamespace="([^"]+)"', open(os.path.join(tmp, f), encoding="utf-8").read())
            if m:
                self.understood.add(m.group(1))
        self.understood.add("http://www.w3.org/XML/1998/namespace")
        self.schema = etree.XMLSchema(etree.parse(os.path.join(tmp, "wml.xsd")))
        shutil.rmtree(tmp, ignore_errors=True)

    def mce(self, root):
        ignorable = set()
        for el in root.iter():
            if not isinstance(el.tag, str):
                continue
            ig = el.get(f"{{{MC}}}Ignorable")
            if ig:
                for p in ig.split():
                    ns = el.nsmap.get(p)
                    if ns and ns not in self.understood:
                        ignorable.add(ns)
        # AlternateContent first (its Choice may live in an ignorable namespace).
        for ac in list(root.iter(f"{{{MC}}}AlternateContent")):
            parent = ac.getparent()
            if parent is None:
                continue
            pick = None
            for ch in ac.findall(f"{{{MC}}}Choice"):
                req = ch.get("Requires", "").split()
                if all(ch.nsmap.get(p) in self.understood for p in req):
                    pick = ch
                    break
            if pick is None:
                pick = ac.find(f"{{{MC}}}Fallback")
            idx = parent.index(ac)
            tail = ac.tail
            parent.remove(ac)
            if pick is not None:
                for i, c in enumerate(list(pick)):
                    parent.insert(idx + i, c)
            if tail and idx > 0:
                prev = parent[idx - 1]
                prev.tail = (prev.tail or "") + tail
        for el in list(root.iter()):
            if not isinstance(el.tag, str):
                continue
            ns = etree.QName(el).namespace
            if ns in ignorable or ns == MC:
                parent = el.getparent()
                if parent is not None:
                    parent.remove(el)
                continue
            for a in list(el.attrib):
                ans = etree.QName(a).namespace
                if ans in ignorable or ans == MC:
                    del el.attrib[a]
        etree.cleanup_namespaces(root)
        return root

    def validate(self, path):
        """[(part, n errors, first error)] for parts that don't validate; [] if valid."""
        out = []
        with zipfile.ZipFile(path) as z:
            for name in z.namelist():
                if not PARTS.fullmatch(name):
                    continue
                try:
                    root = etree.fromstring(z.read(name), etree.XMLParser(huge_tree=True))
                except etree.XMLSyntaxError as e:
                    out.append((name, 1, f"not well-formed: {e}"))
                    continue
                root = self.mce(root)
                if not self.schema.validate(root):
                    errs = list(self.schema.error_log)
                    first = errs[0]
                    out.append((name, len(errs), f"line {first.line}: {first.message[:200]}"))
        return out
