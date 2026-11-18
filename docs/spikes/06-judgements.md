# Spike 6: judgement calls

Decisions made while running Spike 6 without asking, so they can be
reviewed. Results: [`06-results.md`](06-results.md).

1. **spellbook, not zspell.** The brief named zspell "or similar". zspell
   0.5.5 fails to parse `da_DK.aff` and its README lists compound handling
   as unsupported; Danish needs compounds. spellbook 0.4.2 (MPL-2.0, a
   port of Nuspell from the Helix editor's organisation) loads both dictionaries
   and handles `da_DK`'s compound rules. It calls itself alpha.

2. **Three lines of `da_DK.dic` are escaped in memory.** `"A/S"`, `"c/o"`
   and `"I/S"` contain a bare slash, which Hunspell reads as a word plus an
   invalid flag and skips over; spellbook refuses the whole file. The
   loader escapes a slash whose "flags" aren't numeric flags. The file on
   disk is not changed. This should be reported upstream to Stavekontrolden.

3. **en_US is the English dictionary; en_GB only for comparison.** en_GB
   matches how Danish organisations write English (centre, programme,
   organise) and would remove about 40 false flags here, but it is LGPL
   only. en_US (SCOWL) is permissive. Shipping en_GB needs a licence
   decision (ADR 0009 proposes one); this spike did not take it.

4. **Dictionaries are fetched, not committed.** `fetch-dicts.sh` pins the
   LibreOffice dictionaries commit and checks SHA-256 sums
   (`dicts.sha256`). `da_DK` is 3.9 MB and tri-licensed with GPL and LGPL
   options; keeping it out of git avoids a licence question in the
   repository until the data policy is decided.

5. **Wikipedia text is not committed either.** It is CC BY-SA 4.0.
   `fetch_wiki.py` downloads pinned revisions (titles and revision IDs in
   `wiki-manifest.tsv`, which is also the attribution) and `build_wiki.py`
   mixes them with a fixed seed, so the corpus is reproducible. The API
   asks clients to identify themselves; the script sends a generic user
   agent with no personal details.

6. **The handwritten corpus was written for the spike, by the agent that
   built the detectors.** It imitates Danish municipal documents (letters,
   minutes, memos, tenders, a newsletter, a guide, a mail thread, a job
   advert, a report, an EU application, workshop notes, a contact list).
   All names, numbers and references are invented. Released as CC0.

7. **Labelling rule.** Each paragraph gets the language a careful person
   would set for it. Language-neutral fragments (names, addresses, numbers,
   codes) take the language of their surroundings. English passages inside
   Danish paragraphs are marked as spans (`{{en:...}}`) and the paragraph
   stays Danish. In a Danish table with English answers, the requirement
   IDs ("K-01") are Danish.

8. **How Wikipedia islands are placed.** An island of 1–6 paragraphs from
   the other-language article goes only before a body paragraph or heading
   (never into a run of list items, where it would be followed by neutral
   names), some with the heading that preceded it in its article, and 4 of
   10 documents start with an "Abstract"/"Resumé" in the other language.
   Short and glossed paragraphs ("Jeg så {huset}" in the English article
   on Danish) are not used as island text, because their language is not
   the article's. Paragraphs are labelled by their source article.

9. **One label was corrected after the first run.** "Emne: SV: Study visit
   to the citizen service centre" was labelled Danish (a Danish mail
   header); every detector said English and most of its words are English.
   It is now English with "Emne: SV:" marked Danish. No other label was
   changed after seeing results.

10. **False switches are counted over paragraphs with something to
    check.** A paragraph like "K-01" or "20 11 34 56" gets no spell
    checking in any language, so its language has no visible effect; it is
    left out of the switch sequence (it still counts towards pages).
    Counted over every paragraph, the results are worse (best 7.5 instead
    of 5.1 per 10 pages), and both are reported.

11. **What a false switch is.** Compare the predicted and true sequences,
    both starting from the document's default language: a switch in one
    and not the other is a false switch. A single wrong paragraph inside a
    run costs two; a switch found one paragraph late costs two. Spurious
    and missed are reported separately.

12. **A page is 2,500 characters of paragraph text** (as the brief says),
    and the document's default language is known: in ODF it is the
    default paragraph style's `fo:language`.

13. **Policy tuning on half the documents.** The hysteresis grid (minimum
    letters 0–80, switch confidence 0–0.99, short-paragraph override, heading
    look-ahead; 464 policies) is searched on documents at odd positions in
    name order, minimising false switches, then maximising accuracy. The
    other half is reported as held-out, the whole corpus as the main
    number, and the best policy on all data as an optimistic bound. A
    fixed, untuned policy (20 letters, 0.9, heading look-ahead) is reported
    too.

14. **More detectors than the brief named.** Besides lingua and whatlang:
    lingua with Norwegian Bokmål and Swedish (a Bokmål or Swedish answer
    means "not sure"), lingua's low-accuracy mode, whatlang unrestricted, a
    vote of the two spelling dictionaries, and two "must agree"
    combinations. The dictionary vote came first to mind because the
    dictionaries are loaded anyway; it turned out the best single detector.

15. **Name masking was added after seeing a result.** lingua called an
    English sentence about Østerport and København Danish with 0.998
    confidence. Masking capitalised words and numbers was added as a
    variant to test the explanation; both the plain and masked variants are
    reported, and masking is not in the recommendation.

16. **Spell-checking word rules follow Word's defaults**: skip words with
    numbers, words in UPPERCASE and internet and file addresses (plus
    single letters, options, identifiers with `_`). A word followed by a
    full stop is also tried with it (abbreviations); a Danish word ending
    in `'s` or `'` is accepted if the rest is a word (possessives of names
    and abbreviations). Mixed-case product names (SharePoint, TypeScript)
    are checked, and flagged.

17. **The corpus is taken as correctly spelled**, so every flag counts as
    false. A few are real errors in Wikipedia's text ("kr" without a full
    stop); they were left in rather than edited. The categories of false
    flags in the results are a hand classification, given with examples,
    not exact counts.

18. **Typos are judged word by word.** 113 Danish and 58 English seeded
    misspellings, each checked on its own, with suggestions taken for the
    ones flagged. Real-word errors and split compounds are in the set to
    show the limit, and left out of the "detectable" totals.

19. **"Wrong-language flags"** is the measure of what a wrong language
    costs a user: flags a mode adds over checking each word in its true
    language, with the same dictionaries. It is used to compare checking
    strategies, including the both-dictionaries one the recommendation
    rests on.

20. **The verdict separates the criterion from the outcome.** The
    criterion is about switches and fails. The recommendation (check
    against both dictionaries, use detection only where one language is
    needed) is our reading of the numbers, not something the brief asked
    for; it is drafted as a proposed ADR for a decision.

21. **wasm build choices.** A separate prototype crate (not a workspace
    member), like Spike 3's size probe; `opt-level = 3` with LTO (speed
    numbers matter here, not just size); dictionaries passed in as bytes
    rather than embedded, so code and data sizes are separate; an
    integer-only API without wasm-bindgen; lingua's own wasm-bindgen
    exports stubbed at instantiation because we never call them.

22. **lingua's extra language models are a Cargo feature**
    (`lingua-confusers`), on by default for the measurements and off for
    the wasm build, so the wasm numbers are for Danish and English only.

23. **Suggestion timings are reported per language and as measured**,
    although wasm came out faster than native for Danish (median 136–146
    ms against 238 ms). We did not investigate why; the allocator is a
    guess.

24. **ADR 0009 is drafted as proposed**, including a proposed rule for
    data licences that ADR 0005 lacks. Neither is decided here.
