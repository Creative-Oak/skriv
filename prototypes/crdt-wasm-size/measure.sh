#!/bin/sh
# Build the probe for each backend and report raw and gzipped .wasm sizes.
set -e
cd "$(dirname "$0")"
export RUSTFLAGS='--cfg getrandom_backend="wasm_js"'
for f in loro automerge; do
  cargo build -q --release --target wasm32-unknown-unknown --no-default-features --features "$f" --target-dir "target-$f"
  w="target-$f/wasm32-unknown-unknown/release/skriv_crdt_wasm_size.wasm"
  raw=$(wc -c < "$w")
  gz=$(gzip -9 -c "$w" | wc -c)
  echo "$f: $raw bytes, gzip -9 $gz bytes"
done
