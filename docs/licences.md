# Licence ledger

Policy: [ADR 0005](adr/0005-licensing.md). skriv is MPL-2.0. The core takes no
AGPL, GPL or LGPL dependency. `cargo deny check licenses` enforces the allow
list in `deny.toml` on every CI run, transitive dependencies included.

Checked 2026-11-17 against the crates' published `license` metadata
(`cargo metadata`). Where a crate is dual-licensed, we use it under the
permissive option.

## Direct Rust dependencies

| Crate | Licence | Used by |
|---|---|---|
| `anyhow` 1.0.104 | MIT OR Apache-2.0 | skriv-fidelity |
| `automerge` 0.12.0 | MIT | skriv-crdt (feature `automerge`; Spike 3) |
| `clap` 4.6.8 | MIT OR Apache-2.0 | skriv-fidelity |
| `image` 0.25.10 | MIT OR Apache-2.0 | skriv-fidelity |
| `loro` 1.18.0 | MIT | skriv-crdt (feature `loro`; Spike 3) |
| `proptest` 1.12.0 | MIT OR Apache-2.0 | skriv-odf (dev) |
| `quick-xml` 0.42.0 | MIT | skriv-fidelity, skriv-odf |
| `rayon` 1.12.0 | MIT OR Apache-2.0 | skriv-fidelity |
| `serde` 1.0.229 | MIT OR Apache-2.0 | skriv-crdt, skriv-fidelity, skriv-model |
| `serde_json` 1.0.154 | MIT OR Apache-2.0 | skriv-crdt, skriv-fidelity |
| `thiserror` 2.0.21 | MIT OR Apache-2.0 | skriv-odf |
| `uuid` 1.29.3 | Apache-2.0 OR MIT | skriv-model |
| `walkdir` 2.5.0 | Unlicense/MIT | skriv-fidelity |
| `zip` 9.6.1 | MIT | skriv-fidelity, skriv-odf |

**Transitive:** 247 third-party crates in total (2026-11-17, after Spike 3).
They are all MIT, Apache-2.0, Zlib, Unicode-3.0, BSD-2-Clause, BSD-3-Clause,
CC0, 0BSD, Unlicense, MPL-2.0 or BSL-1.0 (or dual-licensed with one of these);
`cargo deny` passes. Where a crate offers a choice we use the permissive one:

- `r-efi` (pulled in by `getrandom` on UEFI targets) offers "MIT OR
  Apache-2.0 OR LGPL-2.1-or-later"; we use it under MIT.
- `xxhash-rust` 0.8.21 (used by Loro) is **BSL-1.0** only, the Boost
  Software License: permissive, no copyleft, OSI-approved. It was not on
  the allow list; Spike 3 added it to `deny.toml`.
- `imbl` 7.0.2 and `imbl-sized-chunks` 0.2.0 (used by Loro) are
  **MPL-2.0+**, file-level copyleft like skriv itself; already allowed.

## Spike 5 inputs (verified 2026-11-18)

Test-time only, nothing in the product:

- **docx-corpus** (SuperDoc, https://docxcorp.us): the collection is
  ODC-BY 1.0; each document remains its authors' copyright. Files are
  fetched by ID and not committed.
- **ECMA-376 5th edition Transitional XML schemas** (Part 4), fetched from
  ecma-international.org; redistribution terms not checked, so not
  committed.
- **`lxml`** (BSD-3-Clause), Python, for the inventory and validator.

## Spike 4 additions (verified 2026-11-18)

`crates/filemode` adds `base64` 0.22 (MIT OR Apache-2.0) and `similar` 2.7
(Apache-2.0); `cargo deny check licenses` passes. Test-time: `python3-uno`
(LibreOffice's UNO bridge, MPL-2.0) drives LibreOffice over a pipe.

## Spike 3 CRDT candidates (verified 2026-11-17)

Checked against the published crates and npm packages, and the licence
files in the crate sources.

| Library | Rust crate | JS package | Licence |
|---|---|---|---|
| Loro | `loro` 1.18.0 (with `loro-internal`, `loro-common`, `loro-delta`, `loro-kv-store`, `loro_fractional_index` 1.18.0, `loro-rle` 1.6.0: all MIT; `loro-thunderdome` 0.6.2: MIT OR Apache-2.0) | `loro-crdt` 1.18.0 | MIT |
| Automerge | `automerge` 0.12.0 (with `hexane` 1.0.0-alpha.5: MIT) | `@automerge/automerge` 3.5.0 | MIT |

Pulling in both adds 164 crates beyond `skriv-model`'s own (96 "MIT OR
Apache-2.0", 36 MIT, the rest as listed above). Both libraries are usable
under ADR 0005.

## Spike 2 view prototypes (verified 2026-11-18)

`prototypes/view/web`, a standalone pnpm package, not shipped. Licences
from the npm registry metadata.

| Package | Licence | Used for |
|---|---|---|
| `prosemirror-view` 1.42.9, `-state` 1.4.4, `-model` 1.25.13, `-transform` 1.12.6, `-commands` 1.7.2, `-history` 1.5.1, `-keymap` 1.2.3 | MIT | View A |
| `vite` 8.3.8 | MIT | Dev server and build |
| `typescript` 7.0.2 | Apache-2.0 | Type checking |
| `playwright` 1.56.1 | Apache-2.0 | Automated latency and functional runs |
| `wasm-bindgen` 0.2.130 (crate and CLI) | MIT OR Apache-2.0 | The engine's JavaScript bindings |

## Test-time tools (not linked, not shipped)

| Tool | Licence | Used for |
|---|---|---|
| LibreOffice 26.8.1.1 (`soffice`) | MPL-2.0 | Oracle: rendering to PDF, re-saving, generating the corpus |
| poppler `pdftoppm` 26.11 | GPL-2.0-or-later | Rasterising PDFs in the fidelity harness |
| Python 3 standard library | PSF | Corpus and preservation scripts |

## Test data

| Corpus | Licence | In git? |
|---|---|---|
| `fidelity/corpus/generated/` | CC0-1.0; our own content, written by LibreOffice | yes |
| `fidelity/corpus/libreoffice/` | LibreOffice core repository (MPL-2.0); individual files may come from bug-report attachments | no; fetched by script from a pinned commit |
| `fidelity/corpus/municipal/` | per partner agreement | no |

## TypeScript workspace

Only tooling so far: `@biomejs/biome` (MIT OR Apache-2.0), `typescript`
(Apache-2.0), `vitest` (MIT).

## Spike 2b layout candidates (verified 2026-11-18)

Used in `prototypes/layout/` only (its own workspace); not yet in the core.

| Crate | Version | Licence |
|---|---|---|
| `parley`, `parley_engine`, `fontique`, `parlance` | 0.13.0 / 0.1.1 | Apache-2.0 OR MIT |
| `harfrust` | 0.15.0 (0.16.0 is current; Parley pins 0.15) | MIT |
| `skrifa`, `read-fonts` | 0.49 / 0.46 | MIT OR Apache-2.0 |
| ICU4X (`icu_segmenter`, `icu_properties`, …) | 2.3–2.4 | Unicode-3.0 |
| `hypher` | 0.1.8 | MIT OR Apache-2.0 |
| Danish hyphenation patterns (`hyph-utf8` `hyph-da`, Frank Jensen 1994, embedded in `hypher`) | | MIT OR LPPL-1.3+; we use MIT |
| `typst`, `typst-ide`, `typst-layout` (prototype b, not adopted) | 0.15.1 | Apache-2.0; pulls 350 crates, of which `arrayref`, `kamadak-exif` and `mutate_once` are BSD-2-Clause, which `deny.toml` doesn't list |

The own engine's 78 crates all pass `deny.toml`.

**Fonts (prototype, fetched by `prototypes/layout/fetch-fonts.sh`, not
committed):** Liberation Serif and Sans 2.1.5, SIL OFL 1.1. Font licences
are not covered by `cargo deny`; OFL needs a policy line in ADR 0005 before
fonts ship.

**Test-time tools added by Spike 2b:** `hyphen-da` (LibreOffice's Danish
hyphenation patterns, MPL-2.0 per its Debian copyright), Playwright's
Chromium, Node 22.

## Spike 6 proofing (verified 2026-11-18)

Checked against the published crates' `license` metadata and the licence
files in the crate sources, and against the dictionaries' own README and
affix-file headers (LibreOffice `dictionaries` repository, commit
`c0865a85eb67`).

**Rust crates.** `crates/proofing` uses spellbook, lingua and whatlang.
Its 56 normal dependencies are all MIT, Apache-2.0, Zlib, Unlicense OR
MIT, MPL-2.0 or Unicode-3.0; `cargo deny check licenses` passes with no
change to `deny.toml`.

| Crate | Version | Licence | Used? |
|---|---|---|---|
| `spellbook` (Hunspell-format checker, a port of Nuspell) | 0.4.2 | MPL-2.0 | yes |
| `lingua` (lingua-rs), default features off | 1.8.0 | Apache-2.0 | yes (feature `lingua`) |
| `lingua-{danish,english,bokmal,swedish}-language-model` | 1.3.0 | Apache-2.0 | yes |
| `whatlang` | 0.18.0 | MIT | yes (feature `whatlang`) |
| `zspell` | 0.5.5 | Apache-2.0 (no SPDX field: `license-file = "LICENSE"`, the Apache-2.0 text) | **no**: it can't parse `da_DK.aff` and has no compound support (its README says so) |
| lingua's own dependencies: `dashmap` 6.2, `fst` 0.4 (Unlicense/MIT), `include_dir` 0.7 (MIT), `counter` 0.7 (MIT), `fastrand`, `regex`, `strum`, `maplit`, `itertools`, `rayon` (native only) | | MIT or MIT/Apache-2.0 | yes |
| spellbook's: `hashbrown` (MIT OR Apache-2.0), `foldhash` (Zlib) | | | yes |

lingua's Cargo manifest pulls `wasm-bindgen` and `serde-wasm-bindgen`
(both MIT OR Apache-2.0) on wasm targets only; its Python and benchmarking
dependencies (`pyo3`, `polars`, `cld2`) sit behind features we don't enable.

**Dictionaries (data, not linked code; fetched by
`crates/proofing/fetch-dicts.sh`, not committed):**

| Dictionary | Version | Licence (as stated by the files) | Policy note |
|---|---|---|---|
| `da_DK` (Stavekontrolden, Foreningen for frit tilgængelige sprogværktøjer; based on data from Det Danske Sprog- og Litteraturselskab) | 2.9.101, 2026-06-07 | **GPL-2.0 / LGPL-2.1 / MPL-1.1 tri-licence** (README and `.aff` header) | Usable under **MPL-1.1**, file-level copyleft like our MPL-2.0: shipping the files unmodified (or modified, with the changes published) next to MPL-2.0 code is fine. The `.dic` file has three lines spellbook can't parse (`"A/S"`, `"c/o"`, `"I/S"`); we escape them in memory at load time, which modifies nothing on disk. |
| `en_US` (SCOWL, Kevin Atkinson; affix file from Ispell) | 2020.12.07 | SCOWL's permissive licence (MIT-like: use, copy, modify, distribute with the notice); affix file BSD (Ispell); Moby, 12Dicts and Brian Kelk's lists in the public domain; WordNet notice | Permissive. The older `README.txt` in the same folder (2010) still says "LGPL" for the word list; the current `README_en_US.txt` is SCOWL's and says otherwise. Ship `README_en_US.txt` with it. |
| `en_GB` (Marco A.G. Pinto et al.) | 2025 | **LGPL** (word list and affix file) | Data under LGPL. Distributing it unmodified as a separate file a user can replace is the usual reading of LGPL for data, but it is copyleft and ADR 0005 has no rule for data: **needs a decision** before it ships. Measured here for comparison only. |

The `en/` folder's `license.txt` is the GPL-2.0 text; it belongs to the
thesaurus and hyphenation files in that folder, which we don't use.

**ADR 0005 has no policy for data files.** It bans copyleft *dependencies*
of the core. Dictionaries are data loaded at run time, not linked. This
spike does not decide the policy; it records that `da_DK` is usable under
its MPL-1.1 option, that `en_US` is permissive, and that British English
(`en_GB`) is LGPL-only. A proposed rule is in
[ADR 0009](adr/0009-proofing.md).

**LanguageTool** (LGPL-2.1, Java) can't be a dependency of the core under
ADR 0005; it could only ever be an optional, separately installed server.
Its Danish coverage was counted from the rule files on its default branch
(commit `f44f5657`, 2026-11-18): `da/grammar.xml` has **78 pattern rules**
(38 top-level rules and rule groups) and no `style.xml`; the Danish module
adds only generic Java rules (whitespace, double punctuation, unpaired
brackets, sentence-start capital, Hunspell). For comparison: English 5,571 +
535 style (2,476 + 290 top-level), German 4,718 + 512, Dutch 1,916 + 1,573,
Swedish 33. The report's "thin" is right.

**Corpus text.** The handwritten part of the Spike 6 corpus
(`crates/proofing/corpus/handwritten/`, `typos.tsv`) is our own, CC0-1.0.
The Wikipedia part (CC BY-SA 4.0) is not committed: `fetch_wiki.py`
downloads pinned revisions listed with their titles in `wiki-manifest.tsv`
(the attribution), and `build_wiki.py` mixes them locally.

## To verify before adoption (from the report, not checked yet)
- **Spike 2:** ProseMirror.
- **Later:** Hayagriva; Tauri.
- **Spike 5:** SuperDoc's DOCX corpus (docxcorp.us). The file licences matter,
  because SuperDoc itself is AGPL.
