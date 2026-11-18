# Spike 2b: judgement calls

Decisions made during Spike 2b that the brief did not settle. Results are in
[`02b-results.md`](02b-results.md).

1. **Spike 2b before Spike 2.** Spike 2 needs real keyboards, input methods,
   phones and screen readers; 2b can run in a cloud container. The layout
   contract that Spike 2 was to fix in its first week is fixed here instead
   (`prototypes/layout/contract`), exactly as the plan wrote it, with one
   addition: every line carries the x of each caret stop, so `locate` and
   `hit` are shared code and can't favour either engine.

2. **A separate Cargo workspace under `prototypes/layout/`**, not crates in
   the main workspace: Typst adds 350 crates and minutes of build time, and
   three BSD-2-Clause crates our licence policy doesn't list. The main CI
   stays as it is. The prototype workspace has its own tests, run with
   `cargo test --release` in `prototypes/layout`.

3. **Independent of the unmerged Spike 3 branch.** The 100-page scaler is
   copied into `bench/src/scale.rs` rather than depending on
   `crates/crdt`, and fixed there (Text origins cleared, so the per-copy
   shuffle reaches the file).

4. **The test documents** are the generated Danish municipal letter scaled
   to 100 sections (101 pages in LibreOffice), and the same file with its
   "Text body" style made justified and hyphenated (Danish, 2/2). The
   letter itself is ragged, so without the second document Knuth–Plass and
   hyphenation are never exercised. Both are written as ODT and read back,
   so LibreOffice and both engines lay out the same bytes.

5. **LibreOffice parity as the yardstick for "beautiful".** The brief asks
   for typography compared with LibreOffice. For users who move documents
   between skriv and LibreOffice, matching its breaks and pages matters more
   than better justification, so parity is measured line by line, and
   Knuth–Plass is measured both with TeX's space shrinking and without (the
   setting that keeps LibreOffice's page count).

6. **The fonts are embedded** (Liberation Serif and Sans, OFL), fetched by
   `fetch-fonts.sh` from the Debian package rather than committed. The test
   document uses only these two families.

7. **The input simplifies what the letter doesn't use:** a tab is a space;
   frames and images are skipped; footnote bodies reserve their height at
   the foot of the page but aren't drawn; header and footer reserve their
   area but aren't drawn; list labels hang in the indent. Both engines get
   the same simplifications.

8. **Typst got every advantage we could give it:**
   - only the edited paragraph's markup is regenerated and applied with
     `Source::edit`;
   - glyph-to-model mapping uses a span index updated per chunk (the first
     version walked the syntax tree per glyph and cost up to 100 ms on its
     own);
   - only pages whose frame hash changed are converted;
   - and a "LibreOffice-compatible" mode (simple line breaks, no shrinking).
   
   The measured cost is Typst's compile.

9. **comemo eviction after every compile** (`evict(10)`, as the Typst CLI
   does in watch mode). Without it the 1,000-keystroke run ran out of
   memory; with it each keystroke pays 8–10 ms more. Evicting less often
   would trade memory for time; it doesn't change the verdict.

10. **Typst in Chromium ran 100 keystrokes per position, not 1,000**, at
    70–100 ms each. Two Typst instances in one page exhausted wasm memory,
    so each variant ran in its own page.

11. **Hyphenation was compared by applying LibreOffice's pattern file
    with Liang's algorithm in Python**, not through LibreOffice's UNO
    hyphenator. LibreOffice's `hyph_da_DK.dic` (Debian `hyphen-da`) was
    installed as a test-time tool; installing a larger Danish word list was
    refused in this environment, so the word list is the 83 distinct words of
    the generated corpus.

12. **Typography is measured, not judged visually:** inter-word space widths
    on justified lines, from carets for the engines and from PDF word boxes
    for LibreOffice. A visual side-by-side was not produced.

13. **Latency is "relayout + locate the caret"**, timed around the engine
    call (`performance.now()` around the wasm call in Chromium). Painting is
    Spike 2's measurement.

14. **The LibreOffice reference is 24.2.7.2**, the Ubuntu package in the
    container, not 26.8.1.1 (Spike 1's macOS oracle). An earlier draft of
    the results named 26.8.1.1 by mistake. Line breaking in Writer changed
    little between these versions as far as we know, but that is not
    verified: the parity run (`scripts/compare_lo.py`) should be repeated
    against 26.8 before ADR 0007 is accepted.
