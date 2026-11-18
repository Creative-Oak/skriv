// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// View A: ProseMirror's view, directed by the engine.
//
// ProseMirror owns a contenteditable with semantic blocks (<h1>–<h6>, <p>,
// <table>), so caret, selection, IME and the accessibility tree are the
// browser's own. The engine decides where every line goes: each paragraph
// is placed by a node decoration, each run on each line by an inline
// decoration that positions it absolutely, so the browser never breaks
// lines itself. ProseMirror's state projects the engine's document: each
// transaction is mirrored into the engine (changed paragraphs, or the
// whole document after structural edits), and the decorations of the
// paragraphs whose lines changed are rebuilt.

import '../style.css'
import { baseKeymap, splitBlock } from 'prosemirror-commands'
import { history, redo, undo } from 'prosemirror-history'
import { keymap } from 'prosemirror-keymap'
import { type Mark, type Node as PMNode, Schema } from 'prosemirror-model'
import {
  type Command,
  EditorState,
  Plugin,
  PluginKey,
  TextSelection,
  type Transaction,
} from 'prosemirror-state'
import { Decoration, DecorationSet, EditorView } from 'prosemirror-view'
import { Engine, type Line, type Outcome } from '../shared/engine'
import { drawPages, fit, GAP, PX, pageTop, runBox, font as runFont } from '../shared/geometry'
import { begin, keyDown, mountHud, setStatus, synthetic } from '../shared/probe'

const eng = await Engine.load()
fit(eng.spec)

const doc = document.querySelector('.doc') as HTMLElement
const pagesEl = doc.querySelector('.pages') as HTMLElement
const flowEl = doc.querySelector('.flow') as HTMLElement
drawPages(pagesEl, eng.spec, eng.pages.length)

// ---------------------------------------------------------------- schema

const schema = new Schema({
  nodes: {
    doc: { content: 'block+' },
    para: {
      group: 'block',
      content: 'inline*',
      attrs: {
        id: { default: '' },
        h: { default: 0 },
        label: { default: null },
        style: { default: '' },
        notes: { default: '[]' },
      },
      toDOM: (n) => [n.attrs.h ? `h${Math.min(n.attrs.h, 6)}` : 'p', { 'data-id': n.attrs.id }, 0],
    },
    table: {
      group: 'block',
      content: 'row+',
      attrs: { id: { default: '' }, cols: { default: '[]' } },
      isolating: true,
      toDOM: () => ['table', ['tbody', 0]],
    },
    row: { content: 'cell+', toDOM: () => ['tr', 0] },
    cell: {
      content: 'para+',
      attrs: { colspan: { default: 1 } },
      isolating: true,
      toDOM: (n) => ['td', n.attrs.colspan > 1 ? { colspan: n.attrs.colspan } : {}, 0],
    },
    text: { group: 'inline' },
    hard_break: { inline: true, group: 'inline', selectable: false, toDOM: () => ['br'] },
  },
  marks: {
    run: {
      attrs: { font: {}, size: {}, bold: {}, italic: {} },
      toDOM: () => ['span', 0],
    },
  },
})

// ------------------------------------------- engine document ↔ ProseMirror

interface InPara {
  id: string
  text: string
  spans: {
    range: { start: number; end: number }
    font: string
    size: number
    bold: boolean
    italic: boolean
  }[]
  style: { heading: number | null; label: string | null; font: string; size: number }
  notes: unknown[]
}
type InBlock =
  | { Para: InPara }
  | { Table: { id: string; cols: number[]; rows: { paras: InPara[]; colspan: number }[][] } }

function paraNode(p: InPara): PMNode {
  const chars = [...p.text]
  const content: PMNode[] = []
  for (const s of p.spans) {
    const mark = schema.marks.run.create({
      font: s.font,
      size: s.size,
      bold: s.bold,
      italic: s.italic,
    })
    // Hard line breaks are nodes in ProseMirror, characters in the engine.
    for (const [i, piece] of chars
      .slice(s.range.start, s.range.end)
      .join('')
      .split('\n')
      .entries()) {
      if (i > 0) content.push(schema.nodes.hard_break.create(null, null, [mark]))
      if (piece) content.push(schema.text(piece, [mark]))
    }
  }
  return schema.nodes.para.create(
    {
      id: p.id,
      h: p.style.heading ?? 0,
      label: p.style.label,
      style: JSON.stringify(p.style),
      notes: JSON.stringify(p.notes),
    },
    content,
  )
}

function toPM(json: string): PMNode {
  const d = JSON.parse(json) as { blocks: InBlock[] }
  return schema.nodes.doc.create(
    null,
    d.blocks.map((b) =>
      'Para' in b
        ? paraNode(b.Para)
        : schema.nodes.table.create(
            { id: b.Table.id, cols: JSON.stringify(b.Table.cols) },
            b.Table.rows.map((r) =>
              schema.nodes.row.create(
                null,
                r.map((c) =>
                  schema.nodes.cell.create({ colspan: c.colspan }, c.paras.map(paraNode)),
                ),
              ),
            ),
          ),
    ),
  )
}

/** A paragraph's text (hard breaks as '\n') and runs `[start, end, font, size, bold, italic]`. */
function fromPara(n: PMNode): {
  text: string
  spans: [number, number, string, number, boolean, boolean][]
} {
  let text = ''
  let len = 0
  const spans: [number, number, string, number, boolean, boolean][] = []
  const style = JSON.parse(n.attrs.style || '{}')
  n.forEach((child) => {
    const t = child.isText ? (child.text ?? '') : '\n'
    const m = child.marks.find((k: Mark) => k.type === schema.marks.run)?.attrs ?? {
      font: style.font ?? 'Liberation Serif',
      size: style.size ?? 12,
      bold: false,
      italic: false,
    }
    const n2 = [...t].length
    const last = spans[spans.length - 1]
    if (
      last &&
      last[2] === m.font &&
      last[3] === m.size &&
      last[4] === m.bold &&
      last[5] === m.italic
    )
      last[1] += n2
    else spans.push([len, len + n2, m.font, m.size, m.bold, m.italic])
    text += t
    len += n2
  })
  return { text, spans }
}

/** Page setup, read once (serialising the whole document per edit would be slow). */
const pageSetup = JSON.parse(eng.ed.doc_json()).page

function toEngine(d: PMNode): string {
  const page = pageSetup
  const para = (n: PMNode) => {
    const { text, spans } = fromPara(n)
    return {
      id: n.attrs.id,
      text,
      spans: spans.map(([s, e, f, z, b, i]) => ({
        range: { start: s, end: e },
        font: f,
        size: z,
        bold: b,
        italic: i,
      })),
      style: JSON.parse(n.attrs.style),
      notes: JSON.parse(n.attrs.notes),
    }
  }
  const blocks: unknown[] = []
  d.forEach((b) => {
    if (b.type === schema.nodes.para) blocks.push({ Para: para(b) })
    else {
      const rows: unknown[] = []
      b.forEach((r) => {
        const cells: unknown[] = []
        r.forEach((c) => {
          const paras: unknown[] = []
          c.forEach((p) => {
            paras.push(para(p))
          })
          cells.push({ paras, colspan: c.attrs.colspan })
        })
        rows.push(cells)
      })
      blocks.push({ Table: { id: b.attrs.id, cols: JSON.parse(b.attrs.cols), rows } })
    }
  })
  return JSON.stringify({ page, blocks })
}

/** Position of each paragraph (by ID) in a ProseMirror document. */
function paraPositions(d: PMNode): Map<string, number> {
  const m = new Map<string, number>()
  d.descendants((n, pos) => {
    if (n.type === schema.nodes.para) {
      m.set(n.attrs.id, pos)
      return false
    }
    return true
  })
  return m
}

/** ProseMirror position of character offset `off` inside the paragraph at `pos`. */
function charToPos(n: PMNode, pos: number, off: number): number {
  let chars = 0
  let p = pos + 1
  let found = -1
  n.forEach((child) => {
    if (found >= 0) return
    if (child.isText) {
      const t = child.text ?? ''
      const cs = [...t]
      if (off <= chars + cs.length) {
        found = p + cs.slice(0, off - chars).join('').length
        return
      }
      chars += cs.length
      p += t.length
    } else {
      if (off <= chars) {
        found = p
        return
      }
      chars += 1
      p += 1
    }
  })
  return found >= 0 ? found : p
}

/** Character offset of ProseMirror position `at` inside the paragraph at `pos`. */
function posToChar(n: PMNode, pos: number, at: number): number {
  let chars = 0
  let p = pos + 1
  let out = -1
  n.forEach((child) => {
    if (out >= 0) return
    const size = child.nodeSize
    if (at <= p + size && child.isText) {
      out = chars + [...(child.text ?? '').slice(0, at - p)].length
      return
    }
    if (at <= p) {
      out = chars
      return
    }
    chars += child.isText ? [...(child.text ?? '')].length : 1
    p += size
  })
  return out >= 0 ? out : chars
}

// ----------------------------------------------------------- decorations

/**
 * Paragraph elements by ID. Each paragraph is a node view that positions
 * its own element at its first line; with ~1,700 paragraphs, node
 * decorations at the top level made every view update quadratic
 * (`DecorationSet.forChild` scans them for each child).
 */
const paraDom = new Map<string, HTMLElement>()

/** Where a paragraph's element goes: its first line's top-left. */
function paraOrigin(id: string): { ox: number; oy: number; h: number } | null {
  const first = eng.linesOf(id)[0]
  if (!first) return null
  const page0 = eng.pageOfLine(id, first.s)
  return { ox: first.x0 * PX, oy: pageTop(eng.spec, page0) + first.y * PX, h: first.h }
}

function placePara(id: string, node: PMNode, el = paraDom.get(id)): void {
  const o = paraOrigin(id)
  if (!el || !o) return
  const style = JSON.parse(node.attrs.style || '{}')
  el.style.cssText = `left:${o.ox}px;top:${o.oy}px;font:${(style.size ?? 12) * PX}px/${o.h * PX}px "${style.font ?? 'Liberation Serif'}"`
}

function paraView(node: PMNode) {
  let n = node
  const dom = document.createElement(n.attrs.h ? `h${Math.min(n.attrs.h, 6)}` : 'p')
  dom.dataset.id = n.attrs.id
  paraDom.set(n.attrs.id, dom)
  placePara(n.attrs.id, n, dom)
  return {
    dom,
    contentDOM: dom,
    update(next: PMNode) {
      if (next.type !== n.type || next.attrs.h !== n.attrs.h) return false
      if (next.attrs.id !== n.attrs.id) {
        if (paraDom.get(n.attrs.id) === dom) paraDom.delete(n.attrs.id)
        paraDom.set(next.attrs.id, dom)
        dom.dataset.id = next.attrs.id
        placePara(next.attrs.id, next, dom)
      }
      n = next
      return true
    },
    destroy() {
      if (paraDom.get(n.attrs.id) === dom) paraDom.delete(n.attrs.id)
    },
  }
}

function decorate(id: string, pos: number, node: PMNode): Decoration[] {
  const lines = eng.linesOf(id)
  const first = lines[0]
  if (!first) return []
  const spec = eng.spec
  const page0 = eng.pageOfLine(id, first.s)
  const ox = first.x0 * PX
  const oy = pageTop(spec, page0) + first.y * PX
  placePara(id, node)
  const out: Decoration[] = []
  lines.forEach((l: Line, i) => {
    const page = eng.pageOfLine(id, l.s)
    if (i === 0 && node.attrs.label && l.runs[0]) {
      const r = l.runs[0]
      const b = runBox(spec, page, l, r)
      const lab = document.createElement('span')
      lab.className = 'lbl'
      lab.textContent = `${node.attrs.label} `
      lab.style.cssText = `left:${b.x - ox - 28 * PX}px;top:${b.y - oy}px;width:${22 * PX}px;text-align:right;font:${runFont(r)}`
      out.push(
        Decoration.widget(pos + 1, lab, {
          side: -1,
          id,
          ignoreSelection: true,
          key: `l${id}${b.x},${b.y}`,
        }),
      )
    }
    for (const r of l.runs) {
      const n = [...r[2]].length
      if (!n) continue
      const b = runBox(spec, page, l, r)
      out.push(
        Decoration.inline(
          charToPos(node, pos, r[3]),
          charToPos(node, pos, r[3] + n),
          {
            nodeName: 'span',
            class: 'r',
            style: `left:${b.x - ox}px;top:${b.y - oy}px;font:${runFont(r)}`,
          },
          { id, inclusiveStart: false, inclusiveEnd: true },
        ),
      )
    }
    const last = l.runs[l.runs.length - 1]
    if (l.hy && last) {
      const b = runBox(spec, page, l, last)
      const hy = document.createElement('span')
      hy.className = 'hy'
      hy.setAttribute('aria-hidden', 'true')
      hy.textContent = '-'
      hy.style.cssText = `left:${(last[0] + last[1]) * PX - ox}px;top:${b.y - oy}px;font:${runFont(last)}`
      out.push(
        Decoration.widget(charToPos(node, pos, l.e), hy, {
          side: -1,
          id,
          ignoreSelection: true,
          key: `h${id}${b.x},${b.y}`,
        }),
      )
    }
  })
  return out
}

interface DecoState {
  set: DecorationSet
}

const decoKey = new PluginKey<DecoState>('engine')
let lastOutcome: Outcome | null = null

/** Mirror a transaction into the engine; returns paragraph IDs to redecorate. */
function mirror(tr: Transaction): Set<string> {
  const before = paraIds(tr.before)
  const after = paraIds(tr.doc)
  const touched = new Set<string>()
  const take = (out: Outcome) => {
    lastOutcome = out
    for (const id of eng.absorb(out)) touched.add(id)
  }
  const pos = paraPositions(tr.doc)
  const sync = (id: string) => {
    const at = pos.get(id)
    const flat = eng.flatOf.get(id)
    if (at === undefined || flat === undefined) return
    const { text, spans } = fromPara(tr.doc.nodeAt(at) as PMNode)
    take(eng.call(() => eng.ed.set_para(flat, text, JSON.stringify(spans))))
  }
  const same = before.length === after.length && before.every((id, i) => id === after[i])
  if (!same) {
    // The two structural edits typing makes: a paragraph split in two
    // (Enter) or two joined (Backspace, Delete). Mirror them as engine
    // edits; anything else reloads the engine's document.
    const k = before.findIndex((id, i) => id !== after[i])
    if (
      after.length === before.length + 1 &&
      k > 0 &&
      after.slice(k + 1).every((id, i) => id === before[k + i])
    ) {
      const prev = after[k - 1] as string
      const flat = eng.flatOf.get(prev)
      const at = pos.get(prev)
      if (flat !== undefined && at !== undefined) {
        const len = [...fromPara(tr.doc.nodeAt(at) as PMNode).text].length
        take(eng.split(flat, len, after[k]))
        sync(prev)
        sync(after[k] as string)
        return touched
      }
    }
    if (
      after.length + 1 === before.length &&
      k > 0 &&
      before.slice(k + 1).every((id, i) => id === after[k + i])
    ) {
      const prev = before[k - 1] as string
      const flat = eng.flatOf.get(prev)
      if (flat !== undefined) {
        take(eng.delete(flat, eng.length(flat), flat + 1, 0))
        sync(prev)
        return touched
      }
    }
    take(eng.call(() => eng.ed.load(toEngine(tr.doc))))
    return touched
  }
  // Paragraphs inside the changed ranges.
  const changed = new Set<string>()
  tr.mapping.maps.forEach((map, i) => {
    const rest = tr.mapping.slice(i + 1)
    map.forEach((_a, _b, from, to) => {
      const f = rest.map(from, -1)
      const t = rest.map(to, 1)
      tr.doc.nodesBetween(Math.max(0, f - 1), Math.min(tr.doc.content.size, t + 1), (n) => {
        if (n.type === schema.nodes.para) {
          changed.add(n.attrs.id)
          return false
        }
        return true
      })
    })
  })
  for (const id of changed) sync(id)
  return touched
}

/**
 * In the document before `tr`: from the start of the paragraph before the
 * first one whose identity changed to the end of the next one, when
 * paragraphs were added or removed.
 */
function structuralRange(tr: Transaction): [number, number] | null {
  const before = paraIds(tr.before)
  const after = paraIds(tr.doc)
  if (before.length === after.length && before.every((id, i) => id === after[i])) return null
  let k = before.findIndex((id, i) => id !== after[i])
  if (k < 0) k = before.length
  const pos = paraPositions(tr.before)
  const a = before[Math.max(0, k - 1)]
  const b = before[Math.min(before.length - 1, k + 1)]
  const from = a === undefined ? 0 : (pos.get(a) ?? 0)
  const bAt = b === undefined ? undefined : pos.get(b)
  const to =
    bAt === undefined ? tr.before.content.size : bAt + (tr.before.nodeAt(bAt)?.nodeSize ?? 0)
  return [from, to]
}

function paraIds(d: PMNode): string[] {
  const out: string[] = []
  d.descendants((n) => {
    if (n.type === schema.nodes.para) {
      out.push(n.attrs.id)
      return false
    }
    return true
  })
  return out
}

/** Paragraph IDs whose decorations are out of date (off screen, redone when idle). */
const stale = new Set<string>()

function visible(id: string): boolean {
  const pages = eng.paraPages.get(id)
  if (!pages?.size) return true
  const top = -doc.getBoundingClientRect().top
  const ph = eng.spec.height * PX + GAP
  const a = Math.floor(top / ph) - 1
  const b = Math.floor((top + innerHeight) / ph) + 1
  return [...pages].some((p) => p >= a && p <= b)
}

function redecorate(set: DecorationSet, d: PMNode, ids: Iterable<string>): DecorationSet {
  const pos = paraPositions(d)
  const remove: Decoration[] = []
  const add: Decoration[] = []
  for (const id of ids) {
    const at = pos.get(id)
    stale.delete(id)
    if (at === undefined) continue
    const node = d.nodeAt(at) as PMNode
    // Whoever they were made for: after a split, the new paragraph still
    // carries (and straddles) decorations of the old one.
    // Everything overlapping the paragraph (neighbours' node decorations
    // only touch its edges).
    remove.push(
      ...set.find(at, at + node.nodeSize).filter((d) => d.to > at && d.from < at + node.nodeSize),
    )
    add.push(...decorate(id, at, node))
  }
  // In document order: `add` validates node decorations with a cursor that
  // only moves forward, and silently drops any that come out of order.
  add.sort((x, y) => x.from - y.from)
  return set.remove(remove).add(d, add)
}

const enginePlugin = new Plugin<DecoState>({
  key: decoKey,
  state: {
    init: (_c, state) => {
      const all: Decoration[] = []
      for (const [id, pos] of paraPositions(state.doc))
        all.push(...decorate(id, pos, state.doc.nodeAt(pos) as PMNode))
      return { set: DecorationSet.create(state.doc, all) }
    },
    apply: (tr, old) => {
      // Paragraphs split or joined: drop their decorations before mapping.
      // Mapped through a split, they straddle the new boundary and leave
      // the set in a state where decorations added later can go missing.
      let set = old.set
      if (tr.docChanged) {
        const range = structuralRange(tr)
        if (range)
          set = set.remove(
            set.find(range[0], range[1]).filter((d) => d.to > range[0] && d.from < range[1]),
          )
      }
      set = set.map(tr.mapping, tr.doc)
      const idle = tr.getMeta(decoKey) as string[] | undefined
      if (idle) set = redecorate(set, tr.doc, idle)

      if (!tr.docChanged) return { set }
      const touched = mirror(tr)
      const now: string[] = []
      for (const id of touched) {
        if (visible(id)) now.push(id)
        else stale.add(id)
      }
      set = redecorate(set, tr.doc, new Set(now))
      if (lastOutcome && lastOutcome.pages !== pagesEl.children.length)
        drawPages(pagesEl, eng.spec, eng.pages.length)
      return { set }
    },
  },
  props: {
    decorations: (state) => decoKey.getState(state)?.set,
  },
})

// ----------------------------------------------------------------- keys

let freshId = 0
/** Enter: split, and give the new paragraph its own ID. */
const enter: Command = (state, dispatch) =>
  splitBlock(state, (tr) => {
    const $from = tr.selection.$from
    if ($from.parent.type === schema.nodes.para) {
      freshId += 1
      tr.setNodeMarkup($from.before(), undefined, { ...$from.parent.attrs, id: `pm-${freshId}` })
    }
    dispatch?.(tr)
  })

function engineCaret(state: EditorState, at: number): [number, number] | null {
  const $p = state.doc.resolve(at)
  if ($p.parent.type !== schema.nodes.para) return null
  const flat = eng.flatOf.get($p.parent.attrs.id)
  if (flat === undefined) return null
  return [flat, posToChar($p.parent, $p.before(), at)]
}

function toPos(state: EditorState, flat: number, off: number): number | null {
  const id = eng.paras[flat]?.id
  const at = id ? paraPositions(state.doc).get(id) : undefined
  if (at === undefined) return null
  return charToPos(state.doc.nodeAt(at) as PMNode, at, off)
}

let goalX: number | null = null
/** Up and down by the engine's lines (the browser's own vertical movement doesn't know them). */
const vertical =
  (dir: number, extend: boolean): Command =>
  (state, dispatch) => {
    const head = state.selection.head
    const c = engineCaret(state, head)
    if (!c) return false
    const loc = eng.ed.locate(c[0], c[1])
    goalX ??= (loc[2] as number) ?? 0
    const g = goalX
    const to = eng.ed.vertical(c[0], c[1], g, dir)
    if (to.length !== 2) return true
    const p = toPos(state, to[0] as number, to[1] as number)
    if (p === null) return true
    dispatch?.(
      state.tr
        .setSelection(TextSelection.create(state.doc, extend ? state.selection.anchor : p, p))
        .scrollIntoView()
        .setMeta('goalX', g),
    )
    return true
  }

const state = EditorState.create({
  doc: toPM(eng.ed.doc_json()),
  plugins: [
    history(),
    keymap({
      'Mod-z': undo,
      'Shift-Mod-z': redo,
      'Mod-y': redo,
      Enter: enter,
      ArrowUp: vertical(-1, false),
      ArrowDown: vertical(1, false),
      'Shift-ArrowUp': vertical(-1, true),
      'Shift-ArrowDown': vertical(1, true),
    }),
    keymap(baseKeymap),
    enginePlugin,
  ],
})

// ------------------------------------------------------- view and probe

let lastInput: number | null = null
const view: EditorView = new EditorView(flowEl, {
  state,
  nodeViews: { para: (node) => paraView(node) },
  attributes: {
    'aria-label': 'Document',
    'aria-multiline': 'true',
    role: 'textbox',
    spellcheck: 'false',
  },
  dispatchTransaction(tr) {
    if (!tr.getMeta('goalX')) goalX = null
    const t = tr.docChanged ? begin(lastInput, 'type', 0) : null
    lastInput = null
    view.updateState(view.state.apply(tr))
    void t?.done()
  },
})
view.dom.addEventListener('keydown', (e) => keyDown(e), true)
for (const type of ['beforeinput', 'compositionupdate', 'compositionend'])
  view.dom.addEventListener(
    type,
    (e) => {
      lastInput = e.timeStamp
    },
    true,
  )

// Redecorate paragraphs left stale (off screen) in idle time.
function idle(): void {
  const run = () => {
    if (stale.size) {
      const ids = [...stale].slice(0, 100)
      view.dispatch(view.state.tr.setMeta(decoKey, ids).setMeta('addToHistory', false))
    }
    setTimeout(idle, stale.size ? 0 : 250)
  }
  'requestIdleCallback' in window ? requestIdleCallback(run, { timeout: 500 }) : setTimeout(run, 50)
}
idle()

function place(page: number, inTable = false): void {
  const tables = new Set<string>()
  for (const b of eng.structure)
    if (b.t === 'table')
      for (const r of b.rows) for (const c of r) for (const p of c.paras) tables.add(p.id)
  let target: [number, number] = [0, 0]
  search: for (let pg = page; pg < eng.pages.length; pg++) {
    for (const l of eng.pages[pg] ?? []) {
      const len = eng.length(l.p)
      if (inTable ? tables.has(l.id) && len > 5 : !tables.has(l.id) && len > 200) {
        target = [l.p, Math.floor(len / 2)]
        break search
      }
    }
  }
  const p = toPos(view.state, target[0], target[1]) ?? 1
  view.focus()
  view.dispatch(
    view.state.tr.setSelection(TextSelection.create(view.state.doc, p)).scrollIntoView(),
  )
}

const TEXT = [...'Æbleøen på Ålborg havn '] as string[]
function runButton(label: string, page: number, inTable: boolean): HTMLElement {
  const b = document.createElement('button')
  b.type = 'button'
  b.textContent = label
  b.addEventListener('click', () => {
    place(page, inTable)
    void synthetic(1000, (i) => {
      const t = begin(null, 'synthetic', page)
      view.updateState(view.state.apply(view.state.tr.insertText(TEXT[i % TEXT.length] as string)))
      return t
    })
  })
  return b
}

mountHud('A-ProseMirror', [
  runButton('1000 × p.1', 0, false),
  runButton('1000 × p.50', 49, false),
  runButton('1000 × table p.80', 79, true),
])
setStatus(`View A (ProseMirror) · ${eng.pages.length} pages · engine hash ${eng.ed.hash()}`)
Object.assign((window as unknown as { spike2: object }).spike2, {
  place,
  pageCount: () => eng.pages.length,
  engineText: (p: number) => eng.text(p),
  pmText: () => view.state.doc.textBetween(0, view.state.doc.content.size, '\n', '\n'),
  view,
  eng,
})
place(0)
document.documentElement.dataset.ready = '1'
