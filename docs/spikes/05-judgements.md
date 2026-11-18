# Spike 5: judgement calls

Decisions made while running Spike 5 without asking, so they can be
reviewed. Results: [`05-results.md`](05-results.md).

1. **The question was narrowed to the path that exists:** DOCX converted by
   LibreOffice at the boundary (ADR 0003 allows it as an optional server
   converter), ODT inside skriv. No native DOCX reader or writer was built;
   the results say which native piece to build first.

2. **docx-corpus files are fetched, not committed.** The collection is
   ODC-BY 1.0, which covers the database, not each file; every file stays
   its authors' copyright (the brief's "check file by file"). Only the IDs
   are committed; `fetch.sh` downloads them.

3. **A Danish random sample plus an English set chosen for tracked
   changes.** Danish files from the web are final versions (1 of 300 had
   tracked changes), so 61 English legal and report files were picked from
   a random 1,000 because they contain tracked changes (3+) or comments.
   That set is skewed towards Australian legislation compilations.

4. **Validation is the ECMA-376 5th edition Transitional schema after
   markup-compatibility processing (ECMA-376 Part 3), written here in
   Python**, as a stand-in for the Open XML SDK, which needs .NET.
   Namespaces in the Transitional schemas are "understood"; everything in
   `mc:Ignorable` namespaces is dropped; `mc:AlternateContent` resolves to
   its first understood `Choice`, else `Fallback`. The schemas are fetched,
   not committed (ECMA's terms for redistributing them weren't checked).

5. **Fidelity is a feature inventory, not a rendering.** Rendering both
   files would mean rendering through LibreOffice, comparing LibreOffice
   with itself; Word isn't available. The inventory compares items that
   survive when the feature survives (texts, effective formatting, list
   labels, field instructions, change authors), not XML spellings.

6. **The inventory's own bugs were fixed, and are recorded here**, because
   the first run reported losses that weren't:
   - formatting compared per run; LibreOffice merges runs → now per
     maximal stretch of characters per property;
   - paragraphs without a style didn't get the default paragraph style's
     formatting; table styles were ignored; theme fonts weren't resolved;
   - computed field results (page numbers, the TOC) counted as text →
     left out; hyperlink fields' text kept;
   - text boxes counted inside their anchor paragraph → their own feature;
   - only the last section was read; headers compared by reference, not
     by what each section shows (inheritance) → per section, by content;
     an empty header counts as no header;
   - symbol-font characters, U+2011 and soft hyphens normalised;
   - Word's hidden bookmarks (`_…`) ignored; form fields compared by kind,
     names separately (LibreOffice renames duplicates);
   - tracked insertions compared per paragraph, joined (LibreOffice merges
     adjacent ones); paragraph-mark changes separately;
   - white cell shading in lower case wasn't recognised as white.

7. **A control path was added after the first run**: DOCX → ODT → DOCX
   through LibreOffice alone. Without it the skriv paths (which include an
   ODT hop) couldn't be told apart from the hop itself.

8. **"Corrupt" means "doesn't convert or doesn't read back".** Schema
   validity is reported beside it, not as the criterion, because originals
   saved by Word fail it too (11–21%).

9. **Style names are compared as names** (lowercased, Danish built-in
   names mapped to English), so LibreOffice's renaming of clashing styles
   counts as a downgrade, although it changes no formatting.

10. **Four English files couldn't be measured** (page margins with units);
    the parser wasn't extended for them.
