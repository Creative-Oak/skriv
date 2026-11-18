# ADR 0004: Render pages to the DOM, behind a replaceable input layer

- Status: accepted (direction); view strategy B **proposed** (Spike 2,
  provisional until screen readers are tested)
- Date: 2026-11-17; view strategy 2026-11-18

## Context

People who live in Word expect a page view, and they judge trust by "does it
look like it will print". Public-sector software in the EU is bound by the
European Accessibility Act, and our users include screen-reader users. Danish
users type on Danish keyboards, dictate, and use phones.

Google Docs moved to canvas rendering in 2021. Its accessibility now depends
on a separately maintained "side DOM" that users must switch on. OnlyOffice
and Euro-Office render to canvas too.

## Decision

- **Pages are rendered to the DOM. Pure canvas rendering is rejected.**
  - The browser's accessibility tree and IME stay available. (With view
    strategy B, below, we draw the selection and the spellcheck underlines
    ourselves; they still live in the DOM, not on a canvas.)
  - We don't maintain a second, parallel accessibility tree.
- **Our own layout engine decides line breaks, page breaks and justification.**
  The DOM displays its result; the browser's own line breaking must never
  contradict it.
- **Text input goes through a replaceable input layer.** We can start with one
  technique and move to another without touching the model or the layout engine.
- **Web first** (decided 2026-11-17). The first product is a web app. Desktop
  packaging comes later.
- **The view never owns the document** (ADR 0002). Any editor framework used
  in the view keeps a projection of the model, not the model itself.
- **View strategy: B, an owned view** (proposed, Spike 2). No
  contenteditable and no editor framework. The DOM holds semantic blocks
  (`<h1>`–`<h6>`, `<p>`, `<table>`) in reading order, each run of text
  absolutely positioned where the layout engine put it, each paragraph a
  CSS containment boundary, and page chrome in an `aria-hidden` layer. We
  draw the caret and selection. Input comes through the replaceable input
  layer, which holds the caret's paragraph:
  - **EditContext** in Chrome and Edge;
  - **a hidden textarea** in Firefox and Safari, positioned at the caret
    for IME windows.

  Edits go straight to the model and the layout engine; nothing is
  mirrored back.

## Evidence

The direction rests on the report and on public facts; the view strategy on
Spike 2 ([`02-results.md`](../spikes/02-results.md)).

- **EditContext is still Chromium only** (re-checked 2026-11-18): Chrome
  and Edge since 121; Firefox positive but unimplemented; Safari none.
- **Latency** (100-page letter, Apple M4, real key events, p95 keystroke to
  painted frame): B 12.9–15.6 ms in Chromium, A (ProseMirror with
  engine-directed lines) 23.2–25.3 ms. Headless WebKit: B 26–28 ms, A
  36–64 ms. After profiling, A's remaining cost is ProseMirror's view update
  over every top-level block and native selection updates in a large
  contenteditable; both grow with the document.
- **Input:** a manual pass in Firefox and Safari on the Mac (Danish
  keyboard, dead keys, macOS input methods, selection across page breaks)
  found no lost compositions in either view, after two fixes.
- **Not yet tested:** screen readers, phones, Windows, and latency on the
  brief's reference hardware (deferred 2026-11-18).
- The canvas accessibility cost is documented in Google's own announcements.

## Consequences

- We take on the hard problem of DOM rendering. The report calls out caret
  movement and selection across page boundaries, and justification fighting
  the browser.
- Desktop (Tauri vs Electron) and the view strategy interact. Electron
  bundles Chromium, so EditContext would be available everywhere on desktop.
- **With B we build what contenteditable gives for free:** caret,
  selection, keyboard navigation, mouse and touch selection with handles,
  clipboard, drag and drop, and undo.
- **Accessibility while editing is ours to prove.** Browse-mode reading is
  semantic in both views, but B has no contenteditable caret tracking: the
  textarea exposes one paragraph at a time, and screen-reader support for
  EditContext is unknown. If screen-reader tests show B can't announce the
  caret, selection and paragraphs while editing, the fallback is A with the
  document split into separately rendered pieces to meet the latency target.
- **No browser spellcheck underlines in B.** Proofing is our own (Spike 6).

## Open decisions

| Question | Settled by |
|---|---|
| View strategy: A (ProseMirror view with engine-directed lines and pages) or B (owned DOM view with EditContext or a hidden textarea) | **Answered provisionally by Spike 2:** B (proposed). Final once screen readers (VoiceOver, NVDA, JAWS) and the reference hardware are tested. |
| Layout kernel: our own on the Linebender stack, or Typst as the kernel | **Answered by Spike 2b:** our own engine on Parley (ADR 0007, proposed). Under 1 ms per keystroke at p95 on 100 pages in Chromium; Typst's kernel takes 75–175 ms. |
| Desktop shell: Tauri (system WebViews; WebKitGTK on Linux is the risk) or Electron | Spike 7, deferred until after the web app |
