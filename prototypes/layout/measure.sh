#!/bin/sh
# Spike 2b: every measurement in the results, in one go (about an hour).
# Needs: Rust 1.100 with wasm32-unknown-unknown, Node 22, Playwright's
# Chromium, LibreOffice (soffice), poppler (pdftotext), Python 3, and
# LibreOffice's Danish hyphenation patterns (/usr/share/hyphen/hyph_da_DK.dic,
# Debian package hyphen-da). Run ./fetch-fonts.sh first.
set -e
cd "$(dirname "$0")"
OUT=../../target/spike2b
RES=$OUT/results
mkdir -p "$RES"
cargo build -q --release -p layout-bench
B=./target/release/layout-bench
$B export > "$RES/hashes-native.txt"
for f in own typst; do
  cargo build -q --profile wasm --target wasm32-unknown-unknown -p layout-wasm --no-default-features --features $f
  cp target/wasm32-unknown-unknown/wasm/layout_wasm.wasm "$OUT/layout-$f.wasm"
done
cp wasm/bench.mjs "$OUT/"
for f in layout-own.wasm layout-typst.wasm; do
  echo "$f raw $(stat -c%s $OUT/$f) gzip $(gzip -9 -c $OUT/$f | wc -c)"
done > "$RES/wasm-size.txt"
$B latency own,greedy,typst 1000 > "$RES/latency-native.txt" 2>&1
SKRIV_JUSTIFY=1 $B latency own,greedy,typst 1000 > "$RES/latency-native-justified.txt" 2>&1
node wasm/chromium.cjs "$OUT" layout-own.wasm 0,10,1,11 1000 > "$RES/latency-chromium-own.txt" 2>&1
node wasm/chromium.cjs "$OUT" layout-typst.wasm 2 100 > "$RES/latency-chromium-typst.txt" 2>&1
node wasm/chromium.cjs "$OUT" layout-typst.wasm 12 100 > "$RES/latency-chromium-typst-justified.txt" 2>&1
$B carets > "$RES/carets.txt" 2>&1
SKRIV_JUSTIFY=1 $B carets > "$RES/carets-justified.txt" 2>&1
(cd "$OUT" && rm -f letter-100.pdf letter-100-justified.pdf && soffice --headless --convert-to pdf letter-100.odt letter-100-justified.odt > /dev/null 2>&1)
$B lines > /dev/null
SKRIV_JUSTIFY=1 $B lines > /dev/null
for v in "" "-justified"; do
  for e in own-greedy own own-shrink typst typst-compat; do
    echo "letter-100$v $e: $(python3 scripts/compare_lo.py $OUT/letter-100$v.pdf $OUT/lines-$e$v.tsv)"
  done
done > "$RES/libreoffice-parity.txt"
$B spaces > "$RES/typography.txt"
python3 scripts/spaces_lo.py $OUT/letter-100-justified.pdf >> "$RES/typography.txt"
$B hyph > /dev/null
python3 scripts/hyphenation_compare.py $OUT/hypher-da.tsv /usr/share/hyphen/hyph_da_DK.dic > "$RES/hyphenation.txt"
echo done
