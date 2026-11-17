# Spike 3 results: Loro vs Automerge for tables, sections and tracked changes

- **Date:** 2026-11-17.
- **Brief:** [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md), "Spike 3";
  risk and kill criteria in [`03-crdt-structure.md`](03-crdt-structure.md).
- **Code:** `crates/crdt` (trait, codec, three encodings, tests, examples),
  `prototypes/crdt-wasm-size`. Judgement calls are listed in
  [`03-judgements.md`](03-judgements.md).
- **Libraries:** `loro` 1.18.0 and `automerge` 0.12.0 (Rust), both MIT.
- **Machine:** a 4-core Linux cloud container, no GPU. Times are wall
  clock, native (not wasm), release builds, and some runs shared the
  machine with others; treat them as ±20%.
- **Reproduce:**

  ```bash
  cargo test -p skriv-crdt                                   # all tests, debug: ~6 min
  SKRIV_FUZZ_RUNS=10000 cargo test --release -p skriv-crdt --test tables -- --nocapture
  cargo run --release -p skriv-crdt --example spike3 -- history 250 400   # ~1 h
  cargo run --release -p skriv-crdt --example spike3 -- create 100
  cargo run -p skriv-crdt --example scenarios                # blind review sheets
  prototypes/crdt-wasm-size/measure.sh                       # wasm32 sizes
  ```

## Verdict

**Continue, with Loro, using a hybrid encoding: a movable tree for
structure and rich text with block markers for paragraphs.** ADR 0008 is
drafted as *proposed*, because two things are still open:

1. **Storage: neither library stays under ~10× with a full year of
   history.** In the trace where text is mostly added, both pass once
   stored compressed (Automerge 7.4×, Loro hybrid 8.4× the zipped ODT).
   In the two traces where text is also revised, both fail
   (Automerge 12.0× and 10.1×, Loro hybrid 12.6× and 12.8×
   deflated). Against the uncompressed XML everything is 1.4–4.6×. So the
   criterion as written is not met by either; meeting it needs a history
   policy (Loro can drop or archive old history; Automerge cannot), or a
   decision that the yardstick should be the XML, not our unusually
   compressible zipped test documents.
2. **The blind review has not happened.** The 21 scenarios are ready
   ([`03-blind-review/`](03-blind-review/)); nobody has rated them.

Why Loro: it is the only one of the two that moves structure correctly
while others edit inside it, it loads a year-old document in milliseconds
where Automerge needs 1.3–1.6 s natively, and it uses less than half the
memory (80–95 MiB against 223–345 MiB). Automerge's advantages are a smaller wasm bundle
(482 KB vs 784 KB gzipped) and smaller raw files, which compression at
rest largely erases.

Neither library lost text in any of the automated tests (the one way an
encoding can discard concurrent typing, deleting the paragraph it is
typed into, is surprise 11). Both kept tables rectangular in 10,000
concurrent runs. Both kept attribution through accepting a suggestion.

## What was built

One trait, `CrdtDoc`, with every test written once and run against three
implementations:

| Encoding | Library | Structure | Paragraphs |
|---|---|---|---|
| `loro` (tree) | Loro | movable tree: every paragraph, section, list, item, table and cell is a node | one `LoroText` per paragraph node |
| `loro-flow` (hybrid) | Loro | movable tree for sections, lists, items, tables and cells | one `LoroText` per run of paragraphs, a `\n` marker per paragraph carrying its ID and properties as marks |
| `automerge` | Automerge | block markers with `parents` (Automerge's 2.2+ rich-text model); tables are separate objects referenced from a marker | one `Text` per flow (the body, each cell) |

The tree encoding is what the brief describes for Loro. The hybrid was
added mid-spike when the tree's size turned out to be mostly per-container
overhead, which confounded "Loro or Automerge" with "tree or text"
(judgement 1).

Shared by all three (`codec.rs`, `logical.rs`):

- **Flow text** with Peritext-style marks. Character style is a `cs` mark;
  direct formatting is one mark per ODF property
  (`Δstyle:text-properties|fo:font-weight`), expanding at the end like
  Peritext's bold; links and inline objects don't expand.
- **Paragraph properties** are one entry per ODF attribute: the paragraph
  style plus the direct-formatting delta, decomposed from LibreOffice's
  automatic styles (`P1` = "Text body" + bold).
- **Tables** are a grid keyed by row and column IDs, with explicit column
  widths (`@width`), merged cells as properties, and spans clamped on
  export when a concurrent delete cuts through them.
- **Sections** are containers with a property map.
- **Tracked changes** are personal branches (the Patchwork model): a
  suggestion is a fork; reviewing shows its diff against the fork point;
  accepting is a merge; every character keeps its author.
- **ODT out:** the CRDT state is exported to the model and written by
  `crates/odf` over the source file. Nodes whose logical form is unchanged
  get their source bytes back (see "Origins and slots" below).

## Verify first: versions, licences, features

Checked 2026-11-17 against crates.io, npm and the crate sources.

| Claim in the research report | Finding |
|---|---|
| Loro is MIT | **True.** `loro` 1.18.0 and its `loro-*` crates are MIT (`loro-thunderdome`: MIT OR Apache-2.0). JS: `loro-crdt` 1.18.0, MIT. One transitive dependency, `xxhash-rust`, is **BSL-1.0**, which was not on our allow list; it is permissive and now allowed (judgement 21). |
| Automerge is MIT | **True.** `automerge` 0.12.0 and `hexane` 1.0.0-alpha.5, MIT. JS: `@automerge/automerge` 3.5.0, MIT. |
| Loro 1.x has a stable encoding | **True as far as we can test.** A snapshot written by `loro` 1.1.0 (rich text with marks, a movable tree, a movable list) loads correctly in 1.18.0. Note: there is no stable `loro` **1.0.0** on crates.io: the Rust crate went from `1.0.0-beta.5` to `1.1.0`; "1.0" was the JS release (October 2024). |
| Loro has a movable tree and a movable list | **True.** `LoroTree` (moves with cycle prevention, fractional-index ordering, `mov_after`/`mov_before`) and `LoroMovableList` (`mov`, `set`). Both used here. |
| Loro imports "10–100× faster" | **Partly checkable.** The claim is Loro's own, comparing 1.0's snapshot format with 0.16; we can't build 0.16 here. What we can measure: importing a 1.x snapshot vs replaying the same history as updates is **2.8–4.6× faster for the tree encoding and 9.5–60× for the hybrid** (table below), and Loro loads a 100-page document in **1.8 ms** vs Automerge's **133 ms**. |
| Automerge 2.2+ has block markers and marks | **True, but the version numbers are the JS package's.** Rich text with block markers and marks shipped in JS 2.2 (April 2024); JS is now 3.5.0 and the Rust crate is 0.12.0. The Rust API has `split_block`, `join_block`, `update_blocks`, `spans`, `update_spans`, `mark`/`unmark` with `ExpandMark`. An Automerge 0.5.12 file with a block marker loads in 0.12.0. |
| (not in the report) | Automerge 0.12 adds first-class **authors** (`set_author`, `get_author_for_actor`), which would replace our own peer → name map. |

## Results

### Concurrent table edits (10,000 seeded runs per encoding)

2–3 peers, two rounds of 1–6 operations each with a partial sync between
them, then a full sync. Operations: insert row, delete row, insert column
(2.5 cm), delete column, type into a cell. Base table: the generated
`tables-widths-merged.odt` (repeated columns, a merged cell). After every
run: all replicas converge; the table is rectangular; every token typed into
a surviving cell is in that cell exactly once and nowhere else; the cell's
original text is intact. Every 10th merged state is written as ODT, read
back, checked to be rectangular, and checked for damage outside the table.

| | loro | loro-flow | automerge |
|---|---|---|---|
| Runs | 10,000 | 10,000 | 10,000 |
| Operations | 174,426 | 174,426 | 174,426 |
| Merges | 39,924 | 39,924 | 39,924 |
| Edits into a cell that a concurrent row + column insert left without a cell object | 5,264 | 5,264 | 5,260 |
| Runs where concurrent deletes removed every row or column | 192 | 192 | 192 |
| **Failures** | **0** | **0** | **0** |
| Wall time (release) | 230 s for both, run in parallel | 80 s | (with loro) |

Notes:

- **Rectangular by construction.** Cells are addressed by (row ID, column
  ID), never by position (judgement 2). Concurrent row insert + column
  delete can't produce a ragged row.
- **The corner cell.** A row and a column inserted concurrently meet in a
  cell nobody created. Reading shows it empty; typing into it creates it.
  If two peers do that concurrently, both create it. Both libraries keep
  both copies (Loro as two tree nodes, Automerge as a map conflict) and the
  reader merges them, so no text is lost. Loro also offers deterministic
  "mergeable" containers that would avoid the duplicate entirely.
- **Emptied tables.** In 1.9% of runs concurrent deletes left no rows or
  no columns: each peer deleted "the second-to-last" row. That is correct
  CRDT behaviour and a UI problem (ODF can't express an empty table); the
  editor should treat "delete the last row" as "delete the table".

### Section moves concurrent with edits inside (100 seeded runs)

The 100-page letter cut to 10 sections. Peer A moves a random section after
another; peer B, concurrently, presses Enter in one of its paragraphs and
types 1–5 tokens into its paragraphs. Then both sync.

| | loro | loro-flow | automerge |
|---|---|---|---|
| Tokens typed | 359 | 359 | 359 |
| …ended up inside the moved section | **359** | **359** | 336 |
| …ended up elsewhere (left behind at the old place) | 0 | 0 | **23** |
| …lost or duplicated | 0 | 0 | 0 |
| Section is where A moved it | 100/100 | 100/100 | **45/100** |
| Section split in two pieces | 0 | 0 | **90/100** |
| B's Enter survives | 100/100 | 100/100 | 100/100 |

Automerge has no move. Moving a section is copy + delete: concurrent typing
in the old copy stays at the old place, and a concurrent Enter leaves a
block marker with the section in its `parents` at the old place, so the
section appears twice. Nothing is lost, but the result needs fixing by
hand. In Loro both encodings move the section with one tree move, and
everything inside travels with it. This is the clearest difference the
spike found, and it is a library difference (Automerge has no move
operation), not an encoding one.

### Branch and merge on a 100-page document

The generated letter repeated 100 times in sections (100 pages in
LibreOffice 26.8.1.1). A reviewer forks and makes 200 edits on the branch
(typing, deleting, formatting, Enter, paragraph properties, table cells);
meanwhile 200 edits land on main. The suggestion is shown as the
paragraphs that differ from the fork point. Accepting merges the branch.

| | loro | loro-flow | automerge |
|---|---|---|---|
| Paragraphs the suggestion changes | 162 | 162 | 162 |
| Merge (accept), native release | 15 ms | 12 ms | 40–46 ms |
| Every token from both sides present exactly once | yes | yes | yes |
| Every suggested character still attributed to the reviewer after accepting, every main character to the case worker | **40/40** | **40/40** | **40/40** |
| Unedited elements in the written ODT | 100% byte-identical | 100% | 100% |
| Nodes regenerated / reused verbatim | 343 / 1,841 | 343 / 1,841 | 343 / 1,841 |

"History survives accepting changes" holds for both libraries: attribution
is a property of the operations, and a merge keeps them. Loro gives the
inserting peer per character (`get_editor_at_unicode_pos`); Automerge gives
it through the op ID of each character.

### One year of history

250 sessions × 400 edits by five authors in turn; every session loads the
saved bytes as a new peer, commits every 20 edits and saves (judgement 7).
Two documents and two edit mixes:

- **letter, growing:** the 2-page letter; deletions are 1–12 characters.
  It ends at 341 pages.
- **letter, revising:** the same, deleting 1–60 characters at a time. It
  still grows (deletions stop at paragraph ends), to 289 pages.
- **100 pages, revising:** the 100-page document, revising mix. It ends at
  353 pages.

"ODT" is the document at the end of the year, written by `crates/odf`
(page counts from LibreOffice). The zipped ODT is the strict yardstick;
our generated text zips 5–6× (real prose zips about 4×), so the ratio
against the uncompressed XML (`content.xml` + `styles.xml`) is also given.

| | loro | loro-flow | automerge |
|---|---|---|---|
| **letter, growing** (ODT 304–307 KiB, XML 1.6 MiB) | | | |
| Stored, history included | 5,295 KiB | 3,240 KiB | 2,259 KiB |
| ÷ zipped ODT (÷ XML) | 17.3× (3.3×) | 10.7× (2.0×) | **7.4×** (1.4×) |
| Deflated at rest, ÷ zipped ODT | 12.1× | **8.4×** | 7.4× |
| Without history (shallow snapshot) | 2,186 KiB | 1,487 KiB | n/a |
| **letter, revising** (ODT 189 KiB, XML 1.0 MiB) | | | |
| Stored, history included | 4,754 KiB | 3,125 KiB | 2,270 KiB |
| ÷ zipped ODT (÷ XML) | 25.2× (4.6×) | 16.6× (2.9×) | 12.0× (2.1×) |
| Deflated at rest, ÷ zipped ODT | 17.5× | 12.6× | 12.0× |
| Without history | 1,626 KiB | 967 KiB | n/a |
| **100 pages, revising** (ODT 250 KiB, XML 1.3 MiB) | | | |
| Stored, history included | 5,670 KiB | 4,576 KiB | 2,522 KiB |
| ÷ zipped ODT (÷ XML) | 22.7× (4.2×) | 18.3× (3.3×) | **10.1×** (1.8×) |
| Deflated at rest, ÷ zipped ODT | 15.5× | 12.8× | 10.1× |
| Without history | 2,344 KiB | 1,911 KiB | n/a |

Automerge's files are already deflated; Loro's are not, and deflate takes
20–30% off them. Without history, the hybrid is 4.9–7.6× the zipped ODT
(the tree 7.1–9.4×): much of Loro's size is the state representation
(surprise 1), not the history.

### Load, memory, bundle

| | loro | loro-flow | automerge |
|---|---|---|---|
| Create the 100-page document | 29 ms | 34 ms | 562 ms |
| Save it (fresh, no history yet) | 703 KB | 666 KB | **74 KB** |
| Load it | **1.9 ms** | **1.8 ms** | 133 ms |
| Read it whole after load | 12 ms | 15 ms | 50 ms |
| Load the year-old letter, growing / revising | 12.9 / 9.6 ms | **6.4 / 5.0 ms** | 1,275 / 1,524 ms |
| Load the year-old 100-page document | 12.3 ms | 11.1 ms | 1,567 ms |
| Resident memory after loading the letter, growing / revising | 86 / 68 MiB | 89 / 80 MiB | 223 / 293 MiB |
| …the 100-page document | 78 MiB | 95 MiB | 345 MiB |
| Replay a year (250 load/edit/save sessions): letter growing, revising, 100 pages | 39, 32, 49 s | 261, 179, 59 s | 562, 549, 657 s |
| wasm32 bundle, raw / gzip -9 | 2.22 MB / 784 KB | (same library) | **1.26 MB / 482 KB** |

The wasm probe exercises create, edit, mark, split, merge, save, load and
read through the trait, built with `opt-level = "z"`, LTO, one codegen
unit, `panic = "abort"`, no `wasm-opt` (judgement 25). We did not measure
speed in wasm; it is usually slower than native, so Automerge's load times
above are a lower bound for the browser.

The hybrid's slow replay of the letter (261 s against the tree's 39 s) is
our code, not Loro's: the letter has no sections, so its whole body is one
flow, and finding a paragraph's extent scans that text from the marker;
re-indexing after structural edits scans every flow. With sections (the
100-page document) the hybrid replays in 59 s. Neither was optimised.
Automerge's replay time includes loading, reading and saving the whole
document in each of the 250 sessions; loading alone takes 1.3–1.5 s by the
end. We didn't profile further.

### Writing ODT after every scenario

Every test writes ODT from the merged CRDT state through `crates/odf` over
the source file and checks it with `skriv_fidelity::structural::compare_edited`:
every element outside an edited node must be byte-identical, and present
canonically. All pass: corpus round trip (9 files), table fuzz (every 10th
run, 3,000 checks), section moves (every 10th run), branch and merge, the
year traces (written, not checked element by element), and all 21 blind
review outcomes (written and read back).

The kept files (`target/spike3-odt/`) also pass the harness CLI in
structural mode (`--no-visual --no-lo-loop`): 9/9 readable, untouched
elements 100% byte-preserved, regeneration 100% canonical. LibreOffice
26.8.1.1 opens and renders all of them.

With no baseline at all (every node regenerated from the CRDT), every
body element of every generated-corpus document still comes back
canonically, so the logical document loses nothing the model knows.

## Surprises

1. **Loro's size is per-container overhead.** The 100-page text is 3 KB as
   one `LoroText` (it is repetitive), 159 KB as one text per paragraph, and
   703 KB in the tree encoding. The hybrid gets 666 KB, still 9× Automerge's
   74 KB: Loro stores state and history side by side and compresses
   little; Automerge's columnar format with deflate is far denser. For a
   fresh document app-level deflate doesn't close the gap (217 KB against
   74 KB). After a year of edits it mostly does: deflated, the hybrid is
   5–27% larger than Automerge (the history table).
2. **Found and fixed in the writer: deleted children came back.** A clean
   node whose child list changed (a paragraph deleted from an untouched
   section, a cell from an untouched row) was copied whole from the source
   file, resurrecting what was deleted. Spike 1 never deleted anything, so
   it never showed. Fixed with `NodeMeta::children_changed`; regression
   test in `crates/odf`.
3. **Found and fixed in the writer:** comments, change-info and image
   titles with no origin and no dirty flag lost their author/date/text.
4. **The harness compared by path.** Inserting or deleting a block shifts
   the path of everything after it, so the Spike 1 comparison reports a
   real edit as damage everywhere. `compare_edited` matches by content.
5. **Automerge: quadratic block-by-block writes.** Writing a 100-page
   document one marker and one mark at a time took 84 s in a debug build.
   `update_spans` (Automerge's bulk API) is the right call: 562 ms release.
   It is generic, so it compiles into *our* crate: unoptimised it is ~80×
   slower; `skriv-crdt` now builds at `opt-level = 1` in debug.
6. **Automerge: marks grow over a new block marker.** A paragraph inserted
   after bold text starts bold, because "expand after" applies to the
   marker. Editors may want that when you press Enter; when inserting a
   paragraph programmatically it is wrong and must be undone with
   `unmark`.
7. **Automerge: `list_range` on a text object returns mark boundaries as
   items**, with duplicate indices. Use `get` per index or `spans`.
8. **Loro: a `:` in a mark key means something.** `comment:alice` is
   "style `comment`, instance `alice`" for expand configuration. ODF names
   are full of colons, so keys are escaped. (The hybrid uses it on
   purpose: all `para:…` keys share the `para` configuration.)
9. **Loro: text style configuration is not stored in the document.** Every
   replica must configure expand behaviour the same way after `new`,
   `fork` and `import`, or two peers will disagree about whether typed text
   is bold.
10. **Empty spans can't be marks.** LibreOffice writes
    `<text:span text:style-name="X"/>` for the format of an empty
    paragraph; with no characters, a mark has nothing to sit on. They ride
    as inline objects.
11. **Deleting a paragraph while someone types in it** leaves the typed
    text glued to the end of the *previous* paragraph in Automerge and in
    the hybrid (both use markers); the tree encoding drops it with the
    deleted node, the only case in this spike where an encoding discards
    text someone typed. Neither behaviour is obviously right; it is
    scenario 6 of the blind review.
12. **Splitting a paragraph** is where tree and text differ: the tree moves
    the tail by delete + insert, so concurrent typing in the tail stays in
    the first half; with markers it follows the split. Scenario 5.

## ADR 0002's open questions

**How the model is encoded in the CRDT: text with Peritext marks plus
block markers, versus a movable tree.** Both, split by level. Paragraph
boundaries are block markers inside rich text: splitting and joining then
behave like text (scenario 5), and it is several times smaller than a node
per paragraph (surprise 1). Sections, lists, list items, tables and cells
are nodes of a movable tree, because moving them concurrently with edits
inside them only works with a real move (the section results). Automerge
can only do the first half.

**Whether properties stay as ODF attribute maps or become a typed property
set when the CRDT needs per-property merge.** Per-property, with ODF's own
vocabulary as the keys. In the CRDT, a paragraph's properties are a map
with one entry per attribute (`text:style-name`, `fo:margin-left`, …), and
direct formatting is decomposed out of automatic styles into one entry (or
one mark) per property, keyed `Δ<property-set>|<attribute>`, so concurrent
"bold" and "italic" both survive (scenarios 3 and 7). Automatic style names
never enter the CRDT; export re-derives them, reusing the source's name
when the property set matches, so an untouched paragraph still points at
`P1`. The model's ordered attribute maps stay as they are: attribute order
is a serialisation concern, kept as a hint (`@order`). Named styles are
stored one JSON value per style for now (judgement 11).

**How origins and slots live with the CRDT** (ADR 0006).

- **Slots go into the CRDT**, as atomic values: an unknown element, a
  footnote, a frame or a comment is carried as JSON of the model node, last
  writer wins. Every replica needs them to write the file.
- **Origins stay out.** They are byte offsets into one source file, so
  they are meaningless to a peer who never had it, and wrong after the
  file changes. Every value serialised into the CRDT has its origin
  stripped.
- **"Dirty" is re-derived at export, not tracked.** On import, each
  replica that has the source file keeps a baseline: per node ID, the node's
  logical form and its original model node with origins. On export, a node
  whose logical form equals its baseline gets its original back and is
  copied verbatim; anything else is regenerated. This measured 100%
  preservation of untouched elements in every test, including after
  merges with peers who never saw the file.
- **A replica without the source file** writes everything from the model
  (regeneration), which round-trips canonically (the "regenerate
  everything" test). So the source file is an optimisation for
  byte-identity, not a dependency. Storing the last imported ODT next to the
  CRDT, and re-baselining on every save to ODT, is the obvious policy;
  Spike 4 should confirm it against LibreOffice editing the same file.

## Kill criteria

| Criterion | Status |
|---|---|
| Blind reviewers find merge results acceptable | **Not yet run.** 21 scenarios ready, 3 outcomes each; in 13 all three encodings give the same outcome. |
| Storage stays under about 10× the document size | **Not met by either library with a full year of history**, measured against the zipped ODT. Mostly-adding trace: Automerge 7.4×, Loro hybrid 8.4× (deflated) — both pass. Revising traces: Automerge 12.0× and 10.1×, Loro hybrid 12.6× and 12.8× (deflated) — both fail. Against the uncompressed XML everything is under 5×. Loro without history: 4.9–7.6×. |

### Trimming Loro's history

Loro can drop history before a point (a shallow snapshot); Automerge
can't. It helps less than expected, because a shallow snapshot stores the
state at the cut as well as the current state:

| Loro hybrid, year-old file | Full history | Keep 3 months | Keep 1 month | No history |
|---|---|---|---|---|
| letter, growing (deflated) | 3,240 KiB (2,543) | 3,028 (2,520) | 3,020 (2,549) | 1,487 |
| letter, revising (deflated) | 3,125 (2,383) | 2,273 (1,863) | 2,054 (1,732) | 967 |
| 100 pages, revising (deflated) | 4,576 (3,193) | 4,022 (2,935) | 3,756 (2,771) | 1,911 |

So the realistic policy is: keep the full history in cold storage (it is
what attribution and "who changed this" need), and serve a shallow
snapshot without history, plus recent updates, to editors.

## Recommendation and what's next

1. **Adopt Loro with the hybrid encoding** (ADR 0008, proposed). Reasons,
   in order:
   - moving structure while others edit inside it works (the section
     results); Automerge cannot do it, and its failure needs manual repair;
   - loading is two orders of magnitude faster (5–11 ms against
     1.3–1.6 s for a year-old document) and memory 2.5–3.6× lower, which
     matters most in a browser;
   - storage is close once compressed: deflated, the hybrid is 5–12%
     larger than Automerge after a year (27% on the 100-page document); and Loro
     can serve documents without history (4.9–7.6× the zipped ODT), which
     Automerge can't;
   - both are MIT; both kept tables rectangular and attribution intact.
   Against it: Loro's wasm bundle is 300 KB larger gzipped, and its raw
   files are larger until compressed.
2. **Decide the storage yardstick and policy.** Neither library meets
   "~10× the ODT" with a full year of history on our generated documents,
   which zip unusually well. Either measure against real municipal
   documents (which zip about 4×, not 5–6×), or adopt the policy above
   (full history cold, shallow snapshot hot), or both.
3. **Run the blind review** (`03-blind-review/README.md`). Scenarios 5, 6,
   14, 15 and 16 are where the encodings differ most.
4. **Don't build on the tree-per-paragraph encoding.** It is 1.2–1.6×
   larger than the hybrid, and its paragraph split strands concurrent
   typing in the first half.
5. **Before building on the hybrid:** make paragraph lookup incremental
   (it scans flows today), shorten the per-paragraph marks (IDs, and
   properties that equal the style's), and let cells hold lists and
   nested tables (cells are tree nodes, so the hybrid can nest; this spike
   carried them as embeds).
