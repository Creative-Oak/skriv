#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Spike 5 inputs, downloaded rather than committed:
# - the DOCX sample from docx-corpus (https://docxcorp.us, SuperDoc; the
#   collection is ODC-BY 1.0, each file stays its authors' copyright), by
#   the IDs listed in docs/spikes/05-results-data/corpus-*.txt;
# - the ECMA-376 5th edition Transitional XML schemas (Part 4).
#
#   fidelity/scripts/docx/fetch.sh [dir]     (default: target/spike5-input)
set -euo pipefail
cd "$(dirname "$0")/../../.."
OUT=${1:-target/spike5-input}
mkdir -p "$OUT/da" "$OUT/en" "$OUT/ecma"
for set in da en; do
  sed 's#.*#https://docxcorp.us/documents/&.docx#' "docs/spikes/05-results-data/corpus-$set.txt" |
    xargs -P 8 -n 1 sh -c 'f="$1/$(basename "$0")"; [ -s "$f" ] || curl -sf -m 60 -o "$f" "$0"' "$OUT/$set"
done
if [ ! -f "$OUT/ecma/xsd/wml.xsd" ]; then
  curl -sfL -m 300 -o "$OUT/ecma/p4.zip" https://ecma-international.org/wp-content/uploads/ECMA-376-4_5th_edition_december_2016.zip
  (cd "$OUT/ecma" && unzip -o -q p4.zip OfficeOpenXML-XMLSchema-Transitional.zip && mkdir -p xsd &&
    unzip -o -q OfficeOpenXML-XMLSchema-Transitional.zip -d xsd && rm -f p4.zip OfficeOpenXML-XMLSchema-Transitional.zip)
fi
echo "$(ls "$OUT/da" | wc -l) Danish, $(ls "$OUT/en" | wc -l) English files; schemas in $OUT/ecma/xsd"
