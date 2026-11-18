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
| `clap` 4.6.8 | MIT OR Apache-2.0 | skriv-fidelity |
| `image` 0.25.10 | MIT OR Apache-2.0 | skriv-fidelity |
| `proptest` 1.12.0 | MIT OR Apache-2.0 | skriv-odf (dev) |
| `quick-xml` 0.42.0 | MIT | skriv-fidelity, skriv-odf |
| `rayon` 1.12.0 | MIT OR Apache-2.0 | skriv-fidelity |
| `serde` 1.0.229 | MIT OR Apache-2.0 | skriv-fidelity, skriv-model |
| `serde_json` 1.0.154 | MIT OR Apache-2.0 | skriv-fidelity |
| `thiserror` 2.0.21 | MIT OR Apache-2.0 | skriv-odf |
| `uuid` 1.29.3 | Apache-2.0 OR MIT | skriv-model |
| `walkdir` 2.5.0 | Unlicense/MIT | skriv-fidelity |
| `zip` 9.6.1 | MIT | skriv-fidelity, skriv-odf |

**Transitive:** 117 third-party crates in total. They are all MIT, Apache-2.0,
Zlib, Unicode-3.0, BSD-3-Clause, CC0 or Unlicense (or dual-licensed with one of
these); `cargo deny` passes. One crate, `r-efi` (pulled in by `getrandom`
on UEFI targets), offers "MIT OR Apache-2.0 OR LGPL-2.1-or-later"; we use it
under MIT.

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

## To verify before adoption (from the report, not checked yet)
- **Spike 3:** Loro, Automerge.
- **Spike 2:** ProseMirror.
- **Spike 6:** zspell, lingua-rs, whatlang; the `da_DK` Hunspell dictionary.
- **Later:** Hayagriva; Tauri.
- **Spike 5:** SuperDoc's DOCX corpus (docxcorp.us). The file licences matter,
  because SuperDoc itself is AGPL.
