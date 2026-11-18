# Spike 4: judgement calls

Decisions made while running Spike 4 without asking, so they can be
reviewed. Results: [`04-results.md`](04-results.md).

1. **LibreOffice 24.2, not 26.8.** The cloud container has Ubuntu's 24.2.7.2
   and installing other software was out of bounds. The pre-work's
   preservation probes were re-run on 24.2 and agree with 26.8 (one
   difference: an unlisted extra part inside a file with many probes was
   dropped rather than refused). The round trips should be repeated on
   26.8 and on the 25.x versions Danish municipalities run.

2. **UNO over a pipe from the system Python.** Ubuntu's LibreOffice has no
   Python script provider, so the pre-work's macro route silently did
   nothing. `fidelity/scripts/uno/lo_pipe.py` starts `soffice --accept=pipe`
   and drives it with `python3-uno`; `lo_preservation.py` falls back to it.

3. **The default snapshot in the file is shallow.** A full snapshot gives
   any recipient every deleted sentence (verified: the test's deleted
   sentence was recoverable from every full snapshot and from no shallow
   one). A shallow snapshot is enough to make a branch that merges back.
   Full history should only be written on request; "ID and version only"
   is the right choice for files that stay inside one skriv installation.

4. **The version goes in `meta.xml` as well as the RDF part.** Added after
   the DOCX run showed `meta:user-defined` survives a trip through Word
   format and the RDF part doesn't.

5. **A change unit** is one character inserted or deleted, one character's
   formatting per property, one property, or one block, row or column
   added, removed or moved (a node that moved to another parent counts once,
   plus whatever changed in it). "Spurious" is measured two ways: against
   the returned file (import errors) and against the same edits made in
   skriv. The second includes places where LibreOffice and skriv simply
   behave differently; those are reported, not hidden.

6. **Edits are described by unique snippets of text**, so LibreOffice (UNO
   search) and skriv agree where they go. This limits targets to
   paragraphs with unique wording; the 20- and 100-page letters repeat
   their text and only get no-edit round trips.

7. **Two scenario definitions were corrected to match LibreOffice**, after
   comparing results:
   - Enter at the end of a list item that has a sub-list: LibreOffice
     moves the sub-list into the new item (so the new line appears right
     under the old text). skriv's re-enactment now does the same; this is
     what Word does too.
   - Copy/cut and paste of whole paragraphs aims only at paragraphs
     followed by another paragraph in the same container, because pasting
     at the start of the next paragraph lands outside a section otherwise.

8. **Noise rules are listed, not inferred.** Each rule in `noise.rs` was
   added after a no-edit round trip produced the difference. Ignoring the
   automatic style *names* of sections, table cells and columns hides real
   changes to those styles; that is a codec gap (it should resolve them like
   paragraph styles), recorded as a limitation.

9. **Two levels of normalisation.** Comparison may rewrite text (spaces
   for `<text:s/>`); the import may not, or every untouched paragraph
   would be churned. The import only strips layout output and otherwise
   compares through equivalences, writing nothing when values are
   equivalent.

10. **The importer accepts LibreOffice's new tracked changes into the
    branch** rather than keeping them as ODF tracked changes: in skriv a
    suggestion is a branch. Changes the document already had are kept. They
    are told apart by kind, author and date, because LibreOffice renumbers
    change IDs; their IDs are then mapped back to skriv's.

11. **Paragraph text is diffed by words, with diff-match-patch's semantic
    cleanup and a left shift for ties.** A character diff matched letters
    of typed words to leftovers of deleted ones and scattered attribution.
    Fixing a typo inside a word now attributes the whole word to the
    person who fixed it.

12. **Container identity follows the nearest paragraphs.** When a list item
    is split, the half holding the item's own paragraph keeps the item's ID,
    not the half the sub-list moved into (paragraph votes are weighted by
    distance).

13. **Small fixes outside the new crate**, needed by the import:
    - the Loro flow backend gained `join_para`, `insert_before`,
      `insert_after`, `insert_first`, `move_first`, `set_line_prop`,
      `fork_at`, `version`, `save_shallow`, `updates_since`,
      `import_updates`;
    - the codec skips soft page breaks between table rows (LibreOffice
      writes them in long documents; the table became opaque before);
    - `codec::Baseline::adopt` for re-baselining with equivalence.

14. **Not fixed here, queued as a task:** the ODT writer writes repeated
    spaces literally, which ODF collapses.

15. **The 100-page letter** is the municipal letter ×100 saved once by
    LibreOffice (`--convert-to`), because the scaled file has duplicate
    footnote and table names that LibreOffice repairs on the first save.
