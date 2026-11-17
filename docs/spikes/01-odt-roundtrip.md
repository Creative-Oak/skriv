# Spike 1: ODT round-trip fidelity

**Risk (report #1):** an ODT round trip loses content in real Danish municipal documents.

**Experiment.**
- Build our own ODT importer and exporter for the core subset: styles, lists,
  sections, tables, images, fields, comments, tracked changes.
- Build a corpus: LibreOffice's own test documents, documents we generate,
  and later municipal templates. The report suggests about 200 real documents.
- Build a harness with LibreOffice as the oracle that diffs structure (bytes
  and canonical XML) and pages (rendered images).

**Kill / continue (from the report).**
- At least 99% of untouched elements preserved byte for byte.
- Zero visible loss after a LibreOffice → skriv → LibreOffice round trip on
  the top templates.

**Decision point 1.** If structural fidelity on untouched content falls below
about 95%, lean more on verbatim preservation slots.

**Status:** done for the LibreOffice and generated corpora; municipal corpus
pending. Results: [`01-odt-roundtrip-results.md`](01-odt-roundtrip-results.md).
