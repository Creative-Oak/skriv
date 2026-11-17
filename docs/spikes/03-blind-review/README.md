# Spike 3 blind review

The second kill criterion of Spike 3: *blind reviewers find merge results
acceptable*. This folder holds what reviewers need. Nobody has rated the
outcomes yet.

| File | What it is |
|---|---|
| [`scenarios.md`](scenarios.md) | 21 merge scenarios. Each shows the starting document, what two people did at the same time, and three outcomes, A, B and C, one per CRDT encoding. Lines that differ from the start are marked ▶. |
| `starting-document.odt` | The starting document as a LibreOffice file, for reviewers who want to see it laid out. |
| `key.txt` | Which encoding produced A, B and C in each scenario, ROT13-encoded. Decode only after all ratings are in. |

## Running a review

1. Pick 3–5 reviewers who don't know which libraries are being compared,
   ideally people who write municipal letters for a living. Who sits on
   the panel is still an open decision (see
   [`02-03-next-spikes-plan.md`](../02-03-next-spikes-plan.md)).
2. Give each a copy of `scenarios.md` (or a printout). They tick
   *acceptable* or *not acceptable* for every outcome and may add a note.
3. Collect the sheets, then decode the key:
   `tr 'A-Za-z' 'N-ZA-Mn-za-m' < key.txt`.
4. Count acceptable outcomes per encoding and scenario, and record the
   result in [`../03-results.md`](../03-results.md).

## Regenerating

```bash
cargo run -p skriv-crdt --example scenarios
```

The output is deterministic: the same code gives the same file, key
included. Every outcome is also written as ODT and read back, so none of
them is a file LibreOffice can't open.
