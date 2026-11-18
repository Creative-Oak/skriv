// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Spike 2 functional check, the brief's "across page breaks" row: select
// from page 3 into page 4 with Shift+Down, delete it, type over it, undo
// twice; the document must change, then be back to exactly what it was.
//
//   node scripts/check.mjs [url] [views]

import { chromium } from 'playwright'

const base = process.argv[2] ?? 'http://localhost:5180'
const views = (process.argv[3] ?? 'b,a').split(',')
const mod = process.platform === 'darwin' ? 'Meta' : 'Control'
let failed = false

const browser = await chromium.launch({ channel: 'chromium' })
for (const view of views) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
  page.on('pageerror', (e) => {
    failed = true
    console.error(`${view}: page error: ${e.message}`)
  })
  await page.goto(`${base}/${view}.html`)
  await page.waitForFunction(() => document.documentElement.dataset.ready === '1')
  const snapshot = () =>
    page.evaluate(() => {
      const e = window.spike2.eng ?? null
      const text = []
      const n = e ? e.paras.length : 0
      for (let i = 0; i < n; i++) text.push(e.text(i))
      return { text: text.join('\n') }
    })
  const before = await snapshot()
  // The last body paragraph on page 3 (index 2).
  await page.evaluate(() => window.spike2.place(2, false))
  for (let i = 0; i < 12; i++) await page.keyboard.press('ArrowDown')
  await page.keyboard.down('Shift')
  for (let i = 0; i < 8; i++) await page.keyboard.press('ArrowDown')
  await page.keyboard.up('Shift')
  await page.keyboard.press('Backspace')
  await page.waitForTimeout(1200) // a new undo step
  await page.keyboard.type('Erstattet tekst ')
  await page.waitForTimeout(1200)
  const mid = await snapshot()
  await page.keyboard.press(`${mod}+z`)
  await page.waitForTimeout(300)
  await page.keyboard.press(`${mod}+z`)
  await page.waitForTimeout(300)
  const after = await snapshot()
  const ok = {
    changed: mid.text !== before.text && mid.text.includes('Erstattet tekst'),
    undone: after.text === before.text,
  }
  console.log(`${view}: ${JSON.stringify(ok)}`)
  if (!ok.changed || !ok.undone) failed = true
  await page.close()
}
await browser.close()
process.exit(failed ? 1 : 0)
