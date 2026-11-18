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

## To verify before adoption (from the report, not checked yet)

- **Spike 2b:** Parley, HarfRust, Skrifa, Fontique, ICU4X, `hypher`;
  `hyph-utf8` patterns (per-language licences, Danish included); bundled
  fonts (OFL expected).
- **Spike 2:** ProseMirror.
- **Spike 6:** zspell, lingua-rs, whatlang; the `da_DK` Hunspell dictionary.
- **Later:** Hayagriva; Tauri.
- **Spike 5:** SuperDoc's DOCX corpus (docxcorp.us). The file licences matter,
  because SuperDoc itself is AGPL.
