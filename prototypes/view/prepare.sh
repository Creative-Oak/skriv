#!/bin/sh
# Spike 2: build the editing core to wasm and put it, the fonts and the
# 100-page test letter where the web prototypes load them from.
# Needs: Rust 1.100 + wasm32-unknown-unknown, wasm-bindgen-cli 0.2.130,
# the Liberation fonts in ../layout/fonts (../layout/fetch-fonts.sh), and
# the test letter (cargo run --release -p layout-bench -- export, in
# ../layout).
set -e
cd "$(dirname "$0")"
cargo build -q --release --target wasm32-unknown-unknown -p view-engine
wasm-bindgen --target web --out-dir web/public/pkg \
  target/wasm32-unknown-unknown/release/view_engine.wasm
mkdir -p web/public/fonts
cp ../layout/fonts/Liberation*.ttf web/public/fonts/
cp ../../target/spike2b/letter-100.json web/public/
echo "wasm: $(gzip -9 -c web/public/pkg/view_engine_bg.wasm | wc -c | tr -d ' ') bytes gzipped"
