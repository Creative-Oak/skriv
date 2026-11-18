# Spike 5 results: DOCX fidelity

- **Date:** 2026-11-18.
- **Brief:** [`05-docx.md`](05-docx.md).
- **Code:** `fidelity/scripts/docx/` (feature inventory, validator,
  pipeline, report), `skriv-fidelity roundtrip` (one ODT through skriv).
- **Judgement calls:** [`05-judgements.md`](05-judgements.md). **Raw
  output:** [`05-results-data/`](05-results-data/) (`report.md` is the full
  per-feature report).
- **Corpus:** 300 Danish files sampled from
  [docx-corpus](https://docxcorp.us) (SuperDoc; Danish, confidence ≥ 0.8),
  and 61 English legal and report files from the same corpus, chosen from a
  sample of 1,000 because they contain tracked changes or comments (57 of
  the 61 measured: 4 write page margins with units, `10mm`, which the
  inventory doesn't parse). The files are fetched by ID (`fetch.sh`), not
  committed.
- **Tools:** LibreOffice 24.2.7.2 (driven over UNO), ECMA-376 5th edition
  Transitional schemas, Python `lxml`. **No Microsoft Word and no Open XML
  SDK** (it needs .NET, which this container doesn't have).
- **Reproduce:** `fidelity/scripts/docx/spike5.sh` (about 25 minutes).

## Verdict

**Continue, with DOCX converted by LibreOffice at the boundary, as ADR
0003 already allows, and ODT inside skriv.** skriv adds no loss of its own;
LibreOffice's losses are listed below; three of them need work before
municipal use (tracked formatting changes, tracked paragraph marks,
content controls). **Not established: whether Word opens LibreOffice's
output cleanly** — the schema finds an element-order error in most files
that carry section breaks inside paragraphs, and only Word can say whether
that matters.

| Kill criterion | Result |
|---|---|
| No corrupt files | **No file failed to convert or to read back** (361 of 361 imported and exported as DOCX; 1 failed the ODT→DOCX step, a timeout). Against the ECMA-376 schema: Danish originals 88.7% valid, LibreOffice DOCX→DOCX 73.3%, via ODT 92.6%; English (tracked changes) originals 78.9%, LibreOffice 24.6%, via ODT 22.8%. Word not tested. |
| The fidelity report lists every downgrade | Yes, per feature with examples: [`05-results-data/report.md`](05-results-data/report.md) and below. |

**skriv's own share is zero.** Every DOCX was also taken DOCX → ODT →
DOCX through LibreOffice alone (the control). With skriv reading and writing
the ODT in between, **all 356 comparable documents gave the same result as
the control, feature by feature** — also when skriv regenerated every node
from its model instead of copying untouched XML.

## How common the risky features are

Share of the 300 Danish files containing each (instances in brackets):

| Feature | Files |
|---|---|
| Lists (numbering) | 44.7% (2,679 paragraphs) |
| Tables / merged cells / cell shading | 45.3% / 17.0% / 16.0% |
| Fields in the body / in headers and footers | 7.3% / 24.0% |
| Drawings in the body / in headers and footers | 12.3% / 7.3% |
| Content controls | 7.0% |
| Footnotes | 4.3% |
| Legacy form fields | 3.7% |
| Table of contents | 2.3% |
| Tracked changes / comments | 0.3% / 0.3% |

Files published on the web are final versions, so tracked changes and
comments are rare in the Danish sample; the English set was picked for
them (74% with tracked changes, 2,161 changes).

## What LibreOffice keeps (DOCX → DOCX, and DOCX → ODT → DOCX)

Danish corpus; English where it adds something. "Kept" is the share of
the original's instances found unchanged in the result.

| Feature | DOCX→DOCX | via ODT (= via skriv) | What is lost |
|---|---|---|---|
| Text | 100.0% | 99.9% | 14 paragraphs in 4 files via ODT |
| Lists (level, format, label) | 100.0% | 99.9% | |
| Outline levels, alignment | 100% | 100% | |
| Character formatting (bold, italic, underline, size, colour, highlight…) | 99.3% | 98.7% | font sizes in some hyperlinks; 28 files via ODT |
| Fonts | 98.7% | 98.6% | |
| Paragraph styles | 93.7% | 93.6% | style **names**: LibreOffice renames styles whose names clash ignoring case (`Indent(i)` and `indent(i)` → `indent(i)1`; a user style "Normal" → "Normal1") |
| Tables | 96.9% | 99.3% | |
| Merged cells | 99.9% | **95.2%** (en **87.1%**) | the ODT hop drops grid columns used only by spans: a 4-column table comes back with 3 |
| Fields | 99.3% | 98.2% | |
| Footnotes, endnotes, comments | 100% | 100% | |
| Hyperlinks, bookmarks | 99.1%, 91.3% | 99.1%, 90.2% | bookmarks of duplicate-named form fields renamed |
| Drawings (kind, anchoring, wrap, size) | **74.7%** | 74.5% | "behind text" flips on wrapped shapes and text boxes; VML shapes become DrawingML |
| Drawings in headers/footers | **41.4%** | 41.4% | the same, mostly "behind text" |
| Legacy form fields | 100% (kind) | 100% | names of duplicates renamed (`Kontrol1` → `Kontrol2 Copy 2`) |
| Content controls | 97.1% | 95.6% | building-block (`docPartObj`) controls dropped; date and block text controls become plain inline controls |
| Page size, margins, columns | 91.9% | 91.8% (en 78.7%) | **margins rewritten** in 23 files (LibreOffice converts Word's header and footer distances into its own model and back) |
| Headers and footers shown per section | 97.9% | 97.9% (en 82.6%) | first-page headers dropped in some files |

### Tracked changes (English set, 2,161 changes)

| Change | Original | DOCX→DOCX | via ODT (= via skriv) |
|---|---|---|---|
| Inserted text | 1,047 | 1,034 (98.8%) | 1,035 |
| Deleted text | 365 | 364 (99.7%) | 364 |
| Inserted paragraph marks | 704 | **453 (64.3%)** | 453 |
| Deleted paragraph marks | 9 | 8 | 8 |
| Formatting changes (`rPrChange`) | 27 | **2 (7%)** | 2 |
| Paragraph property changes (`pPrChange`) | 9 | 9 | **0** |

Text insertions and deletions survive with their authors. Changes to
paragraph marks, formatting and paragraph properties mostly don't: a
reviewer would see those edits as already accepted.

## Validity (ECMA-376 Transitional, after markup-compatibility processing)

New errors that LibreOffice introduces into files whose original
validates, most common first:

- `w:sectPr` written as the **first** child of `w:pPr` (the schema wants it
  after `w:rPr`): 38 of 57 English files, 5 Danish. Element order is the
  kind of error Word may report as unreadable content; not verified.
- `w:id` / `w:docPartGallery` where the schema doesn't allow them in
  footers (content controls), `w:docVars` and `w:documentProtection` in the
  wrong place in `settings.xml`.

The ODT hop *removes* most of those (the Danish rate rises from 73.3% to
92.6%) because LibreOffice's ODT→DOCX export doesn't carry the elements
involved. That is not fidelity; those elements are lost.

## What this means for skriv

1. **DOCX in and out through LibreOffice on the server is viable** for
   text, lists, tables, notes, comments and fields, and skriv in the middle
   costs nothing. That is what the web app can do now.
2. **Round-tripping through ODT costs a little extra** (merged-cell grids,
   `pPrChange`, some character formatting), because skriv stores ODT. A
   DOCX file whose owner wants DOCX back should be converted from the
   original where nothing was edited (the Spike 4 baseline idea, applied to
   DOCX: keep the original DOCX and only re-export what changed) — not
   built.
3. **Tracked changes are the weak spot**, as the report expected: anything
   but text insertions and deletions is at risk. A native DOCX reader for
   `w:ins`/`w:del` on paragraph marks, `rPrChange` and `pPrChange` into
   skriv's suggestion branches (Spike 4) is the first native piece worth
   building.
4. **Floating objects**: anchoring and size survive; "behind text" and
   VML don't. Visible in letterheads (header drawings 41%).
5. **Word must be in the loop** before DOCX export is called safe: open
   LibreOffice's output in Word 365 for at least the 43 files with the
   `sectPr` order error.

## Limitations

- **No Word, no Open XML SDK.** Validity is the schema plus markup
  compatibility, implemented here; the SDK's semantic checks (relationship
  targets, ID uniqueness) are not done. Nothing was opened in Word.
- **The inventory is ours.** It compares what survives in each file's
  terms, not what renders; several of its early "losses" were its own bugs
  (run splitting, theme fonts, field results, header inheritance), fixed
  and recorded in `05-judgements.md`. What it can't see (exact positions,
  colours of shapes, table borders, line spacing) is not measured.
- **No visual comparison.** Rendering both files means rendering through
  LibreOffice, which would compare LibreOffice with itself.
- **One LibreOffice version** (24.2). 26.8 may differ, especially in the
  DOCX export.
- **No native DOCX reader was built.** The spike measured the path that
  exists; point 3 above says what to build first.
- **The English set is skewed** towards Australian legislation compilations
  (they carry machine-made tracked changes); real municipal review
  documents may use formatting changes more.
