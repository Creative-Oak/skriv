# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 5: what a DOCX file contains, as comparable items per feature.

`inventory(path)` reads a .docx and returns {feature: [item, ...]}, where an
item is a hashable tuple describing one instance in terms that survive a
conversion when the feature survives (texts, effective formatting, list
labels, field instructions, change authors...), not in terms of how the XML
happens to spell it (IDs, style IDs, rsids, attribute order).

Two inventories compare feature by feature (`compare`): an item of the
original that the other file doesn't have is a downgrade.
"""

import collections
import re
import zipfile

from lxml import etree

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
WP = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
V = "urn:schemas-microsoft-com:vml"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"
PIC = "http://schemas.openxmlformats.org/drawingml/2006/picture"
WPS = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
C = "http://schemas.openxmlformats.org/drawingml/2006/chart"
DGM = "http://schemas.openxmlformats.org/drawingml/2006/diagram"
M = "http://schemas.openxmlformats.org/officeDocument/2006/math"


def w(tag):
    return f"{{{W}}}{tag}"


def wattr(el, name):
    if el is None:
        return None
    return el.get(w(name))


def norm_text(s):
    s = s.replace("\u00a0", " ").replace("\u2011", "-").replace("\u00ad", "")
    # Symbol-font characters (w:sym, private use) and emoji: LibreOffice
    # turns symbol characters into Unicode; compare the rest of the text.
    s = "".join(c for c in s if not (0xE000 <= ord(c) <= 0xF8FF or ord(c) >= 0x1F000))
    return re.sub(r"\s+", " ", s).strip()


def read_xml(z, name):
    try:
        data = z.read(name)
    except KeyError:
        return None
    try:
        return etree.fromstring(data, etree.XMLParser(huge_tree=True, recover=True))
    except etree.XMLSyntaxError:
        return None


def resolve_mce(root):
    """Pick mc:AlternateContent's Fallback (or first Choice) in place."""
    if root is None:
        return root
    for ac in list(root.iter(f"{{{MC}}}AlternateContent")):
        parent = ac.getparent()
        if parent is None:
            continue
        # Prefer the Choice for wps (DrawingML shapes) when present, it carries
        # the real content; otherwise the Fallback (VML).
        pick = ac.find(f"{{{MC}}}Choice")
        if pick is None:
            pick = ac.find(f"{{{MC}}}Fallback")
        idx = parent.index(ac)
        parent.remove(ac)
        if pick is not None:
            for i, child in enumerate(list(pick)):
                parent.insert(idx + i, child)
    return root


# ---------------------------------------------------------------- styles ---


class Styles:
    """Style names by ID, with basedOn chains, and docDefaults."""

    def __init__(self, root):
        self.by_id = {}
        self.defaults_r = {}
        self.defaults_p = {}
        self.default_para = None
        if root is None:
            return
        dd = root.find(w("docDefaults"))
        if dd is not None:
            self.defaults_r = run_props(dd.find(f"{w('rPrDefault')}/{w('rPr')}"))
        for s in root.findall(w("style")):
            sid = wattr(s, "styleId")
            name_el = s.find(w("name"))
            name = wattr(name_el, "val") or sid
            based = wattr(s.find(w("basedOn")), "val")
            num = s.find(f"{w('pPr')}/{w('numPr')}")
            outline = wattr(s.find(f"{w('pPr')}/{w('outlineLvl')}"), "val")
            self.by_id[sid] = {
                "name": canon_style_name(name),
                "type": wattr(s, "type"),
                "based": based,
                "rpr": run_props(s.find(w("rPr"))),
                "num": numpr(num),
                "outline": outline,
            }
            if wattr(s, "type") == "paragraph" and wattr(s, "default") in ("1", "true"):
                self.default_para = sid

    def name(self, sid):
        if sid is None:
            sid = self.default_para
        st = self.by_id.get(sid)
        return st["name"] if st else canon_style_name(sid or "")

    def chain(self, sid):
        out = []
        seen = set()
        while sid and sid in self.by_id and sid not in seen:
            seen.add(sid)
            out.append(self.by_id[sid])
            sid = self.by_id[sid]["based"]
        return out

    def rpr(self, sid):
        """Run properties a style gives, base first."""
        props = {}
        for st in reversed(self.chain(sid)):
            props.update(st["rpr"])
        return props

    def para_num(self, sid):
        for st in self.chain(sid or self.default_para):
            if st["num"]:
                return st["num"]
        return None

    def outline(self, sid):
        for st in self.chain(sid or self.default_para):
            if st["outline"] is not None:
                return st["outline"]
        return None


def canon_style_name(name):
    n = name.strip().lower()
    # Word's localised built-in names and their English IDs: compare the
    # built-in identity, not the language of the UI that saved the file.
    n = n.replace("overskrift", "heading").replace("titel", "title")
    n = re.sub(r"\s+", " ", n)
    return n


TOGGLES = ["b", "i", "strike", "dstrike", "caps", "smallCaps", "vanish"]


def onoff(el):
    v = wattr(el, "val")
    return v not in ("0", "false", "off")


def run_props(rpr):
    p = {}
    if rpr is None:
        return p
    for t in TOGGLES:
        el = rpr.find(w(t))
        if el is not None:
            p[t] = onoff(el)
    u = rpr.find(w("u"))
    if u is not None:
        p["u"] = wattr(u, "val") or "single"
    sz = rpr.find(w("sz"))
    if sz is not None:
        p["sz"] = wattr(sz, "val")
    col = rpr.find(w("color"))
    if col is not None and wattr(col, "val") not in (None, "auto"):
        p["color"] = wattr(col, "val").upper()
    hl = rpr.find(w("highlight"))
    if hl is not None:
        p["highlight"] = wattr(hl, "val")
    sh = rpr.find(w("shd"))
    if sh is not None and wattr(sh, "fill") not in (None, "auto", "FFFFFF"):
        p["shd"] = wattr(sh, "fill").upper()
    f = rpr.find(w("rFonts"))
    if f is not None and wattr(f, "ascii"):
        p["font"] = wattr(f, "ascii")
    va = rpr.find(w("vertAlign"))
    if va is not None:
        p["vertAlign"] = wattr(va, "val")
    rs = rpr.find(w("rStyle"))
    if rs is not None:
        p["_rstyle"] = wattr(rs, "val")
    return p


def numpr(el):
    if el is None:
        return None
    num = wattr(el.find(w("numId")), "val")
    lvl = wattr(el.find(w("ilvl")), "val") or "0"
    if num is None:
        return None
    return (num, lvl)


# ------------------------------------------------------------- numbering ---


class Numbering:
    def __init__(self, root):
        self.abstract = {}
        self.nums = {}
        if root is None:
            return
        for a in root.findall(w("abstractNum")):
            lvls = {}
            for lv in a.findall(w("lvl")):
                lvls[wattr(lv, "ilvl")] = (
                    wattr(lv.find(w("numFmt")), "val") or "decimal",
                    wattr(lv.find(w("lvlText")), "val") or "",
                    wattr(lv.find(w("start")), "val") or "1",
                )
            link = wattr(a.find(w("numStyleLink")), "val")
            self.abstract[wattr(a, "abstractNumId")] = (lvls, link)
        for n in root.findall(w("num")):
            aid = wattr(n.find(w("abstractNumId")), "val")
            over = {}
            for o in n.findall(w("lvlOverride")):
                lv = o.find(w("lvl"))
                start = wattr(o.find(w("startOverride")), "val")
                over[wattr(o, "ilvl")] = (lv, start)
            self.nums[wattr(n, "numId")] = (aid, over)

    def level(self, num, ilvl):
        """(format, label template) of a list level, or None for 'no list'."""
        if num is None or num == "0" or num not in self.nums:
            return None
        aid, over = self.nums[num]
        lvls, _ = self.abstract.get(aid, ({}, None))
        lv = lvls.get(ilvl)
        o = over.get(ilvl)
        if o and o[0] is not None:
            el = o[0]
            lv = (
                wattr(el.find(w("numFmt")), "val") or "decimal",
                wattr(el.find(w("lvlText")), "val") or "",
                wattr(el.find(w("start")), "val") or "1",
            )
        if lv is None:
            return ("?", "")
        fmt, text = lv[0], lv[1]
        # Bullet characters differ by font (Symbol  vs •); compare "bullet".
        if fmt == "bullet":
            text = "•"
        return (fmt, text)


# --------------------------------------------------------------- walking ---


class Walk:
    """Collect items from the body (and, for text, headers and footers)."""

    def __init__(self, styles, numbering):
        self.st = styles
        self.nb = numbering
        self.items = collections.defaultdict(list)
        self.field_stack = []
        self.table_styles = []
        self.even_odd = False
        self.hf_ids = []

    # Text of an element as Word shows it: deleted text, field results
    # (computed: page numbers, TOC entries) and text boxes (their own
    # feature) left out. Field state carries across paragraphs.
    def para_text(self, p):
        out = []
        for el in p.iter():
            if not isinstance(el.tag, str):
                continue
            q = etree.QName(el)
            if q.namespace != W:
                continue
            tag = q.localname
            if tag == "fldChar":
                kind = wattr(el, "fldCharType")
                if kind == "begin":
                    self.field_stack.append("instr")
                elif kind == "separate" and self.field_stack:
                    self.field_stack[-1] = "result"
                elif kind == "end" and self.field_stack:
                    self.field_stack.pop()
                continue
            if tag not in ("t", "tab", "br", "cr", "noBreakHyphen"):
                continue
            if self.field_stack and "result" in self.field_stack:
                continue
            if self.inside(el, "del") or self.inside(el, "fldSimple") or self.inside(el, "txbxContent"):
                continue
            if tag == "t":
                out.append(el.text or "")
            elif tag == "tab":
                if el.getparent() is not None and etree.QName(el.getparent()).localname == "r":
                    out.append("\t")
            elif tag in ("br", "cr"):
                out.append(" ")
            elif tag == "noBreakHyphen":
                out.append("-")
        return "".join(out)

    @staticmethod
    def inside(el, local):
        p = el.getparent()
        while p is not None:
            if isinstance(p.tag, str) and etree.QName(p).localname == local and etree.QName(p).namespace == W:
                return True
            p = p.getparent()
        return False

    def paragraph(self, p, where):
        text = self.para_text(p)
        nt = norm_text(text)
        ppr = p.find(w("pPr"))
        sid = wattr(ppr.find(w("pStyle")), "val") if ppr is not None else None
        key = nt[:80]
        self.items["text"].append((where, nt))
        if not nt:
            return
        self.items["paragraph style"].append((key, self.st.name(sid)))
        # Lists: direct numPr or the style's.
        num = numpr(ppr.find(w("numPr"))) if ppr is not None else None
        if num is None:
            num = self.st.para_num(sid)
        lvl = self.nb.level(*num) if num else None
        if lvl is not None:
            self.items["list"].append((key, num[1], lvl[0], lvl[1]))
        outline = wattr(ppr.find(w("outlineLvl")), "val") if ppr is not None else None
        outline = outline if outline is not None else self.st.outline(sid)
        if outline is not None and outline != "9":
            self.items["outline level"].append((key, outline))
        if ppr is not None:
            jc = wattr(ppr.find(w("jc")), "val")
            if jc:
                self.items["alignment"].append((key, {"left": "start", "right": "end"}.get(jc, jc)))
        # Effective character formatting, per character, reported as maximal
        # stretches with the same value per property (so how the text is
        # cut into runs doesn't matter).
        prpr = self.st.rpr(sid or self.st.default_para)
        base = dict(self.st.defaults_r)
        for ts in self.table_styles:
            base.update(self.st.rpr(ts))
        base.update(prpr)
        chars = []
        for r in p.iter(w("r")):
            if self.inside(r, "del"):
                continue
            t = "".join(x.text or "" for x in r.findall(w("t")))
            if not t:
                continue
            rp = run_props(r.find(w("rPr")))
            eff = dict(base)
            if "_rstyle" in rp:
                eff.update(self.st.rpr(rp["_rstyle"]))
            eff.update(rp)
            eff.pop("_rstyle", None)
            visible = {k: str(v) for k, v in eff.items() if v not in (False, None, "none")}
            chars.extend((c, visible) for c in t)
        props = sorted({k for _, v in chars for k in v})
        for prop in props:
            i0 = 0
            while i0 < len(chars):
                val = chars[i0][1].get(prop)
                i1 = i0
                while i1 < len(chars) and chars[i1][1].get(prop) == val:
                    i1 += 1
                seg = "".join(c for c, _ in chars[i0:i1]).strip()
                if val is not None and seg:
                    feature = "font" if prop == "font" else "character formatting"
                    self.items[feature].append((key, seg[:40], prop, val))
                i0 = i1
        # Tracked changes inside the paragraph: per kind and author, the
        # changed text joined (adjacent insertions may be one element or
        # several); paragraph-mark changes separately.
        changed = collections.defaultdict(list)
        for tag in ("ins", "del", "moveFrom", "moveTo"):
            for c in p.iter(w(tag)):
                parent = c.getparent()
                if parent is not None and etree.QName(parent).localname == "rPr":
                    pp = parent.getparent()
                    if pp is not None and etree.QName(pp).localname == "pPr":
                        self.items["tracked change"].append(("paragraph mark " + tag, wattr(c, "author") or "", key[:60]))
                    continue
                txt = "".join(x.text or "" for x in c.iter(w("t"), w("delText")))
                changed[(tag, wattr(c, "author") or "")].append(txt)
        for (tag, author), parts in changed.items():
            self.items["tracked change"].append((tag, author, norm_text("".join(parts))[:60]))
        for c in p.iter(w("rPrChange")):
            r = c.getparent().getparent()
            txt = "".join(x.text or "" for x in r.findall(w("t")))
            self.items["tracked change"].append(("rPrChange", wattr(c, "author") or "", norm_text(txt)[:60]))
        for c in p.iter(w("pPrChange")):
            self.items["tracked change"].append(("pPrChange", wattr(c, "author") or "", key[:60]))
        # Fields: simple and complex.
        for f in p.iter(w("fldSimple")):
            self.items["field"].append(field_item(wattr(f, "instr") or ""))
        instr = None
        for el in p.iter(w("fldChar"), w("instrText")):
            if etree.QName(el).localname == "fldChar":
                kind = wattr(el, "fldCharType")
                if kind == "begin":
                    instr = []
                elif kind == "separate" and instr is not None:
                    self.items["field"].append(field_item("".join(instr)))
                    instr = None
                elif kind == "end" and instr is not None:
                    self.items["field"].append(field_item("".join(instr)))
                    instr = None
            elif instr is not None:
                instr.append(el.text or "")
        for h in p.iter(w("hyperlink")):
            txt = norm_text("".join(x.text or "" for x in h.iter(w("t"))))
            self.items["hyperlink"].append((txt[:60],))
        for b in p.iter(w("bookmarkStart")):
            name = wattr(b, "name") or ""
            if name != "_GoBack" and not name.startswith("_Toc") and not name.startswith("_Ref"):
                self.items["bookmark"].append((name,))
        for fn in p.iter(w("footnoteReference")):
            self.items["note reference"].append(("footnote", key[:40]))
        for fn in p.iter(w("endnoteReference")):
            self.items["note reference"].append(("endnote", key[:40]))
        for cr in p.iter(w("commentReference")):
            self.items["comment anchor"].append((key[:40],))
        # Drawings.
        for d in p.iter(w("drawing")):
            self.items["drawing"].append(drawing_item(d))
        for tb in p.iter(w("txbxContent")):
            txt = norm_text(" ".join(x.text or "" for x in tb.iter(w("t"))))
            if txt and not self.inside(tb, "txbxContent"):
                self.items["text box text"].append((txt[:80],))
        for pict in p.iter(w("pict")):
            self.items["drawing"].append(("vml", "", "", "", ""))
        if p.find(f".//{{{M}}}oMath") is not None:
            self.items["equation"].append((key[:40],))
        for ff in p.iter(w("ffData")):
            kind = "text" if ff.find(w("textInput")) is not None else "check" if ff.find(w("checkBox")) is not None else "drop"
            self.items["form field"].append((key[:40], kind))
            self.items["form field name"].append((wattr(ff.find(w("name")), "val") or "",))

    def table(self, t, where):
        ts = wattr(t.find(f"{w('tblPr')}/{w('tblStyle')}"), "val")
        self.table_styles.append(ts)
        try:
            self._table(t, where)
        finally:
            self.table_styles.pop()

    def _table(self, t, where):
        rows = t.findall(w("tr"))
        grid = len(t.findall(f"{w('tblGrid')}/{w('gridCol')}"))
        self.items["table"].append((where, len(rows), grid))
        for ri, tr in enumerate(rows):
            col = 0
            for tc in tr.findall(w("tc")):
                tcpr = tc.find(w("tcPr"))
                span = int(float(wattr(tcpr.find(w("gridSpan")), "val") or 1)) if tcpr is not None else 1
                vm = tcpr.find(w("vMerge")) if tcpr is not None else None
                if span > 1:
                    self.items["merged cell"].append(("span", ri, col, span))
                if vm is not None:
                    self.items["merged cell"].append(("vmerge", ri, col, wattr(vm, "val") or "continue"))
                if tcpr is not None:
                    shd = tcpr.find(w("shd"))
                    if shd is not None and wattr(shd, "fill") not in (None, "auto", "FFFFFF"):
                        self.items["cell shading"].append((ri, col, wattr(shd, "fill").upper()))
                col += span
                self.blocks(tc, "cell")

    def sdt(self, sdt, where):
        pr = sdt.find(w("sdtPr"))
        tag = wattr(pr.find(w("tag")), "val") if pr is not None else None
        alias = wattr(pr.find(w("alias")), "val") if pr is not None else None
        kind = "?"
        if pr is not None:
            for k in ("docPartObj", "date", "dropDownList", "comboBox", "text", "richText", "checkbox", "picture"):
                if pr.find(w(k)) is not None or pr.find(f".//{{http://schemas.microsoft.com/office/word/2010/wordml}}{k}") is not None:
                    kind = k
                    break
        self.items["content control"].append((kind, alias or tag or ""))
        content = sdt.find(w("sdtContent"))
        if content is not None:
            self.blocks(content, where)

    def blocks(self, parent, where):
        for el in parent:
            if not isinstance(el.tag, str):
                continue
            tag = etree.QName(el).localname
            if tag == "p":
                self.paragraph(el, where)
                for sdt in el.findall(w("sdt")):
                    self.sdt_inline(sdt)
            elif tag == "tbl":
                self.table(el, where)
            elif tag == "sdt":
                self.sdt(el, where)
            elif tag in ("ins", "del", "moveFrom", "moveTo", "customXml", "smartTag"):
                self.blocks(el, where)
            elif tag == "sectPr":
                self.section(el)
            elif tag == "altChunk":
                self.items["embedded document"].append(("altChunk",))

    def sdt_inline(self, sdt):
        pr = sdt.find(w("sdtPr"))
        alias = wattr(pr.find(w("alias")), "val") if pr is not None else None
        tag = wattr(pr.find(w("tag")), "val") if pr is not None else None
        self.items["content control"].append(("inline", alias or tag or ""))

    def section(self, s):
        pg = s.find(w("pgSz"))
        mar = s.find(w("pgMar"))
        cols = s.find(w("cols"))
        size = (round(float(wattr(pg, "w") or 0) / 56.7), round(float(wattr(pg, "h") or 0) / 56.7), wattr(pg, "orient") or "portrait") if pg is not None else None
        margins = tuple(round(float(wattr(mar, k) or 0) / 56.7) for k in ("top", "bottom", "left", "right")) if mar is not None else None
        ncols = wattr(cols, "num") if cols is not None else None
        self.items["section"].append((size, margins, ncols or "1"))
        # Which headers and footers apply: default always, first only with
        # titlePg, even only with evenAndOddHeaders in settings.
        title = s.find(w("titlePg")) is not None and onoff(s.find(w("titlePg")))
        refs = []
        for r in s:
            if not isinstance(r.tag, str) or etree.QName(r).localname not in ("headerReference", "footerReference"):
                continue
            t = wattr(r, "type") or "default"
            if (t == "first" and not title) or (t == "even" and not self.even_odd):
                continue
            refs.append((etree.QName(r).localname, t))
            self.hf_ids.append(r.get(f"{{{R}}}id"))
        self.items["header/footer set"].append(tuple(sorted(refs)))


def field_item(instr):
    s = instr.strip()
    kind = s.split()[0].upper() if s.split() else ""
    if kind == "TOC":
        return (kind, "")
    args = norm_text(s[len(kind):])
    # Switches change formatting only; keep the kind and the main argument.
    main = re.sub(r"\\[*@#a-zA-Z]+\s*(\"[^\"]*\"|\S+)?", "", args).strip().strip('"')
    return (kind, main[:60])


def drawing_item(d):
    inline = d.find(f"{{{WP}}}inline")
    anchor = d.find(f"{{{WP}}}anchor")
    el = inline if inline is not None else anchor
    kind = "picture"
    if el is not None:
        g = el.find(f".//{{{A}}}graphicData")
        uri = g.get("uri") if g is not None else ""
        if "chart" in uri:
            kind = "chart"
        elif "diagram" in uri:
            kind = "smartart"
        elif "wordprocessingShape" in uri:
            kind = "textbox" if el.find(f".//{{{WPS}}}txbx") is not None else "shape"
        elif "wordprocessingGroup" in uri:
            kind = "group"
        elif "canvas" in uri:
            kind = "canvas"
    ext = el.find(f"{{{WP}}}extent") if el is not None else None
    size = (round(float(ext.get("cx", 0)) / 36000), round(float(ext.get("cy", 0)) / 36000)) if ext is not None else ("", "")
    if inline is not None:
        return (kind, "inline", size[0], size[1], "")
    wrap = ""
    pos = ""
    if anchor is not None:
        for c in anchor:
            ln = etree.QName(c).localname
            if ln.startswith("wrap"):
                wrap = ln
        ph = anchor.find(f"{{{WP}}}positionH")
        pv = anchor.find(f"{{{WP}}}positionV")
        pos = f"{ph.get('relativeFrom') if ph is not None else ''}/{pv.get('relativeFrom') if pv is not None else ''}"
        if anchor.get("behindDoc") in ("1", "true"):
            wrap += "+behind"
    return (kind, "anchor:" + pos + ":" + wrap, size[0], size[1], "")


def inventory(path):
    with zipfile.ZipFile(path) as z:
        doc = resolve_mce(read_xml(z, "word/document.xml"))
        if doc is None:
            raise ValueError("no word/document.xml")
        styles = Styles(resolve_mce(read_xml(z, "word/styles.xml")))
        numbering = Numbering(resolve_mce(read_xml(z, "word/numbering.xml")))
        wk = Walk(styles, numbering)
        settings = read_xml(z, "word/settings.xml")
        if settings is not None and settings.find(w("evenAndOddHeaders")) is not None:
            wk.even_odd = onoff(settings.find(w("evenAndOddHeaders")))
        body = doc.find(w("body"))
        wk.blocks(body, "body")
        rels = read_xml(z, "word/_rels/document.xml.rels")
        targets = {}
        if rels is not None:
            for rel in rels:
                targets[rel.get("Id")] = "word/" + rel.get("Target", "").lstrip("/").removeprefix("word/")
        for name in sorted({targets[i] for i in wk.hf_ids if i in targets}):
            root = resolve_mce(read_xml(z, name))
            if root is not None:
                sub = Walk(styles, numbering)
                sub.blocks(root, "hf")
                for t in sub.items["text"]:
                    if t[1]:
                        wk.items["header/footer text"].append((t[1][:80],))
                for f in sub.items["field"]:
                    wk.items["header/footer field"].append(f)
                for d in sub.items["drawing"]:
                    wk.items["header/footer drawing"].append(d[:2])
                for t in sub.items["text box text"]:
                    wk.items["header/footer text"].append(t)
        for part, label in (("word/footnotes.xml", "footnote"), ("word/endnotes.xml", "endnote")):
            root = read_xml(z, part)
            if root is None:
                continue
            for n in root.findall(w(label)):
                if wattr(n, "type") in ("separator", "continuationSeparator", "continuationNotice"):
                    continue
                sub = Walk(styles, numbering)
                sub.blocks(n, label)
                wk.items["note text"].append((label, norm_text(" ".join(t[1] for t in sub.items["text"]))[:80]))
        root = read_xml(z, "word/comments.xml")
        if root is not None:
            for c in root.findall(w("comment")):
                sub = Walk(styles, numbering)
                sub.blocks(c, "comment")
                wk.items["comment"].append((wattr(c, "author") or "", norm_text(" ".join(t[1] for t in sub.items["text"]))[:80]))
        # The TOC is a field; mark it explicitly for the census.
        if any(f[0] == "TOC" for f in wk.items["field"]):
            wk.items["table of contents"].append(("TOC",))
    return {k: v for k, v in wk.items.items()}


FEATURES = [
    "text", "paragraph style", "outline level", "list", "alignment", "character formatting", "font",
    "table", "merged cell", "cell shading", "field", "hyperlink", "bookmark", "table of contents",
    "tracked change", "comment", "comment anchor", "note reference", "note text",
    "drawing", "text box text", "equation", "form field", "form field name", "content control", "embedded document",
    "section", "header/footer set", "header/footer text", "header/footer field", "header/footer drawing",
]


def compare(orig, other):
    """Per feature: (items in orig, of which found in other, examples lost, examples added)."""
    out = {}
    for f in FEATURES:
        a = collections.Counter(orig.get(f, []))
        b = collections.Counter(other.get(f, []))
        if f == "text":
            # Text: compare the body's non-empty paragraphs as a multiset.
            a = collections.Counter(t for t in orig.get(f, []) if t[1])
            b = collections.Counter(t for t in other.get(f, []) if t[1])
        total = sum(a.values())
        found = sum(min(n, b[k]) for k, n in a.items())
        lost = list((a - b).elements())[:5]
        added = list((b - a).elements())[:5]
        added_n = sum((b - a).values())
        out[f] = {"total": total, "kept": found, "lost": [repr(x) for x in lost], "added": added_n, "added_examples": [repr(x) for x in added]}
    return out
