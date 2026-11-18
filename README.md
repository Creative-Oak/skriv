# skriv

An open-source, ODF-first word processor. It is the first part of an
open-source replacement for Microsoft 365, built for people who live in Word,
including Danish public-sector staff moving to LibreOffice.

We are not cloning Word. The aim is something narrower and better: true to
ODT, beautiful by default, versioned, local-first and calm.

**skriv is a working name.** Renaming touches crate and package names only.

## Where we are: Phase 0

Phase 0 retires risk; it doesn't build product. There is no editor UI yet.

- **Spike 1 (ODT round-trip fidelity): done.** Results are in
  [docs/spikes/01-odt-roundtrip-results.md](docs/spikes/01-odt-roundtrip-results.md).
- **Spike 2 (paginated editing): provisional.** The owned view (B):
  EditContext in Chrome and Edge, a hidden textarea in Firefox and Safari;
  screen readers and reference hardware not yet tested; see
  [docs/spikes/02-results.md](docs/spikes/02-results.md).
- **Spike 2b (layout engine): done.** Our own engine on Parley; see
  [docs/spikes/02b-results.md](docs/spikes/02b-results.md) (ADR 0007, proposed).
- **Spike 3 (CRDT): done.** Loro with a hybrid encoding; see
  [docs/spikes/03-results.md](docs/spikes/03-results.md) (ADR 0008, proposed).
- **Spike 4 (files through LibreOffice): done.** Returned files become exact,
  attributed suggestion branches; see
  [docs/spikes/04-results.md](docs/spikes/04-results.md).
- **Spike 5 (DOCX): done.** DOCX via LibreOffice at the boundary; skriv
  adds no loss; tracked formatting and paragraph-mark changes are the weak
  spot; see [docs/spikes/05-results.md](docs/spikes/05-results.md).
- **Spike 6 (proofing mixed Danish and English): done.** Automatic language
  switching fails the criterion (5.1 false switches per 10 pages); checking
  every word against both dictionaries works; see
  [docs/spikes/06-results.md](docs/spikes/06-results.md) (ADR 0009, proposed).
- **Next:** Spike 2's deferred manual runs (screen readers, phones,
  Windows, reference hardware), then decide the proposed ADRs and leave
  Phase 0; see [docs/PLAN.md](docs/PLAN.md).
- **Decisions:** [docs/adr/](docs/adr/). **Progress:** [docs/PLAN.md](docs/PLAN.md).

## Layout

```
crates/model      the semantic document model: ODF-shaped, stable IDs, preservation slots
crates/odf        ODT reader/writer: projection with preservation
crates/fidelity   round-trip fidelity harness, with LibreOffice as oracle (CLI: skriv-fidelity)
crates/crdt       Spike 3: the model in Loro and Automerge behind one trait
crates/proofing   Spike 6: language per paragraph and spell checking (spellbook, da_DK, en_US)
prototypes/       spike prototypes outside the workspace (crdt-wasm-size, proofing-wasm)
packages/         TypeScript (empty: tooling only, for the web UI and @skriv/* WASM packages)
fidelity/         corpus, scripts, reports
docs/adr          architecture decision records
docs/spikes       spike briefs and results
prototypes/layout Spike 2b: the layout contract, our own paginator and a Typst kernel
```

Reserved for later: `crates/layout`, `crates/docx`.

## Running it

You need Rust (pinned in `rust-toolchain.toml`), and for the fidelity
harness, LibreOffice and poppler (`pdftoppm`).

```bash
cargo test --workspace
```

```bash
fidelity/scripts/fetch-lo-corpus.sh
```

```bash
cargo run --release -p skriv-fidelity -- run fidelity/corpus/libreoffice fidelity/corpus/generated --canary
```

The report is written to `fidelity/reports/latest/index.html`, alongside
`summary.md` and `report.json`.

```bash
python3 fidelity/scripts/lo_preservation.py
```

```bash
python3 fidelity/scripts/generate_corpus.py
```

The first re-runs the LibreOffice preservation experiment. The second
rebuilds the generated corpus through LibreOffice.

## Embedding in birk

The core is MPL-2.0, which permits combination with AGPL code (MPL §3.3), so
birk can depend on it. The plan is to build the Rust core to WASM and publish
`@skriv/*` packages that birk's pnpm workspace consumes. See
[ADR 0005](docs/adr/0005-licensing.md).

## Licence

[MPL-2.0](LICENSE). Dependencies are checked by `cargo deny` and recorded in
[docs/licences.md](docs/licences.md).
