# Spike 2 results: paginated editing, input methods and screen readers

**Status: provisional verdict.** The prototypes, the automated
measurements and a manual pass in Firefox and Safari on the Mac are done.
Phones, Windows, VoiceOver and latency on the reference hardware were
deferred for this prototype (decided 2026-11-18; see
[`02-manual-tests.md`](02-manual-tests.md)), so no screen reader has been
tested and no latency has been measured on the brief's hardware. The
verdict stands until those runs say otherwise.

- **Date:** 2026-11-18.
- **Brief:** [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md),
  "Spike 2"; risk in [`02-paginated-editing.md`](02-paginated-editing.md).
- **Code:** [`prototypes/view/`](../../prototypes/view/README.md).
- **Raw output:** [`02-results-data/`](02-results-data/).
- **Machine so far:** Apple M4, 16 GB, macOS 27.2.1, Playwright's
  Chromium 141 and WebKit 26 (headless). This is not the brief's reference
  hardware (a mid-range Windows laptop, a MacBook Air M1, a mid-range
  Android phone); those runs are part of the manual sheet.

## Verdict

**Build the web app on B, the owned view: EditContext in Chrome and Edge,
a hidden textarea in Firefox and Safari.** This refutes the brief's
expectation that A would be the likelier web choice, on the evidence we
have. It is provisional: the screen-reader criterion is untested, and A
stays the fallback if B fails it.

| Brief's criterion | B (owned view) | A (ProseMirror) |
|---|---|---|
| p95 keystroke to paint under 16 ms (mid-range laptop) | **Passes on an M4** in Chromium: 12.9–15.6 ms. WebKit headless 26–28 ms. Reference hardware not measured | **Fails on an M4** in Chromium: 23.2–25.3 ms. WebKit headless 36–64 ms |
| No IME composition lost | No losses in the manual pass (Firefox, Safari: the textarea path) or the automated runs (Chromium: EditContext). Phones, Windows IMEs and EditContext by hand not tested | No losses in the manual pass (Firefox, Safari) or the automated runs |
| Headings and paragraphs navigable by screen reader | **Untested** | **Untested** |

Per browser:

- **Chrome and Edge: B with EditContext.** The fastest path measured, and
  the one the input layer was designed around.
- **Firefox and Safari: B with the hidden textarea.** Faster than A in
  WebKit; worked in the manual pass after the Backspace fix (finding 7).
  If EditContext ships in either browser, B switches to it there without
  touching the model or the engine: both input paths already sit behind one
  interface.

Why B, beyond the numbers:

- **The latency gap is structural, not tuning.** After profiling, A's
  remaining cost is ProseMirror's view update walking every top-level
  block and the browser moving the native selection in a large
  contenteditable (finding 2). Both grow with the document; closing the gap
  means splitting the document into separately rendered pieces, which
  ProseMirror doesn't do. B's costs were all ours and came down to under
  2 ms of script (finding 3).
- **B keeps one source of truth.** In B, edits go straight to the model and
  the engine. A has to mirror every ProseMirror transaction back into the
  model (ADR 0002 forbids the reverse), and the mirroring is where its
  subtle bugs were (lost decorations after a split, finding 2).
- **ADR 0004 already asks for a replaceable input layer.** B is that layer;
  in A, ProseMirror owns input.

What B costs, all of it ours to build (none of it is in the prototype):

- **Accessibility of editing.** In browse mode the DOM is semantic and in
  reading order in both views. While editing, A gets the contenteditable's
  native caret tracking and announcements. B relies on the textarea's own
  (one paragraph at a time) or on EditContext with a mirrored DOM
  selection, whose screen-reader support is unknown. **This is the risk
  that could reverse the verdict.**
- Caret, selection, keyboard navigation (word, line, page), mouse and
  touch selection with handles, clipboard, drag and drop, undo.
- The browser's spellcheck underlines: B shows none. Proofing is our own
  anyway (Spike 6).

**What would reverse it:** a screen-reader test (NVDA, JAWS, VoiceOver)
where B can't give caret echo, selection announcements or paragraph
reading while editing, and no ARIA or input-surface change fixes it. Then
A, with the document split into separately rendered pieces to meet the
latency target.

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

## Findings

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

**7. Manual pass in Firefox and Safari: works well, two bugs fixed.**
Spaces typed at the end of a paragraph didn't move the caret until the
next character: the engine gave trailing spaces no width, right at a line
break but wrong at the end of a paragraph (fixed in the engine; see the
note in [`02b-results-data/`](02b-results-data/README.md)). And in Safari,
B's textarea ignored Backspace at the start of a paragraph: Safari fires no
`beforeinput` when there's nothing in the textarea to delete, so B now
catches paragraph-boundary deletions on `keydown` too. Both are checked in
WebKit and Chromium, in both views.

**8. Layout is deterministic across macOS and Linux.** The 2b layout hashes
computed on this Mac, natively and in Chromium's wasm, equal those
recorded on Linux in Spike 2b, for both engines and both test letters.
This answers part of ADR 0007's open question; Windows remains. (The
trailing-space fix in finding 7 later changed the own engine's hashes; on
macOS native still equals wasm, and Linux has not been re-run.)

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

## Before the verdict is final

1. **Screen readers first**, since they could reverse it: VoiceOver on the
   Mac (Safari, Chrome), then NVDA and JAWS on Windows, against B and A.
2. Latency on the reference hardware (mid-range Windows laptop, MacBook Air
   M1, mid-range Android phone). This also confirms ADR 0007.
3. Phones (Gboard, iOS keyboard) and Windows input methods, against B's
   textarea and EditContext paths.
4. Firefox latency: Playwright's Firefox would not start on this machine.
