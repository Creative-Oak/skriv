# Contributing

- **Licence.** skriv is MPL-2.0. Every source file starts with the MPL header:

  ```
  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at https://mozilla.org/MPL/2.0/.
  ```

- **Dependencies.** Check a dependency's exact licence before adding it. Record
  it in `docs/licences.md`. `cargo deny check licenses` must pass. Never add
  AGPL, GPL or LGPL code to the core ([ADR 0005](docs/adr/0005-licensing.md)).
- **The model stays independent.** `crates/model` must not depend on XML, zip,
  or any editor or UI framework ([ADR 0002](docs/adr/0002-own-odf-shaped-model.md)).
- **Fidelity is a product asset.** A change to `crates/odf` needs tests, and
  the fidelity harness must still pass on the generated corpus:

  ```bash
  cargo run --release -p skriv-fidelity -- run fidelity/corpus/generated --canary --enforce
  ```

- **Decisions.** A decision that changes architecture gets an ADR. A claim
  from the research report that turns out wrong is corrected where it was
  relied on, with the evidence.
