# Spike 4: Does CRDT history survive files going through LibreOffice or email?

**Risk (report #4):** CRDT state and history are lost when files go through
LibreOffice or email.

**Experiment.**
- Embed a snapshot and our IDs in the `.odt`.
- Round-trip the file through LibreOffice.
- Test the import that diffs a returned file into a branch.

**Kill / continue (from the report).** Returned copies become attributed
suggestion branches with under 5% spurious changes.

**Status:** run 2026-11-18 on LibreOffice 24.2: **continue**. Results:
[`04-results.md`](04-results.md); judgement calls:
[`04-judgements.md`](04-judgements.md). Pre-work done in Spike 1. LibreOffice 26.8 was tested for what it
keeps; see [`04-libreoffice-preservation-findings.md`](04-libreoffice-preservation-findings.md).
The findings change the report's proposed design: an extra package part does
**not** survive, but an RDF metadata file does.
