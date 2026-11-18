#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Spike 4: skriv files go through LibreOffice and come back.
#
#   fidelity/scripts/spike4.sh [out dir]      (default: target/spike4)
#
# 1. Build the inputs: the generated corpus, plus a long document (the
#    municipal letter 20 times (SPIKE4_COPIES), saved once by LibreOffice so it is valid ODF).
# 2. `spike4 prepare`: write every file with skriv's state in it (IDs,
#    document ID, skriv/state.rdf with a shallow snapshot), and the jobs.
# 3. `spike4_lo.py`: LibreOffice opens each file, makes the job's edits the
#    way a person would, and saves (ODT, convert, or via DOCX).
# 4. `spike4 evaluate`: every returned file becomes a branch; compare it with
#    the same edits made in skriv. Writes results.json.
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT=${1:-target/spike4}
COPIES=${SPIKE4_COPIES:-20}
SOFFICE=${SKRIV_SOFFICE:-soffice}
rm -rf "$OUT"
mkdir -p "$OUT/long-src" "$OUT/long"
cargo build -q --release -p skriv-filemode
BIN=target/release/spike4
"$BIN" scale fidelity/corpus/generated/danish-municipal-letter.odt "$COPIES" "$OUT/long-src/letter-x$COPIES.odt"
PROFILE=$(mktemp -d)
"$SOFFICE" "-env:UserInstallation=file://$PROFILE" --headless --norestore \
  --convert-to odt --outdir "$OUT/long" "$OUT/long-src/letter-x$COPIES.odt" >/dev/null 2>&1
rm -rf "$PROFILE"
"$BIN" prepare fidelity/corpus/generated "$OUT/long" "$OUT/work"
python3 fidelity/scripts/spike4_lo.py "$OUT/work" --soffice "$SOFFICE"
"$BIN" evaluate "$OUT/work"
