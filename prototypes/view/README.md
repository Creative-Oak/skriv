# Spike 2 prototypes: two views over the Spike 2b layout engine

See [`docs/spikes/02-results.md`](../../docs/spikes/02-results.md) and the
manual test sheet [`docs/spikes/02-manual-tests.md`](../../docs/spikes/02-manual-tests.md).

| Path | What |
|---|---|
| `engine/` | The editing core (Rust → wasm): the 100-page letter as layout input, edits (type, delete across paragraphs, split, join), incremental layout with Spike 2b's own engine (greedy breaking), dirty pages, `locate`/`hit`/vertical caret movement, selection rectangles |
| `web/src/b/` | **View B, owned view.** Semantic `<h1>`–`<h6>`/`<p>`/`<table>` in reading order, runs absolutely positioned by the engine, page chrome in an `aria-hidden` layer. Own caret and selection. Input: EditContext (Chrome, Edge) or a hidden textarea (others, or `?input=textarea`) |
| `web/src/a/` | **View A, ProseMirror.** ProseMirror's contenteditable with the same semantic blocks; each paragraph is a node view placed at its first line, each run an inline decoration placed by the engine. Native caret, selection and IME |
| `web/src/shared/` | Engine wrapper and page cache, geometry (points → px, baselines), the latency probe and its panel |
| `web/scripts/measure.mjs` | Automated latency runs (Playwright, real key events) |
| `web/scripts/check.mjs` | Functional check: selection across a page break, delete, type, undo |

Its own Cargo workspace and pnpm package; neither is part of the main
workspace or CI.

## Run

```bash
../layout/fetch-fonts.sh            # Linux; on macOS copy Liberation 2.1.5 TTFs into ../layout/fonts
(cd ../layout && cargo run --release -p layout-bench -- export)   # the 100-page letter
./prepare.sh                         # wasm, fonts and the letter into web/public
cd web && pnpm install --ignore-workspace && pnpm dev
```

The dev server listens on all interfaces (port 5180), so phones and other
computers on the same network can open `http://<this machine's IP>:5180/`.
The panel at the bottom shows which input path is in use, keystroke-to-paint
latency, and buttons for synthetic 1,000-keystroke runs; **Download JSON**
saves every sample.

```bash
cd web
node scripts/measure.mjs http://localhost:5180 1000 chromium,webkit b,a
node scripts/check.mjs
cd .. && cargo test --release        # engine: edits match a fresh layout
```

## What the prototypes don't do

Enough to type, select and edit on a 100-page document, not an editor:
no formatting commands, clipboard beyond the browser's own (A) or none (B),
touch selection handles in B, list renumbering, footnote text (its space is
reserved, the text isn't drawn), table borders, justified text (the test
letter is ragged, as LibreOffice lays it out). B's undo snapshots the
document in idle time per burst of typing.
