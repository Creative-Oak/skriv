# Spike 3: judgement calls

Decisions made during Spike 3 that the brief did not settle, so they can be
reviewed. Each says what was chosen, why, and what it costs. Results are in
[`03-results.md`](03-results.md).

## Scope and method

1. **Each library in its own idiom, not the same encoding twice.** Loro
   stores a *movable tree* of blocks (one node per paragraph, section, list,
   table, cell); Automerge stores *rich text with block markers* (its 2.2+
   model) with tables as separate objects. This is what ADR 0002's open
   question asks ("Peritext marks plus block markers, versus a movable
   tree"), and it is how each library is meant to be used. The cost: library
   and encoding are confounded. Loro also has rich text and could do block
   markers; Automerge has no move operation and cannot do a movable tree
   without building one by hand (last-writer-wins parent pointers plus
   fractional indices). Where a finding is about the encoding rather than
   the library, the results say so.

   **Added mid-spike: a third, hybrid encoding on Loro** (`loro-flow`): the
   movable tree for sections, lists, tables and cells, and one rich text
   with block markers per run of paragraphs. The tree encoding's size and
   its paragraph-split behaviour both turned out to be encoding effects,
   and the hybrid is the design I would actually build; measuring it was
   the only way to separate library from encoding.

2. **Tables are a grid keyed by row ID and column ID in both libraries**,
   not positional nested lists (the brief suggested nested lists for
   Automerge). A positional row of cells cannot stay rectangular when one
   peer inserts a row (with N cells) while another deletes a column (making
   the other rows N−1). With cells keyed by `(row, column)`, rectangularity
   holds by construction and the fuzzer tests the rest.

3. **"No lost cell text" is defined as:** every piece of text typed into a
   cell whose row and column both survive the merge is in that cell exactly
   once, and nowhere else; and the cell's original text is still there.
   Text in a deleted row or column is gone by intent. The fuzzer types
   unique tokens (`⟨s12p1n7⟩`) and strips them innermost-first, because
   later typing can land inside an earlier token.

4. **Fuzzer operation mix:** 20% row insert, 10% row delete, 10% column
   insert, 10% column delete, 50% typing into a cell; 1–6 operations per
   peer per round, two rounds, a partial sync between rounds, then a full
   sync. 2 or 3 peers, chosen by the seed. Cell deletions of text were left
   out so that every token's expected location is known.

5. **The 100-page document** is the generated Danish municipal letter
   repeated 100 times (100 pages in LibreOffice 26.8), each copy in its own
   named section ("Afsnit 1" …), with fresh IDs, unique table names and
   footnote IDs. It is generated at test time (`workload::scale`), not
   committed. **Correction (found in Spike 2b):** `scale` also shuffles the
   words of each text run per copy so the copies wouldn't compress to
   nothing, but the shuffle has no effect on the written ODT. Text runs
   keep their source origin, so the writer copies the original spelling,
   and all 100 copies are identical. Every Spike 3 measurement was taken
   on the written (identical-copy) file, so the numbers stand; only this
   description was wrong. The fresh 100-page document therefore
   compresses unusually well (372 KB of XML in a 33 KB zip), which flatters
   compression-friendly formats in the "fresh document" sizes. The year
   traces add varied text, and their end-of-year ratios are unaffected.

6. **"10× the document size" is measured against the zipped ODT**, the
   strict reading, and also reported against the uncompressed XML
   (`content.xml` + `styles.xml`). Real prose zips about 4×; the fresh
   100-page letter (identical copies, see the correction above) zips 11×,
   and the year-end documents 5–6×, which makes the zipped ratio
   pessimistic.

7. **The one-year trace** is 250 sessions × 400 user-level edits by five
   authors in turn. Every session loads the saved bytes as a *new peer*
   (as a fresh browser tab would), commits every 20 edits, and saves. An
   edit is: type 1–4 words (55%), delete 1–12 characters (25%), format a
   range (12%), press Enter (4%), set a paragraph property (4%); 4% type
   into a table cell. That mix makes a 2-page letter grow to 341 pages, so
   there is also a **revising** variant (`steady` in the code) that deletes
   1–60 characters at a time. It was meant to keep the size steady; it
   doesn't (deletions stop at paragraph ends), and the letter still grows
   to 289 pages. The results report the real end sizes.

8. **Suggestions are accepted whole**, as Patchwork does: a reviewer's
   branch is a fork with its own peer; the suggestion shown is the diff of
   the branch against its fork point; accepting is a merge. Per-change
   accept/reject inside one branch was not built. Authors are recorded in
   the document (a peer → name map) rather than with Automerge 0.12's new
   built-in `Author` support, so both libraries are tested the same way.

## Mapping the model

9. **Direct formatting is decomposed into one mark per ODF property**
   (`Δstyle:text-properties|fo:font-weight = bold`), plus `cs` for the named
   character style. Automatic style names (`T1`, `P3`) don't enter the CRDT;
   export re-derives them, reusing the source's name when the property set
   matches and minting `P…`/`T…`/`Co…` otherwise. This is what makes
   concurrent bold + italic merge, and it answers ADR 0002's property
   question (see the results).

10. **Paragraph properties are one map entry per attribute**, with the
    source attribute order kept as a hint (`@order`). Last writer wins per
    attribute.

11. **Named and automatic styles themselves are stored as one JSON value per
    style**, atomic, last writer wins per style. Concurrent edits to the
    same named style are rare and per-property merge there was not needed
    to answer the spike's questions. The same map-per-property encoding
    would apply if it is.

12. **Inline objects** (fields, footnotes, frames, comments, bookmarks,
    change marks, controls) are one U+FFFC character with an `obj` mark
    holding the model inline as JSON: atomic, carried, not editable inside
    through the CRDT. So are **empty spans** (LibreOffice writes them for the
    format of an empty paragraph): a mark needs characters to sit on.

13. **Blocks the CRDT doesn't edit** (tracked-change regions in ODF's own
    form, unknown elements, tables we can't map — repeated cells, unknown
    groups) are embeds: atomic JSON. Inside table cells, anything other than
    a paragraph is an embed too, so cells hold flat text only.

14. **Origins stay out of the CRDT** (they are byte offsets into one source
    file). Every value serialised into the CRDT has its origin stripped.
    The ODT writer gets origins back from a per-replica baseline keyed by
    node ID, and a node is "clean" exactly when its logical form equals its
    form at import.

15. **Loro mark keys are escaped** (`:` → `~`), because Loro reads `a:b` as
    "style `a`, instance `b`" when looking up how a mark expands. ODF names
    are full of colons.

16. **Splitting a paragraph in Loro** creates a sibling node and moves the
    tail by delete + insert. Automerge's split only inserts a marker. This
    is the honest consequence of choice 1, not tuned away.

17. **Automerge writes a whole flow with `update_spans`**, its bulk API,
    instead of block-by-block inserts, which were quadratic (see results).
    Block-by-block writes also remove marks a new block inherits from the
    text before it (`clear_inherited`): Automerge's "expand after" marks grow
    over a freshly inserted block marker.

## Changes outside `crates/crdt`

18. **Model: `NodeMeta::children_changed`.** The writer copied a clean node
    whole when only its child list had changed, so deleted children came
    back. A model change was the smallest correct fix; `propagate_dirty`
    honours it.

19. **Writer:** comments, change-info and `svg:title`/`svg:desc` with no
    origin are now written from the model even when not flagged dirty.

20. **Harness: `structural::compare_edited`**, a path-independent check for
    real edits (insertions and deletions shift paths without damaging
    anything). Elements inside or around an edited node are exempt; every
    other element must be byte-identical and canonically present.

21. **`deny.toml` allows BSL-1.0** (the Boost licence) for Loro's
    `xxhash-rust`. It is permissive with no copyleft, so it fits ADR 0005.

22. **Build profile:** dependencies are compiled with `opt-level = 2` and
    `skriv-crdt` with `opt-level = 1` even in debug builds. Automerge's
    generic `update_spans` is monomorphised into our crate and ran ~80×
    slower unoptimised.

## Blind review

23. **21 scenarios**, each run on all three encodings from the same short
    municipal decision letter. Outcomes A/B/C are assigned per scenario by a
    seeded shuffle; the key is ROT13-encoded in a separate file so it isn't
    read by accident. Scenarios where all three produce the same result are
    kept (13 of 21), because reviewers rate each outcome on its own, and the
    identical ones calibrate the reviewers.

24. **Lines that differ from the starting document are marked ▶** and
    removed lines are listed. This helps reviewers find the change and
    applies equally to both outcomes. I did not rate the outcomes.

## Measurement

25. **wasm32 size** is a `cdylib` probe that creates, edits, marks, splits,
    merges, saves, loads and reads a document through the trait, built with
    `opt-level = "z"`, LTO, one codegen unit, `panic = "abort"`, stripped;
    no `wasm-opt`, no `wasm-bindgen` glue. Real bundles add glue and lose
    some size to `wasm-opt`.

26. **Memory** is the resident set of a child process after loading the
    saved document and reading it once, minus nothing (the process
    baseline is reported alongside).
