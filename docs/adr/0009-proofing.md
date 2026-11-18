# ADR 0009: Spell checking against both dictionaries; detection only where one language is needed

- Status: **proposed** (draft from Spike 6; needs a decision, including the
  data-licence rule below)
- Date: 2026-11-18
- Implemented in: `crates/proofing` (spike code)
- Evidence: [`docs/spikes/06-results.md`](../spikes/06-results.md)

## Context

Danish public-sector documents mix Danish and English: English abstracts,
quoted contract clauses, vendor answers in tender tables, bilingual
newsletters, English loanwords and product names in Danish sentences, and
English text full of Danish place names. Word and LibreOffice proof each
run of text in one language, set by style or by detection. A wrong language
covers correct text in red.

Spike 6's kill criterion was "fewer than 1 false language switch per 10
pages". Automatic per-paragraph detection with hysteresis reached 5.1 at
best on a 41-page mixed corpus (0.0 on the long-paragraph part, 16.5 on the
short-fragment part), so detection alone can't decide which dictionary
checks a word.

## Decision

1. **Checker: spellbook** (MPL-2.0) with Hunspell-format dictionaries,
   loaded at run time, in the core and in wasm.
2. **Dictionaries: `da_DK` (Stavekontrolden) under its MPL-1.1 option and
   `en_US` (SCOWL, permissive).** Fetched, not committed, until the data
   rule below is decided.
3. **Every word is checked against Danish and English; a word is correct
   if either dictionary accepts it.** This is the default for documents
   whose languages are Danish and English. A user can turn it off per
   document (strict mode: one language per paragraph).
4. **Each paragraph still has one language**, used for suggestions (the
   paragraph's dictionary first), hyphenation, grammar if we ever add it,
   and the `fo:language` written to the file. It comes from, in order:
   a language the user set on the text or paragraph; the paragraph style;
   detection with hysteresis (a paragraph of 20+ letters switches the
   running language when the dictionary vote and, if built in, an n-gram
   detector agree; shorter paragraphs inherit; short headings take the
   next paragraph's language); the document default.
5. **The detector is the dictionary vote** (words only Danish knows against
   words only English knows). It needs no model. lingua (6.2 MB gzipped in
   wasm for two languages) and whatlang are not needed; whatlang (66 KB) is
   the fallback if a third language without a dictionary must be detected.
   Detectors are always restricted to the languages being proofed.
6. **Suggestions run asynchronously** (a worker), on demand, and can be
   cancelled: Danish suggestions take a median of 140–240 ms and up to
   4 s per word.
7. **No LanguageTool in the core** (LGPL-2.1, Java; 78 Danish pattern
   rules).

### Proposed rule for data files (amends ADR 0005)

ADR 0005 bans copyleft dependencies of the core but says nothing about
data loaded at run time. Proposed: **dictionaries, hyphenation patterns and
similar data may be file-level copyleft (MPL, LGPL) if they ship as
separate, unmodified, replaceable files with their licence and source
noted; GPL-only and AGPL data may not ship.** Under this rule `da_DK`
(MPL-1.1 option), `en_US` (permissive) and `en_GB` (LGPL) could all ship.
Without it, British English can't.

## Consequences

- Wrong-language flags disappear (0 in the corpus, against 10.4 per 10
  pages with the predicted paragraph language), and false flags halve
  (10.3 against 23.4 per 1,000 words).
- Some misspellings are accepted because they are words in the other
  language: in the seeded set, "born" (børn), "gar" (går), and three
  English typos that Danish compounding accepts ("succesful" = succes +
  ful). That is 2 of 95 Danish and 3 of 53 English detectable typos.
- Danish compounding accepts some misspellings in any mode (5 of 95:
  "tidspunk" = tids + punk). This is the dictionary, not the design.
- A web build costs about 0.12 MB of code and 1.1 MB of dictionaries
  (gzip), plus 0.1–0.15 s to load `da_DK`.
- A third language (German, Swedish) would widen the union and lower
  recall further; the union should cover the languages a document
  declares, not every installed dictionary.

## Open questions

| Question | Settled by |
|---|---|
| The data-licence rule above (and so en_GB) | Licence review, before the first release that ships dictionaries |
| Whether paragraph languages already present in real ODT files make detection mostly unnecessary | Measuring real municipal files (Spike 1's corpus) |
| The density of language switches in real documents | The same |
| Per-word cache and worker design for live checking | The editor (Phase 1) |
| Upstream fix for the three unparseable `da_DK.dic` lines | A report to Stavekontrolden |
