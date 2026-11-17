# ADR 0006: Round-trip ODT by projection with preservation

- Status: accepted
- Date: 2026-11-17
- Implemented in: `crates/odf`

## Context

Our promise to users is "open, edit, save, continue in LibreOffice without
loss" for real-world files. Real files carry far more than our model
understands: page layouts, number formats, forms, themes, drawing styles,
LibreOffice extensions (`loext:`), and things from producers we've never seen.

## Decision

The ODT reader and writer project between the file and the model, and
preserve what they don't change.

- **Byte spans.** The reader parses into a tree that remembers each node's
  exact byte span in its part (`xml.rs`).
- **Typed nodes and slots.** Known elements become typed model nodes carrying
  their `Origin`. Unknown ones become slots holding their exact source text.
- **Per-node dirty flags.** On write, each node is handled one of three ways:
  - clean, nothing changed below: its source bytes are copied;
  - clean, something below changed: it is **passed through**. Its original
    start and end tags are kept, and its children are visited. This is what
    stops an edit deep inside an untouched list from being lost;
  - dirty: its tags are regenerated from the model, and its clean children
    are still copied verbatim.
- **Raw package parts.** Parts we don't rewrite are copied as raw compressed
  bytes, in their original order. That covers pictures, settings, thumbnails,
  `manifest.rdf`, RDF metadata files and unknown parts. `mimetype` stays
  first and stored.
- **Producer dialect.** Regenerated XML follows the source part's escaping
  conventions. Older LibreOffice writes `'` raw in text and attributes;
  current LibreOffice writes `&apos;` and `&quot;`.
- **Empty-element spelling is kept.** `<x/>` versus `<x></x>` follows the original.
- **ID stamping is optional and minimal.** Writing our IDs as `xml:id` on
  paragraphs, headings and sections patches only the start tag.
- **Refusal over guessing.** If a namespace prefix we rely on is bound to an
  unexpected URI, the file is refused rather than misread. Encrypted packages
  are refused with a clear error.

## Evidence (Spike 1)

Measured on 480 LibreOffice test documents; 476 are readable (3 encrypted,
1 deliberately malformed). See `docs/spikes/01-odt-roundtrip-results.md`.

- **Touch mode** (20% of nodes marked dirty): 100% of untouched elements came
  out byte-identical. The kill criterion is ≥ 99%.
- **Regen mode** (every node regenerated, all text re-escaped):
  - every element survives canonically;
  - `content.xml` came out byte-identical to LibreOffice's own bytes in 472
    of 474 files, and `styles.xml` and `meta.xml` in all 474;
  - no page renders differently in LibreOffice;
  - LibreOffice's re-save of our file agrees with its re-save of the original
    on every stable element.

## Consequences

- An untouched document is byte-identical part by part, whatever the producer.
- Holding source text costs memory roughly equal to the XML size. That is
  acceptable for documents; the CRDT layer (Spike 3) decides how origins are
  stored long term.
- An origin is only meaningful against the exact source bytes. Once a
  document lives in the CRDT, origins must be dropped or re-anchored on the
  next import (Spike 4).

## Open decisions

| Question | Settled by |
|---|---|
| Behaviour on documents from other producers (Word-converted, older OpenOffice, generators) | Municipal corpus run (Spike 1 follow-up) |
| Real edits (not just touching) verified against LibreOffice making the same edit | Spike 1 follow-up: "edit equivalence" mode |
| How origins and slots are stored once the model lives in the CRDT | Spike 3 and Spike 4 |
