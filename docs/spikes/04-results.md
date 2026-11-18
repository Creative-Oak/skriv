# Spike 4 results: skriv files through LibreOffice and back

- **Date:** 2026-11-18.
- **Brief:** [`04-crdt-and-libreoffice.md`](04-crdt-and-libreoffice.md);
  pre-work in [`04-libreoffice-preservation-findings.md`](04-libreoffice-preservation-findings.md).
- **Code:** `crates/filemode` (new), plus small additions to `crates/crdt`
  (Loro flow backend operations, `codec::Baseline::adopt`, soft page breaks
  in tables) and `crates/model` (`Slot::is_layout`).
- **Judgement calls:** [`04-judgements.md`](04-judgements.md). **Raw
  output:** [`04-results-data/`](04-results-data/).
- **Oracle:** LibreOffice **24.2.7.2** (the Ubuntu package in the cloud
  container), driven over a UNO pipe. The pre-work used 26.8.1.1 on macOS;
  the pre-work's preservation probes were re-run on 24.2 and agree
  ([`04-results-data/preservation-lo-24.2.md`](04-results-data/preservation-lo-24.2.md)).
- **Reproduce:** `fidelity/scripts/spike4.sh` (about a minute);
  `SPIKE4_COPIES=100 fidelity/scripts/spike4.sh` for the 100-page document.
  `cargo test -p skriv-filemode` covers the pipeline without LibreOffice.

## Verdict

**Continue, with the design below.** A skriv file edited and saved in
LibreOffice comes back as a suggestion branch that is exactly what the
person did there, attributed to them, and mergeable into a document that
kept changing in skriv meanwhile.

| Kill criterion: "returned copies become attributed suggestion branches with under 5% spurious changes" | Result (LibreOffice 24.2, 10 documents, 53 ODT round trips) |
|---|---|
| Spurious changes from a save without edits | **0 change units** in 20 round trips (convert and UNO store, 10 documents incl. a 20- and a 100-page letter) |
| Branch against the returned file (import errors) | **0 units** in all 66 jobs, DOCX included |
| Branch against the same edits made in skriv | **27 units of 4,211 (0.6%)** over 33 edit scenarios; 28 scenarios exact. All 27 are LibreOffice doing something different from skriv, not import errors (below) |
| Attribution | **99.99%** of characters (107,155 of 107,170) have the same author as when the edit is made in skriv; every letter typed in LibreOffice is attributed to its user |
| Merge into skriv's document after a concurrent edit there | both edits present in all 51 cases; residual 27 units, the same 27 |
| Re-export after making the returned file skriv's source file | **98.6%** of nodes copied verbatim from LibreOffice's file (1,385 of 1,404) |
| Import time | ≤ 4 ms for the corpus, 43 ms for 20 pages, 248 ms for 100 pages |

**The DOCX path fails the criterion**, for reasons that belong to Spike 5:
a trip through Word format keeps skriv's document ID and version (they're
in `meta:user-defined`, which becomes a custom document property) but
loses every `xml:id` and the RDF part, and LibreOffice's DOCX filter
rewrites the document (headings with outline numbering come back as list
items, style formatting comes back as direct formatting). The import is
still exact against the file, but 52–195 change units per document appear
without any edit. See [DOCX](#docx).

## What was built

**State in the file** (`state.rs`), following the pre-work:

- `xml:id` on every paragraph, heading and section (the codec's
  `stamp_ids`). LibreOffice keeps them, including on the paragraph being
  edited.
- `meta:user-defined` `skriv:doc-id` and `skriv:version` (the Loro
  frontiers the file was written at, base64).
- `skriv/state.rdf`, an ODF 1.2 metadata file registered in `manifest.rdf`
  and listed in the manifest, carrying the document ID, the version, the
  format (`loro-flow-1`) and, by default, a **shallow snapshot**: the state
  at the version, without history.

**Import** (`noise.rs`, `tracked.rs`, `align.rs`, `apply.rs`), in this order:

1. Find the starting state: the file's snapshot; else skriv's copy forked
   at the file's version; else (no version) skriv's current state by
   document ID. The branch is a fork of it with a new peer, named after the
   file's last author.
2. Accept the file's **new tracked changes** into the branch (marks and
   regions removed, text as the person left it); keep the ones the document
   already had, and give those their skriv change IDs back (LibreOffice
   renumbers every change ID on save).
3. Remove **layout output** (soft page breaks).
4. **Recover IDs** LibreOffice drops (lists, list items, tables, rows,
   columns, cells) from the IDs of the paragraphs inside them, weighted
   towards the nearest container; then match what's left by position and
   text under the same parent.
5. **Diff into CRDT operations** on the branch: joins and splits recognised
   as such (so moved text keeps its authors), moves within and across
   parents, inserts, deletes; text diffed by words with a semantic cleanup;
   marks and properties per key. Anything equal up to LibreOffice's
   respelling (lengths in inches, revision IDs, attribute order, named
   style written directly or through an automatic style, footnote and
   change IDs, IDs and bookkeeping inside inline objects) is left alone.

**Measurement** (`units.rs`, `scenario.rs`, `src/bin/spike4.rs`,
`fidelity/scripts/spike4_lo.py`): every edit scenario is described by
unique snippets of text and performed twice, by LibreOffice through UNO
(typing, the Bold command, Enter, copy and paste through the view) and by
skriv through CRDT operations. A change unit is one character inserted or
deleted, one character's formatting per property, one property, or one
block/row/column added, removed or moved.

## What LibreOffice changes on every save

Found by saving without edits and measuring; each became a rule in
`noise.rs` (counts are from the 20 no-edit round trips):

| Change | Rule |
|---|---|
| Attribute order (1,122 nodes) and our `xml:id` read back as a property (836) | bookkeeping, never compared |
| `officeooo:rsid` / `paragraph-rsid` on everything it touches | ignored |
| Lengths rewritten in inches (`8.5cm` → `3.3465in`), in properties and inside frames | compared in millimetres to 0.1 mm |
| Automatic style names of table columns and cells renumbered (`Table1.A1` → `Table1.A2`), sections given an automatic style (`Sect1`) | names ignored (the codec doesn't resolve these styles into properties yet) |
| A paragraph's named style written directly or through an automatic style with it as parent | the same |
| Footnote IDs renumbered (`ftn0` → `ftn1`), tracked-change IDs renumbered, tracked-change regions reordered | compared by content |
| Soft page breaks in long documents (34), including between table rows | removed (layout output) |
| Leading and repeated spaces written as `<text:s/>` | equal to spaces when measuring |
| Field results recomputed (a page number field showed `0` after headless convert) | field definitions compared, results not |
| `--convert-to` writes only relative column widths for tables it hasn't laid out; a later save adds absolute ones | a width on one side only is no change |

Nothing else changed. In particular **paragraph text, character
formatting, list structure and table structure came back unchanged** in
every no-edit round trip.

## Edit scenarios

| Scenario | Documents | Residual against skriv | Note |
|---|---|---|---|
| type, delete, bold, split, delete paragraph, duplicate paragraph, move paragraph, cell type, list item, session (several edits), tracked (Track Changes on) | 1–5 each | **0** | |
| row insert | 2 | 0 and 3 | LibreOffice's new row takes the style of the row below the header (`Table Contents`), skriv's copies the row above (`Table Heading`) |
| column delete | 1 | 1 | LibreOffice widens the remaining column to keep the table width; skriv keeps the other widths |
| join | 2 | 0 and 20 | Joining a paragraph whose bold came from its paragraph style: LibreOffice keeps the joined text bold (as character formatting); skriv's join drops it |
| heading | 2 | 0 and 1 | Applying "Heading 2" in LibreOffice cleared the paragraph's direct bold |
| split, in a document with Track Changes already recording | 1 | 2 | LibreOffice renamed the automatic style of a paragraph inside an earlier tracked deletion; deleted content isn't style-resolved |

These are skriv-vs-LibreOffice behaviour, not file mode. Two are worth
copying into skriv's own editing: keep the look of joined text, and widen
the remaining columns when one is deleted. How a new row is styled should
be checked against Word before choosing.

Two discoveries changed the scenario definitions (`04-judgements.md` 7):
Enter at the end of a list item that has a sub-list moves the sub-list into
the new item (LibreOffice and Word both do this; the first skriv
re-enactment didn't), and pasting a paragraph "after itself" at the end of a
section lands outside the section.

## History and privacy

The default snapshot is **shallow**: the state, no history. The test
typed a sentence and deleted it before writing each file:

| Snapshot in the file | Deleted sentence recoverable | Size (municipal letter; 100-page letter) |
|---|---|---|
| none (ID and version only) | no | 29 KB; 54 KB |
| shallow (default) | **no** in all 10 documents | 41 KB (+41%); 278 KB (×5.1) |
| full history | **yes** in all 10 documents | 48 KB; 400 KB |

The file without any edit history still lets skriv make the branch from
the file alone (a skriv that never saw the document), and that branch
merges into the full document (`shallow_snapshot_keeps_history_out_and_still_merges`).
Full history in a file someone emails out would hand over everything ever
deleted; it should only ever be an explicit choice.

## Email

Email doesn't change the file: MIME base64 round-trips bytes, and nothing
in the ODT depends on the file name. The risks are elsewhere: a recipient
opening an older copy (the version in the file makes the branch start in
the right place, and the merge test covers a document that moved on), a
recipient saving as DOCX (below), and the shallow snapshot's size on large
documents (×5 at 100 pages; `none` keeps the file at the plain size for
documents that are only sent inside the organisation).

## DOCX

Edit in LibreOffice, save as DOCX, reopen, save as ODT:

- `skriv:doc-id` and `skriv:version` **survive** (custom properties), so
  the branch starts at the right version. Writing the version to
  `meta.xml` as well as to the RDF part was added because of this.
- `xml:id`s and the RDF part are lost. Paragraphs are matched by text.
- The import is exact against the file, but the DOCX filter changes the
  document: 52–195 units per document without any edit (3,835 in 20
  pages), mostly properties (style formatting written as direct formatting)
  and structure (outline-numbered headings become lists). Attribution
  survives where structure does (4 of 9 documents fully) and is lost where
  headings turned into list items.

This is a Spike 5 problem (DOCX fidelity): the same normalisation approach
applies, with a second set of rules for what the DOCX round trip rewrites.
Until then, a file that came back through DOCX should be shown as such, with
the diff limited to text.

## Re-baselining (Spike 3's open question)

Spike 3 proposed keeping the last imported ODT next to the CRDT as the
source file, so untouched elements are written back byte for byte. After a
LibreOffice round trip:

- Using the returned file as the new source with plain baselines,
  **623 nodes reused and 1,153 regenerated**: skriv's state keeps its own
  spelling of things LibreOffice re-spelled, so nearly every node looks
  changed.
- With `Baseline::adopt` (record skriv's form wherever it is equivalent to
  the file's, by the same rules as the import), **1,385 reused and 19
  regenerated**. The 19 are the paragraphs and tracked-change lists where
  skriv accepted LibreOffice's tracked changes into the branch, which do
  differ from the file.

So the policy holds: **re-baseline on every file that comes back**, with
equivalence, not equality.

## Limitations

- **One LibreOffice version** (24.2). 26.8 was only probed for
  preservation. 25.x, Collabora Online and OnlyOffice/Euro-Office not run.
- **Generated corpus** (9 documents + the scaled letter), not real
  municipal files. Scenarios aim at plain paragraphs; edits next to fields,
  footnotes and comments are not covered.
- **Scripted edits through UNO**, not a person typing in the UI.
  AutoCorrect, autocomplete and the clipboard path of a real desktop are
  not exercised.
- **Automatic styles of sections, table cells and columns** are compared by
  name only; the codec should resolve them like paragraph and text styles.
  Until it does, a change of cell background made in LibreOffice is not
  imported.
- **The ODT writer writes repeated spaces literally** (found here; a task
  is queued): LibreOffice would show "din  bygge" with one space. The
  importer keeps LibreOffice's `<text:s/>` objects so nothing is lost, but
  the one space each delete or split leaves behind is attributed to the
  LibreOffice user (the 15 characters of the 99.99% attribution).
- **Per-change authors**: a file with several people's tracked changes
  becomes one branch under the last saver's name. Each LibreOffice author
  could become their own peer; not done.
- **Moves of paragraphs** are copy and delete in the Loro flow encoding,
  so a paragraph moved in LibreOffice is re-attributed to the mover (Spike 3
  limitation).

## Answers for ADRs

- **ADR 0003** (LibreOffice as oracle): what LibreOffice keeps is settled
  for 24.2 and 26.8, and the noise of a re-save is fully characterised for
  this corpus. LibreOffice stays out of the runtime path; file mode needs no
  LibreOffice.
- **ADR 0006** (projection with preservation): re-baseline on import, with
  equivalence (`Baseline::adopt`).
- **ADR 0008** (Loro): shallow snapshots, `fork_at` and update export
  between a shallow-snapshot branch and the full document work as file mode
  needs. The file format is `loro-flow-1`; the format string is in the file
  so a future encoding can be detected.
