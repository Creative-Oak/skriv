# Real-world public-sector documents (not in git)

Put real public-sector documents here (Danish municipal ones ideally; EU
documents exported from Word are here now), or anywhere else, and
point the harness at them:

```bash
cargo run --release -p skriv-fidelity -- run fidelity/corpus/municipal --out fidelity/reports/municipal
```

Everything in this directory except this README is ignored by git, because
real documents may contain personal data or be covered by confidentiality.
Before a file is committed anywhere, record its source and licence (or the
partner's written permission) in a manifest next to it, the way
`fidelity/corpus/libreoffice/manifest.toml` does.
