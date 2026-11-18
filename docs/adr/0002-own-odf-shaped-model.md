# ADR 0002: Our own semantic document model, shaped like ODF

- Status: accepted
- Date: 2026-11-17
- Implemented in: `crates/model`

## Context

The editor needs a document model it can version, sync, reference by stable
ID and render with its own layout engine. The report weighed five options
(Layer 2). Here is why each alternative was rejected:

- **ODF XML as the live model (the WebODF approach).** XML DOM is a poor
  editing and CRDT substrate, and WebODF went dormant.
- **An OOXML-shaped model (SuperDoc V2).** It makes ODT the lossy side, which
  contradicts "ODT first-class".
- **Typst or LaTeX source as the model.** Office workers won't edit markup.
- **Block-based (Notion-like).** It can't represent sections, page styles,
  fields or numbering faithfully.

The SuperDoc V1 postmortem adds one more constraint. V1 used ProseMirror's
tree as the source of truth, and V2 had to undo that. The editor
framework's tree must never be the document.

## Decision

The document model is our own. It is ODF-shaped, but it is not ODF XML.

- **Structure is typed.**
  - Block types: `Paragraph` (and headings), `List`, `Table`, `Section`, `TrackedChanges`.
  - Inline types: `Text`, `Span`, `Field`, `Bookmark`, `ChangeMark`, `Annotation`, `Frame`.
- **Properties keep ODF's own vocabulary.** They are stored as ordered
  attributes and read through typed accessors, such as `Paragraph::style_name`,
  `Table::columns`, `Field::target` and `Field::format`.
- **Direct formatting is a delta from a named style.** In ODF it lives in
  automatic styles whose parent is the named style.
  `Styles::explain("paragraph", "P1")` yields "Heading 2 + bold added by hand"
  (design principle 1).
- **Every block and inline that can be referenced has a stable `NodeId`.** IDs
  from the source file (`xml:id`) are kept. Minted IDs are UUID v7 with an
  `sk-` prefix, so they are valid NCNames for `xml:id`.
- **Fields store target and format.** Their rendered text is kept only so an
  untouched file round-trips, and the layout engine recomputes it.
- **Anything not understood goes into a `Slot`.** That covers unknown
  elements, comments and inter-block whitespace. A slot holds the exact source
  text, sits where the content sat, and is tagged with the format it came from.
  Slots exist per element, per style and (through the package) per part.
- **The model has no dependency on XML, zip or any editor framework.** Its
  only dependencies are `uuid` and `serde`.

## Evidence (Spike 1)

- 485 readable documents (480 LibreOffice test documents plus 9 we generated)
  were round-tripped through this model:
  - 100% of untouched elements were preserved byte for byte;
  - 100% of 197,970 elements survived full regeneration canonically;
  - no page rendered differently in LibreOffice.

  See `docs/spikes/01-odt-roundtrip-results.md`.
- 9.7% of XML bytes still travel in slots.
  Mostly that is tab stops, page layouts, notes configuration, number formats,
  tables of contents, footnotes and drawing styles. The model doesn't understand these
  yet, but nothing is lost.

## Consequences

- ODT is the natural projection. DOCX will be a second, engineered projection (Spike 5).
- Keeping properties as ODF attributes means a regenerated element usually
  comes out byte-identical to LibreOffice's (482 of 485 `content.xml` files).
  The price: a later DOCX projection must translate ODF property vocabulary,
  not a neutral one.

## Open decisions

| Question | Settled by |
|---|---|
| How much to lean on verbatim slots (report decision point 1) | Spike 1. Recommendation in the results doc: no extra reliance is needed for LibreOffice-written files. Re-check against municipal templates. |
| How the model is encoded in the CRDT: text with Peritext marks plus block markers, versus a movable tree | **Answered by Spike 3: both, split by level.** Paragraph boundaries are block markers in rich text (splits and joins behave like text, and it is several times smaller than a node per paragraph); sections, lists, items, tables and cells are movable-tree nodes, because moving them while others edit inside only works with a real move. See `docs/spikes/03-results.md` and ADR 0008 (proposed). |
| Whether properties stay as ODF attribute maps or become a typed property set when the CRDT needs per-property merge | **Answered by Spike 3: per-property in the CRDT, keyed by ODF's own vocabulary.** One map entry (or mark) per attribute; direct formatting decomposed from automatic styles into one entry per property (`Δ<property-set>\|<attribute>`), with automatic style names re-derived on export. The model keeps its ordered attribute maps; order is a serialisation hint. |
| Float rules for figures (top, bottom, here) layered over ODF anchoring | Spike 2b |
