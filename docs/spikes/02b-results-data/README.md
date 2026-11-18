# Spike 2b raw output

Output of `prototypes/layout/measure.sh` (2026-11-18, 4-core Linux
container, Chromium 157 headless, LibreOffice 24.2.7.2), compiler warnings
removed. The numbers in [`../02b-results.md`](../02b-results.md) come from
these files.

| File | What |
|---|---|
| `latency-native*.txt` | 1,000 keystrokes per position, native release build |
| `latency-chromium-*.txt` | wasm in headless Chromium: 1,000 keystrokes (own), 100 (Typst) |
| `carets*.txt` | caret round trip on 10 pages; Typst's own `jump_from_cursor` cross-check |
| `libreoffice-parity.txt` | line breaks and page starts against LibreOffice's PDF |
| `typography.txt` | inter-word space widths on justified lines |
| `hyphenation.txt` | hypher vs LibreOffice's Danish patterns, word by word |
| `hashes-native.txt` | layout hashes, to compare with the wasm runs |
| `wasm-size.txt` | wasm sizes (fonts and test input embedded) |
