# ADR 0008: Loro, with a movable tree for structure and rich text for paragraphs

- Status: **proposed** (draft from Spike 3; needs the blind review and a
  decision on the storage trade below)
- Date: 2026-11-17
- Implemented in: `crates/crdt` (`loro_flow_backend` is the proposed encoding)
- Evidence: [`docs/spikes/03-results.md`](../spikes/03-results.md)

## Context

skriv's document model (ADR 0002) must live in a CRDT so that people can
edit together, offline, and so that suggestions can be personal branches
(the Patchwork model) instead of ODF's tracked-change regions. The CRDT has
to carry Word-grade structure that no CRDT ships a proven model for:
tables, sections, lists, styles with direct formatting, and slots for what
we don't understand. Two candidates were shortlisted by the research
report: Loro and Automerge. Both are MIT (ADR 0005).

Spike 3 built the model on both behind one trait and ran the same tests on
three encodings: Loro as a movable tree of blocks, Loro as a hybrid (tree
for structure, rich text with block markers for paragraphs), and Automerge
with its own rich-text block markers.

## Decision

**Use Loro (`loro`, MIT), with the hybrid encoding:**

- **Structure is a `LoroTree`**: sections, lists, list items, tables and
  table cells are nodes. Moving a section is one tree move.
- **Paragraphs are rich text.** Each run of consecutive paragraphs under one
  parent is one `LoroText`. A paragraph starts with a marker character that
  carries its ID and, as marks, its properties.
- **Formatting is Peritext marks**, one per ODF property for direct
  formatting (`Δstyle:text-properties|fo:font-weight`), plus the character
  style. Automatic style names are re-derived on export, never stored.
- **Properties are per key**: paragraph and container properties are one
  entry per ODF attribute; last writer wins per attribute.
- **Tables are a grid keyed by row and column ID**, with rows and columns
  in `LoroMovableList`s and explicit column widths.
- **Slots and inline objects** (footnotes, fields, frames, comments) are
  atomic JSON values in the CRDT.
- **Origins stay outside the CRDT**, in a per-replica baseline taken from
  the last imported ODT; dirty flags are re-derived on export by comparing
  each node with its baseline form.
- **Suggestions are branches**: fork with a new peer, show the diff,
  accept by merging. Attribution is per character.

## Why Loro over Automerge

| | Loro (hybrid) | Automerge |
|---|---|---|
| Section moved while someone edits inside it | edits travel with it (359/359) | 23/359 edits left behind; section split in two in 90/100 runs |
| Load a fresh 100-page document | 1.8 ms | 133 ms |
| Load after a year of edits | 5–11 ms | 1,275–1,567 ms |
| Memory after loading that | 80–95 MiB | 223–345 MiB |
| Stored after a year, ÷ zipped ODT (deflated at rest) | 8.4×, 12.6×, 12.8× | 7.4×, 12.0×, 10.1× |
| Stored without history | 4.9–7.6× | not possible |
| wasm32 bundle, gzip | 784 KB | 482 KB |
| Tables, 10,000 concurrent runs | 0 failures | 0 failures |
| Attribution after accepting a suggestion | kept | kept |

Automerge has no move operation, so moving any structure is copy + delete,
and work done concurrently in the moved part is stranded. Its wasm bundle
is smaller and its files are 5–27% smaller than Loro's after compression.
We judge correct moves, load time and memory to matter more for a browser
editor. Automerge's missing move can't be fixed without building one on
top (last-writer-wins parent pointers with fractional indices: what Loro's
tree already is).

## Consequences

- **Storage needs a decision.** With a full year of history, neither
  library stays under the ~10× target on our generated documents in the
  traces where text is revised (Loro hybrid 12.6–12.8×, Automerge
  10.1–12.0×, both compressed). Against the uncompressed XML both are under
  5×. The policy we expect to adopt: compress at rest; keep the full
  history in cold storage; serve editors a shallow snapshot without
  history (4.9–7.6×) plus recent updates. Trimming history to a recent
  window saves little in Loro (a shallow snapshot stores two states).
- **Text style configuration is not stored in Loro documents**: every
  replica must configure mark expansion identically, in code, at load.
- **Mark keys must not contain `:`** unless deliberately (Loro reads it
  as "style:instance"); ODF names are escaped.
- **Bundle size**: Loro adds about 0.8 MB gzipped to the web app.
- **Deleting a paragraph someone is typing in** leaves their text at the
  end of the previous paragraph. The editor may want to surface that.
- **Emptied tables**: concurrent deletes can leave a table with no rows or
  columns; the editor must treat that as a deleted table.
- **Licences**: Loro brings `xxhash-rust` (BSL-1.0, now allowed) and `imbl`
  (MPL-2.0).

## Alternatives considered

- **Automerge, block markers.** Smaller bundle, smaller files, a mature
  rich-text model with first-class authors. Rejected for the missing move
  and for load time and memory (see above).
- **Loro, a tree node per paragraph.** The encoding the brief suggested. It
  is 1.2–1.6× larger than the hybrid (per-container overhead) and splits
  paragraphs by moving text, so concurrent typing in the tail stays behind.
- **Building our own CRDT.** Not considered seriously: both libraries passed
  the correctness tests.

## Open decisions

| Question | Settled by |
|---|---|
| Do blind reviewers find the merge outcomes acceptable? | The Spike 3 blind review (`docs/spikes/03-blind-review/`) |
| The storage yardstick (zipped ODT or XML; generated or real documents) and the history policy | Team decision; measure on municipal documents |
| Lists and tables inside table cells | When the editor needs them (cells are tree nodes, so the hybrid can nest) |
| Named styles: per-property merge, or one value per style? | When concurrent style editing is a feature |
