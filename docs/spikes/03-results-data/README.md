# Spike 3 raw output

Unedited output of the runs behind [`../03-results.md`](../03-results.md),
2026-11-17, on a 4-core Linux container.

| File | From |
|---|---|
| `history-loro-automerge.md` | `spike3 history 250 400` for `loro` and `automerge`. The run hit a 30-minute limit before Automerge's 100-page case and has no `time` footer; that case was re-run separately. "steady size" in these files is the *revising* mix of the results. |
| `history-automerge-100pages.md` | Automerge's 100-page year, re-run on its own. |
| `history-loro-flow.md` | All three year traces for the hybrid encoding. |
| `fuzz-10000-*.txt` | `SKRIV_FUZZ_RUNS=10000 cargo test --release --test tables` |
| `sections-branches-debug.txt` | `cargo test --test sections --test branches` (debug build, before the hybrid existed). Release merge timings are in the results. |
| `wasm-size.txt` | `prototypes/crdt-wasm-size/measure.sh` |
