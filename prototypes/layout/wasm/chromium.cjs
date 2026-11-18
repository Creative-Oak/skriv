// Run the wasm keystroke benchmark in headless Chromium (Playwright).
// usage: node chromium.cjs <dir served> <wasm file> <engines> <n>
const { chromium } = require("playwright");
const http = require("http");
const fs = require("fs");
const path = require("path");
const [dir, file, engines, n] = process.argv.slice(2);
const types = { ".html": "text/html", ".mjs": "text/javascript", ".wasm": "application/wasm" };
const server = http.createServer((req, res) => {
  const p = path.join(dir, decodeURIComponent(req.url.split("?")[0]));
  if (!fs.existsSync(p)) { res.writeHead(404); return res.end(); }
  res.writeHead(200, { "content-type": types[path.extname(p)] || "application/octet-stream" });
  fs.createReadStream(p).pipe(res);
}).listen(0, async () => {
  const port = server.address().port;
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${port}/bench.html`);
  await page.waitForFunction(() => window.ready);
  const out = await page.evaluate(([f, e, k]) => window.runBench(f, e, k), [file, engines.split(",").map(Number), Number(n)]);
  console.log(`Chromium ${browser.version()}`);
  for (const l of out.lines) console.log(l);
  await browser.close();
  server.close();
});
