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

**Later change (2026-11-18, Spike 2):** the own engine now moves the caret
over spaces at the end of a paragraph (they took no width, so a typed space
didn't show until the next character). Line breaks are unchanged; caret
stops after trailing spaces on a paragraph's last line move. The own
engine's hashes are therefore new: ragged (own and own-greedy)
`988b0d52f689256d`, justified own `0e0cc69cb305ee29`, justified own-greedy
`50244fd0994b4c4c`; Typst's are unchanged. Re-checked on macOS: native
equals wasm in Chromium and WebKit, and the caret round trip is still
18,289/18,289. Not yet re-run on Linux.

