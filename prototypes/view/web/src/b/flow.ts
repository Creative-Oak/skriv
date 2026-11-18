// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// View B's DOM: semantic blocks (<h1>–<h6>, <p>, <table>) in reading
// order, each paragraph's text in absolutely positioned runs placed by the
// engine. Page chrome lives in a separate, aria-hidden layer, so a screen
// reader reads paragraphs continuously across pages.
//
// Two-phase updates: paragraphs on the pages in view are redrawn at once;
// the rest (pages pushed down by an edit) are queued and redrawn in idle
// time.

import type { BlockInfo, Engine, ParaInfo } from '../shared/engine'
import { drawPages, font, GAP, PX, pageTop, runBox } from '../shared/geometry'

export class Flow {
  readonly paraEls = new Map<string, HTMLElement>()
  private tableEls = new Map<string, { el: HTMLElement; ids: string }>()
  private queue = new Set<string>()
  private idle = 0

  constructor(
    readonly eng: Engine,
    readonly doc: HTMLElement,
    readonly pagesEl: HTMLElement,
    readonly flow: HTMLElement,
  ) {}

  build(): void {
    drawPages(this.pagesEl, this.eng.spec, this.eng.pages.length)
    this.reconcile()
    for (const id of this.paraEls.keys()) this.renderPara(id)
  }

  private makePara(p: ParaInfo): HTMLElement {
    const el = document.createElement(p.h ? `h${Math.min(p.h, 6)}` : 'p')
    el.dataset.id = p.id
    this.paraEls.set(p.id, el)
    return el
  }

  private tableEl(b: Extract<BlockInfo, { t: 'table' }>): HTMLElement {
    const ids = b.rows
      .map((r) => r.map((c) => c.paras.map((p) => p.id).join(',')).join('|'))
      .join('/')
    const old = this.tableEls.get(b.id)
    if (old && old.ids === ids) return old.el
    const t = document.createElement('table')
    for (const row of b.rows) {
      const tr = t.insertRow()
      for (const c of row) {
        const td = tr.insertCell()
        if (c.span > 1) td.colSpan = c.span
        for (const p of c.paras) td.append(this.paraEls.get(p.id) ?? this.makePara(p))
      }
    }
    this.tableEls.set(b.id, { el: t, ids })
    return t
  }

  /** Bring the block elements in line with the engine's structure. */
  reconcile(): void {
    const live = new Set(this.eng.paras.map((p) => p.id))
    for (const id of [...this.paraEls.keys()]) {
      if (!live.has(id)) {
        this.paraEls.get(id)?.remove()
        this.paraEls.delete(id)
      }
    }
    const want: Element[] = this.eng.structure.map((b) =>
      b.t === 'p' ? (this.paraEls.get(b.id) ?? this.makePara(b)) : this.tableEl(b),
    )
    const keep = new Set(want)
    for (const el of [...this.flow.children]) if (!keep.has(el)) el.remove()
    let cur = this.flow.firstElementChild as Element | null
    for (const el of want) {
      if (cur === el) {
        cur = cur.nextElementSibling
        continue
      }
      this.flow.insertBefore(el, cur)
    }
  }

  renderPara(id: string): void {
    const el = this.paraEls.get(id)
    if (!el) return
    const flat = this.eng.flatOf.get(id)
    const info = flat === undefined ? undefined : this.eng.paras[flat]
    const lines = this.eng.linesOf(id)
    const spec = this.eng.spec
    const out: HTMLElement[] = []
    lines.forEach((l, i) => {
      const page = this.eng.pageOfLine(id, l.s)
      if (i === 0 && info?.label) {
        const lab = document.createElement('span')
        lab.className = 'lbl'
        lab.textContent = `${info.label} `
        const r0 = l.runs[0]
        const box = r0 ? runBox(spec, page, l, r0) : { x: l.x0 * PX, y: 0 }
        Object.assign(lab.style, {
          left: `${box.x - 28 * PX}px`,
          top: `${box.y}px`,
          width: `${22 * PX}px`,
          font: r0 ? font(r0) : '',
        })
        out.push(lab)
      }
      for (const r of l.runs) {
        const span = document.createElement('span')
        span.className = 'r'
        span.dataset.s = `${r[3]}`
        span.textContent = r[2]
        const { x, y } = runBox(spec, page, l, r)
        span.style.cssText = `left:${x}px;top:${y}px;font:${font(r)}`
        out.push(span)
      }
      if (l.hy && l.runs.length) {
        const r = l.runs[l.runs.length - 1] as (typeof l.runs)[0]
        const hy = document.createElement('span')
        hy.className = 'hy'
        hy.setAttribute('aria-hidden', 'true')
        hy.textContent = '-'
        const { y } = runBox(spec, page, l, r)
        hy.style.cssText = `left:${(r[0] + r[1]) * PX}px;top:${y}px;font:${font(r)}`
        out.push(hy)
      }
    })
    el.replaceChildren(...out)
    this.queue.delete(id)
  }

  /** Pages (indices) currently on screen, plus one either side. */
  visiblePages(): [number, number] {
    const top = -this.doc.getBoundingClientRect().top
    const ph = this.eng.spec.height * PX + GAP
    const a = Math.max(0, Math.floor(top / ph) - 1)
    const b = Math.floor((top + window.innerHeight) / ph) + 1
    return [a, b]
  }

  /** Redraw touched paragraphs: those in view now, the rest when idle. */
  update(touched: Set<string>, pageCountChanged: boolean): void {
    if (pageCountChanged) drawPages(this.pagesEl, this.eng.spec, this.eng.pages.length)
    const [a, b] = this.visiblePages()
    for (const id of touched) {
      const pages = this.eng.paraPages.get(id)
      const inView = !pages || pages.size === 0 || [...pages].some((p) => p >= a && p <= b)
      if (inView) this.renderPara(id)
      else this.queue.add(id)
    }
    if (this.queue.size && !this.idle) this.scheduleIdle()
  }

  private scheduleIdle(): void {
    const run = (deadline?: IdleDeadline) => {
      this.idle = 0
      const until = performance.now() + 8
      for (const id of this.queue) {
        this.renderPara(id)
        if (deadline ? deadline.timeRemaining() < 1 : performance.now() > until) break
      }
      if (this.queue.size) this.scheduleIdle()
    }
    this.idle =
      'requestIdleCallback' in window
        ? requestIdleCallback(run, { timeout: 200 })
        : (setTimeout(run, 16) as unknown as number)
  }

  /** Document point (CSS px from the doc's top-left) → page and point in pt. */
  toPage(xDoc: number, yDoc: number): { page: number; x: number; y: number } {
    const ph = this.eng.spec.height * PX + GAP
    const page = Math.max(0, Math.min(this.eng.pages.length - 1, Math.floor(yDoc / ph)))
    return { page, x: xDoc / PX, y: (yDoc - pageTop(this.eng.spec, page)) / PX }
  }
}
