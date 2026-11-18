# ADR 0004: Render pages to the DOM, behind a replaceable input layer

- Status: accepted (direction); the view strategy is still open
- Date: 2026-11-17

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
  - The browser's accessibility tree, spellcheck UI, selection and IME stay available.
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

## Evidence

This decision rests on the report and on public facts. We have not measured
anything for it yet; Spike 2 does that.

- EditContext shipped in Chrome 121 and Edge 121 (January 2024). Firefox's
  position is "positive" with concerns, and Safari has not shipped it. This is
  the report's claim; Spike 2 re-checks browser status before relying on it.
- The canvas accessibility cost is documented in Google's own announcements.

## Consequences

- We take on the hard problem of DOM rendering. The report calls out caret
  movement and selection across page boundaries, and justification fighting
  the browser.
- Desktop (Tauri vs Electron) and the view strategy interact. Electron
  bundles Chromium, so EditContext would be available everywhere on desktop.

## Open decisions

| Question | Settled by |
|---|---|
| View strategy: A (ProseMirror view with engine-directed lines and pages) or B (owned DOM view with EditContext or a hidden textarea) | Spike 2 |
| Layout kernel: our own on the Linebender stack, or Typst as the kernel | **Answered by Spike 2b:** our own engine on Parley (ADR 0007, proposed). Under 1 ms per keystroke at p95 on 100 pages in Chromium; Typst's kernel takes 75–175 ms. |
| Desktop shell: Tauri (system WebViews; WebKitGTK on Linux is the risk) or Electron | Spike 7, deferred until after the web app |
