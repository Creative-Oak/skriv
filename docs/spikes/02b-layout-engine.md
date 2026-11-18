# Spike 2b: Our own layout engine vs Typst as the kernel

**Risk (report #2b):** building our own layout engine costs too much compared
with reusing Typst.

**Experiment.** Prototype two paginators:

- Parley + Knuth–Plass + Danish hyphenation (`hyph-utf8` patterns);
- Typst as the kernel, through a model → Typst content bridge.

Measure per-keystroke relayout on a 100-page document, and check whether a
caret position maps to a layout position and back.

**Kill / continue (from the report).** Pick whichever reaches the latency
target (about 30 ms relayout per keystroke) with editable caret mapping.

**Unverified report claims to check first.**
- Parley 0.12 added `parley_engine`.
- Typst 0.14 (24 October 2025) produces tagged PDFs by default and adds
  `par.justification-limits`.
- The licences of Parley, HarfRust, Skrifa, Fontique, ICU4X, `hypher` and the
  `hyph-utf8` patterns. The patterns carry per-language licences; the Danish
  one needs checking.

**Status:** done, 2026-11-18, except latency on real devices (Spike 2).
Results: [`02b-results.md`](02b-results.md); judgement calls:
[`02b-judgements.md`](02b-judgements.md); decision: ADR 0007 (proposed).
Plan: [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md).
