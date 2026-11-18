# Spike 6 results: proofing documents that mix Danish and English

- **Date:** 2026-11-18.
- **Brief:** [`06-proofing.md`](06-proofing.md).
- **Code:** `crates/proofing` (new), `prototypes/proofing-wasm` (new).
- **Judgement calls:** [`06-judgements.md`](06-judgements.md). **Raw
  output:** [`06-results-data/`](06-results-data/). **Licences:**
  [`docs/licences.md`](../licences.md), "Spike 6". **Proposed ADR:**
  [ADR 0009](../adr/0009-proofing.md).
- **Reproduce:**
  `crates/proofing/fetch-dicts.sh`,
  `python3 crates/proofing/corpus/fetch_wiki.py && python3 crates/proofing/corpus/build_wiki.py`,
  then `cargo run --release -p skriv-proofing --bin spike6 -- <out-dir>`
  (about 40 s). wasm: `prototypes/proofing-wasm/measure.sh`.
  `cargo test -p skriv-proofing` runs without downloads; the spelling tests
  skip when the dictionaries are absent.

## Verdict

**The kill criterion fails for automatic language switching.** On a
41-page corpus of mixed Danish and English, the best detector with
hysteresis makes **5.1 false switches per 10 pages**, five times the limit.
Even a policy tuned on the very data it is scored on gets no lower than
3.6.

**The failure is confined to short paragraphs.** Where the language
changes at a normal paragraph (the 28 Wikipedia-based pages, 70 real
switches), three detectors make **0 false switches** and lingua makes 0.7
per 10 pages. The errors are in the 12.7 handwritten pages: a newsletter
that alternates Danish and English paragraph by paragraph, short headings
("Flu vaccination", "Krav til drift"), mail headers and signatures. Several
of these are ambiguous to a person too.

**Spell checking mixed documents still works, if it doesn't depend on the
switch.** Checking every word against Danish *and* English (accept a word
if either dictionary knows it) removes every wrong-language flag and halves
all false flags (20.5 → 10.3 per 1,000 words). It costs 2 of 95 detectable
Danish typos and 3 of 53 English ones in our seeded set.

**Recommendation: continue proofing, but drop automatic switching as the
mechanism.** Check against both dictionaries; use detection (with
hysteresis) only for what needs one language (suggestions, hyphenation, the
`fo:language` written to the file), and let an explicit paragraph or style
language override it. This needs a decision; see ADR 0009 (proposed).

| Kill criterion: "fewer than 1 false language switch per 10 pages" | Result (41.2 pages, 676 paragraphs, 142 real switches) |
|---|---|
| Best detector + hysteresis (lingua with names masked, agreeing with a dictionary vote) | **5.1 per 10 pages** (21: 5 spurious, 16 missed). **Fails.** |
| Same, on the held-out half (policy tuned on the other half) | 10.5 per 10 pages |
| Same, best policy tuned on all data (optimistic bound) | 4.1; best of all detectors 3.6 |
| lingua (Danish + English) + hysteresis | 5.6 (raw, no hysteresis: 9.0) |
| whatlang (Danish + English) + hysteresis | 5.8 (raw: 25.0) |
| Wikipedia part only (28.5 pages, long paragraphs) | **0.0** (whatlang, dictionary vote, the two combinations); 0.7 (lingua). **Passes.** |
| Handwritten part only (12.7 pages, short fragments) | 16.5–17.3 for the best four |
| Paragraphs of 20+ letters only | 1.0 (dictionary vote) to 2.4 (whatlang) |
| Per 100 real switches | 15 false switches (best) |

| Spelling | Result |
|---|---|
| False flags on correct text, each word in its true language | Danish 85 / 8,237 words (10.3 per 1,000); English (en_US) 201 / 5,731 (35 per 1,000; en_GB 27 per 1,000) |
| Same, both dictionaries for every word | **10.3 per 1,000 words overall**, 0 wrong-language flags |
| Danish words of 15+ letters (compounds) flagged | 8 of 285 (2.8%) |
| Seeded typos caught (detectable classes) | Danish 88 / 95 (93%), English 53 / 53 (100%); with both dictionaries 86 / 95 and 50 / 53 |
| Real-word errors and split compounds ("bygge tilladelse") | 0 of 23 caught (expected: a word checker can't) |
| Right word in the top 3 suggestions | Danish 71 / 88 (81%), English 53 / 53 (100%) |
| Check speed | ~90,000 words/s native and in wasm (Node, Chromium) |
| Re-check a paragraph per keystroke (check + lingua) | median paragraph 0.06–0.1 ms; 95th-percentile paragraph (290 chars) 1.5–1.7 ms in wasm |
| Suggestions, Danish | median 140–240 ms, worst 1.5–3.8 s per word (wasm faster than native here). English: median 13–24 ms |
| wasm code, gzip | checker 124 KB; + whatlang 66 KB; **+ lingua (Danish + English models only) 6.2 MB** |
| Dictionaries, gzip | da_DK 911 KB, en_US 192 KB (en_GB 442 KB) |
| Load in wasm (Chromium) | da_DK 116 ms, en_US 9 ms; 27 MB of wasm memory with lingua |

## What was built

**`crates/proofing`** (MPL-2.0, workspace member):

- `detect`: one `Detector` trait, with **lingua** (Danish and English;
  optionally Norwegian Bokmål and Swedish as confusers; optionally the low
  accuracy mode), **whatlang** (restricted to Danish and English, or
  unrestricted), a **dictionary vote** (words only Danish knows against
  words only English knows) and **`Agree`** (two detectors must agree).
  Any n-gram detector can run on the paragraph with names masked
  (capitalised words not starting a sentence, and words with numbers).
- `assign`: a language per paragraph. Raw: each paragraph takes its own
  detection. Hysteresis: a running language that a paragraph of at least
  `min_letters` letters switches only with confidence ≥ `switch_confidence`;
  shorter paragraphs inherit it (or, optionally, take their own detection
  above a stricter bar without moving the running language); short headings
  take the language of the next paragraph. Only the heading rule looks
  ahead, so an edit changes languages only from the edited paragraph on.
- `spell`: a Hunspell-format checker on **spellbook** 0.4.2 (a Rust port of
  Nuspell). Word rules as in Word's defaults: words with numbers, words in
  UPPERCASE, single letters, and e-mail addresses, URLs, paths, file names
  and options are not checked; abbreviations are tried with their full
  stop ("f.eks.", "jf."); Danish possessives of names ("KL's").
- `text`: words, sentence-and-quote segments, name masking.
- `corpus`: the labelled format and the seeded typos;
  `src/bin/spike6.rs`: every measurement here.
- `fetch-dicts.sh` downloads `da_DK`, `en_US` and `en_GB` from
  LibreOffice's dictionaries repository at a pinned commit, checks their
  SHA-256 and keeps them out of git.

**`prototypes/proofing-wasm`**: the crate as a `cdylib` for
`wasm32-unknown-unknown` with an integer-only API (dictionaries are passed
in as bytes, not embedded), `bench.mjs` for Node and `chromium.cjs` for
headless Chromium 157 through Playwright.

**zspell was not used.** It fails on `da_DK.aff` ("error parsing flag of
type 'num'") and its README lists compound handling as unsupported, which
rules it out for Danish whatever the parse error. spellbook loads both
dictionaries once three lines of `da_DK.dic` are escaped in memory
(`"A/S"`, `"c/o"`, `"I/S"`: a bare slash, which Hunspell skips over).

## The corpus

**41.2 pages** (103,002 characters, a page being 2,500), 23 documents, 676
paragraphs, each labelled with its language. 142 real switches between
paragraphs that have words to check.

- **Handwritten, 12.7 pages, 13 documents** (`corpus/handwritten/`, CC0):
  a building permit with an English quote from the contractor; meeting
  minutes with Danish IT jargon, a risk table and an English vendor update;
  a project description with an English abstract and evaluation plan; tender
  requirements whose table alternates Danish requirements and English
  answers; a data-protection memo quoting English contract clauses; a
  bilingual staff newsletter (Danish and English paragraphs alternate); a
  technical guide with commands, paths, configuration keys and English
  error messages; a Danish reply above an English mail thread; a job
  advert in Danish then English; a climate report with an English
  reference list; an English EU application with a Danish cover note and
  appendix; workshop notes full of English loanwords and quoted English
  sentences; a contact list and supplier table. 72 real switches, 92
  English paragraphs, 10 inline English passages marked inside Danish
  paragraphs.
- **Wikipedia-based, 28.5 pages, 10 documents**: Danish articles (the
  2007 municipal reform, Folketinget, the Copenhagen Metro, the Great Belt
  link, Statistics Denmark and KL, the Danish language, the folkeskole and
  Region Midtjylland, Aarhus Kommune) with "islands" of 1–6 paragraphs from
  English articles on related topics inserted, some with an English
  heading, some documents opening with an English abstract; and two
  English-based documents (GDPR; OpenDocument and LibreOffice) with Danish
  islands from the Danish constitution article. 70 real switches.
  Pinned revisions (`wiki-manifest.tsv`); fetched, not committed (CC BY-SA).

A paragraph's label is the language a careful person would set for it.
Language-neutral fragments (names, numbers, addresses) take the language
of their surroundings. **False switches are counted over paragraphs that
have at least one word the checker checks**: "K-01" or "20 11 34 56" get no
spell checking whatever language they are given. Counted over every
paragraph, the best result is 7.5 per 10 pages instead of 5.1.

A false switch is a switch the prediction has and the labels don't, or one
the labels have and the prediction doesn't, both counted from the document's
default language. One wrong paragraph inside a run is two false switches; a
switch found one paragraph late is two.

## Language per paragraph

Full table: [`06-results-data/detection.md`](06-results-data/detection.md);
every remaining error, per detector:
[`detection-errors.txt`](06-results-data/detection-errors.txt).

The policy is chosen on half of the documents (odd positions in name order)
and reported on all of them and on the other half.

| Detector | Raw, per 10 pages | Hysteresis (tuned on dev), per 10 pages | Held-out half | Handwritten | Wikipedia | Accuracy | Native µs per paragraph |
|---|---|---|---|---|---|---|---|
| lingua, Danish + English | 9.0 | 5.6 | 11.5 | 16.5 | 0.7 | 97.6% | 92 |
| lingua, names masked | 8.0 | 7.5 | 14.5 | 22.7 | 0.7 | 95.9% | 60 |
| lingua + Bokmål + Swedish | 10.9 | 8.0 | 15.5 | 21.2 | 2.1 | 95.9% | 123 |
| lingua, low accuracy mode | 21.6 | 9.0 | 17.5 | 22.7 | 2.8 | 95.1% | 83 |
| whatlang, Danish + English | 25.0 | 5.8 | 11.0 | 18.8 | 0.0 | 97.6% | 11 |
| whatlang, all languages | 22.1 | 10.0 | 18.5 | 23.5 | 3.9 | 94.3% | 58 |
| dictionary vote | 6.8 | 5.3 | 10.0 | 17.3 | 0.0 | 98.1% | 384 |
| lingua (masked) & dictionary vote agree | 7.5 | **5.1** | 10.5 | 16.5 | 0.0 | 98.0% | 536 |
| whatlang & dictionary vote agree | 8.3 | 5.3 | 11.0 | 17.3 | 0.0 | 98.0% | 463 |

What the numbers say:

- **Long paragraphs are solved.** Paragraphs of 60+ letters are 98–100%
  right for every detector restricted to Danish and English, without
  hysteresis.
- **Short paragraphs are the problem.** Under 20 letters, detectors alone
  get 61–79% right; inheriting (hysteresis) raises that to 91–93%, and the
  rest are the false switches. Every tuned policy is "under 20 (or 40)
  letters, inherit; short headings, look ahead".
- **Hysteresis matters most for whatlang** (25.0 → 5.8): its raw
  confidence on short text is low and noisy, and the tuned threshold is
  only 0.5. For lingua it cuts them by about 40%.
- **Confusers hurt.** Adding Norwegian and Swedish to lingua, or letting
  whatlang pick any language, makes Danish short paragraphs come out
  "Norwegian" and lose the switch. Restrict detectors to the languages you
  proof.
- **lingua reads Danish names as Danish.** "It branches off the M3 line at
  Østerport in the north and at København H in the south." is Danish with
  confidence 0.998: two words with ø/æ/å outweigh fourteen English ones.
  English text about Danish places is what a Danish municipality writes.
  Masking capitalised words fixes this case and helps raw detection (9.0 →
  8.0) but hurts with hysteresis (5.6 → 7.5), because it leaves short
  paragraphs with almost nothing to read. Agreeing with the dictionary
  vote is more robust.
- **The dictionary vote is the best single detector here**, and needs no
  model at all, just the two spelling dictionaries already loaded. It is
  slow on long paragraphs (each word is looked up in both, and Danish
  compound search on English words is expensive: 4.6 ms for the longest
  paragraph, 1,716 characters).
- **The held-out half is worse than the tuned half** for every detector
  (10.0–18.5 per 10 pages against 0.0–1.9): the newsletter, mail thread and requirements table
  all fell into the held-out half. The corpus is small enough that the
  split matters; read the "all" column as the estimate and the held-out
  column as the warning.

Paragraphs that remain wrong with the best policy (13 paragraphs, 21
false switches, all in the handwritten part): Danish short headings after
an English table answer ("Krav til sikkerhed", "Krav til drift"); the
newsletter's alternating headings ("Influenzavaccination" / "Flu
vaccination", "Christmas lunch", "Questions?") and its first and last
English lines ("News from HR – November 2026", "Write to hr@kommune.dk or
call 89 40 10 00."); mail headers ("From: Claire Dubois", "Sent: 30 October
2026 14:12", "Emne: SV: Study visit to the citizen service centre"); a
signature line ("Rasmus Holt"); and "Litteratur" above an English
reference list.

**Sentence level.** Splitting paragraphs into sentences and quotations and
re-detecting segments of 15+ letters (lingua, confidence ≥ 0.9) cut the
wrong-language spelling flags only from 43 to 39: most
quoted English inside Danish paragraphs is a few words ("the Services",
"room for the river").

## Spelling

Full lists: [`06-results-data/spelling.md`](06-results-data/spelling.md).
The corpus is taken to be correctly spelled, so every flag is a false flag
(some are real errors in Wikipedia's text or Wikipedia quoting Old Norse).
"Wrong-language flags" are flags a mode adds over checking each word in
its true language.

| How each word is checked | Flags | per 1,000 words | Wrong-language flags, per 10 pages |
|---|---|---|---|
| In its true language (oracle, word level) | 286 | 20.5 | 0 |
| In its paragraph's true language (oracle, paragraph level) | 316 | 22.6 | 7.3 |
| In the predicted paragraph language | 327 | 23.4 | 10.4 |
| … plus sentence and quote segments | 314 | 22.5 | 9.5 |
| … plus both dictionaries for capitalised words and paragraphs under 20 letters | 203 | 14.5 | 4.1 |
| **Both dictionaries for every word** | **144** | **10.3** | **0** |
| en_GB instead of en_US, true language | 242 | 17.3 | 0 |

13,968 words were checked; 954 with numbers, 540 single letters, 152 in
capitals and 103 in addresses were skipped.

**What the false flags are** (hand classification of the oracle list):

- **Names**, by far the largest group: Danish places in English text
  (Østerport, Dragør, Høje-Taastrup, Ørestad), people (Thorning-Schmidt,
  Vestager), companies and products (Collabora, SharePoint, TypeScript,
  Kubernetes, OpenDocument ×9). `da_DK` knows many Danish place names, so
  checking against both dictionaries accepts most of them.
- **British spellings in en_US**: centre ×6, programme ×6, councillors,
  licence, organise, recognised, harbours, neighbour, travelling. Danish
  organisations mostly write British English. **en_GB fixes these but is
  LGPL** (see licences); en_US is the only permissive English dictionary
  we found.
- **English loanwords in Danish text** that weren't marked as English:
  scope ×4, backlog, dashboard, onboarding, stakeholders, product owner,
  code review. Both dictionaries accept them.
- **Danish words the dictionary lacks**: servicedesken ×3,
  gruppepolitikken, bogtitelen, bondevennerne, Interreg-programmet,
  Kubernetes-platform, UX-designer; and "kr" without its full stop (×3, a
  real error in the source).
- **Commands and Old Norse/Latin/Greenlandic quotations**: systemctl,
  journalctl, gpupdate; konungr, Proconsule, inatsit.

**Compounds.** `da_DK` (COMPOUNDMIN 2, up to 10 parts) accepted 277 of
285 Danish words of 15+ letters in the corpus (97.2%), including
"affaldsindsamlingsordningen" and "medarbejderudviklingssamtale". All 10
seeded linking errors ("arbejdmiljø", "forsyningselskab") and all 15
errors inside compounds were caught. The price is false acceptance: of 7
Danish typos missed, 5 are accepted as compounds of real words
("tidspunk" = tids + punk, "eksemel" = eksem + el, "bikag", "desvære",
"kontak") and 2 are real words ("samen", "losning"). Danish compounding
even accepts English typos: "succesful" (succes + ful), "refered",
"tounge".

**Seeded typos** ([`typos.md`](06-results-data/typos.md),
[`typos.txt`](06-results-data/typos.txt)): 113 Danish (double consonants,
swaps, æ/ø/å written as ae/oe/aa or a/o, dropped and extra letters,
neighbouring keys, errors inside compounds, linking s/e, inflection,
real-word errors, split compounds) and 58 English.

| | Danish | English |
|---|---|---|
| Caught, own dictionary (detectable classes) | 88 / 95 (93%) | 53 / 53 (100%) |
| Caught, both dictionaries | 86 / 95 (91%): "born" (børn) and "gar" (går) are English words | 50 / 53 (94%): three accepted by Danish compounding |
| Right word first | 52 / 88 (59%) | 47 / 53 (89%) |
| Right word in top 3 | 71 / 88 (81%) | 53 / 53 (100%) |
| Real-word errors, split compounds | 0 / 18 | 0 / 5 |

Danish suggestions are good for compounds (15 / 15 in the top 3) and weak
for æ/ø/å written as ascii (6 / 12) and dropped letters (6 / 9).

**Speed.**

- Loading: da_DK 86–104 ms native, 104–146 ms in wasm; en_US 8–11 ms.
- Checking: ~90,000 words/s native, 83,000–97,000 in wasm (Node 22,
  Chromium 157). A known word costs under 10 µs. An unknown word or a long
  compound costs far more: 40 µs for "servicedesken", 0.9 ms for
  "affaldsindsamlingsordningen". A 600-character paragraph re-checks in
  about 0.5 ms; per keystroke, the median paragraph (33–51 characters)
  re-checks and re-detects in 0.06–0.1 ms, a 290-character one in about
  1.7 ms (wasm). Caching results per word would make this negligible.
- **Suggestions are slow for Danish**: median 238 ms native and 136–146 ms
  in wasm, worst 1.5–3.8 s per word (spellbook's suggester trying
  compounds). English: 13–24 ms. Suggestions must be asynchronous (a
  worker), on demand, and cancellable.

## wasm32

Built with `opt-level = 3`, LTO, `panic = "abort"`, stripped; sizes
gzip -9 ([`06-results-data/wasm/`](06-results-data/wasm/)).

| What | Raw | gzip |
|---|---|---|
| Checker only (spellbook + the crate) | 381 KB | **124 KB** |
| + whatlang | 639 KB | 190 KB |
| + lingua, Danish and English models only | 12.2 MB | **6.36 MB** |
| + lingua + whatlang | 12.4 MB | 6.43 MB |
| `da_DK.aff` + `.dic` (loaded at run time) | 3.98 MB | **911 KB** |
| `en_US.aff` + `.dic` | 555 KB | 192 KB |
| `en_GB.aff` + `.dic` (comparison) | 1.27 MB | 442 KB |

- **lingua's models can be limited to Danish and English** (Cargo features;
  every other language model stays out), but those two are still 6.2 MB
  gzipped, compiled into the binary. With Norwegian and Swedish added
  the binary was 23 MB raw. The low-accuracy mode uses fewer n-grams at run
  time but embeds the same files.
- lingua's crate exports `wasm-bindgen` functions on wasm targets; our
  integer API doesn't use them, so the page stubs those imports.
- Timings in Node 22.23 and headless Chromium 157: compile and instantiate
  18–24 ms; first lingua call 21–31 ms (it loads its models lazily); lingua
  90–115 µs per paragraph, whatlang 14–19 µs, dictionary vote 104–125 µs.
  wasm memory is 27 MB after loading both dictionaries with lingua built in.

**A web download of 0.12 MB of code plus 1.1 MB of dictionaries** gives
Danish and English spell checking and the dictionary-vote detector. lingua
would add 6.2 MB for a detector that is no better on this corpus.

## LanguageTool

Its Danish module has **78 pattern rules** (38 top-level rules and rule
groups) and no style rules, against 6,106 for English and 5,230 for German;
the rest of its Danish checking is generic (whitespace, punctuation,
capitals) plus Hunspell. The report's "thin" is right. LanguageTool is
LGPL-2.1 Java: it can't be a dependency of the core (ADR 0005), only an
optional separate server, and for Danish it would add little.
[`languagetool-rules.txt`](06-results-data/languagetool-rules.txt).

## Limitations

- **The corpus is switch-dense on purpose**: 3.4 real switches per page,
  and the newsletter alone has 21 in 0.7 pages. A Danish report with one
  English abstract has far fewer, and its false switches per page would be
  lower at the same error rate per switch (15 per 100 real switches here).
  We have no real municipal documents to measure the actual density.
- **One person wrote and labelled the handwritten part**, the same person
  who built the detectors. One label was corrected after the first run (an
  English mail subject line marked Danish; 06-judgements.md 9). The
  Wikipedia part is mixed mechanically and labelled by source.
- **Spelling false flags assume the corpus is correct.** A few flags are
  real errors in Wikipedia's text; they were not removed.
- **Typos are isolated words**, not typed into running text; real
  typing errors were not collected. Suggestion quality is top-3 inclusion,
  not a ranking judged by people.
- **Not tested:** grammar checking; hyphenation; how Word or LibreOffice
  assign languages on the same corpus; paragraph languages already present
  in real ODT files (`fo:language` on styles); spell checking inside
  fields, footnotes and tables of a real document; memory on low-end
  devices; Safari and Firefox; a Web Worker.
- spellbook calls itself alpha and is tested mostly on `en_US`. On
  `da_DK` it behaved sensibly in everything measured here, but we did not
  compare it with Hunspell itself word by word.

## Answers for ADRs

- **Proofing (ADR 0009, proposed):** spellbook with `da_DK` and `en_US`;
  check against both dictionaries by default; paragraph language from the
  document and styles first, detection with hysteresis second, never
  switching on short paragraphs; the dictionary vote as the detector
  (lingua and whatlang are not needed); suggestions asynchronous; no
  LanguageTool in the core.
- **ADR 0005 (licensing):** needs a rule for **data files**. `da_DK` is
  usable under its MPL-1.1 option; `en_US` is permissive; `en_GB` is LGPL
  only. Without a rule, British English can't ship.
- **ADR 0004 (DOM rendering):** spelling marks are per word and cheap to
  recompute per paragraph; nothing here needs the canvas.
