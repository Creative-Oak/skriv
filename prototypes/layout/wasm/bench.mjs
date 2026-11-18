// Spike 2b: time keystrokes in wasm. Runs in Node (`node bench.mjs <wasm>`)
// and in a browser (bench.html imports `run`).

export async function run(bytes, engines, n, log) {
  const results = [];
  for (const kind of engines) {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const w = instance.exports;
    const name = ["own", "own-greedy", "typst"][kind % 10] + (kind >= 10 ? " (justified)" : "");
    let t = performance.now();
    const pages = w.init(kind);
    const full = performance.now() - t;
    const hash = (BigInt(w.hash_hi() >>> 0) << 32n | BigInt(w.hash_lo() >>> 0)).toString(16).padStart(16, "0");
    for (const [label, page, table] of [["page 1", 0, 0], ["page 50", 49, 0], ["table, page 80", 79, 1]]) {
      // Fresh state per position, as natively.
      if (label !== "page 1") w.init(kind);
      const para = w.pick(page, table);
      const samples = [];
      for (let i = 0; i < n; i++) {
        t = performance.now();
        const ok = w.keystroke(para);
        samples.push(performance.now() - t);
        if (!ok) throw new Error(`caret lost at keystroke ${i}`);
      }
      samples.sort((a, b) => a - b);
      const pct = (p) => samples[Math.round((samples.length - 1) * p)];
      const r = { engine: name, position: label, pages, full_ms: full, hash, p50: pct(0.5), p95: pct(0.95), max: pct(1) };
      results.push(r);
      log(`${name}, ${label}: ${pages} pages (full ${full.toFixed(0)} ms, hash ${hash}); p50 ${r.p50.toFixed(2)} ms, p95 ${r.p95.toFixed(2)} ms, max ${r.max.toFixed(2)} ms`);
    }
  }
  return results;
}

if (typeof process !== "undefined" && process.argv[1] && process.argv[1].endsWith("bench.mjs")) {
  const fs = await import("node:fs");
  const bytes = fs.readFileSync(process.argv[2]);
  const engines = (process.argv[3] || "0,2").split(",").map(Number);
  const n = Number(process.argv[4] || 1000);
  await run(bytes, engines, n, console.log);
}
