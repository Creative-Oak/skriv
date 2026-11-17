# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Runs inside soffice (see generate_corpus.py). Builds one document per Spike 1
# feature through the UNO API, so every file is genuine LibreOffice output, and
# saves them to $SKRIV_OUT. Writes $SKRIV_OUT/generate.log.json.

import json
import os
import struct
import traceback
import zlib

import uno
from com.sun.star.beans import PropertyValue
from com.sun.star.text.ControlCharacter import PARAGRAPH_BREAK
from com.sun.star.text.TextContentAnchorType import AS_CHARACTER, AT_PARAGRAPH

DANISH = (
    "Kommunen har modtaget din ansøgning om byggetilladelse til en tilbygning på "
    "ejendommen. Vi har vurderet sagen efter byggeloven og bygningsreglementet, og "
    "vi træffer afgørelse inden for den frist, der gælder for sagstypen. Æblerne "
    "på gården, øens færge og ålens vandring er ikke en del af sagen."
)


def prop(name, value):
    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


def new_doc(desktop):
    doc = desktop.loadComponentFromURL("private:factory/swriter", "_blank", 0, (prop("Hidden", True),))
    CURRENT_DOC[0] = doc
    return doc


# Programmatic style names vary between LibreOffice versions ("Text body"
# became "Body Text" in 7.6), so resolve them against what the document has.
ALIASES = {
    "Text Body": ["Body Text", "Text body", "Text Body"],
    "Figure": ["Figure", "Illustration", "Caption"],
}
_RESOLVED = {}


def style_name(doc, family, wanted):
    key = (family, wanted)
    if key not in _RESOLVED:
        names = set(doc.StyleFamilies.getByName(family).getElementNames())
        found = next((c for c in ALIASES.get(wanted, [wanted]) if c in names), None)
        if found is None:
            raise KeyError(f"no {family} style like {wanted!r}; have {sorted(names)[:40]}")
        _RESOLVED[key] = found
    return _RESOLVED[key]


def list_style(doc, kind):
    names = doc.StyleFamilies.getByName("NumberingStyles").getElementNames()
    # "List 1" is LibreOffice's bullet list; older versions call it "List Bullet".
    pick = [n for n in names if ("123" in n if kind == "number" else n in ("List 1", "List Bullet") or "•" in n)]
    if not pick:
        raise KeyError(f"no {kind} list style in {list(names)}")
    return pick[0]


CURRENT_DOC = [None]


def para(text, s, style=None):
    cur = text.createTextCursorByRange(text.getEnd())
    if style:
        cur.ParaStyleName = style_name(CURRENT_DOC[0], "ParagraphStyles", style)
    text.insertString(cur, s, False)
    text.insertControlCharacter(text.getEnd(), PARAGRAPH_BREAK, False)
    return cur


def save(doc, out, name):
    path = os.path.join(out, name)
    doc.storeToURL(uno.systemPathToFileUrl(path), (prop("FilterName", "writer8"), prop("Overwrite", True)))
    doc.close(True)


def png(path, w=64, h=40):
    """A small gradient PNG, written with zlib so no imaging library is needed."""
    rows = bytearray()
    for y in range(h):
        rows.append(0)
        for x in range(w):
            rows += bytes(((x * 4) % 256, (y * 6) % 256, 160))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(bytes(rows))))
        f.write(chunk(b"IEND", b""))


def styles_and_direct(desktop, out):
    doc = new_doc(desktop)
    fam = doc.StyleFamilies.getByName("ParagraphStyles")
    body = doc.createInstance("com.sun.star.style.ParagraphStyle")
    fam.insertByName("Brødtekst kommune", body)
    body.ParentStyle = style_name(doc, "ParagraphStyles", "Text Body")
    body.CharHeight = 11.0
    body.ParaBottomMargin = 212
    note = doc.createInstance("com.sun.star.style.ParagraphStyle")
    fam.insertByName("Brødtekst kommune note", note)
    note.ParentStyle = "Brødtekst kommune"
    note.CharPosture = uno.Enum("com.sun.star.awt.FontSlant", "ITALIC")
    cfam = doc.StyleFamilies.getByName("CharacterStyles")
    em = doc.createInstance("com.sun.star.style.CharacterStyle")
    cfam.insertByName("Fremhævning", em)
    em.CharWeight = 150.0
    t = doc.Text
    para(t, "Afgørelse om byggetilladelse", "Heading 1")
    para(t, "Baggrund", "Heading 2")
    cur = para(t, DANISH, "Brødtekst kommune")
    # Direct formatting on part of a paragraph: becomes an automatic style.
    c = t.createTextCursorByRange(cur.getStart())
    c.goRight(7, True)
    c.CharWeight = 150.0
    c2 = t.createTextCursorByRange(t.getEnd())
    t.insertString(c2, "Ord med tegnformat. ", False)
    c3 = t.createTextCursorByRange(t.getEnd())
    c3.CharStyleName = "Fremhævning"
    t.insertString(c3, "Fremhævet med en tegnstil.", False)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    p = para(t, "Et afsnit med hånd-justeret venstremargen.", "Brødtekst kommune")
    p.ParaLeftMargin = 1000
    para(t, "En note i en arvet stil.", "Brødtekst kommune note")
    save(doc, out, "styles-inheritance-and-direct-formatting.odt")


def lists(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Vilkår for tilladelsen", "Heading 1")
    for level, s in [(0, "Byggeriet skal opføres som ansøgt."), (1, "Facaden skal være i tegl."), (1, "Taget skal være sort."), (2, "Tagpap er ikke tilladt."), (0, "Byggeriet skal færdigmeldes.")]:
        cur = t.createTextCursorByRange(t.getEnd())
        cur.NumberingStyleName = list_style(doc, "number")
        cur.NumberingLevel = level
        t.insertString(cur, s, False)
        t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    cur = t.createTextCursorByRange(t.getEnd())
    cur.NumberingStyleName = ""
    para(t, "Punktopstilling:", "Text Body")
    for s in ["æbler", "pærer", "blommer"]:
        cur = t.createTextCursorByRange(t.getEnd())
        cur.NumberingStyleName = list_style(doc, "bullet")
        t.insertString(cur, s, False)
        t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    save(doc, out, "lists-multilevel-numbering.odt")


def sections(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Før sektionen.", "Text Body")
    sec = doc.createInstance("com.sun.star.text.TextSection")
    cols = doc.createInstance("com.sun.star.text.TextColumns")
    cols.ColumnCount = 2
    sec.TextColumns = cols
    cur = t.createTextCursorByRange(t.getEnd())
    t.insertTextContent(cur, sec, False)
    sec.setName("Tospaltet")
    sc = t.createTextCursorByRange(sec.getAnchor().getStart())
    t.insertString(sc, DANISH + " " + DANISH, False)
    para(t, "Efter sektionen.", "Text Body")
    save(doc, out, "sections-columns.odt")


def tables(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Gebyrer", "Heading 2")
    tbl = doc.createInstance("com.sun.star.text.TextTable")
    tbl.initialize(4, 3)
    t.insertTextContent(t.getEnd(), tbl, False)
    tbl.setName("Gebyrer")
    data = [("Sagstype", "Gebyr", "Frist"), ("Tilbygning", "4.500 kr.", "40 dage"), ("Carport", "1.200 kr.", "20 dage"), ("Nedrivning", "0 kr.", "10 dage")]
    for r, row in enumerate(data):
        for c, v in enumerate(row):
            tbl.getCellByName("ABC"[c] + str(r + 1)).setString(v)
    seps = tbl.TableColumnSeparators
    seps[0].Position = 5000
    seps[1].Position = 7500
    tbl.TableColumnSeparators = seps
    cur = tbl.createCursorByCellName("A4")
    cur.gotoCellByName("B4", True)
    cur.mergeRange()
    para(t, "Efter tabellen.", "Text Body")
    save(doc, out, "tables-widths-merged.odt")


def figures_and_fields(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Kort over området", "Heading 1")
    img = os.path.join(out, "_map.png")
    png(img)
    g = doc.createInstance("com.sun.star.text.TextGraphicObject")
    g.GraphicURL = uno.systemPathToFileUrl(img)
    g.AnchorType = AT_PARAGRAPH
    g.Width = 6000
    g.Height = 3750
    g.Title = "Kort over matriklen"
    g.Description = "Skitse af matriklen med tilbygningen markeret"
    t.insertTextContent(t.getEnd(), g, False)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    # Caption with a sequence field ("Figur 1").
    master = doc.createInstance("com.sun.star.text.fieldmaster.SetExpression")
    master.Name = "Figur"
    master.SubType = 1  # SEQUENCE
    seq = doc.createInstance("com.sun.star.text.TextField.SetExpression")
    seq.attachTextFieldMaster(master)
    seq.Content = "Figur+1"
    seq.NumberingType = 4  # ARABIC
    cur = t.createTextCursorByRange(t.getEnd())
    cur.ParaStyleName = style_name(doc, "ParagraphStyles", "Figure")
    t.insertString(cur, "Figur ", False)
    t.insertTextContent(t.getEnd(), seq, False)
    t.insertString(t.getEnd(), ": Matriklen.", False)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    # Bookmark + cross-reference + page number + fixed date.
    bm = doc.createInstance("com.sun.star.text.Bookmark")
    bm.setName("sk-vilkaar")
    cur = t.createTextCursorByRange(t.getEnd())
    t.insertString(cur, "Vilkårene står her.", False)
    c = t.createTextCursorByRange(t.getEnd())
    c.gotoStartOfParagraph(True)
    t.insertTextContent(c, bm, True)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    ref = doc.createInstance("com.sun.star.text.TextField.GetReference")
    ref.SourceName = "sk-vilkaar"
    ref.ReferenceFieldSource = 2  # BOOKMARK
    ref.ReferenceFieldPart = 1  # PAGE
    t.insertString(t.getEnd(), "Se side ", False)
    t.insertTextContent(t.getEnd(), ref, False)
    t.insertString(t.getEnd(), ". Denne side er nummer ", False)
    pn = doc.createInstance("com.sun.star.text.TextField.PageNumber")
    pn.NumberingType = 4
    pn.SubType = uno.Enum("com.sun.star.text.PageNumberType", "CURRENT")
    t.insertTextContent(t.getEnd(), pn, False)
    t.insertString(t.getEnd(), ". Dato: ", False)
    d = doc.createInstance("com.sun.star.text.TextField.DateTime")
    d.IsFixed = True
    d.IsDate = True
    dt = uno.createUnoStruct("com.sun.star.util.DateTime")
    dt.Year, dt.Month, dt.Day = 2026, 11, 17
    d.DateTimeValue = dt
    t.insertTextContent(t.getEnd(), d, False)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    doc.getTextFields().refresh()
    save(doc, out, "figures-captions-fields.odt")
    os.remove(img)


def comments(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Sagsfremstilling", "Heading 2")
    cur = t.createTextCursorByRange(t.getEnd())
    t.insertString(cur, DANISH, False)
    a = doc.createInstance("com.sun.star.text.TextField.Annotation")
    a.Author = "Sagsbehandler"
    a.Initials = "SB"
    a.Content = "Skal fristen nævnes her?"
    dt = uno.createUnoStruct("com.sun.star.util.DateTime")
    dt.Year, dt.Month, dt.Day, dt.Hours = 2026, 11, 17, 10
    a.DateTimeValue = dt
    rng = t.createTextCursorByRange(t.getEnd())
    rng.gotoStartOfParagraph(True)
    rng.goRight(8, False)
    rng.goRight(10, True)
    t.insertTextContent(rng, a, True)
    t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    save(doc, out, "comments-ranged.odt")


def tracked_changes(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    para(t, "Udkast til afgørelse", "Heading 2")
    para(t, "Vi giver tilladelse til tilbygningen på 40 m2.", "Text Body")
    para(t, "Tilladelsen bortfalder efter et år.", "Text Body")
    doc.RecordChanges = True
    # Insertion
    c = t.createTextCursorByRange(t.getEnd())
    t.insertString(c, "Denne sætning er indsat med ændringssporing.", False)
    # Deletion in the second paragraph
    enum = t.createEnumeration()
    paras = []
    while enum.hasMoreElements():
        paras.append(enum.nextElement())
    p2 = paras[1]
    dc = t.createTextCursorByRange(p2.getStart())
    dc.goRight(3, False)
    dc.goRight(5, True)
    dc.setString("")
    # Format change on the third paragraph
    fc = t.createTextCursorByRange(paras[2].getStart())
    fc.goRight(11, True)
    fc.CharWeight = 150.0
    save(doc, out, "tracked-changes-pending.odt")


def danish_letter(desktop, out):
    doc = new_doc(desktop)
    pages = doc.StyleFamilies.getByName("PageStyles")
    page = pages.getByName("Standard" if pages.hasByName("Standard") else "Default Page Style")
    page.HeaderIsOn = True
    page.FooterIsOn = True
    page.HeaderText.setString("Teknik og Miljø · Byggesag 2026-0193")
    ft = page.FooterText
    ft.setString("Side ")
    pn = doc.createInstance("com.sun.star.text.TextField.PageNumber")
    pn.NumberingType = 4
    pn.SubType = uno.Enum("com.sun.star.text.PageNumberType", "CURRENT")
    ft.insertTextContent(ft.getEnd(), pn, False)
    ft.insertString(ft.getEnd(), " af ", False)
    pc = doc.createInstance("com.sun.star.text.TextField.PageCount")
    pc.NumberingType = 4
    ft.insertTextContent(ft.getEnd(), pc, False)
    t = doc.Text
    addr = doc.createInstance("com.sun.star.text.TextTable")
    addr.initialize(1, 2)
    t.insertTextContent(t.getEnd(), addr, False)
    addr.getCellByName("A1").setString("Anne Østergaard\nSøndergade 12\n8000 Aarhus C")
    addr.getCellByName("B1").setString("Dato: 17. november 2026\nSagsnr.: 2026-0193")
    para(t, "Afgørelse om byggetilladelse", "Heading 1")
    for i in range(6):
        para(t, DANISH, "Text Body")
        if i == 2:
            fn = doc.createInstance("com.sun.star.text.Footnote")
            t.insertTextContent(t.getEnd(), fn, False)
            fn.setString("Jf. byggelovens § 16.")
            t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    para(t, "Klagevejledning", "Heading 2")
    for level, s in [(0, "Du kan klage til Planklagenævnet."), (0, "Klagefristen er 4 uger."), (1, "Fristen regnes fra modtagelsen.")]:
        cur = t.createTextCursorByRange(t.getEnd())
        cur.NumberingStyleName = list_style(doc, "number")
        cur.NumberingLevel = level
        t.insertString(cur, s, False)
        t.insertControlCharacter(t.getEnd(), PARAGRAPH_BREAK, False)
    cur = t.createTextCursorByRange(t.getEnd())
    cur.NumberingStyleName = ""
    para(t, "Med venlig hilsen", "Text Body")
    para(t, "Byggesagsafdelingen", "Text Body")
    save(doc, out, "danish-municipal-letter.odt")


def toc(desktop, out):
    doc = new_doc(desktop)
    t = doc.Text
    idx = doc.createInstance("com.sun.star.text.ContentIndex")
    idx.CreateFromOutline = True
    t.insertTextContent(t.getEnd(), idx, False)
    for h, lvl in [("Indledning", 1), ("Baggrund", 2), ("Vurdering", 1), ("Konklusion", 1)]:
        para(t, h, f"Heading {lvl}")
        para(t, DANISH, "Text Body")
    idx.update()
    save(doc, out, "table-of-contents.odt")


GENERATORS = [styles_and_direct, lists, sections, tables, figures_and_fields, comments, tracked_changes, danish_letter, toc]


def run(*_args):
    ctx = uno.getComponentContext()
    desktop = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
    out = os.environ["SKRIV_OUT"]
    log = []
    for g in GENERATORS:
        try:
            g(desktop, out)
            log.append({"generator": g.__name__, "ok": True})
        except Exception:
            log.append({"generator": g.__name__, "ok": False, "error": traceback.format_exc()})
    with open(os.path.join(out, "generate.log.json"), "w") as f:
        json.dump(log, f, indent=1)
    desktop.terminate()
