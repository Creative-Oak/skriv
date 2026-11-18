
## Corpus `da`: 300 files, 300 readable

| Feature | Documents with it | Instances |
|---|---|---|
| text | 300 (100.0%) | 31873 |
| paragraph style | 294 (98.0%) | 18254 |
| outline level | 91 (30.3%) | 670 |
| list | 134 (44.7%) | 2679 |
| alignment | 124 (41.3%) | 3827 |
| character formatting | 294 (98.0%) | 28435 |
| font | 294 (98.0%) | 18576 |
| table | 136 (45.3%) | 637 |
| merged cell | 51 (17.0%) | 1073 |
| cell shading | 48 (16.0%) | 1235 |
| field | 22 (7.3%) | 281 |
| hyperlink | 81 (27.0%) | 575 |
| bookmark | 19 (6.3%) | 92 |
| tracked change | 1 (0.3%) | 4 |
| comment | 1 (0.3%) | 2 |
| comment anchor | 1 (0.3%) | 2 |
| note reference | 13 (4.3%) | 65 |
| note text | 13 (4.3%) | 65 |
| drawing | 37 (12.3%) | 95 |
| text box text | 5 (1.7%) | 7 |
| form field | 11 (3.7%) | 230 |
| form field name | 11 (3.7%) | 230 |
| content control | 21 (7.0%) | 274 |
| section | 299 (99.7%) | 332 |
| header/footer set | 299 (99.7%) | 332 |
| header/footer text | 102 (34.0%) | 290 |
| header/footer field | 72 (24.0%) | 148 |
| header/footer drawing | 22 (7.3%) | 29 |

- LibreOffice DOCX→DOCX: 300/300 produced a readable DOCX
- LibreOffice DOCX→ODT→DOCX: 299/300 produced a readable DOCX
- …→ODT→skriv→ODT→… (preserving): 299/300 produced a readable DOCX
- …→ODT→skriv→ODT→… (regenerated): 299/300 produced a readable DOCX

| Validates against ECMA-376 Transitional | original | LibreOffice DOCX→DOCX | LibreOffice DOCX→ODT→DOCX | …→ODT→skriv→ODT→… (preserving) | …→ODT→skriv→ODT→… (regenerated) |
|---|---|---|---|---|---|
| files | 266/300 (88.7%) | 220/300 (73.3%) | 277/299 (92.6%) | 277/299 (92.6%) | 277/299 (92.6%) |

New validation errors (in files whose original validates), most common:

- lo, footer: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}id': This element is not expected. (32)
- lo, settings: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}docVars': This element is not expected. Expected (18)
- lo, styles: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}color': The attribute '{http://schemas.openxmlfo (13)
- lo, footer: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}docPartGallery': This element is not expected. (10)
- lo, settings: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}documentProtection': This element is not expecte (9)
- lo, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}cnfStyle': This element is not expected. Expecte (6)
- lo, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (5)
- lo2, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (5)

| Feature (instances kept) | LibreOffice DOCX→DOCX | LibreOffice DOCX→ODT→DOCX | …→ODT→skriv→ODT→… (preserving) | …→ODT→skriv→ODT→… (regenerated) |
|---|---|---|---|---|
| text | 100.0% (1 lost, 1 docs) | 99.9% (14 lost, 4 docs) | 99.9% (14 lost, 4 docs) | 99.9% (14 lost, 4 docs) |
| paragraph style | 93.7% (1155 lost, 6 docs) | 93.6% (1168 lost, 9 docs) | 93.6% (1168 lost, 9 docs) | 93.6% (1168 lost, 9 docs) |
| outline level | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| list | 100.0% (0 lost, 0 docs) | 99.9% (4 lost, 1 docs) | 99.9% (4 lost, 1 docs) | 99.9% (4 lost, 1 docs) |
| alignment | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| character formatting | 99.3% (198 lost, 17 docs) | 98.7% (378 lost, 28 docs) | 98.7% (378 lost, 28 docs) | 98.7% (378 lost, 28 docs) |
| font | 98.7% (247 lost, 18 docs) | 98.6% (260 lost, 20 docs) | 98.6% (260 lost, 20 docs) | 98.6% (260 lost, 20 docs) |
| table | 96.9% (20 lost, 2 docs) | 99.3% (4 lost, 4 docs) | 99.3% (4 lost, 4 docs) | 99.3% (4 lost, 4 docs) |
| merged cell | 99.9% (1 lost, 1 docs) | 95.2% (52 lost, 4 docs) | 95.2% (52 lost, 4 docs) | 95.2% (52 lost, 4 docs) |
| cell shading | 99.8% (3 lost, 1 docs) | 99.8% (3 lost, 1 docs) | 99.8% (3 lost, 1 docs) | 99.8% (3 lost, 1 docs) |
| field | 99.3% (2 lost, 1 docs) | 98.2% (5 lost, 2 docs) | 98.2% (5 lost, 2 docs) | 98.2% (5 lost, 2 docs) |
| hyperlink | 99.1% (5 lost, 3 docs) | 99.1% (5 lost, 3 docs) | 99.1% (5 lost, 3 docs) | 99.1% (5 lost, 3 docs) |
| bookmark | 91.3% (8 lost, 3 docs) | 90.2% (9 lost, 4 docs) | 90.2% (9 lost, 4 docs) | 90.2% (9 lost, 4 docs) |
| tracked change | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| comment | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| comment anchor | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| note reference | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| note text | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| drawing | 74.7% (24 lost, 12 docs) | 74.5% (24 lost, 12 docs) | 74.5% (24 lost, 12 docs) | 74.5% (24 lost, 12 docs) |
| text box text | 85.7% (1 lost, 1 docs) | 85.7% (1 lost, 1 docs) | 85.7% (1 lost, 1 docs) | 85.7% (1 lost, 1 docs) |
| form field | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| form field name | 20.0% (184 lost, 9 docs) | 20.0% (184 lost, 9 docs) | 20.0% (184 lost, 9 docs) | 20.0% (184 lost, 9 docs) |
| content control | 97.1% (8 lost, 5 docs) | 95.6% (12 lost, 7 docs) | 95.6% (12 lost, 7 docs) | 95.6% (12 lost, 7 docs) |
| section | 91.9% (27 lost, 23 docs) | 91.8% (27 lost, 23 docs) | 91.8% (27 lost, 23 docs) | 91.8% (27 lost, 23 docs) |
| header/footer set | 97.9% (7 lost, 7 docs) | 97.9% (7 lost, 7 docs) | 97.9% (7 lost, 7 docs) | 97.9% (7 lost, 7 docs) |
| header/footer text | 96.2% (11 lost, 7 docs) | 96.2% (11 lost, 7 docs) | 96.2% (11 lost, 7 docs) | 96.2% (11 lost, 7 docs) |
| header/footer field | 96.6% (5 lost, 4 docs) | 96.6% (5 lost, 4 docs) | 96.6% (5 lost, 4 docs) | 96.6% (5 lost, 4 docs) |
| header/footer drawing | 41.4% (17 lost, 16 docs) | 41.4% (17 lost, 16 docs) | 41.4% (17 lost, 16 docs) | 41.4% (17 lost, 16 docs) |

## Corpus `en`: 61 files, 57 readable

| Feature | Documents with it | Instances |
|---|---|---|
| text | 57 (100.0%) | 57100 |
| paragraph style | 57 (100.0%) | 53771 |
| outline level | 42 (73.7%) | 4848 |
| list | 23 (40.4%) | 755 |
| alignment | 56 (98.2%) | 1105 |
| character formatting | 57 (100.0%) | 70912 |
| font | 57 (100.0%) | 53787 |
| table | 42 (73.7%) | 312 |
| merged cell | 33 (57.9%) | 1742 |
| cell shading | 4 (7.0%) | 235 |
| field | 41 (71.9%) | 258 |
| hyperlink | 17 (29.8%) | 61 |
| bookmark | 17 (29.8%) | 97 |
| tracked change | 42 (73.7%) | 2157 |
| comment | 5 (8.8%) | 9 |
| comment anchor | 5 (8.8%) | 9 |
| note reference | 3 (5.3%) | 15 |
| note text | 3 (5.3%) | 15 |
| drawing | 38 (66.7%) | 42 |
| section | 57 (100.0%) | 235 |
| header/footer set | 57 (100.0%) | 235 |
| header/footer text | 53 (93.0%) | 407 |
| header/footer field | 42 (73.7%) | 905 |
| header/footer drawing | 1 (1.8%) | 1 |

- LibreOffice DOCX→DOCX: 57/57 produced a readable DOCX
- LibreOffice DOCX→ODT→DOCX: 57/57 produced a readable DOCX
- …→ODT→skriv→ODT→… (preserving): 57/57 produced a readable DOCX
- …→ODT→skriv→ODT→… (regenerated): 57/57 produced a readable DOCX

| Validates against ECMA-376 Transitional | original | LibreOffice DOCX→DOCX | LibreOffice DOCX→ODT→DOCX | …→ODT→skriv→ODT→… (preserving) | …→ODT→skriv→ODT→… (regenerated) |
|---|---|---|---|---|---|
| files | 45/57 (78.9%) | 14/57 (24.6%) | 13/57 (22.8%) | 13/57 (22.8%) | 13/57 (22.8%) |

New validation errors (in files whose original validates), most common:

- lo, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (38)
- lo2, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (38)
- sk, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (38)
- skr, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}pStyle': This element is not expected. Expected (38)
- lo, settings: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}docVars': This element is not expected. Expected (31)
- lo, footer: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}id': This element is not expected. (5)
- lo2, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}ins': This element is not expected. Expected is (3)
- sk, document: Element '{http://schemas.openxmlformats.org/wordprocessingml/2006/main}ins': This element is not expected. Expected is (3)

| Feature (instances kept) | LibreOffice DOCX→DOCX | LibreOffice DOCX→ODT→DOCX | …→ODT→skriv→ODT→… (preserving) | …→ODT→skriv→ODT→… (regenerated) |
|---|---|---|---|---|
| text | 100.0% (1 lost, 1 docs) | 100.0% (1 lost, 1 docs) | 100.0% (1 lost, 1 docs) | 100.0% (1 lost, 1 docs) |
| paragraph style | 99.6% (215 lost, 21 docs) | 99.6% (195 lost, 17 docs) | 99.6% (195 lost, 17 docs) | 99.6% (195 lost, 17 docs) |
| outline level | 99.9% (6 lost, 4 docs) | 100.0% (1 lost, 1 docs) | 100.0% (1 lost, 1 docs) | 100.0% (1 lost, 1 docs) |
| list | 100.0% (0 lost, 0 docs) | 99.7% (2 lost, 1 docs) | 99.7% (2 lost, 1 docs) | 99.7% (2 lost, 1 docs) |
| alignment | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| character formatting | 99.9% (83 lost, 9 docs) | 99.9% (89 lost, 7 docs) | 99.9% (89 lost, 7 docs) | 99.9% (89 lost, 7 docs) |
| font | 99.9% (67 lost, 12 docs) | 99.9% (67 lost, 12 docs) | 99.9% (67 lost, 12 docs) | 99.9% (67 lost, 12 docs) |
| table | 100.0% (0 lost, 0 docs) | 98.4% (5 lost, 4 docs) | 98.4% (5 lost, 4 docs) | 98.4% (5 lost, 4 docs) |
| merged cell | 99.7% (6 lost, 1 docs) | 87.1% (225 lost, 4 docs) | 87.1% (225 lost, 4 docs) | 87.1% (225 lost, 4 docs) |
| cell shading | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| field | 97.3% (7 lost, 2 docs) | 97.3% (7 lost, 2 docs) | 97.3% (7 lost, 2 docs) | 97.3% (7 lost, 2 docs) |
| hyperlink | 93.4% (4 lost, 1 docs) | 93.4% (4 lost, 1 docs) | 93.4% (4 lost, 1 docs) | 93.4% (4 lost, 1 docs) |
| bookmark | 99.0% (1 lost, 1 docs) | 99.0% (1 lost, 1 docs) | 99.0% (1 lost, 1 docs) | 99.0% (1 lost, 1 docs) |
| tracked change | 86.5% (291 lost, 32 docs) | 86.1% (299 lost, 32 docs) | 86.1% (299 lost, 32 docs) | 86.1% (299 lost, 32 docs) |
| comment | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| comment anchor | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| note reference | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| note text | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| drawing | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
| section | 78.7% (50 lost, 27 docs) | 78.7% (50 lost, 27 docs) | 78.7% (50 lost, 27 docs) | 78.7% (50 lost, 27 docs) |
| header/footer set | 83.0% (40 lost, 27 docs) | 82.6% (41 lost, 28 docs) | 82.6% (41 lost, 28 docs) | 82.6% (41 lost, 28 docs) |
| header/footer text | 90.9% (37 lost, 14 docs) | 90.9% (37 lost, 14 docs) | 90.9% (37 lost, 14 docs) | 90.9% (37 lost, 14 docs) |
| header/footer field | 97.1% (26 lost, 8 docs) | 97.1% (26 lost, 8 docs) | 97.1% (26 lost, 8 docs) | 97.1% (26 lost, 8 docs) |
| header/footer drawing | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) | 100.0% (0 lost, 0 docs) |
