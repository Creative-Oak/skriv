// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Points (the engine) to CSS pixels (the view), page stacking, and the
// CSS for a run so the browser draws its glyphs on the engine's baseline.

import type { Line, PageSpec, Run } from './engine'

export const GAP = 24

/** CSS px per point: 96/72, scaled down to fit narrow screens. */
export let PX = 4 / 3

export function fit(spec: PageSpec): void {
  const avail = Math.max(320, window.innerWidth - 16)
  PX = Math.min(4 / 3, avail / spec.width)
}

export function pageTop(spec: PageSpec, page: number): number {
  return page * (spec.height * PX + GAP)
}

/**
 * Where a glyph's baseline sits in a `line-height: 1` box, as a fraction of
 * the font size: half the leading plus the ascent, from the fonts' hhea
 * tables (Liberation 2.1.5, units per em 2048).
 */
const BASELINE: Record<string, number> = {
  'Liberation Serif': (1 + 0.89111328125 - 0.21630859375) / 2,
  'Liberation Sans': (1 + 0.9052734375 - 0.2119140625) / 2,
}

export function font(r: Run): string {
  return `${r[7] ? 'italic ' : ''}${r[6] ? 'bold ' : ''}${r[5] * PX}px/1 "${r[4]}"`
}

/** Absolute position of a run (relative to the document's top-left). */
export function runBox(spec: PageSpec, page: number, l: Line, r: Run): { x: number; y: number } {
  const k = BASELINE[r[4]] ?? 0.84
  return { x: r[0] * PX, y: pageTop(spec, page) + (l.b - k * r[5]) * PX }
}

/** Draw the pages (paper, margins guide, page numbers): presentation only. */
export function drawPages(host: HTMLElement, spec: PageSpec, count: number): void {
  host.setAttribute('aria-hidden', 'true')
  while (host.children.length > count) host.lastElementChild?.remove()
  for (let i = 0; i < count; i++) {
    let el = host.children[i] as HTMLElement | undefined
    if (!el) {
      el = document.createElement('div')
      el.className = 'page'
      el.innerHTML = '<span class="pnum"></span>'
      host.append(el)
    }
    Object.assign(el.style, {
      top: `${pageTop(spec, i)}px`,
      width: `${spec.width * PX}px`,
      height: `${spec.height * PX}px`,
    })
    const n = el.firstElementChild as HTMLElement
    n.textContent = `${i + 1}`
    n.style.top = `${(spec.height - spec.bottom / 2) * PX}px`
  }
  const doc = host.parentElement
  if (doc) {
    doc.style.width = `${spec.width * PX}px`
    doc.style.height = `${pageTop(spec, count)}px`
  }
}
