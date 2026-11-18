# ADR 0007: Our own layout engine on Parley, not Typst as the kernel

- Status: **proposed** (draft from Spike 2b; latency is measured natively
  and in headless Chromium on Linux, and still needs real devices)
- Date: 2026-11-18
- Implemented in: `prototypes/layout/own` (prototype)
- Evidence: [`docs/spikes/02b-results.md`](../spikes/02b-results.md)

## Context

ADR 0004 decided that our own layout engine decides line breaks, page
breaks and justification, and the DOM only displays the result. Spike 2b
asked how to get that engine: build it on the Linebender stack (Parley for
shaping, bidi and fonts; our own line breaking and pagination), or use
Typst's layout as the kernel, feeding it the model as Typst content.

The target (from the research report) is about 30 ms of relayout per
keystroke on a 100-page document, with caret positions that map between
the model and the page in both directions.

## Decision

**Build our own engine on Parley.**

- **Shaping, bidi, font fallback:** Parley (HarfRust underneath), once per
  paragraph; paragraph layouts are cached by content, style and width.
- **Line breaking:** our own. First-fit (greedy) by default, because that is
  what LibreOffice does and it reproduces LibreOffice's line breaks.
  Knuth–Plass is available for justified text, without shrinking spaces
  unless a document opts in.
- **Line-break opportunities:** UAX #14 from `parley_engine`'s analysis
  (Parley's clusters don't expose them).
- **Hyphenation:** `hypher` with the TeX `hyph-utf8` patterns (Danish: MIT
  OR LPPL-1.3+, used under MIT); on our corpus they agree with
  LibreOffice's Danish dictionary at every break point.
- **Pagination:** widows and orphans, keep-with-next, page breaks before,
  footnote areas, header and footer areas; table rows kept whole.
- **Incremental:** after an edit, re-lay out the edited paragraph and
  re-paginate from its block until a later block starts where it started
  before.
- **Line metrics follow LibreOffice:** the line box is the font's ascent +
  descent + line gap (hhea), scaled by proportional line spacing; every line
  is at least as tall as the paragraph's own font.

## Why not Typst

| Measure (100 pages) | Own engine | Typst kernel |
|---|---|---|
| Per keystroke, native, p95 | 0.27–0.81 ms | 107–174 ms |
| Per keystroke, Chromium (wasm), p95 | 0.4–1.6 ms | 76–107 ms |
| Body-text lines identical to LibreOffice's, ragged / justified | 100% / 92.4% | 96.8% / 38.2% (69.1% with LibreOffice settings) |
| Caret round trip on 10 pages | 100% | 98.9% |
| wasm code size, gzip (fonts excluded) | ≈ 0.66 MB | ≈ 9.8 MB |

Typst is an excellent typesetter, but as a kernel for this editor:

- **It misses the latency target.** Its incremental compile of a 100-page
  document takes about 50–70 ms natively per keystroke whatever the edit, and
  more in wasm. The flow is laid out again; comemo caches paragraphs, but
  not pagination.
- **It answers a different question.** Typst is built to typeset its own
  markup beautifully, not to reproduce LibreOffice. To match LibreOffice's
  page we had to override its text edges, add LibreOffice's below-line
  spacing by hand, turn off its default space shrinking, and switch to
  simple line breaks; even then, justified text matches LibreOffice less
  well than our first-fit breaker.
- **Its caret mapping is ours to build anyway.** `typst-ide`'s
  `jump_from_cursor` is line-precise (it returns the start of the text run);
  character-precise carets need a span map maintained by the bridge.
- **Memory and size:** comemo's caches must be evicted by the host or memory
  grows without bound (a 1,000-keystroke run was killed for lack of
  memory); the wasm build is about 15× larger.

## Consequences

- We own line breaking, pagination and their edge cases (tab stops,
  footnote placement, keep rules, tables across pages, bidi). The prototype
  covers the subset the municipal letter needs; the rest is product work.
- LibreOffice compatibility becomes a measurable property: the spike's
  comparison (`scripts/compare_lo.py`) should become a CI check over the
  corpus.
- Parley's public API doesn't expose break opportunities or line reshaping
  hooks; we depend on `parley_engine` for the former and accept unbroken
  shaping at line ends for now (no ligature or kerning change at a break).
- Determinism holds between native Linux and wasm (identical layout hashes).
  macOS and Windows still need CI runners to confirm.

## Open decisions

| Question | Settled by |
|---|---|
| Latency on the reference hardware (mid-range laptop, Android phone) | Spike 2, on real devices |
| Determinism on macOS and Windows | CI runners |
| Why LibreOffice picks an earlier hyphenation point in about 7% of justified lines | Follow-up against LibreOffice's source (`SwTextGuess`) |
| LibreOffice's page breaks around footnotes inside sections | Follow-up; see the results |
| Fonts to bundle (metric-compatible substitutes for Calibri, Cambria) | Product decision; Carlito and Caladea (OFL) are the obvious candidates |
