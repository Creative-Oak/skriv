# Spike 6: Proofing in mixed Danish and English text

**Risk (report #6):** proofing in documents that mix Danish and English.

**Experiment.** Language detection per paragraph (lingua-rs or whatlang),
with hysteresis, plus a Hunspell-format checker (zspell or similar) compiled
to WASM, using the community `da_DK` dictionary.

**Kill / continue (from the report).** Fewer than 1 false language switch per
10 pages.

**Unverified report claims to check first.**
- LanguageTool's Danish rule coverage. The report expects it to be thin.
- The licences of zspell, lingua-rs and whatlang, and of the `da_DK` dictionary.

**Status:** done (2026-11-18). The criterion fails for automatic language
switching (5.1 false switches per 10 pages at best; 0 on long paragraphs);
spell checking against both dictionaries avoids the problem. See
[`06-results.md`](06-results.md) and ADR 0009 (proposed).
