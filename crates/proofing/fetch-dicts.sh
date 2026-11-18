#!/bin/sh
# Fetch the Hunspell dictionaries Spike 6 measures with, from LibreOffice's
# dictionaries repository at a pinned commit, into crates/proofing/dicts/
# (gitignored). Licences (see docs/licences.md, "Spike 6"):
#   da_DK  Stavekontrolden 2.9.101: GPL-2.0 / LGPL-2.1 / MPL-1.1 tri-licence
#   en_US  SCOWL 2020.12.07: SCOWL's permissive licence; affix file BSD (Ispell)
#   en_GB  LGPL (word list and affix file); fetched for comparison only
# Tests that need the files skip when they are absent.
set -e
COMMIT=c0865a85eb67dc377f2cf8e8fe2c4fa9e53f0dcb
BASE="https://raw.githubusercontent.com/LibreOffice/dictionaries/$COMMIT"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$here/dicts"
cd "$here/dicts"
fetch() {
  n=0
  until curl -fsS -o "$2" "$BASE/$1"; do
    n=$((n + 1))
    [ $n -ge 4 ] && exit 1
    sleep $((n * 2))
  done
}
fetch da_DK/da_DK.aff da_DK.aff
fetch da_DK/da_DK.dic da_DK.dic
fetch da_DK/README_da_DK.txt README_da_DK.txt
fetch en/en_US.aff en_US.aff
fetch en/en_US.dic en_US.dic
fetch en/README_en_US.txt README_en_US.txt
fetch en/en_GB.aff en_GB.aff
fetch en/en_GB.dic en_GB.dic
fetch en/README_en_GB.txt README_en_GB.txt
if [ -s "$here/dicts.sha256" ]; then
  sha256sum -c "$here/dicts.sha256"
fi
