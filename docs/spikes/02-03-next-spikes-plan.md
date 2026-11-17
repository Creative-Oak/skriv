# Proposed plan: Spikes 2, 2b and 3

Status: proposal, 2026-11-17. This needs a decision before work starts.

These three spikes settle the remaining Phase 0 decisions:

- **Spike 2:** the view strategy (ADR 0004).
- **Spike 2b:** the layout kernel.
- **Spike 3:** the CRDT (ADR 0002).

Spikes 2 and 3 are independent and can run in parallel. 2b shares one
interface with 2, the *layout contract*, which is fixed in the first week of
Spike 2 so that neither waits for the other.

```
week   1    2    3    4    5    6
S2     [contract][ A and B prototypes ][ device and AT test rounds ]
S2b         [ own engine prototype  ][ Typst-kernel prototype ][compare]
S3     [ mapping ][ concurrency fuzz ][ history size ][ blind review ]
S7                                    (starts when S2 has a prototype)
```

## What Spike 1 gives all three

- **`crates/model`:** the document model to project into a view, a layout
  engine and a CRDT. Its accessors are the API.
- **The generated corpus:** a Danish municipal letter, styles, lists,
  sections, tables, figures, fields, comments and tracked changes. These are
  real LibreOffice files. Scaled to 100 pages, they are the test documents for
  all three spikes.
- **The fidelity harness:** a spike prototype that edits a document can be
  checked by writing ODT and running the harness. Edits must not damage what
  they didn't touch.

## The layout contract, shared by Spikes 2 and 2b

The view must not depend on which layout kernel wins. Week 1 of Spike 2 fixes this interface:

```
layout(model snapshot, edit hint) -> Pages
Pages  = [Page { size, frames: [Line { y, height, baseline, runs: [Run { x, width, source: NodeId + text range, glyphs?, hyphen? }] }] }]
locate(Pages, NodeId, offset) -> (page, line, x)     // caret placement
hit(Pages, page, x, y) -> (NodeId, offset)            // click, touch, selection
```

Spike 2 starts with a throwaway greedy line breaker in Rust/WASM that
implements this contract. Spike 2b swaps in its two candidates.

---

## Spike 2: Paginated editing, IME and screen readers

**Question.** Which view strategy gives us page-view editing with native-grade
input and accessibility?

- **A (ProseMirror view).** Each paragraph is a NodeView rendering the
  engine's lines. Page boundaries are widget decorations. ProseMirror's state
  is a projection of the model, never the source of truth, and transactions
  map to model operations.
- **B (owned view).** The engine positions lines in the DOM. Input comes
  through EditContext in Chromium and Electron, or a hidden textarea
  elsewhere. Semantic `<h1>`–`<h6>`/`<p>` elements stay in reading order, so
  the accessibility tree is the DOM itself, not a side DOM.

Verify before building:

- ProseMirror's licence (MIT expected).
- EditContext status in Chrome, Edge, Firefox and Safari as of the spike
  date. The report's claim is "Chromium only".

**Test document.** The generated Danish municipal letter, scaled to 100 pages.
It has headings, numbered lists, tables, figures with captions, footnotes and
comments.

**Keyboard and input methods.** Scripted sequences, run on every platform that applies:

| Input | What to type | What to check |
|---|---|---|
| Danish keyboard (macOS, Windows, Linux) | `Æbleøen på Ålborg`; dead keys `´ e` → `é`, `¨ u` → `ü`, `^ o` → `ô`, `` ` a `` → `à`; AltGr combinations (`@ £ $ € { } [ ] \|`) | No lost or doubled characters; the caret is right after each dead-key sequence |
| macOS IME | Press-and-hold accent menu (`e` → `é`); Japanese Kana (`にほん` → 日本); Pinyin | The composition underline shows; committing replaces the whole composition; Escape cancels it |
| Windows IME | Microsoft Japanese IME; Microsoft Pinyin; the Windows emoji panel (Win+.) | Same as macOS |
| Android Gboard (Danish layout) | Typing with autocorrect on; swipe typing; tapping a suggestion; long-press for `æ ø å`; backspace across a word Gboard has "composed" | No duplicated or vanished words (the classic contenteditable failure); the caret stays put |
| iOS keyboard (Danish) | As Gboard, plus QuickType suggestions | Same |
| Dictation | macOS Dictation, Windows Voice Typing (Win+H), Gboard voice | Dictated text arrives once, in the right place |
| Across page breaks | Shift+arrows and mouse-drag selection from page 3 to page 4; delete that selection; type over it; undo | The selection is continuous; nothing on page 5+ changes |

**Screen readers.**

| Reader | Browser | Checks |
|---|---|---|
| NVDA (current) | Firefox, Chrome | Heading navigation (H, 1–6); reading paragraphs continuously across page boundaries without page chrome being read out; caret echo while typing; selection announced; figure alt text read |
| JAWS (current) | Chrome, Edge | Same |
| VoiceOver | Safari on macOS, Safari on iOS | Rotor headings; reading continuously; typing echo; selection |
| Narrator (bonus) | Edge | Smoke test |

**Measurement.**
- **Latency** is keystroke-to-paint: the `beforeinput` timestamp to the next
  frame that contains the change. It is measured with the Event Timing API
  plus a requestAnimationFrame probe, and cross-checked with a Chrome
  performance trace.
- **Hardware:**
  - a mid-range Windows laptop (Intel Core i5 class, 8 GB, integrated GPU);
  - a MacBook Air M1;
  - a mid-range Android phone (Pixel 6a or Samsung Galaxy A5x class).
- **Load:** 1,000 keystrokes per run at three caret positions: page 1, page
  50, and mid-table on page 80.

**Kill / continue (from the report).**
- p95 keystroke-to-paint under 16 ms on the mid-range laptop.
- No IME composition is lost in any scripted sequence.
- Headings and paragraphs are navigable with NVDA, JAWS and VoiceOver.

Report every failure per platform. A strategy that fails only on one platform
may still win with a documented fallback.

**Deliverable.** `docs/spikes/02-results.md` recommends A or B per platform
(web, desktop) and updates ADR 0004. The prototype code is kept in
`prototypes/` behind the layout contract.

---

## Spike 2b: Our own layout engine vs Typst as the kernel

**Question.** Can we reach beautiful, deterministic, incremental pagination
faster by building on the Linebender stack, or by using Typst's layout as
the kernel?

**Verify first.** None of the report's claims below has been checked yet:

- Parley's current version, and its `parley_engine` hook for custom line breaking.
- The licences of HarfRust, Skrifa, Fontique and ICU4X.
- That `hypher` includes Danish patterns, and what licence the Danish
  `hyph-utf8` patterns carry (it varies by language).
- Typst's current version, its library API for incremental compilation
  (comemo), and its cursor/position APIs (`jump_from_cursor` /
  `jump_from_click` or their successors).
- Font licences (OFL) for the bundled families. We need metric-compatible
  substitutes for Calibri, Cambria, Times and Arial, plus a default family.

**Prototype (a): our own engine.**
- Parley for shaping, bidi and font fallback.
- A Knuth–Plass breaker for justified text and a greedy one for
  ragged-right; the report also suggests a "LibreOffice-compatible" mode.
- Danish hyphenation from TeX patterns.
- Widow and orphan control, and keep-with-next.
- Incremental pagination: memoised per paragraph, re-paginating forward from
  the edit until page boundaries stabilise.

**Prototype (b): Typst as the kernel.**
- Map the model into Typst content, then let Typst lay out pages incrementally.
- Map caret and click positions back through Typst's introspection.

**Measure.**
- Per-keystroke relayout time on the 100-page document, p50/p95/max, both in
  WASM in the browser and natively. The target is under ~30 ms (report).
  Spike 2's 16 ms paint budget is met with the report's two-phase commit:
  paint with the previous line metrics, then reconcile.
- Caret mapping: round-trip `locate(hit(x, y))` stays stable for every
  character on 10 sample pages, including ligatures, hyphenated breaks and
  bidi runs.
- Determinism: a hash of the layout result is identical on macOS, Windows,
  Linux and WASM.
- Danish hyphenation: correctness on a word list, compared with LibreOffice's
  choices. They are expected to differ, and we report how much.
- Typography: a side-by-side of justified paragraphs against LibreOffice's
  output, for rivers and line badness.

**Kill / continue (from the report).** Pick whichever reaches the latency
target with editable caret mapping. If both do, prefer (a) for control over
ODF semantics (tab stops, exact line spacing, keep-with-next variants)
unless (b) is clearly cheaper.

**Deliverable.** `docs/spikes/02b-results.md`; ADR 0007 on the layout kernel;
licence entries for every crate and font adopted.

---

## Spike 3: Loro vs Automerge for tables, sections and tracked changes

**Question.** Which CRDT carries this model, especially the Word-grade
structure no CRDT ships a proven model for?

**Verify first.**
- The licences of Loro and Automerge (MIT expected).
- Loro's stable encoding, movable tree and movable list, and the "10–100×
  faster import" claim.
- Automerge 2.2+ block markers and marks.

**Mapping.** Build model ↔ CRDT for both libraries, behind one trait
(`crates/crdt`) so the tests are shared:

- **Flow:** rich text with Peritext-style marks for character styles and
  direct formatting. Block boundaries carry a paragraph style plus a
  direct-formatting delta.
- **Tables:** a movable tree in Loro or nested lists in Automerge. Each cell
  holds its own text, and column widths are stored explicitly.
- **Sections:** anchored regions with a properties map.
- **Tracked changes:** the Patchwork model. A suggestion is an edit on a
  personal branch, shown as a diff; accepting it merges and keeps attribution.
- **Origins and slots** (ADR 0006): how a node remembers it came from bytes in
  an ODT. Can that survive in the CRDT, or is it dropped at the first edit and
  re-derived on export?

**Tests.**
- Concurrent table edits: 2–3 peers insert rows, delete columns and edit
  cells concurrently. A deterministic fuzzer runs 10,000 seeded runs per
  library; the invariant is a rectangular table with no lost cell text.
- Section moves concurrent with edits inside the moved section.
- Branch and merge on a 100-page document: a reviewer's branch with 200 edits,
  merged after 200 concurrent edits on main.
- Accepting suggestions: each library's attribution survives
  ("history survives accepting changes").
- One year of history: a synthetic trace of 250 sessions × 400 operations.
  Measure stored size against the document's ODT size, load time, memory, and
  WASM bundle size.
- Writing ODT from the CRDT state after each test, then running the fidelity
  harness. The CRDT must not corrupt what nobody edited.

**Blind review.** Produce about 20 merge scenarios from both libraries. Three
to five people who don't know which library produced which rate each outcome
as acceptable or not.

**Kill / continue (from the report).**
- Blind reviewers find merge results acceptable.
- Storage stays under about 10× the document size.

**Deliverable.** `docs/spikes/03-results.md` and ADR 0008 (the CRDT choice).
ADR 0002 is updated on property encoding and origins.

---

## Decisions needed before starting

1. **Hardware and accessibility-tool access** for Spike 2:
   - JAWS licence;
   - a mid-range Windows laptop and an Android phone;
   - at least one regular screen-reader user for a usability pass, ideally
     from a Danish public-sector partner.
2. **Whether Spike 7 (Tauri on Linux) starts as soon as Spike 2 has a
   prototype,** or waits for its decision.
3. **Who sits on the Spike 3 blind review.**
