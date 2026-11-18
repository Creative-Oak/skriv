// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// View B: an owned view. The engine lays out lines and pages; this file
// draws the caret and selection itself and takes text input from
//
// - EditContext (Chrome, Edge): the flow element is the editing host; the
//   EditContext holds the caret's paragraph, and the DOM selection mirrors
//   the model so assistive technology can follow the caret; or
// - a hidden textarea (Firefox, Safari, or `?input=textarea`) holding the
//   caret's paragraph, positioned at the caret for IME windows; edits are
//   read back by diffing its value.
//
// Paragraph text is the unit both surfaces hold, so IME and autocorrect
// see real context; cross-paragraph edits (Enter, Backspace at a
// paragraph's start, deleting a selection) are handled here.

import '../style.css'
import { Engine, type Outcome } from '../shared/engine'
import { fit, PX, pageTop } from '../shared/geometry'
import { begin, keyDown, mountHud, type Sample, setStatus, synthetic } from '../shared/probe'
import { Flow } from './flow'

type Pos = [number, number]

/** Per-phase time accumulators (ms), for finding where DOM time goes. */
const phases: Record<string, number> = {}
let phaseT = 0
function phase(name: string): void {
  const now = performance.now()
  if (name !== 'start') phases[name] = (phases[name] ?? 0) + now - phaseT
  phaseT = now
}

const params = new URLSearchParams(location.search)
const eng = await Engine.load()
fit(eng.spec)

const doc = document.querySelector('.doc') as HTMLElement
const pagesEl = doc.querySelector('.pages') as HTMLElement
const flowEl = doc.querySelector('.flow') as HTMLElement
const overlay = doc.querySelector('.overlay') as HTMLElement
const view = new Flow(eng, doc, pagesEl, flowEl)
view.build()

// ------------------------------------------------------------- selection

let anchor: Pos = [0, 0]
let head: Pos = [0, 0]
let goalX: number | null = null
let composing: Pos | null = null

const cmp = (a: Pos, b: Pos) => a[0] - b[0] || a[1] - b[1]
const collapsed = () => cmp(anchor, head) === 0
const ordered = (): [Pos, Pos] => (cmp(anchor, head) <= 0 ? [anchor, head] : [head, anchor])

const caretEl = document.createElement('div')
caretEl.className = 'caret'
const rectPool: HTMLElement[] = []

function draw(scroll = true): void {
  phase('start')
  for (const r of rectPool) r.remove()
  rectPool.length = 0
  overlay.append(caretEl)
  if (!collapsed()) {
    const [a, b] = ordered()
    addRects(eng.ed.selection_rects(a[0], a[1], b[0], b[1]), 'sel')
  }
  if (composing && cmp(composing, head) < 0)
    addRects(eng.ed.selection_rects(composing[0], composing[1], head[0], head[1]), 'comp')
  const c = eng.ed.locate(head[0], head[1])
  if (c.length) {
    const [page, , x, y, h] = c as unknown as number[]
    const left = (x as number) * PX
    const top = pageTop(eng.spec, page as number) + (y as number) * PX
    const height = (h as number) * PX
    Object.assign(caretEl.style, { left: `${left}px`, top: `${top}px`, height: `${height}px` })
    // Solid while typing, blinking when idle; no forced reflow.
    caretEl.classList.remove('blink')
    clearTimeout(blinkTimer)
    blinkTimer = setTimeout(() => caretEl.classList.add('blink'), 500) as unknown as number
    // Where the caret is on screen, from cached geometry (reading layout
    // here would force it on every keystroke).
    const rect = new DOMRect(docOrigin.x - scrollX + left, docOrigin.y - scrollY + top, 1, height)
    if (scroll && (rect.top < 0 || rect.bottom > innerHeight - 90))
      scrollTo({ top: scrollY + rect.top - innerHeight / 3 })
    surface.place(rect)
  }
  phase('caret')
  surface.mirror()
  phase('mirror')
}

let blinkTimer = 0
/** The document's top-left in page coordinates (changes only on resize). */
const docOrigin = { x: 0, y: 0 }
function measureOrigin(): void {
  const r = doc.getBoundingClientRect()
  docOrigin.x = r.left + scrollX
  docOrigin.y = r.top + scrollY
}
measureOrigin()
addEventListener('resize', measureOrigin)

function addRects(rs: Float32Array | number[], cls: string): void {
  for (let i = 0; i + 4 < rs.length; i += 5) {
    const el = document.createElement('div')
    el.className = cls
    const page = rs[i] as number
    Object.assign(el.style, {
      left: `${(rs[i + 1] as number) * PX}px`,
      top: `${pageTop(eng.spec, page) + (rs[i + 2] as number) * PX}px`,
      width: `${(rs[i + 3] as number) * PX}px`,
      height: `${(rs[i + 4] as number) * PX}px`,
    })
    overlay.append(el)
    rectPool.push(el)
  }
}

function select(a: Pos, h: Pos = a, keepGoal = false): void {
  anchor = a
  head = h
  if (!keepGoal) goalX = null
  surface.sync()
  draw()
}

// --------------------------------------------------------------- editing

const undo: string[] = []
const redo: string[] = []
let lastEdit = 0
let lastKind = ''
/**
 * The document as of the last quiet moment, serialised in idle time so no
 * keystroke pays for it; each burst of typing pushes it as its undo step.
 */
let settled = JSON.stringify({ doc: eng.ed.doc_json(), anchor, head })
let settleTimer = 0

function checkpoint(kind: string): void {
  const now = performance.now()
  if (kind !== lastKind || now - lastEdit > 1000 || kind !== 'type') {
    undo.push(settled)
    if (undo.length > 100) undo.shift()
    redo.length = 0
  }
  lastEdit = now
  lastKind = kind
  clearTimeout(settleTimer)
  settleTimer = setTimeout(() => {
    settled = JSON.stringify({ doc: eng.ed.doc_json(), anchor, head })
  }, 300) as unknown as number
}

function restore(from: string[], to: string[]): void {
  const snap = from.pop()
  if (!snap) return
  to.push(JSON.stringify({ doc: eng.ed.doc_json(), anchor, head }))
  const s = JSON.parse(snap)
  const t = begin(null, 'undo', head[0])
  commit(
    eng.call(() => eng.ed.load(s.doc)),
    null,
  )
  settled = snap
  select(s.anchor, s.head)
  void t.done()
}

/** Apply an engine outcome to the view; returns the probe handle's promise. */
function commit(out: Outcome, caret: Pos | null): boolean {
  if (!out.ok) {
    setStatus(`Refused: ${out.error}`)
    return false
  }
  const before = eng.pages.length
  phase('start')
  const touched = eng.absorb(out)
  phase('absorb')
  if (out.structural) view.reconcile()
  view.update(touched, before !== out.pages)
  phase('render')
  if (caret) {
    anchor = caret
    head = caret
    goalX = null
  }
  return true
}

function edit(kind: string, ts: number | null, f: () => Outcome): Promise<Sample> | null {
  const t = begin(ts, kind, eng.ed.locate(head[0], head[1])[0] ?? 0)
  checkpoint(kind)
  const out = f()
  if (!commit(out, out.caret as Pos)) return null
  surface.sync()
  draw()
  return t.done()
}

/** Delete the selection (if any); true when something was deleted. */
function deleteSelection(ts: number | null): boolean {
  if (collapsed()) return false
  const [a, b] = ordered()
  edit('delete', ts, () => eng.delete(a[0], a[1], b[0], b[1]))
  return true
}

function typeText(text: string, ts: number | null): void {
  deleteSelection(ts)
  edit('type', ts, () => eng.insert(head[0], head[1], text))
}

function enter(ts: number | null): void {
  deleteSelection(ts)
  edit('enter', ts, () => eng.split(head[0], head[1]))
}

function backspace(ts: number | null): void {
  if (deleteSelection(ts)) return
  const [p, o] = head
  if (o > 0) edit('delete', ts, () => eng.delete(p, o - 1, p, o))
  else if (p > 0) edit('join', ts, () => eng.delete(p - 1, eng.length(p - 1), p, 0))
}

function forwardDelete(ts: number | null): void {
  if (deleteSelection(ts)) return
  const [p, o] = head
  if (o < eng.length(p)) edit('delete', ts, () => eng.delete(p, o, p, o + 1))
  else if (p + 1 < eng.paras.length) edit('join', ts, () => eng.delete(p, o, p + 1, 0))
}

// ------------------------------------------------------- input surfaces

/** UTF-16 offset in `text` → character offset, and back. */
const toChars = (text: string, u: number) => [...text.slice(0, u)].length
const toUnits = (text: string, c: number) => [...text].slice(0, c).join('').length

interface Surface {
  readonly name: string
  focus(): void
  /** Load the caret's paragraph and the selection into the surface. */
  sync(): void
  /** Tell the surface where the caret is on screen (IME windows). */
  place(caret: DOMRect): void
  /** Mirror the model selection into the DOM (for assistive technology). */
  mirror(): void
}

function domPoint(pos: Pos): [Node, number] | null {
  const id = eng.paras[pos[0]]?.id
  const el = id ? view.paraEls.get(id) : undefined
  if (!el) return null
  let best: [Node, number] | null = [el, 0]
  for (const span of el.querySelectorAll<HTMLElement>('.r')) {
    const s = Number(span.dataset.s)
    const t = span.firstChild
    if (!t) continue
    const text = t.textContent ?? ''
    const n = [...text].length
    if (pos[1] >= s && pos[1] <= s + n) best = [t, toUnits(text, pos[1] - s)]
    if (pos[1] < s + n) break
  }
  return best
}

class EditContextSurface implements Surface {
  readonly name = 'EditContext'
  ec: EditContext
  para = -1
  text = ''

  constructor() {
    this.ec = new EditContext()
    flowEl.editContext = this.ec
    flowEl.tabIndex = 0
    if (params.get('role') === 'textbox') {
      flowEl.setAttribute('role', 'textbox')
      flowEl.setAttribute('aria-multiline', 'true')
    }
    flowEl.setAttribute('aria-label', 'Document')
    this.ec.addEventListener('textupdate', (e) => this.onText(e as TextUpdateEvent))
    this.ec.addEventListener('textformatupdate', (e) => {
      const fs = (e as TextFormatUpdateEvent).getTextFormats()
      const f0 = fs[0]
      composing = f0 ? [this.para, toChars(this.text, f0.rangeStart)] : null
      draw(false)
    })
    this.ec.addEventListener('compositionend', () => {
      composing = null
      draw(false)
    })
    this.ec.addEventListener('characterboundsupdate', (e) => {
      const ev = e as CharacterBoundsUpdateEvent
      const rects: DOMRect[] = []
      const base = doc.getBoundingClientRect()
      for (let u = ev.rangeStart; u < ev.rangeEnd; u++) {
        const c = eng.ed.locate(this.para, toChars(this.text, u))
        const c2 = eng.ed.locate(this.para, toChars(this.text, u + 1))
        if (c.length < 5 || c2.length < 5) continue
        const [pg, , x, y, h] = c as unknown as number[]
        const x2 = (c2[1] === c[1] ? (c2[2] as number) : (x as number) + 4) * PX
        rects.push(
          new DOMRect(
            base.left + (x as number) * PX,
            base.top + pageTop(eng.spec, pg as number) + (y as number) * PX,
            Math.max(1, x2 - (x as number) * PX),
            (h as number) * PX,
          ),
        )
      }
      this.ec.updateCharacterBounds(ev.rangeStart, rects)
    })
  }

  onText(e: TextUpdateEvent): void {
    // The EditContext has already changed its own copy of the paragraph.
    const a = toChars(this.text, e.updateRangeStart)
    const b = toChars(this.text, e.updateRangeEnd)
    const p = this.para
    this.text = this.ec.text
    const caret: Pos = [p, toChars(this.text, e.selectionEnd)]
    const t = begin(e.timeStamp, composing ? 'compose' : 'type', 0)
    checkpoint('type')
    let out: Outcome | null = null
    if (b > a) out = eng.delete(p, a, p, b)
    if (e.text) out = eng.insert(p, a, e.text)
    if (out && commit(out, caret)) {
      anchor = caret
      head = caret
      draw()
      void t.done()
    }
  }

  focus(): void {
    flowEl.focus({ preventScroll: true })
  }

  sync(): void {
    if (composing) return
    const p = head[0]
    const text = eng.text(p)
    if (p !== this.para) {
      this.ec.updateText(0, this.ec.text.length, text)
    } else if (text !== this.text) {
      // Only the changed range, as typing into the EditContext would.
      const old = this.text
      let pre = 0
      while (pre < old.length && pre < text.length && old[pre] === text[pre]) pre++
      let suf = 0
      while (
        suf < old.length - pre &&
        suf < text.length - pre &&
        old[old.length - 1 - suf] === text[text.length - 1 - suf]
      )
        suf++
      this.ec.updateText(pre, old.length - suf, text.slice(pre, text.length - suf))
    }
    this.para = p
    this.text = text
    const a = anchor[0] === p ? anchor[1] : head[1]
    this.ec.updateSelection(toUnits(text, a), toUnits(text, head[1]))
  }

  place(caret: DOMRect): void {
    const w = eng.spec.width * PX
    this.ec.updateControlBounds(
      new DOMRect(
        docOrigin.x - scrollX,
        docOrigin.y - scrollY,
        w,
        pageTop(eng.spec, eng.pages.length),
      ),
    )
    this.ec.updateSelectionBounds(caret)
  }

  mirror(): void {
    const a = domPoint(anchor)
    const h = domPoint(head)
    if (!a || !h) return
    getSelection()?.setBaseAndExtent(a[0], a[1], h[0], h[1])
  }
}

class TextareaSurface implements Surface {
  readonly name = 'textarea'
  ta = document.createElement('textarea')
  para = -1
  text = ''

  constructor() {
    const ta = this.ta
    ta.className = 'input'
    ta.setAttribute('aria-label', 'Document text at the caret')
    ta.spellcheck = false
    doc.append(ta)
    ta.addEventListener('beforeinput', (e) => this.onBeforeInput(e))
    ta.addEventListener('input', (e) => this.onInput(e as InputEvent))
    ta.addEventListener('compositionstart', () => {
      deleteSelectionForComposition()
      composing = [this.para, toChars(ta.value, ta.selectionStart)]
    })
    ta.addEventListener('compositionend', (e) => {
      composing = null
      this.onInput(e as unknown as InputEvent)
    })
    ta.addEventListener('keydown', (e) => onKey(e))
  }

  onBeforeInput(e: InputEvent): void {
    const t = e.inputType
    const ta = this.ta
    const atStart = ta.selectionStart === 0 && ta.selectionEnd === 0
    const atEnd = ta.selectionStart === ta.value.length && ta.selectionEnd === ta.value.length
    if (t === 'insertParagraph' || t === 'insertLineBreak') {
      e.preventDefault()
      enter(e.timeStamp)
    } else if (
      !collapsed() &&
      anchor[0] !== head[0] &&
      !e.isComposing &&
      t !== 'insertCompositionText'
    ) {
      // A selection across paragraphs: the textarea only holds one.
      e.preventDefault()
      if (t.startsWith('delete')) deleteSelection(e.timeStamp)
      else if (e.data) typeText(e.data, e.timeStamp)
    } else if (t === 'deleteContentBackward' && atStart && collapsed()) {
      e.preventDefault()
      backspace(e.timeStamp)
    } else if (t === 'deleteContentForward' && atEnd && collapsed()) {
      e.preventDefault()
      forwardDelete(e.timeStamp)
    }
  }

  /** Read the textarea back: whatever changed is the edit. */
  onInput(e: InputEvent): void {
    const ta = this.ta
    const old = this.text
    const now = ta.value
    if (now === old) {
      this.selectFromTextarea()
      return
    }
    let pre = 0
    while (pre < old.length && pre < now.length && old[pre] === now[pre]) pre++
    let suf = 0
    while (
      suf < old.length - pre &&
      suf < now.length - pre &&
      old[old.length - 1 - suf] === now[now.length - 1 - suf]
    )
      suf++
    // Don't split a surrogate pair.
    if (pre > 0 && /[\uD800-\uDBFF]/.test(old[pre - 1] ?? '')) pre--
    const p = this.para
    const a = toChars(old, pre)
    const b = toChars(old, old.length - suf)
    const ins = now.slice(pre, now.length - suf)
    this.text = now
    const t = begin(e.timeStamp ?? null, composing ? 'compose' : 'type', 0)
    checkpoint('type')
    let out: Outcome | null = null
    if (b > a) out = eng.delete(p, a, p, b)
    if (ins) out = eng.insert(p, a, ins)
    const caret: Pos = [p, toChars(now, ta.selectionEnd)]
    if (out && commit(out, caret)) {
      anchor = [p, toChars(now, ta.selectionStart)]
      head = caret
      draw()
      void t.done()
    }
  }

  selectFromTextarea(): void {
    const p = this.para
    anchor = [p, toChars(this.text, this.ta.selectionStart)]
    head = [p, toChars(this.text, this.ta.selectionEnd)]
    draw()
  }

  focus(): void {
    this.ta.focus({ preventScroll: true })
  }

  sync(): void {
    if (composing) return
    const p = head[0]
    const text = eng.text(p)
    if (p !== this.para || text !== this.ta.value) {
      this.ta.value = text
      this.para = p
    }
    this.text = text
    const a = anchor[0] === p ? anchor[1] : head[1]
    const [s, e] = [toUnits(text, a), toUnits(text, head[1])]
    this.ta.setSelectionRange(Math.min(s, e), Math.max(s, e), s <= e ? 'forward' : 'backward')
  }

  place(caret: DOMRect): void {
    this.ta.style.left = `${caret.left - docOrigin.x + scrollX}px`
    this.ta.style.top = `${caret.top - docOrigin.y + scrollY}px`
    this.ta.style.height = `${caret.height}px`
  }

  mirror(): void {}
}

function deleteSelectionForComposition(): void {
  if (!collapsed() && anchor[0] !== head[0]) deleteSelection(null)
}

const useEC = 'EditContext' in window && params.get('input') !== 'textarea'
const surface: Surface = useEC ? new EditContextSurface() : new TextareaSurface()
document.documentElement.dataset.input = surface.name

// ------------------------------------------------------------- keyboard

function lineEnds(pos: Pos): [number, number] {
  const c = eng.ed.locate(pos[0], pos[1])
  if (c.length < 2) return [0, eng.length(pos[0])]
  const line = eng.pages[c[0] as number]?.[c[1] as number]
  return line ? [line.s, line.e] : [0, eng.length(pos[0])]
}

function step(pos: Pos, dir: number, word: boolean): Pos {
  const [p, o] = pos
  const text = [...eng.text(p)]
  if (dir < 0) {
    if (o === 0) return p > 0 ? [p - 1, eng.length(p - 1)] : pos
    let k = o - 1
    if (word) {
      while (k > 0 && /\s/.test(text[k] ?? '')) k--
      while (k > 0 && !/\s/.test(text[k - 1] ?? '')) k--
    }
    return [p, k]
  }
  if (o >= text.length) return p + 1 < eng.paras.length ? [p + 1, 0] : pos
  let k = o + 1
  if (word) {
    while (k < text.length && !/\s/.test(text[k] ?? '')) k++
    while (k < text.length && /\s/.test(text[k] ?? '')) k++
  }
  return [p, k]
}

// Returns whatever the action returned (nothing); the early returns keep each case one line.
function onKey(e: KeyboardEvent) {
  keyDown(e)
  if (e.isComposing || e.keyCode === 229) return
  const mod = e.metaKey || e.ctrlKey
  const word = e.altKey || (e.ctrlKey && !e.metaKey)
  const move = (to: Pos, keepGoal = false) => {
    e.preventDefault()
    select(e.shiftKey ? anchor : to, to, keepGoal)
  }
  switch (e.key) {
    case 'ArrowLeft':
      if (!e.shiftKey && !collapsed()) return move(ordered()[0])
      return move(e.metaKey ? [head[0], lineEnds(head)[0]] : step(head, -1, word))
    case 'ArrowRight':
      if (!e.shiftKey && !collapsed()) return move(ordered()[1])
      return move(e.metaKey ? [head[0], lineEnds(head)[1]] : step(head, 1, word))
    case 'ArrowUp':
    case 'ArrowDown': {
      const c = eng.ed.locate(head[0], head[1])
      goalX ??= (c[2] as number) ?? 0
      const g = goalX
      const to = eng.ed.vertical(head[0], head[1], g, e.key === 'ArrowUp' ? -1 : 1)
      if (to.length === 2) move([to[0] as number, to[1] as number], true)
      else e.preventDefault()
      goalX = g
      return
    }
    case 'Home':
      return move([head[0], lineEnds(head)[0]])
    case 'End':
      return move([head[0], lineEnds(head)[1]])
    case 'Enter':
      e.preventDefault()
      return enter(e.timeStamp)
    // The textarea handles these itself (its beforeinput handler catches
    // paragraph boundaries); the EditContext only holds one paragraph.
    case 'Backspace':
      if (useEC && (!collapsed() || head[1] === 0)) {
        e.preventDefault()
        backspace(e.timeStamp)
      }
      return
    case 'Delete':
      if (useEC && (!collapsed() || head[1] === eng.length(head[0]))) {
        e.preventDefault()
        forwardDelete(e.timeStamp)
      }
      return
  }
  if (mod && e.key.toLowerCase() === 'a') {
    e.preventDefault()
    const last = eng.paras.length - 1
    return select([0, 0], [last, eng.length(last)])
  }
  if (mod && e.key.toLowerCase() === 'z') {
    e.preventDefault()
    if (e.shiftKey) restore(redo, undo)
    else restore(undo, redo)
    return
  }
  if (mod && e.key.toLowerCase() === 'y') {
    e.preventDefault()
    return restore(redo, undo)
  }
  // Typing over a selection that spans paragraphs: the surface only
  // holds one, so delete it here first and let the key through.
  if (!mod && e.key.length === 1 && !collapsed() && anchor[0] !== head[0]) {
    deleteSelection(e.timeStamp)
    surface.sync()
  }
}

if (useEC) flowEl.addEventListener('keydown', onKey)

// ---------------------------------------------------------------- mouse

function posAt(e: PointerEvent): Pos | null {
  const r = doc.getBoundingClientRect()
  const { page, x, y } = view.toPage(e.clientX - r.left, e.clientY - r.top)
  const h = eng.ed.hit(page, x, y)
  return h.length === 2 ? [h[0] as number, h[1] as number] : null
}

let dragging = false
let downAt: { x: number; y: number; type: string } | null = null
doc.addEventListener('pointerdown', (e) => {
  if ((e.target as HTMLElement).closest('.hud')) return
  downAt = { x: e.clientX, y: e.clientY, type: e.pointerType }
  if (e.pointerType === 'touch') return // a tap places the caret on pointerup
  e.preventDefault()
  const p = posAt(e)
  if (!p) return
  surface.focus()
  select(e.shiftKey ? anchor : p, p)
  dragging = true
  doc.setPointerCapture(e.pointerId)
})
doc.addEventListener('pointermove', (e) => {
  if (!dragging) return
  const p = posAt(e)
  if (p && cmp(p, head) !== 0) {
    head = p
    draw(false)
  }
})
doc.addEventListener('pointerup', (e) => {
  dragging = false
  if (downAt?.type === 'touch' && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) < 10) {
    const p = posAt(e)
    if (p) {
      select(p)
      surface.focus()
    }
  }
  downAt = null
  surface.sync()
})

// ----------------------------------------------------------- probe, hooks

/** A paragraph to type into on `page` (body text, or a table cell). */
function target(page: number, inTable: boolean): Pos {
  const tables = new Set<string>()
  for (const b of eng.structure)
    if (b.t === 'table')
      for (const r of b.rows) for (const c of r) for (const p of c.paras) tables.add(p.id)
  for (let pg = page; pg < eng.pages.length; pg++) {
    for (const l of eng.pages[pg] ?? []) {
      const len = eng.length(l.p)
      if (inTable ? tables.has(l.id) && len > 5 : !tables.has(l.id) && len > 200)
        return [l.p, Math.floor(len / 2)]
    }
  }
  return [0, 0]
}

function place(page: number, inTable = false): void {
  surface.focus()
  select(target(page, inTable))
}

const TEXT = [...'Æbleøen på Ålborg havn '] as string[]
function runButton(label: string, page: number, inTable: boolean): HTMLElement {
  const b = document.createElement('button')
  b.type = 'button'
  b.textContent = label
  b.addEventListener('click', () => {
    place(page, inTable)
    void synthetic(1000, (i) => {
      const h = head
      const t = begin(null, 'synthetic', page)
      const out = eng.insert(h[0], h[1], TEXT[i % TEXT.length] as string)
      if (!commit(out, out.caret as Pos)) return null
      phase('start')
      surface.sync()
      phase('sync')
      draw()
      return t
    })
  })
  return b
}

mountHud(`B-${surface.name}`, [
  runButton('1000 × p.1', 0, false),
  runButton('1000 × p.50', 49, false),
  runButton('1000 × table p.80', 79, true),
])
setStatus(
  `View B (owned) · input: ${surface.name} · ${eng.pages.length} pages · engine hash ${eng.ed.hash()}`,
)
Object.assign((window as unknown as { spike2: object }).spike2, {
  place,
  selection: () => ({ anchor, head }),
  text: (p: number) => eng.text(p),
  pageCount: () => eng.pages.length,
  phases,
  eng,
})
place(0)
document.documentElement.dataset.ready = '1'
