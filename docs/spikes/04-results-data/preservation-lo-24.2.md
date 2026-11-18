LibreOffice: LibreOffice 24.2.7.2 420(Build:2)

Edit applied: True

| Item | Our file | convert | store | edit | edit (all probes in one file) |
|---|---|---|---|---|---|
| xml:id on <text:p> | kept | kept | kept | kept | kept |
| xml:id on <text:p> | kept | kept | kept | kept | kept |
| RDF metadata file (manifest.rdf + skriv/meta.rdf) | kept | kept | kept | kept | kept |
| xml:id on <text:h> | kept | kept | kept | kept | kept |
| xml:id on <text:section> | kept | kept | kept | kept | kept |
| xml:id on <text:list> | kept | **dropped**<br><sub><text:list> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list> survived but xml:id is gone</sub> |
| xml:id on <text:list-item> | kept | **dropped**<br><sub><text:list-item> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list-item> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list-item> survived but xml:id is gone</sub> | **dropped**<br><sub><text:list-item> survived but xml:id is gone</sub> |
| xml:id on <table:table> | kept | **dropped**<br><sub><table:table> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table> survived but xml:id is gone</sub> |
| xml:id on <table:table-row> | kept | **dropped**<br><sub><table:table-row> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-row> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-row> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-row> survived but xml:id is gone</sub> |
| xml:id on <table:table-cell> | kept | **dropped**<br><sub><table:table-cell> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-cell> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-cell> survived but xml:id is gone</sub> | **dropped**<br><sub><table:table-cell> survived but xml:id is gone</sub> |
| xml:id on <draw:frame> | kept | **dropped**<br><sub><draw:frame> survived but xml:id is gone</sub> | **dropped**<br><sub><draw:frame> survived but xml:id is gone</sub> | **dropped**<br><sub><draw:frame> survived but xml:id is gone</sub> | **dropped**<br><sub><draw:frame> survived but xml:id is gone</sub> |
| cross-paragraph bookmark name | kept | kept | kept | kept | kept |
| xml:id on <text:bookmark-start> | kept | kept | kept | kept | kept |
| <text:meta> with xml:id | kept | kept | kept | kept | kept |
| RDF metadata file (manifest.rdf + skriv/meta.rdf) | kept | kept | kept | kept | kept |
| <text:meta-field> with xml:id | kept | kept | kept | kept | kept |
| foreign attribute on <text:p> | kept | **dropped**<br><sub>foreign attributes now none</sub> | **dropped**<br><sub>foreign attributes now none</sub> | **dropped**<br><sub>foreign attributes now none</sub> | **dropped**<br><sub>foreign attributes now none</sub> |
| foreign attribute on <table:table-cell> | kept | **dropped**<br><sub>sk:id=None</sub> | **dropped**<br><sub>sk:id=None</sub> | **dropped**<br><sub>sk:id=None</sub> | **dropped**<br><sub>sk:id=None</sub> |
| foreign block element (with a paragraph inside) | kept | **changed**<br><sub><sk:block> removed but its text content was kept</sub> | **changed**<br><sub><sk:block> removed but its text content was kept</sub> | **changed**<br><sub><sk:block> removed but its text content was kept</sub> | **changed**<br><sub><sk:block> removed but its text content was kept</sub> |
| foreign empty inline element | kept | **dropped**<br><sub><sk:anchor> and its content are gone</sub> | **dropped**<br><sub><sk:anchor> and its content are gone</sub> | **dropped**<br><sub><sk:anchor> and its content are gone</sub> | **dropped**<br><sub><sk:anchor> and its content are gone</sub> |
| foreign inline element with text | kept | **changed**<br><sub><sk:note> removed but its text content was kept</sub> | **changed**<br><sub><sk:note> removed but its text content was kept</sub> | **changed**<br><sub><sk:note> removed but its text content was kept</sub> | **changed**<br><sub><sk:note> removed but its text content was kept</sub> |
| xml:id on the paragraph the user edits | kept | kept | kept | kept | kept |
| RDF metadata file (manifest.rdf + skriv/meta.rdf) | kept | kept | kept | kept | kept |
| package part skriv/state.bin | kept | **dropped**<br><sub>skriv/state.bin not in package</sub> | **dropped**<br><sub>skriv/state.bin not in package</sub> | **dropped**<br><sub>skriv/state.bin not in package</sub> | **dropped**<br><sub>skriv/state.bin not in package</sub> |
| package part skriv/unlisted.bin | kept | **refused**<br><sub>LibreOffice did not open the file (no document loaded)</sub> | **refused**<br><sub>LibreOffice did not open the file (no document loaded)</sub> | **refused**<br><sub>LibreOffice did not open the file (no document loaded)</sub> | **dropped**<br><sub>skriv/unlisted.bin not in package</sub> |
| package part skriv-state.json | kept | **dropped**<br><sub>skriv-state.json not in package</sub> | **dropped**<br><sub>skriv-state.json not in package</sub> | **dropped**<br><sub>skriv-state.json not in package</sub> | **dropped**<br><sub>skriv-state.json not in package</sub> |
| meta:user-defined (doc ID, typed value) | kept | kept | kept | kept | kept |
| foreign attribute on <style:style> | kept | **dropped**<br><sub>style kept, attribute gone</sub> | **dropped**<br><sub>style kept, attribute gone</sub> | **dropped**<br><sub>style kept, attribute gone</sub> | **dropped**<br><sub>style kept, attribute gone</sub> |
| foreign attribute on <style:text-properties> | kept | **dropped**<br><sub>attribute gone; now {'{urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0}font-style': 'italic'}</sub> | **dropped**<br><sub>attribute gone; now {'{urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0}font-style': 'italic'}</sub> | **dropped**<br><sub>attribute gone; now {'{urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0}font-style': 'italic'}</sub> | **dropped**<br><sub>attribute gone; now {'{urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0}font-style': 'italic'}</sub> |
| foreign attribute on root element | kept | **dropped**<br><sub>gone</sub> | **dropped**<br><sub>gone</sub> | **dropped**<br><sub>gone</sub> | **dropped**<br><sub>gone</sub> |
| ~256 KB snapshot as an RDF literal | kept | kept | kept | kept | kept |
| foreign element in office:meta | kept | kept | kept | kept | kept |
| foreign element in office:styles | kept | **dropped**<br><sub><sk:style-note> and its content are gone</sub> | **dropped**<br><sub><sk:style-note> and its content are gone</sub> | **dropped**<br><sub><sk:style-note> and its content are gone</sub> | **dropped**<br><sub><sk:style-note> and its content are gone</sub> |
