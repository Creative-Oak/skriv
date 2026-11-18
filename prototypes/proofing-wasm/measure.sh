#!/bin/sh
# Spike 6: build the proofing crate for wasm32 in four variants, report the
# raw and gzipped sizes of code and of the dictionaries, then time loading
# and checking in Node. Needs ../../crates/proofing/dicts (fetch-dicts.sh).
set -e
cd "$(dirname "$0")"
here="$(pwd)"
dicts="$here/../../crates/proofing/dicts"
out="${1:-$here/out}"
mkdir -p "$out"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$here/target}"
gz() { gzip -9 -c "$1" | wc -c; }
echo "variant,raw_bytes,gzip9_bytes" > "$out/sizes.csv"
for v in spell whatlang lingua lingua,whatlang; do
  case $v in spell) f="" ;; *) f="--features $v" ;; esac
  # shellcheck disable=SC2086
  cargo build -q --release --target wasm32-unknown-unknown $f
  w="$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/skriv_proofing_wasm.wasm"
  name=$(echo "$v" | tr , +)
  cp "$w" "$out/proofing-$name.wasm"
  echo "$name,$(wc -c < "$w"),$(gz "$w")" >> "$out/sizes.csv"
done
for f in da_DK.aff da_DK.dic en_US.aff en_US.dic en_GB.aff en_GB.dic; do
  echo "$f,$(wc -c < "$dicts/$f"),$(gz "$dicts/$f")" >> "$out/sizes.csv"
done
cat "$out/sizes.csv"
node "$here/bench.mjs" "$out/proofing-lingua+whatlang.wasm" "$dicts" | tee "$out/node.txt"
