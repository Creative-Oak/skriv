// Dump the wasm layout of engine `kind` as JSON lines (for determinism checks).
import fs from "node:fs";
const { instance } = await WebAssembly.instantiate(fs.readFileSync(process.argv[2]), {});
const w = instance.exports;
const pages = w.init(Number(process.argv[3] || 0));
const out = [];
for (let p = 0; p < pages; p++) {
  const n = w.info(p, 0, 0);
  for (let l = 0; l < n; l++) out.push([p, ...[7, 1, 2, 3, 4, 5, 6].map((f) => w.info(p, l, f))]);
}
fs.writeFileSync(process.argv[4], JSON.stringify(out));
