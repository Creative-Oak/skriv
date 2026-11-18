# Spike 2 results: paginated editing, input methods and screen readers

**Status: in progress.** The prototypes and the automated measurements are
done. The tests that need a person, real devices and screen readers are
next: [`02-manual-tests.md`](02-manual-tests.md). This document gets its
verdict, and ADR 0004 its update, when that sheet is filled in.

- **Date:** 2026-11-18.
- **Brief:** [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md),
  "Spike 2"; risk in [`02-paginated-editing.md`](02-paginated-editing.md).
- **Code:** [`prototypes/view/`](../../prototypes/view/README.md).
- **Raw output:** [`02-results-data/`](02-results-data/).
- **Machine so far:** Apple M4, 16 GB, macOS 27.2.1, Playwright's
  Chromium 141 and WebKit 26 (headless). This is not the brief's reference
  hardware (a mid-range Windows laptop, a MacBook Air M1, a mid-range
  Android phone); those runs are part of the manual sheet.

## Verified before building

- **ProseMirror is MIT** (prosemirror-view 1.42.9, -state 1.4.4, -model
  1.25.13, -transform 1.12.6, -commands 1.7.2, -history 1.5.1, -keymap
  1.2.3; npm metadata). Vite 8.3.8: MIT. Dev and test only: Playwright
  (Apache-2.0), TypeScript (Apache-2.0).
- **EditContext is still Chromium only.** Chrome and Edge since 121;
  Firefox has a positive standards position but no implementation (bug
  1904161); Safari has none ([web-features explorer](https://web-platform-dx.github.io/web-features-explorer/features/edit-context/),
  [caniuse](https://caniuse.com/mdn-api_editcontext)). The report's claim
  holds, so B on the web means a hidden textarea for Firefox and Safari.

## What was built

Both views share one editing core: Spike 2b's own engine (greedy
breaking, as LibreOffice) in wasm, behind the layout contract, holding the
100-page Danish test letter. It applies edits (typing, deleting across
paragraphs, Enter, Backspace joining paragraphs), re-lays out
incrementally, and reports which pages changed. Both views render the same
way: semantic `<h1>`–`<h6>`, `<p>` and `<table>` elements in reading
order, each run of text absolutely positioned where the engine put it, and
the page chrome in a separate `aria-hidden` layer. So a screen reader
should read paragraphs straight across page breaks. Each paragraph is its
own CSS containment boundary.

- **A (ProseMirror):** ProseMirror owns a contenteditable. Each paragraph
  is a node view placed at its first line; each run is an inline
  decoration placed by the engine, so the browser never breaks a line
  itself. ProseMirror's transactions are mirrored into the engine (changed
  paragraphs; Enter and joins as engine edits); up/down arrows use the
  engine's lines. Caret, selection, IME and undo are native or
  ProseMirror's.
- **B (owned view):** no contenteditable. Input from EditContext (Chrome,
  Edge) holding the caret's paragraph, or a hidden textarea holding it
  (other browsers), placed at the caret for IME windows. Caret, selection,
  arrows, clicks and undo are our own. With EditContext the DOM selection
  mirrors ours, so assistive technology can follow the caret.

## Findings so far

**1. Both views meet the latency target in Chromium on this machine; B
with room to spare, A barely.** Real key events, 80–120 ms apart, 300 per
position:

| Chromium 141, Apple M4 | B (EditContext) | A (ProseMirror) |
|---|---|---|
| Keystroke to painted frame, p95: page 1 / page 50 / table on page 80 | **12.9 / 13.3 / 15.6 ms** | 23.2 / 24.9 / 25.3 ms |
| Work per keystroke (engine + DOM + frame), p95 | **12.1 / 12.7 / 13.2 ms** | 17.6 / 17.4 / 16.9 ms |
| Engine (layout) p95 | 0.9–1.2 ms | 0.7–0.8 ms |

"Keystroke to painted frame" is the brief's measure. At 60 Hz it includes
0–16.7 ms of waiting for the next frame, which no implementation can
avoid, so "work" is also reported: it is the part that can overrun a
frame. The engine is never the problem; the DOM and the browser's
rendering are.

**2. Where A's time goes, and what isn't ProseMirror's fault.** A first ran
at 25–27 ms p95. Profiling found two costs of *how* ProseMirror was used:

- A node decoration per paragraph (about 1,400 at the top level) made
  every view update quadratic: `DecorationSet.forChild` scans them for
  each child. Node views that place their own element fixed it.
- `DecorationSet.add` silently drops node decorations passed out of
  document order (it validates them with a forward-only cursor). It took
  some debugging to find; worth knowing for any decoration-heavy editor.

What remains is ProseMirror's view update walking ~1,500 top-level blocks
(`forChild`'s linear search, about 1 ms per keystroke) and the browser
moving the native selection in a large contenteditable (`collapse`, about
2 ms per keystroke). Neither goes away without splitting the document
into separately rendered pieces.

**3. B's first version cost 6–7 ms of DOM work per keystroke; four fixes
brought it to under 2 ms of script.** Each fix generalises:

- Make each paragraph a containment boundary (`contain: layout size style`);
  otherwise one paragraph's change re-lays out every absolutely positioned
  run in the document.
- Redraw only paragraphs whose lines changed, not every paragraph on a
  dirty page.
- Send EditContext only the changed range, not the whole paragraph.
- Compute the caret's screen position from cached geometry instead of
  reading layout (`getBoundingClientRect` and `scrollIntoView` forced a
  layout on every keystroke).

**4. Typing faster than one key per frame queues EditContext's text
updates.** With keys 4 ms apart (Playwright's default), `textupdate`
events piled up behind frames and latency climbed to 50–190 ms; at human
speed (80–120 ms apart) it disappeared. Real typists rarely exceed one key
per frame, but autorepeat and paste-like input from IMEs can. A real-device
check belongs on the manual sheet.

**5. WebKit (Playwright's build, not Safari) is slower in both views**:
B with the textarea 26–28 ms p95, A 36–64 ms. WebKit rounds timers to
1 ms and renders headless in software, so these numbers are indicative
only; real Safari on the M1 is on the manual sheet.

**6. Selection across a page break works in both views**: select from page
3 into page 4, delete, type, undo twice, and the document is restored
exactly (`scripts/check.mjs`).

**7. Layout is deterministic across macOS and Linux.** The 2b layout hashes
computed on this Mac, natively and in Chromium's wasm, equal those
recorded on Linux in Spike 2b, for both engines and both test letters.
This answers part of ADR 0007's open question; Windows remains.

## Judgement calls

- **"Work" alongside "keystroke to paint".** Explained in finding 1; both
  are recorded per sample in the JSON.
- **Headless browsers.** The in-app browser pane throttles frames when
  hidden, so automated runs use Playwright's own browsers. Headless
  Chromium uses the GPU here; headless WebKit doesn't.
- **A mirrors structural edits as engine edits**, not by reloading the
  whole document; reloading would have penalised A for a prototype
  shortcut, not a property of the approach.
- **B's undo** snapshots the document in idle time, once per burst of
  typing, so no keystroke pays for serialising it. A real editor would
  keep an operation log, probably the CRDT's (Spike 3).

## Still to do

1. The manual sheet: keyboards and input methods (Danish dead keys, macOS
   and Windows IMEs, Gboard, iOS, dictation), screen readers (VoiceOver,
   NVDA, JAWS, Narrator), and latency on the reference hardware.
2. Firefox: Playwright's Firefox would not start on this machine, so the
   Firefox runs are manual too.
3. Then: the verdict per browser, the ADR 0004 update, and ADR 0007's
   latency confirmation on real devices.
