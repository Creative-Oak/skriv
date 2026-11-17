# Spike 1 results: ODT round-trip fidelity

- **Date:** 2026-11-17.
- **Oracle:** LibreOffice 26.8.1.1 (`bf6ef266`), macOS, headless.
- **Code:** `crates/model`, `crates/odf`, `crates/fidelity`.
- **Reproduce:**

  ```bash
  fidelity/scripts/fetch-lo-corpus.sh
  ```

  ```bash
  cargo run --release -p skriv-fidelity -- run fidelity/corpus/libreoffice fidelity/corpus/generated --canary --enforce
  ```

  The full run takes about 4 minutes with 6 LibreOffice workers.

## Verdict

**Continue.** Both kill criteria are met on everything we could test:

- 100% of untouched elements were preserved byte for byte;
- no page rendered differently after a LibreOffice → skriv → LibreOffice
  round trip, in 484 renderable documents.

**One major caveat.** The corpus is LibreOffice's own test documents plus
documents LibreOffice generated for us. The real test, Danish municipal
templates, is still to come. Several findings below say where that corpus
could behave differently.

## Corpus

| Corpus | Files | Notes |
|---|---|---|
| LibreOffice core test documents | 480 | Every `.odt` under 3 MB from `sw/qa/extras/{odfexport,odfimport,layout,uiwriter}/data` and `sw/qa/core/data/odt` at tag `libreoffice-26.8.1.1`. Fetched, not committed. |
| Generated | 9 | One real LibreOffice document per core feature, built through UNO, including a Danish municipal letter. Committed, CC0. |
| Municipal templates | 0 | Placeholder in `fidelity/corpus/municipal/`; waiting for files from partners. |

**Readable: 485 of 489.** The 4 unreadable files are:
- 3 password-encrypted documents, refused with a clear error; encryption is out of scope;
- 1 deliberately malformed test file (`rovas-numbering.odt`), whose
  `office:body` holds no `office:text`. We refuse it rather than guess.

Of the 485, LibreOffice itself could not render one
(`unreferenced_stream.odt`, a test for a package with an unlisted part; see
Spike 4 findings). It is excluded from visual scoring. That leaves **484**
visually scored files: 21 MB of XML, 197,970 elements.

## How it was measured

Every file is round-tripped in three modes, plus a negative control.

| Mode | What it does | What it measures |
|---|---|---|
| identity | read, write, change nothing | the baseline |
| touch | mark every 5th modelled node dirty and regenerate it. The regenerated start tags get their attribute order **reversed on purpose** (see "Surprises") | **Kill criterion 1:** is every element outside a touched node byte-identical? Are touched elements canonically equal? |
| regen | mark *every* node dirty and re-escape all text | how much the model genuinely understands; **kill criterion 2** |
| canary | empty every third non-empty paragraph | the harness must report this as loss. If it doesn't, the harness is broken. |

The comparisons:

- **Bytes, per element.** Original and output are parsed independently, and
  elements are aligned by tree path.
- **Canonical form, per element subtree.** Attribute order is ignored, and so
  is whitespace between blocks. Every reference to an automatic style is
  replaced by a hash of that style's properties, so LibreOffice's invented
  names (`P1`, `T3`) don't count.
- **Pages.** LibreOffice renders both files to PDF, poppler rasterises them
  at 75 dpi, and the pages are diffed pixel by pixel. Any changed pixel counts
  (threshold 0). This is justified because identity round trips measure
  exactly 0, and rendering six identical copies gives six identical images.
  Every variant is rendered from the same path, because some documents print
  their own full file path in a header.
- **The LibreOffice loop.** LibreOffice re-saves the original twice and our
  regenerated file once. An element counts as kept if it appears in our
  re-save as in the original's. Elements that differ between the two
  re-saves of the original are noise and are excluded: 1,047 of them, mostly
  unfixed date fields, which store the time of saving.

## Numbers

| Criterion | Target | Result | Met |
|---|---|---|---|
| Untouched elements byte-preserved (touch mode) | ≥ 99% | **100%** (157,359 / 157,359) | yes |
| Files with visible loss, identity | 0 | **0** of 484 | yes |
| Files with visible loss, regenerate everything | 0 | **0** of 484 | yes |
| Files with visible loss, touch (attributes deliberately reversed) | diagnostic | 5 of 484; see "Surprises" | n/a |

| Further measure | Result |
|---|---|
| Touched elements canonically equal after regeneration | 100% (25,257 / 25,257) |
| All elements canonically equal after regenerating everything | 100% (197,970 / 197,970) |
| Regenerated parts byte-identical to LibreOffice's own bytes | `content.xml` 482/485; `styles.xml` 485/485; `meta.xml` 485/485 |
| LibreOffice loop: stable elements LibreOffice reads the same way | 100% (199,075 / 199,075) |
| Negative control: damaged files flagged as visible loss | 314 / 320 |
| XML carried in preservation slots (not yet modelled) | 9.7% of bytes |
| Read time per file | median 0.4 ms; max 78 ms (1.3 MB of XML) |
| Regenerate-everything write time per file | median 0.05 ms; max 1.6 ms |

Every feature bucket scores 100% in every column. The buckets are paragraphs,
headings, lists, list styles, styles, tables, sections, frames and images,
fields, comments, tracked changes, bookmarks, spans and links, spaces, tabs
and breaks, and metadata. Per-feature counts are in the generated report
(`summary.md`/`index.html`).

### What the model doesn't understand yet (slots)

These are carried verbatim and lost nowhere, but the model can't edit them.
The most common, by count:

- `style:tab-stops` (1,364);
- `text:notes-configuration` (959);
- `style:page-layout` (770);
- `text:linenumbering-configuration`;
- `text:sequence-decls`;
- `style:background-image`;
- `loext:graphic-properties`;
- `style:columns`;
- `text:note` (footnotes, 107);
- `office:forms`;
- number formats;
- tables of contents and indexes;
- drawing shapes, controls and embedded objects.

Footnotes, tab stops, page layouts and tables of contents are the obvious
next modelling work. All four matter for municipal letters.

## Addendum: ODT exported by Microsoft Word (2026-11-17)

**The corpus.** Ten public EU documents (EU-FOSSA 2 project reports, a
code-list guideline, school-survey questionnaires and results), 10–125 pages
each. All were written by **Microsoft Word's own ODT export**
(`MicrosoftOffice/15.0`). That is the opposite of the main corpus, and close
to what public-sector users will bring: documents made in Word and saved as
ODT. They live in `fidelity/corpus/municipal/` and are not committed.

**Results.** Everything passed:
- 0 unreadable;
- 100% of untouched elements byte-preserved (81,020 elements);
- 100% canonical survival when regenerating everything;
- 0 files with visible loss in identity and regen mode;
- 100% LibreOffice re-save agreement;
- the canary flagged all 10 damaged files.

**Byte identity.** Regenerated `content.xml` matched Word's bytes in 9 of 10
files; `styles.xml` and `meta.xml` matched in all 10.

**Slots.** 4.8% of the XML went into slots, mostly `style:tab-stops` (1,722)
and footnotes (`text:note`, 317). Word documents lean even harder on
footnotes and tab stops than LibreOffice's, which supports putting them first
in the modelling order.

**The attribute-order diagnostic.** In touch mode, 5 files rendered slightly
differently (up to 0.5% of a page). This is the same LibreOffice import
quirk, and Word's export triggers it more often. It confirms that "write new
attributes in LibreOffice's serialisation order" is real work, not polish.

**Caveat.** These are EU-level documents, not Danish municipal ones, and only
ten of them. They are a much better proxy than LibreOffice's test suite, but
the decision-point-1 re-check should still use Danish municipal documents
when we can get some.

## Surprises and corrections

1. **Our regenerated XML is byte-identical to LibreOffice's.** Properties stay
   in ODF vocabulary with their order preserved (ADR 0002), so regenerating a
   LibreOffice document reproduces LibreOffice's bytes almost everywhere.

   That made the first version of the kill metric meaningless: in touch
   mode, *every* file came out identical, so "untouched elements preserved"
   could not be told apart from "everything happened to serialise the same".

   **Fix:** touch mode now reverses attribute order on regenerated start
   tags. Touched elements really differ (484 of 485 files changed), and
   untouched ones are still 100% identical.

2. **LibreOffice's import depends on attribute order.** XML says attribute
   order is insignificant. With order reversed, 5 documents render
   differently. Bisection found the elements responsible:
   - language and country attributes. Their order changes which locale an
     apostrophe gets, and so which glyph is drawn;
   - `fo:font-style` with its Asian and complex variants;
   - `style:text-rotation-angle` with `style:text-rotation-scale`.

   This does **not** affect the real writer: it keeps each element's source
   attribute order, and regen mode has 0 visual differences.

   It does matter when we *add* attributes. The model now inserts the known
   pairs in LibreOffice's order (`skriv_model::ORDER_SENSITIVE`). That table
   is known to be incomplete: some orders that respect it still differ.
   Follow-up: emit new attributes in LibreOffice's own serialisation order.

3. **Producer dialect.** Older LibreOffice writes `'` raw. Current LibreOffice
   writes `&apos;`, and `&quot;` in text. The writer now learns this per part.
   The 3 remaining files whose regenerated `content.xml` differs in bytes
   (still canonically equal) were hand-edited: mixed conventions, numeric
   character references (`&#x2013;`) and a byte-order mark. Expect more of
   this from non-LibreOffice producers.

4. **Real bugs the corpus found in our code:**
   - **A UTF-8 byte-order mark** shifted every byte span by three. Three files
     failed to parse.
   - **Edits inside untouched ancestors were lost.** The first writer
     copied a clean ancestor whole, which would have dropped an edit inside a
     list item. Fixed with pass-through (ADR 0006) before the corpus ran;
     unit and property tests pin it down.
   - **Slot statistics undercounted.** A slot inside an untouched paragraph
     was never visited.

5. **Harness bugs the negative control found.** The first visual check
   passed deliberately damaged documents:
   - the per-page threshold (0.1%) was coarser than a missing sentence;
   - a relative LibreOffice profile path silently stopped rendering;
   - documents that print their own path differed only because of our
     directory names.

   All are fixed. The canary is now part of every run and of CI.

6. **What the visual check can't see.** The 6 damaged documents it missed
   hide the damage from rendering:
   - spreadsheet-style table cells whose display text LibreOffice recomputes
     from `office:value`;
   - a hidden section;
   - single CJK variation-sequence glyphs.

   The structural checks catch all of these, so neither check is enough alone.

7. **The report was wrong about LibreOffice preservation.**
   - Embedding a snapshot as an extra package part fails. LibreOffice drops
     listed parts, and refuses to open a file with an unlisted part.
   - RDF metadata files survive, so they are the vehicle.
   - "UserDefinedAttributes keeps foreign attributes" did not hold.

   See [04-libreoffice-preservation-findings.md](04-libreoffice-preservation-findings.md).

8. **The report's version is outdated.** It refers to LibreOffice 25.x; the
   current release used here is 26.8.

## Where we fall short, honestly

- **Corpus bias.** 480 of the 489 files were written by LibreOffice, mostly
  recent versions. Municipal documents are often:
  - converted from `.docx` by LibreOffice or by Word's ODF export;
  - created years ago in older versions;
  - produced by template generators.

  Each of these could break assumptions that LibreOffice files never test:
  dialect, prefixes, attribute order, element order inside comments and
  change-info, and prefixes bound to standard namespaces. The reader refuses
  a file whose prefixes are bound to unexpected namespaces; it does not
  remap them.
- **"Edited" means touched, not changed.** Touch mode regenerates elements
  without changing content. Real edits (typing, splitting paragraphs,
  changing styles) are covered by unit tests, but not yet at corpus scale
  with an oracle. Follow-up: an **edit-equivalence mode**. It applies the
  same edit in skriv and in LibreOffice through UNO, then compares the
  results.
- **The model is ODF-vocabulary-shaped.** Properties are ODF attribute maps
  read through accessors. That is why fidelity is excellent. It is also why
  DOCX (Spike 5) will need a real property translation layer, and why Spike 3
  must decide whether properties stay as maps in the CRDT.
- **Not tested here:** flat ODT (`.fodt`), templates (`.ott`; the harness
  reads them but the corpus has none), encrypted documents, and files over 3 MB.
- **CI not run.** The workflow is written, and its LibreOffice download URL
  is verified to exist, but there is no GitHub remote yet, so it has never
  run.

## Recommendation on decision point 1

> *If ODT round-trip of real municipal documents is below ~95% structural
> fidelity on untouched content, increase reliance on verbatim preservation
> slots.*

**Don't increase reliance on slots now.** On LibreOffice-written documents,
untouched content is 100% byte-preserved and the model regenerates 100% of
elements faithfully. More slots would only reduce what we can edit.

1. **Keep the current split.** Typed structure, ODF-vocabulary properties,
   and verbatim slots only for what isn't modelled. That is 9.7% of XML bytes,
   mostly page and notes configuration, number formats and drawing styles.
2. **Shrink the slot share deliberately**, in this order: footnotes and
   endnotes, tab stops, page layouts and master pages, tables of contents and
   indexes. These are what municipal letters use.
3. **Re-check decision point 1 on the municipal corpus.** The likely failure
   mode isn't slots; it's dialect and convention differences in *regenerated*
   elements from non-LibreOffice producers. The harness already reports this
   per element (touched elements' canonical equality, the regen byte
   identity of each part).
4. **If the municipal corpus falls below 95%**, the cheaper lever is finer
   dirty tracking, not more slots. One option is regenerating only the
   changed attribute of a start tag, the way ID stamping already works.

## Next steps

- **Municipal templates.** Run the harness on them as soon as they arrive.
  The README in `fidelity/corpus/municipal/` explains where they go; they
  stay out of git.
- **Edit-equivalence mode** for corpus-scale real edits.
- **Model** footnotes, tab stops, page layouts and tables of contents.
- **Emit new attributes in LibreOffice's serialisation order** (replacing the
  minimal pair table).
- **Push to a remote and let CI run.** Nightly runs give a per-feature trend
  over time; publish it.
- **Plan for Spikes 2, 2b and 3:** [02-03-next-spikes-plan.md](02-03-next-spikes-plan.md).
