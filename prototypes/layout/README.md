# Spike 2b prototypes: two paginators behind one layout contract

See [`docs/spikes/02b-results.md`](../../docs/spikes/02b-results.md).

| Crate | What |
|---|---|
| `contract` | The layout contract (`Pages`, `locate`, `hit`, `layout_hash`), the layout input, and model → input (style resolution) |
| `own` | Prototype (a): our own paginator on Parley, greedy and Knuth–Plass breaking, Danish hyphenation, incremental pagination |
| `typst-kernel` | Prototype (b): Typst as the kernel, through a model → Typst markup bridge with a span map for carets |
| `bench` | Measurements (`cargo run --release -p layout-bench -- <command>`): `latency`, `carets`, `lines`, `spaces`, `hyph`, `export` |
| `wasm` | Both engines as a wasm32 `cdylib` with an integer API; `bench.mjs` and `chromium.cjs` time them in Node and headless Chromium |

This is its own Cargo workspace (Typst is heavy), outside the main CI.

```bash
./fetch-fonts.sh                                     # Liberation fonts (OFL), from fonts-liberation
cargo run --release -p layout-bench -- export        # test input; the wasm crate embeds it
cargo test --release --workspace
./measure.sh                                         # everything in the results, about an hour
```

`measure.sh` also needs LibreOffice, poppler's `pdftotext`, Node 22,
Playwright's Chromium, and LibreOffice's Danish hyphenation patterns
(`/usr/share/hyphen/hyph_da_DK.dic`, Debian package `hyphen-da`).
