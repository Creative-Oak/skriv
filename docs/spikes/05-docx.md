# Spike 5: DOCX fidelity

**Risk (report #5):** DOCX fidelity, especially numbering, fields, floats and
tracked changes.

**Experiment.**
- Import and export a public DOCX corpus. The report suggests SuperDoc's
  docxcorp.us; its licence must be checked file by file, because SuperDoc
  itself is AGPL.
- Validate output with the Open XML SDK (MIT).
- Open the results in Word and in LibreOffice.

**Kill / continue (from the report).**
- No corrupt files.
- The fidelity report lists every downgrade.

**Notes from Spike 1.** The harness already scores per feature and has an
oracle loop. A DOCX mode needs a second canonicaliser for OOXML and the
`fldChar`, `numbering.xml` and `w:rPrChange` mappings.

**Status:** not started.
