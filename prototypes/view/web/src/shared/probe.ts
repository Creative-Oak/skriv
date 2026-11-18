// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Keystroke-to-paint latency, as the brief defines it: from the input
// event's timestamp to the end of the next frame that contains the change.
// "End of the frame" is a requestAnimationFrame callback followed by a
// message-channel task, which runs after that frame's style, layout and
// paint. The Event Timing API cross-checks it: it reports every input
// event whose own input-to-next-paint duration is 16 ms or more.

import { timing } from './engine'

export interface Sample {
  /** Keystroke (or input event) to the end of the painted frame. */
  total: number
  engine: number
  dom: number
  /** The frame's own rendering work: rAF callback to after paint. */
  render: number
  /**
   * Work this keystroke caused: engine + DOM + the frame's rendering. At
   * 60 Hz `total` also includes waiting for the next frame (0–16.7 ms),
   * which no implementation can avoid; `busy` is what can overrun it.
   */
  busy: number
  kind: string
  page: number
  at: number
}

const samples: Sample[] = []
const slowEvents: { name: string; duration: number; at: number }[] = []
let pendingKey: number | null = null
let hud: HTMLElement | null = null
let label = ''

/** Remember a keydown's timestamp: the edit it causes is timed from it. */
export function keyDown(e: KeyboardEvent): void {
  pendingKey = e.timeStamp
}

/**
 * Time an edit. `start` is the triggering event's timestamp (or
 * `performance.now()` for synthetic edits). Call `done` right after the
 * DOM has been updated.
 */
export function begin(start: number | null, kind: string, page: number) {
  const t0 =
    pendingKey !== null && start !== null && start - pendingKey < 500
      ? pendingKey
      : (start ?? performance.now())
  pendingKey = null
  timing.engine = 0
  const d0 = performance.now()
  return {
    done(): Promise<Sample> {
      // Synchronous work since `begin`: the engine call, then the DOM update.
      const engine = timing.engine
      const dom = performance.now() - d0 - engine
      return new Promise((resolve) => {
        requestAnimationFrame(() => {
          const raf = performance.now()
          const ch = new MessageChannel()
          ch.port1.onmessage = () => {
            const end = performance.now()
            const s: Sample = {
              total: end - t0,
              engine,
              dom,
              render: end - raf,
              busy: engine + dom + (end - raf),
              kind,
              page,
              at: Math.round(performance.timeOrigin + t0),
            }
            samples.push(s)
            render()
            resolve(s)
          }
          ch.port2.postMessage(0)
        })
      })
    },
  }
}

function pct(xs: number[], p: number): number {
  if (!xs.length) return Number.NaN
  const s = [...xs].sort((a, b) => a - b)
  return s[Math.min(s.length - 1, Math.ceil((p / 100) * s.length) - 1)] ?? Number.NaN
}

export function stats(xs = samples) {
  const t = xs.map((s) => s.total)
  return {
    n: t.length,
    p50: pct(t, 50),
    p95: pct(t, 95),
    max: Math.max(...t),
    engineP95: pct(
      xs.map((s) => s.engine),
      95,
    ),
    domP95: pct(
      xs.map((s) => s.dom),
      95,
    ),
    renderP95: pct(
      xs.map((s) => s.render),
      95,
    ),
    busyP50: pct(
      xs.map((s) => s.busy),
      50,
    ),
    busyP95: pct(
      xs.map((s) => s.busy),
      95,
    ),
    over16: t.filter((x) => x >= 16).length,
  }
}

const f = (x: number) => (Number.isFinite(x) ? x.toFixed(1) : '–')

function render(): void {
  if (!hud) return
  const s = stats()
  const out = hud.querySelector('.numbers')
  if (out)
    out.textContent = `${label}  n ${s.n}  p50 ${f(s.p50)}  p95 ${f(s.p95)}  max ${f(s.max)} ms · work p50 ${f(s.busyP50)} p95 ${f(s.busyP95)} (engine ${f(s.engineP95)}, DOM ${f(s.domP95)}, frame ${f(s.renderP95)}) · ≥16 ms: ${s.over16} · slow events: ${slowEvents.length}`
}

export function reset(): void {
  samples.length = 0
  slowEvents.length = 0
  render()
}

export function report() {
  return {
    view: label,
    userAgent: navigator.userAgent,
    platform: (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
      ?.platform,
    hardwareConcurrency: navigator.hardwareConcurrency,
    deviceMemory: (navigator as Navigator & { deviceMemory?: number }).deviceMemory,
    screen: [screen.width, screen.height, devicePixelRatio],
    date: new Date().toISOString(),
    stats: stats(),
    samples,
    slowEvents,
  }
}

/** The floating panel: numbers, reset, download, and extra controls. */
export function mountHud(name: string, extra: HTMLElement[] = []): HTMLElement {
  label = name
  hud = document.createElement('aside')
  hud.className = 'hud'
  hud.setAttribute('aria-label', 'Latency probe')
  hud.innerHTML = `<div class="numbers"></div><div class="buttons">
    <button type="button" data-act="reset">Reset</button>
    <button type="button" data-act="save">Download JSON</button></div>`
  const buttons = hud.querySelector('.buttons') as HTMLElement
  buttons.append(...extra)
  hud.addEventListener('mousedown', (e) => e.preventDefault())
  hud.addEventListener('click', (e) => {
    const act = (e.target as HTMLElement).dataset.act
    if (act === 'reset') reset()
    if (act === 'save') {
      const blob = new Blob([JSON.stringify(report(), null, 1)], { type: 'application/json' })
      const a = document.createElement('a')
      a.href = URL.createObjectURL(blob)
      a.download = `spike2-${label}-${new Date().toISOString().replace(/[:.]/g, '-')}.json`
      a.click()
    }
  })
  document.body.append(hud)
  ;(window as unknown as { spike2: unknown }).spike2 = { report, reset, stats }
  if ('PerformanceEventTiming' in window) {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) {
        if (['keydown', 'keypress', 'beforeinput', 'input', 'keyup'].includes(e.name))
          slowEvents.push({ name: e.name, duration: e.duration, at: e.startTime })
      }
      render()
    }).observe({ type: 'event', durationThreshold: 16, buffered: false } as PerformanceObserverInit)
  }
  render()
  return hud
}

export function setStatus(text: string): void {
  let el = document.querySelector('.status')
  if (!el) {
    el = document.createElement('div')
    el.className = 'status'
    el.setAttribute('role', 'status')
    hud?.prepend(el)
  }
  el.textContent = text
}

/**
 * A synthetic run: `n` edits made by `edit(i)`, one per frame, each timed
 * from the moment it starts. It measures engine + DOM + paint, not the
 * browser's input path (use real typing or Playwright for that).
 */
export async function synthetic(
  n: number,
  edit: (i: number) => { done(): Promise<Sample> } | null,
): Promise<void> {
  for (let i = 0; i < n; i++) {
    const h = edit(i)
    if (!h) return
    await h.done()
    await new Promise((r) => setTimeout(r, 0))
  }
}
