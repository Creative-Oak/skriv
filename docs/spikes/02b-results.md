# Spike 2b results: our own layout engine vs Typst as the kernel

- **Date:** 2026-11-18.
- **Brief:** [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md), "Spike 2b";
  risk in [`02b-layout-engine.md`](02b-layout-engine.md).
- **Code:** `prototypes/layout/` (its own Cargo workspace): `contract`
  (the layout contract and the model → layout input), `own` (prototype a),
  `typst-kernel` (prototype b), `bench` (measurements), `wasm` (both engines
  in wasm32, driven from JavaScript).
- **Judgement calls:** [`02b-judgements.md`](02b-judgements.md). **Raw
  output:** [`02b-results-data/`](02b-results-data/).
- **Machine:** a 4-core Linux cloud container; Chromium 157 headless for the
  browser runs; LibreOffice 24.2.7.2 (Ubuntu package) as the reference; Spike 1 used 26.8.1.1 on macOS, and the parity run should be repeated against it. None of this is the
  reference hardware of the brief (mid-range Windows laptop, Android
  phone); Spike 2 has to repeat the latency runs there.
- **Reproduce:** `prototypes/layout/fetch-fonts.sh`, then
  `prototypes/layout/measure.sh` (about an hour).

## Verdict

**Build our own engine on Parley (prototype a).** It relays out a 100-page
document in **under 1 ms per keystroke at p95**, natively and in Chromium,
about a hundred times inside the ~30 ms target. It reproduces **100% of
LibreOffice's line breaks** in ragged text and 92% in justified, hyphenated
Danish text. Its caret mapping round-trips every character we tested, and its
layout is bit-identical between native and wasm. The Typst kernel
(prototype b) misses the target: **75–175 ms per keystroke at p95**,
because Typst re-lays out the whole flow on every compile. It also needs
more work to resemble LibreOffice, not less. ADR 0007 is drafted as
*proposed* until Spike 2 confirms latency on real devices.

| 100-page document | Own engine | Typst kernel |
|---|---|---|
| Relayout per keystroke, native, p95 (ragged / justified) | **0.29–0.36 ms / 0.27–0.81 ms** | 107–174 ms / 110–166 ms |
| Relayout per keystroke, Chromium wasm, p95 (ragged / justified) | **0.4–0.7 ms / 0.5–1.6 ms** | 76–107 ms / 99–104 ms |
| Incremental result equals a fresh layout | yes, all positions | yes |
| LibreOffice's body-text line breaks reproduced (ragged / justified) | **100% / 92.4%** (greedy) | 96.8%* / 38.2% (69.1% in LibreOffice mode) |
| Pages (LibreOffice: 101, ragged and justified) | 101 / 101 | 100 / 89 (100 in LibreOffice mode) |
| Caret round trip, every character on 10 pages | **100%** | 98.9% |
| Native = wasm layout (hash) | yes | yes |
| Memory (process, after 1,000 keystrokes) | 46 MiB | 243–274 MiB |
| wasm32 code size, gzip, fonts and input excluded | **≈ 0.66 MB** | ≈ 9.8 MB |

\* The missing 3.2% are the lines holding footnote citations, which the
Typst bridge draws as Typst footnotes and doesn't map back. Excluding them,
Typst's ragged line breaks also match LibreOffice's.

## What was built

**The layout contract** (`contract/src/lib.rs`), as fixed in the plan:
`layout(model snapshot, edit hint) -> Pages`, `locate(Pages, para, offset)
-> (page, line, x)`, `hit(Pages, page, x, y) -> (para, offset)`. Every line
carries the x of every caret stop, so `locate` and `hit` are shared by
both engines and can't flatter either.

**The input** (`contract/src/from_model.rs`): the skriv model with ODF
styles resolved through inheritance (paragraph and character styles,
percentages, line height, spacing, keep-with-next, widows and orphans,
hyphenation), page size and margins, header and footer areas, list labels,
table column widths, footnotes. Both engines lay out exactly this.

**Prototype (a), our own engine** (`own/src/lib.rs`):

- Parley 0.13 shapes each paragraph once (HarfRust underneath), with the
  Liberation fonts embedded so native and wasm see the same bytes.
- Line-break opportunities come from `parley_engine`'s UAX #14 analysis.
- Breaking: first-fit (greedy, as LibreOffice) or Knuth–Plass (total fit,
  for justified text), with or without TeX's space shrinking.
- Danish hyphenation with `hypher` (the TeX `hyph-utf8` patterns).
- Pagination with widows/orphans, keep-with-next, page break before,
  footnote areas (laid out as real 10 pt paragraphs), header and footer
  areas, table rows kept whole.
- Incremental: paragraph layouts cached by content and width; after an edit,
  pagination restarts at the edited block and stops at the first later block
  that starts where it started before.

**Prototype (b), Typst as the kernel** (`typst-kernel/src/lib.rs`):

- A `World` with the same embedded fonts.
- The input becomes Typst markup, one chunk per paragraph, recording for
  every model character the source byte that produced it.
- An edit regenerates only the edited paragraph's chunk and applies it with
  `Source::edit` (incremental reparse), then compiles (comemo memoisation).
- Typst's frames are converted to contract `Pages`. Each glyph's span is
  mapped back to a paragraph and character through a span index that is
  updated only for the edited chunk. Only pages whose frame hash changed are
  converted.

## Verify first

Checked 2026-11-18 against crates.io, the crate sources and the Typst
changelog.

| Claim in the research report | Finding |
|---|---|
| Parley 0.12 added `parley_engine` | **True.** `parley_engine` first shipped as 0.12.0 with Parley 0.12; both are now 0.13.0. It is a lower-level shaping API rather than a "hook": custom line breaking is done by shaping with Parley and breaking yourself. **Parley's public clusters don't expose line-break opportunities** (the flag is private); `parley_engine::Analysis::char_info()` does, so we run its analysis pass too. |
| Typst 0.14 (24 October 2025) produces tagged PDFs by default and adds `par.justification-limits` | **True** ([changelog](https://typst.app/docs/changelog/0.14.0/)). Current: Typst 0.15.1. Default justification limits let spaces shrink to 2/3 and stretch to 150%. |
| `hypher` includes Danish patterns | **True.** `Lang::Danish`, bounds 2/2, `tries/da.bin`, built from `hyph-utf8`. |
| Licences: Parley, HarfRust, Skrifa, Fontique, ICU4X, hypher | Parley/`parley_engine`/Fontique/`parlance` 0.13: Apache-2.0 OR MIT. HarfRust 0.15.0: MIT. Skrifa 0.49 and read-fonts 0.46: MIT OR Apache-2.0. ICU4X 2.x (`icu_segmenter` and friends): Unicode-3.0. `hypher` 0.1.8: MIT OR Apache-2.0. All allowed by ADR 0005; the own engine's 78 crates pass. |
| The Danish `hyph-utf8` patterns' licence | **MIT OR LPPL-1.3+** (Frank Jensen, 1994; per the upstream `hyph-da.lic.txt` and Debian). We use them under MIT. |
| Typst's licence | Apache-2.0 (typst, typst-ide, typst-layout 0.15.1; comemo 0.5.1 MIT OR Apache-2.0). Typst pulls 350 crates, three of them BSD-2-Clause only (`arrayref`, `kamadak-exif`, `mutate_once`), which our allow list doesn't include yet. Moot if Typst isn't adopted. |
| Fonts | Liberation 2.1.5 (Serif, Sans; metric-compatible with Times New Roman and Arial): SIL OFL 1.1. Carlito and Caladea (metric-compatible with Calibri and Cambria, OFL) are installed here too but not used by the test document. |

## Results

### Latency

1,000 keystrokes per position (natively, and for the own engine in
Chromium); 100 for Typst in Chromium, where each takes 70–100 ms. Each
keystroke types one character in the middle of a paragraph, relays out with
an edit hint, and locates the caret. Positions: a body paragraph on page 1,
one on page 50, and a table cell on page 80. The 100-page document is the
generated Danish municipal letter repeated in 100 sections (101 pages in
LibreOffice), ragged as written, and a variant whose "Text body" style is
justified and hyphenated.

| Engine | Page 1 p50 / p95 / max | Page 50 | Table, page 80 |
|---|---|---|---|
| own (greedy), native | 0.24 / 0.35 / 6.3 ms | 0.21 / 0.32 / 2.8 | 0.17 / 0.27 / 1.2 |
| own (Knuth–Plass), native, justified | 0.42 / 0.80 / 5.4 | 0.45 / 0.81 / 3.0 | 0.18 / 0.27 / 1.4 |
| Typst, native | 86 / 107 / 206 | 117 / 146 / 273 | 130 / 174 / 248 |
| Typst, native, justified | 91 / 110 / 557 | 126 / 166 / 332 | 131 / 165 / 208 |
| own, Chromium | 0.3 / 0.7 / 6.0 | 0.3 / 0.6 / 3.6 | 0.2 / 0.4 / 1.6 |
| own (Knuth–Plass), Chromium, justified | 0.5 / 1.0 / 5.3 | 0.6 / 1.6 / 4.4 | 0.2 / 0.5 / 2.3 |
| Typst, Chromium (100 keystrokes) | 67 / 76 / 107 | 80 / 107 / 117 | 89 / 104 / 126 |
| Typst, Chromium, justified (100) | 71 / 99 / 103 | 83 / 102 / 347 | 90 / 104 / 155 |

The own engine's worst cases (2–6 ms) are keystrokes that add a line and
push every later page: re-pagination then runs to the end of the document.
Even that is well inside the target, and a later version can stop
re-pagination at the end of the viewport.

Where Typst's time goes (median of 30 keystrokes, native):

| | Page 1 | Page 50 | Table, page 80 |
|---|---|---|---|
| Edit the source and reparse | 1.6 ms | 1.5 | 1.7 |
| Compile (layout) | 57 | 57 | 69 |
| Evict comemo's caches | 8.3 | 8.5 | 9.7 |
| Convert frames to the contract (only changed pages) | 3.1 | 3.7 | 4.4 |

The compile is the floor: Typst memoises paragraph layouts but lays the
flow out again on every compile, so a 100-page document costs 50–70 ms
whatever is edited. Over 1,000 keystrokes the edited paragraph grows and the
numbers rise further. Typst can't reach the 30 ms target on 100 pages
without changes to Typst itself.

Full layout from scratch: own engine 52–103 ms natively, 77–219 ms in
Chromium; Typst 0.34–0.9 s natively, 0.85–1.1 s in Chromium (the very first
compile in a process, before any caches, took 6.6 s).

### Line breaks and pages against LibreOffice

LibreOffice 24.2.7.2 rendered both 100-page documents to PDF. Its body-text
lines (header, footer, footnotes and table cells left out) were compared,
in order, with each engine's (`scripts/compare_lo.py`). List labels and
footnote numbers, which LibreOffice draws but aren't paragraph text, are
normalised away.

| Engine | Ragged: lines identical | Pages (LO 101) | Justified + hyphenated: lines identical | Pages (LO 101) |
|---|---|---|---|---|
| own, greedy | **100.0%** | 101 | **92.4%** | 101 |
| own, Knuth–Plass (no shrink) | 100.0% | 101 | 88.1% | 101 |
| own, Knuth–Plass with TeX shrink | 100.0% | 101 | 43.2% | 91 |
| Typst, default | 96.8%* | 100 | 38.2% | 89 |
| Typst, LibreOffice-compatible settings | 96.8%* | 100 | 69.1% | 100 |

- **Ragged text:** greedy breaking with LibreOffice's metrics reproduces
  LibreOffice exactly. 100 of 101 pages start on the same line.
- **Justified text:** LibreOffice breaks first-fit and never shrinks spaces.
  Knuth–Plass and Typst's defaults fit more per line, so documents get
  shorter than in LibreOffice (89–91 pages instead of 101). For an
  editor whose users move between skriv and LibreOffice, **parity is the
  default; Knuth–Plass should be an explicit typographic choice**.
- **The remaining 7.6% (greedy, justified)** are lines where LibreOffice
  hyphenates a word at an earlier point than we do ("byggetil-" vs
  "byggetilla-"), or doesn't hyphenate a short word at all ("sagen.").
  The patterns are the same (see Hyphenation). Narrowing our measure by
  0.25–2 pt made agreement worse, so it isn't LibreOffice's twip rounding.
  The cause is open.
- **Typst needed several overrides to look like LibreOffice:**
  - explicit text edges (its `"ascender"`/`"descender"` edges use different
    metrics from LibreOffice's: 13.5 pt line pitch instead of 15.85 pt);
  - LibreOffice's proportional line spacing below each paragraph's last
    line, added by hand;
  - `linebreaks: "simple"` and no space shrinking.
  
  Even then, justified text agrees on 69%.

### Caret mapping

For every character on 10 pages spread through the document: `hit(locate(c))
== c`.

| | Ragged | Justified |
|---|---|---|
| own | **18,289 / 18,289 (100%)** | **18,289 / 18,289 (100%)** |
| Typst | 20,042 / 20,254 (98.95%) | 21,897 / 22,136 (98.92%) |

The Typst failures are characters next to escaped punctuation (`\.`) in
the generated markup, a bridge mapping detail. **Typst's own
`jump_from_cursor` is line-precise only:** it returns the start of the text
run containing the cursor (same page and baseline as our caret in 59 of 60
samples, but x at the run's start). A character-precise caret needs the span
map the bridge maintains anyway. Ligatures (Liberation's "fi", "fl") get
caret stops interpolated inside the cluster in both engines. The test
document has no bidi text, so bidi carets are untested.

### Determinism

Layout hashes (positions rounded to 1/64 pt, every caret stop included):

| | Native (Linux x86-64) | wasm32 in Chromium |
|---|---|---|
| own, ragged | 5c75c68ac3118cad | 5c75c68ac3118cad |
| own, justified (Knuth–Plass) | 4a220d64e67d86e9 | 4a220d64e67d86e9 |
| own-greedy, justified | 91728d8ddd78c40c | 91728d8ddd78c40c |
| Typst, ragged | fd94f89835f7b00a | fd94f89835f7b00a |
| Typst, justified | 9a9571c37a722b6f | 9a9571c37a722b6f |

macOS and Windows weren't available; CI runners there should compare these
hashes.

### Hyphenation

The 83 distinct Danish words (five or more letters) in the generated corpus,
hyphenated by `hypher` and by LibreOffice's own dictionary
(`hyph_da_DK.dic`, applied with Liang's algorithm at 2/2 characters,
`scripts/hyphenation_compare.py`): **identical, 151 break points, no
differences**. Both are Frank Jensen's patterns. The word list is small;
LibreOffice's dictionary wasn't installed in the container to begin with,
and installing a larger Danish word list was not possible here. The
differences in justified text above come from where LibreOffice *chooses*
to break, not from the patterns.

### Typography

Inter-word spaces on the justified lines of the 100-page document, as a
multiple of a natural space:

| | Mean | p95 | Max | Wider than 2× |
|---|---|---|---|---|
| LibreOffice | 1.22 | 1.53 | 2.30 | 18 |
| own, greedy | 1.22 | 1.51 | 1.90 | 0 |
| own, Knuth–Plass | 1.22 | 1.49 | **1.75** | 0 |
| own, Knuth–Plass with shrink | 1.03 | 1.30 | 1.44 | 0 |
| Typst, default | 1.15 | 1.92 | 2.88 | 698 |
| Typst, LibreOffice-compatible | 1.21 | 2.05 | 2.75 | 1,386 |

Knuth–Plass without shrink keeps LibreOffice's average and trims the
loosest lines; with shrink it is tighter and evener, at the cost of parity.
Typst's numbers are higher partly because the bridge measures its spaces
from glyph positions, which include Typst's justification of the line's
last space. Treat them as indicative. A visual side-by-side was not made in
this spike.

### Size and memory

| | wasm32, raw | gzip | of which fonts + test input (gzip) | Code, gzip |
|---|---|---|---|---|
| own | 7.16 MB | 2.49 MB | 1.83 MB | **≈ 0.66 MB** |
| Typst | 36.8 MB | 11.6 MB | 1.83 MB | ≈ 9.8 MB |

(Release build, `opt-level = 3`, LTO, no `wasm-opt`.) Natively, the
benchmark process holds 46 MiB with the own engine and 243–274 MiB with
Typst after 1,000 keystrokes.

## Surprises

1. **Typst runs out of memory without eviction.** comemo's caches grow with
   every compile; a 1,000-keystroke run on 100 pages was killed by the
   container's memory limit until the bridge called `comemo::evict(10)`
   after each compile, as the Typst CLI does in watch mode. Eviction costs
   8–10 ms per keystroke.
2. **Parley hides line-break opportunities.** `Cluster::is_soft_line_break`
   reports breaks Parley *made*, not where it *may* break; the first version
   of the engine never broke a line. `parley_engine`'s analysis has them.
3. **LibreOffice's line box includes the line gap, and every line is at
   least the paragraph font's height.** A paragraph holding only a small
   footnote citation is still a full line. This mattered for page parity.
4. **Widows and orphans in short paragraphs.** A 3-line paragraph with
   widows = orphans = 2 can't be split, so LibreOffice moves it whole. Our
   first rule skipped short paragraphs and left a widow.
5. **The 100-page test document was 100 identical copies.** The per-copy
   word shuffle in `scale` (written for Spike 3) never reached the file:
   text runs keep their source origin, so the ODT writer copied the
   original spelling. With identical copies, LibreOffice left page 3 almost
   empty after a footnoted paragraph inside a section, which neither engine
   reproduced. With varied text that doesn't happen, so it is an edge case
   of LibreOffice's footnote handling in sections, left open. The Spike 3
   documents are corrected on their branch.
6. **`usize` in a hash breaks cross-platform determinism checks**: it is
   4 bytes on wasm32 and 8 natively. The first native/wasm comparison
   "failed" for that reason alone.
7. **Typst collapses whitespace at line breaks**, so some characters are
   drawn nowhere; the contract now puts their caret at the end of the line
   before, as editors do.

## Kill / continue

| Criterion (from the report) | Own engine | Typst kernel |
|---|---|---|
| ~30 ms relayout per keystroke on 100 pages | **met** (≤ 1.6 ms p95, Chromium) | **not met** (75–175 ms p95) |
| Editable caret mapping | **met** (100% round trip) | met with our own span map (98.9%) |

**Continue with (a).**

## What's next

1. **Spike 2** builds the view on this engine behind the contract and
   measures keystroke-to-paint on the reference laptop and phone.
2. **Make the LibreOffice comparison a CI check** over the generated and
   municipal corpora (`scripts/compare_lo.py`), for ragged and justified
   text.
3. **Find LibreOffice's hyphenation choice rule** (the 7.6% of justified
   lines) in its source (`SwTextGuess`), and its footnote/section page rule.
4. **Product features the prototype skips:** tab stops, bidi, tables split
   across pages and repeated header rows, frames and images, footnote
   bodies drawn, reshaping at line ends, multi-column sections.
5. **Fonts:** decide the bundled set (Liberation, Carlito, Caladea, all
   OFL) and add OFL handling to the licence policy for fonts.
