# ADR 0003: LibreOffice is the oracle, not the engine

- Status: accepted
- Date: 2026-11-17
- Implemented in: `crates/fidelity`, `fidelity/scripts/`

## Context

Our users are being moved onto LibreOffice and ODF. A file must survive the
journey: LibreOffice, then skriv, then LibreOffice. The obvious shortcuts all
build on LibreOffice itself:

- Collabora Online: server-rendered tiles;
- ZetaOffice: LibreOffice compiled to WASM;
- LOKit;
- wrapping `soffice` for file I/O.

They give perfect parity, but inherit a 35-year-old layout model, greedy line
breaking, a large C++ dependency and a server round trip per keystroke.

## Decision

- **Runtime ODT I/O is our own Rust code** (`crates/odf`). LibreOffice is never
  in the load or save path of the editor.
- **LibreOffice headless is the test oracle.** The fidelity harness has it
  render original and round-tripped files to PDF for page-image comparison,
  and re-save both for a canonical comparison of how it reads them.
- **LibreOffice may later be an optional server-side converter** for formats
  we don't handle (`.doc`, `.rtf`, `.wpd`). It is an escape hatch, not a dependency.
- **The oracle version is pinned and stamped into every report.** CI installs
  the official TDF build, not a distribution package, so local and CI runs agree.

## Evidence

- Oracle in Spike 1: LibreOffice 26.8.1.1, commit `bf6ef266`. The report talks
  about "LibreOffice 25.x"; that is now one major version old.
- LibreOffice is a usable oracle because its output is deterministic where it
  needs to be:
  - rendering the same file twice gave identical page images in every case we checked;
  - the noise floor of the visual diff is exactly zero.
- Re-saving is *not* fully deterministic. An unfixed date field stores the
  time of saving. The harness therefore re-saves the original twice and
  scores only elements that are stable across both runs.
- A negative control (`--canary`) empties paragraphs on purpose. The visual
  check flagged 314 of 320 damaged files. The six it missed have content
  LibreOffice doesn't draw from the paragraph text:
  - spreadsheet-style table cells whose display is recomputed from `office:value`;
  - a hidden section;
  - single CJK variation-sequence glyphs.

  The structural checks cover those cases.

## Consequences

- We own ODF I/O correctness. The harness is how we prove it, so it is a
  product asset, and its reports are meant to be published.
- Layout will not match LibreOffice page for page (ADR 0004, Spike 2b). The
  oracle checks *file* fidelity: does LibreOffice see the same document?
  It does not check that our pages match LibreOffice's pages.

## Open decisions

| Question | Settled by |
|---|---|
| What LibreOffice keeps of our IDs, metadata and private data on re-save | **Answered by Spike 4** (LibreOffice 24.2 and 26.8): `xml:id` on paragraphs, headings and sections, `meta:user-defined` and an RDF metadata part survive; everything a re-save changes is listed in `docs/spikes/04-results.md`. File mode needs no LibreOffice at runtime. |
| Whether a server-side LibreOffice converter ships at all, and where it runs | Later. Not needed for Phase 1. |
