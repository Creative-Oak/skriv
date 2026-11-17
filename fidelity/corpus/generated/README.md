# Generated corpus

One LibreOffice-authored document per Spike 1 feature, built through the UNO
API by `fidelity/scripts/uno/skriv_generate.py` (run with
`python3 fidelity/scripts/generate_corpus.py`). Because LibreOffice writes them,
they are real LibreOffice output, not hand-written XML. Regenerating with a
different LibreOffice version changes the bytes; commit the result.

Content is our own. Licence: CC0-1.0.

| File | Exercises |
|---|---|
| `styles-inheritance-and-direct-formatting.odt` | named styles with parents, a character style, bold and margin applied by hand |
| `lists-multilevel-numbering.odt` | three-level numbering from a list style, a bullet list |
| `sections-columns.odt` | a named two-column section |
| `tables-widths-merged.odt` | explicit column widths, merged cells |
| `figures-captions-fields.odt` | embedded image with alt text, caption sequence, bookmark, cross-reference, page number, fixed date |
| `comments-ranged.odt` | a comment on a text range |
| `tracked-changes-pending.odt` | pending insertion, deletion and format change |
| `danish-municipal-letter.odt` | header/footer page fields, address table, footnote, numbered complaint guidance, Danish text |
| `table-of-contents.odt` | a generated table of contents |
