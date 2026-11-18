#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Spike 5 end to end: fetch inputs, convert, measure, report.
#   fidelity/scripts/docx/spike5.sh [out dir]   (default: target/spike5)
set -euo pipefail
cd "$(dirname "$0")/../../.."
OUT=${1:-target/spike5}
IN=target/spike5-input
fidelity/scripts/docx/fetch.sh "$IN"
cargo build -q --release -p skriv-fidelity
python3 fidelity/scripts/docx/spike5.py --xsd "$IN/ecma/xsd" --out "$OUT" "$IN/da" "$IN/en"
python3 fidelity/scripts/docx/spike5_report.py "$OUT/results.json" > "$OUT/report.md"
echo "report: $OUT/report.md"
