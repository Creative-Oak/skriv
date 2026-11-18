// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Spike 2 automated latency runs: real key events (Playwright's keyboard,
// which goes through the browser's input pipeline) at the brief's three
// caret positions, in each view, in Chromium, Firefox and WebKit.
//
//   node scripts/measure.mjs [url] [keystrokes] [browsers] [views]
//   node scripts/measure.mjs http://localhost:5180 1000 chromium,firefox,webkit b,a
//
// Writes results/<date>-<browser>-<view>.json and prints a table.
// Machine-dependent: these are this machine's numbers, not the reference
// hardware's.

import { mkdirSync, writeFileSync } from 'node:fs'
import { chromium, firefox, webkit } from 'playwright'

const base = process.argv[2] ?? 'http://localhost:5180'
const n = Number(process.argv[3] ?? 1000)
const browsers = (process.argv[4] ?? 'chromium').split(',')
const views = (process.argv[5] ?? 'b,a').split(',')
const kinds = { chromium, firefox, webkit }
const positions = [
  ['p.1', 0, false],
  ['p.50', 49, false],
  ['table p.80', 79, true],
]
const TEXT = 'Æbleøen på Ålborg havn '

mkdirSync('results', { recursive: true })
const rows = []
for (const name of browsers) {
  const browser = await kinds[name].launch(
    name === 'chromium'
      ? { channel: process.env.CHANNEL ?? 'chromium', headless: process.env.HEADED !== '1' }
      : { headless: process.env.HEADED !== '1' },
  )
  for (const view of views) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
    page.on('pageerror', (e) => console.error(`${name} ${view}: ${e.message}`))
    await page.goto(`${base}/${view}.html`)
    await page.waitForFunction(() => document.documentElement.dataset.ready === '1', null, {
      timeout: 60000,
    })
    const input = await page.evaluate(
      () => document.documentElement.dataset.input ?? 'contenteditable',
    )
    const runs = {}
    for (const [label, pg, inTable] of positions) {
      await page.evaluate(([p, t]) => window.spike2.place(p, t), [pg, inTable])
      await page.evaluate(() => window.spike2.reset())
      // Typing speed: 80–120 ms between keys (about 10 characters a
      // second, a fast typist). Faster than one key per frame, edits queue.
      for (let i = 0; i < n; i++) {
        await page.keyboard.type(TEXT[i % TEXT.length])
        await page.waitForTimeout(80 + Math.random() * 40)
      }
      // Let the last frame land.
      await page.waitForTimeout(200)
      const r = await page.evaluate(() => window.spike2.report())
      runs[label] = r
      const s = r.stats
      rows.push([
        name,
        view,
        input,
        label,
        s.n,
        s.p50,
        s.p95,
        s.max,
        s.busyP50,
        s.busyP95,
        s.engineP95,
        s.domP95,
        s.renderP95,
        s.over16,
      ])
    }
    const file = `results/${new Date().toISOString().slice(0, 10)}-${name}-${view}.json`
    writeFileSync(
      file,
      JSON.stringify({ browser: name, version: browser.version(), view, input, runs }, null, 1),
    )
    await page.close()
  }
  await browser.close()
}
const f = (x) => (typeof x === 'number' ? (Number.isInteger(x) ? `${x}` : x.toFixed(1)) : `${x}`)
console.log(
  'browser\tview\tinput\twhere\tn\tp50\tp95\tmax\twork p50\twork p95\tengine p95\tDOM p95\tframe p95\t>=16ms',
)
for (const r of rows) console.log(r.map(f).join('\t'))
