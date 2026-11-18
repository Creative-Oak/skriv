# Spike 3: Can the CRDT model tables, sections and tracked changes?

**Risk (report #3):** the CRDT can't model tables, sections or tracked
changes well.

**Experiment.** Compare Loro and Automerge on:

- concurrent edits to a table (row inserts plus cell edits);
- moving sections;
- branch and merge on a 100-page document;
- the size of one year of history.

**Kill / continue (from the report).**
- Blind reviewers find the merge results acceptable.
- Storage stays under about 10× the document size.

**Decides:** Loro or Automerge (ADR 0002 open decision), and how model
properties and origins are encoded.

**Unverified report claims to check first.**
- Loro 1.x stable encoding, movable tree and movable list, and the "10–100×
  faster import" figure.
- Automerge 2.2 block markers.
- Both licences (MIT expected).

**Status:** done except the blind review, 2026-11-17. Results:
[`03-results.md`](03-results.md); judgement calls:
[`03-judgements.md`](03-judgements.md); blind review sheets:
[`03-blind-review/`](03-blind-review/); decision: ADR 0008 (proposed).
Plan: [`02-03-next-spikes-plan.md`](02-03-next-spikes-plan.md).
