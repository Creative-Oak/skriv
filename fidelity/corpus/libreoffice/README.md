# LibreOffice test documents

Fetched, not committed. Run:

```bash
fidelity/scripts/fetch-lo-corpus.sh
```

This takes every `.odt` under 3 MB from five Writer test-data directories of
LibreOffice core at tag `libreoffice-26.8.1.1` (commit `bf6ef266`, the same
build used as the oracle): `sw/qa/extras/{odfexport,odfimport,layout,uiwriter}/data`
and `sw/qa/core/data/odt`. That is 480 files today. `manifest.toml` (generated)
records each file's source path.

Licence: the LibreOffice core repository is MPL-2.0. Many of these files began
as attachments to bug reports, so we fetch them from a pinned commit instead of
redistributing them.
