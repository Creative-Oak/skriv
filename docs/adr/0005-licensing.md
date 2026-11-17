# ADR 0005: MPL-2.0, embeddable in birk, no AGPL in the core

- Status: accepted
- Date: 2026-11-17
- Enforced by: `deny.toml` (`cargo deny check licenses` in CI)

## Context

skriv must work as a standalone open-source product. It must also be
embeddable in **birk**, our European workspace suite (mail, calendar, chat,
files), which is licensed **AGPL-3.0-only**. Public-sector procurement needs
a licence it already knows. The European office-suite field has active
licence fights. OnlyOffice alleged AGPL Section 7 attribution violations
against the Euro-Office fork and suspended its Nextcloud partnership on
31 March 2026; that is the report's account, and we have not verified it
independently.

## Decision

- **skriv is licensed MPL-2.0** (`LICENSE`), with a per-file MPL header on
  source files. This applies to the core crates and the fidelity harness; the
  apps default to it too.
- **No AGPL, GPL or LGPL dependency in the core.**
  - This rules out OnlyOffice, SuperDoc, Vivliostyle, and citeproc-js under AGPL.
  - We prefer MIT, Apache-2.0, MPL-2.0, BSD and Zlib.
  - The allow list lives in `deny.toml`. Anything else fails CI.
- **Every dependency's licence is checked when it is added** and recorded in
  `docs/licences.md`. Dual licences are recorded with the branch we use.
- **Test-time tools are not linked and don't count as dependencies.** That
  covers LibreOffice (MPL-2.0) and poppler's `pdftoppm` (GPL). They are listed
  separately.

### Why MPL-2.0 and not AGPL like birk

MPL-2.0 is file-level copyleft. Section 3.3 explicitly allows a "Larger Work"
that combines MPL code with GPL-family code, including AGPL-3.0. So:

- **birk can depend on skriv.** The combination is distributed under AGPL,
  and skriv's own files stay MPL.
- **The reverse would not hold.** An AGPL core could not be embedded in a
  vendor's or a municipality's non-AGPL product, which would shrink the
  ecosystem around ODF-first editing.
- **It matches LibreOffice**, which public-sector buyers already accept.

## Consequences

- Each new dependency is a licence decision. `cargo deny` makes that a CI failure, not a review comment.
- A future self-hosted sync server may choose AGPL to discourage closed SaaS
  forks, as the report suggests. That is a separate decision for a separate
  component, and it would not affect embedding the core.

## Open decisions

| Question | Settled by |
|---|---|
| Licence of a future sync server (MPL or AGPL) | When it is built (Phase 2) |
| Whether to add per-file SPDX identifiers alongside the MPL header | Before the first public release |
| Font licences for bundled fonts (OFL expected) | Spike 2b, when fonts are chosen |
