# What LibreOffice keeps when it re-saves our ODT

Spike 4 pre-work, run during Spike 1 on 2026-11-17.

- **Oracle:** LibreOffice 26.8.1.1 (`bf6ef2662401c7d6dce671ab4deb26ee0af3a7c1`), macOS, headless.
- **Reproduce:** `python3 fidelity/scripts/lo_preservation.py` writes
  `fidelity/work/preservation/findings.{md,json}`.

## Question

The report's "file mode" design has three parts:

- embed a compact CRDT snapshot as an extra package part listed in `META-INF/manifest.xml`;
- write stable IDs as `xml:id` plus RDF metadata;
- keep a document UUID in `meta.xml` user-defined metadata.

When the file comes back from LibreOffice, we resume if the snapshot survived,
and otherwise diff against the last snapshot we know. The report states plainly
that whether LibreOffice preserves any of this is **unverified**. It assumed
LibreOffice drops our private elements and possibly regenerates IDs.

## Method

We built small ODT files, each carrying one probe, plus one file carrying all
of them. Each was re-saved through LibreOffice three ways:

| Save path | What it models |
|---|---|
| **convert** | `soffice --convert-to odt` |
| **store** | UNO `loadComponentFromURL` then `storeToURL` (writer8), as an integration would |
| **edit** | UNO load, insert "Edited in LibreOffice." at the start of the first paragraph, store. A person opening the file, typing and saving. The edit was confirmed present in the output. |

The UNO paths run as a Python macro inside soffice
(`fidelity/scripts/uno/skriv_uno.py`), because LibreOffice's standalone
Python is killed on start on this machine (exit 137). Before LibreOffice
touched anything, every probe was checked against our own input to make sure
the checks pass.

## Results

The three save paths agreed on every probe. The single combined file agreed
with the separate files.

| Probe | Result after convert, store and edit |
|---|---|
| `xml:id` on `<text:p>` (no RDF refers to it) | **kept** |
| `xml:id` on `<text:p>` referenced from RDF | **kept** |
| `xml:id` on the paragraph the user edits | **kept** |
| `xml:id` on `<text:h>` | **kept** |
| `xml:id` on `<text:section>` | **kept** |
| `xml:id` on `<text:bookmark-start>`; cross-paragraph bookmark name | **kept**; **kept** |
| `<text:meta>` with `xml:id` (inline span of annotated words) | **kept** |
| `<text:meta-field>` with `xml:id` | **kept** |
| `xml:id` on `<text:list>`, `<text:list-item>` | **dropped** (the elements survive, the IDs don't) |
| `xml:id` on `<table:table>`, `<table:table-row>`, `<table:table-cell>` | **dropped** |
| `xml:id` on `<draw:frame>` | **dropped** |
| RDF metadata file (`skriv/meta.rdf`) registered in `manifest.rdf` | **kept**. Re-serialised, but our statements survive. |
| A ~256 KB base64 literal inside such an RDF file (stand-in for a CRDT snapshot) | **kept intact** (file re-serialised) |
| `meta:user-defined` (string doc ID; `float` revision) | **kept**, including the value type |
| Foreign element inside `office:meta` | **kept** |
| Extra package part listed in the manifest (`skriv/state.bin`, `skriv-state.json`) | **dropped** |
| Extra package part **not** listed in the manifest | **file refused**: LibreOffice will not load the document at all |
| Foreign attribute on `<text:p>`, `<table:table-cell>`, `<style:style>`, `<style:text-properties>`, the root element | **dropped** |
| Foreign block element with a paragraph inside | **changed**: the element is removed, the paragraph is kept |
| Foreign inline element with text | **changed**: the element is removed, the text is kept |
| Foreign empty inline element | **dropped** |
| Foreign element inside `office:styles` | **dropped** |

## Where the report was right, wrong or incomplete

**Right.**
- LibreOffice drops our private elements and attributes in content and styles.
  "Design for LibreOffice stripping your private data" is correct.

**Wrong or outdated.**
- *"LibreOffice keeps unknown attributes through UserDefinedAttributes."* Not
  in this test: a foreign attribute on `style:text-properties`, exactly where
  that mechanism lives, was dropped. Unknown attributes are not preserved.
  UserDefinedAttributes may need a specific namespace registration or apply
  only to some property families; that is unverified.
- *"Embed a compact CRDT snapshot as an extra package part listed in
  `META-INF/manifest.xml`."* **This does not work.** LibreOffice drops listed
  extra parts on save. An unlisted part is worse: LibreOffice refuses to open
  the file at all. LibreOffice's own test file `unreferenced_stream.odt`
  behaves the same in the fidelity harness.
- *"Possibly regenerates IDs."* Not observed. Where IDs survive, they survive
  unchanged. Where they don't, they are removed, never replaced.
- *"LibreOffice 25.x."* We tested 26.8.1.1.

**Not covered by the report.**
- **The RDF metadata mechanism survives.** That includes large literals and
  an edit-and-save in LibreOffice. It is the standards-blessed route
  (ODF 1.2+ `manifest.rdf`, `pkg:MetadataFile`).

## Consequences for the design

1. **Embed the snapshot as an RDF metadata file, not as a raw package part.**
   Register `skriv/state.rdf` in `manifest.rdf` as a `pkg:MetadataFile` and
   carry the CRDT snapshot as a literal. That literal could be base64, or an
   `rdf:XMLLiteral` if size matters. It survives LibreOffice's convert,
   store and edit-and-save paths.
2. **Stable IDs:**
   - **paragraphs, headings and sections:** `xml:id` survives, including on
     the paragraph being edited;
   - **passages:** use bookmarks (`text:bookmark-start`/`end` with `xml:id`)
     or `text:meta`; both survive;
   - **tables, lists and figures:** their `xml:id` does not survive. Options
     are a bookmark inside the first cell or the caption, `text:meta` around
     the caption text, or recovering them by diffing against the snapshot
     (structure plus position). Spike 4 decides.
3. **Document identity:** `meta:user-defined` survives with its type, so the
   document UUID belongs there.
4. **Never write a package part without a manifest entry.** The ODT writer
   already only copies existing entries and never adds parts. A test should
   pin this down when the writer gains the ability to add parts.
5. **Never put private data in foreign elements or attributes** in content or
   styles. If something must survive, it goes in RDF or user-defined metadata.

## Not yet tested (Spike 4 proper)

- **Structural edits to paragraphs that carry an ID.** Splitting the paragraph
  (Enter in the middle), merging two, and copy-and-paste (duplicate IDs?).
- **Real UI typing and the "Save" path**, as opposed to UNO `storeToURL`. The
  same filter is involved, but autosave and recovery may differ.
- **Other LibreOffice versions** still common in Danish municipalities (24.x and 25.x).
- **Collabora Online and Euro-Office/OnlyOffice** as the round-trip partner.
- **The size limits of RDF literals**, and whether the RDF part survives
  "Save As" to `.docx` and back. It should not be expected to.
