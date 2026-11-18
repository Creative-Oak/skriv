// Spike 6: run bench.mjs in headless Chromium (Playwright).
// usage: node chromium.cjs <wasm file> <dicts dir>
const { chromium } = require('playwright')
const http = require('node:http')
const fs = require('node:fs')
const path = require('node:path')

const [wasm, dicts] = process.argv.slice(2).map((p) => path.resolve(p))
const routes = {
  '/': [path.join(__dirname, 'bench.mjs'), 'text/html', '<!doctype html><title>bench</title>'],
  '/bench.mjs': [path.join(__dirname, 'bench.mjs'), 'text/javascript'],
  '/proofing.wasm': [wasm, 'application/wasm'],
}
for (const f of ['da_DK.aff', 'da_DK.dic', 'en_US.aff', 'en_US.dic']) {
  routes[`/${f}`] = [path.join(dicts, f), 'application/octet-stream']
}
const server = http
  .createServer((req, res) => {
    const r = routes[req.url]
    if (!r) {
      res.writeHead(404)
      return res.end()
    }
    res.writeHead(200, { 'content-type': r[1] })
    if (r[2]) return res.end(r[2])
    fs.createReadStream(r[0]).pipe(res)
  })
  .listen(0, async () => {
    const port = server.address().port
    const { loadInputs } = await import(path.join(__dirname, 'bench.mjs'))
    const { texts, typos } = loadInputs(fs, path, dicts)
    const browser = await chromium.launch()
    const page = await browser.newPage()
    await page.goto(`http://127.0.0.1:${port}/`)
    const out = await page.evaluate(
      async ([texts, typos]) => {
        const { run } = await import('/bench.mjs')
        const get = async (u) => new Uint8Array(await (await fetch(u)).arrayBuffer())
        const files = {}
        for (const f of ['da_DK.aff', 'da_DK.dic', 'en_US.aff', 'en_US.dic'])
          files[f] = await get(`/${f}`)
        const lines = []
        const r = await run(await get('/proofing.wasm'), files, texts, typos, (l) => lines.push(l))
        return { r, lines }
      },
      [texts, typos],
    )
    console.log(`Chromium ${browser.version()}`)
    for (const l of out.lines) console.log(l)
    console.log(JSON.stringify(out.r))
    await browser.close()
    server.close()
  })
