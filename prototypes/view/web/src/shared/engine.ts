// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The editing core (Rust/wasm, `prototypes/view/engine`) and a cache of
// what is on each page, shared by both views.

import init, { Editor } from '../../public/pkg/view_engine.js'

/** A run on a line: x, width, text, start offset, font, size, bold, italic. */
export type Run = [number, number, string, number, string, number, boolean, boolean]

export interface Line {
  id: string
  p: number
  y: number
  h: number
  b: number
  s: number
  e: number
  hy: boolean
  x0: number
  runs: Run[]
}

export interface ParaInfo {
  id: string
  flat: number
  h: number
  label: string | null
  indent: number
}

export type BlockInfo =
  | ({ t: 'p' } & ParaInfo)
  | {
      t: 'table'
      id: string
      cols: number[]
      rows: { span: number; paras: ParaInfo[] }[][]
    }

export interface Outcome {
  ok: boolean
  error?: string
  pages: number
  dirty: number[]
  structural: boolean
  caret: [number, number]
}

export interface PageSpec {
  width: number
  height: number
  top: number
  bottom: number
  left: number
  right: number
}

/** Timings of the last engine call, in ms. */
export const timing = { engine: 0 }

export class Engine {
  readonly ed: Editor
  readonly spec: PageSpec
  /** Lines per page. */
  pages: Line[][] = []
  /** Pages each paragraph (by ID) has lines on. */
  paraPages = new Map<string, Set<number>>()
  structure: BlockInfo[] = []
  /** Flat index → paragraph info, and ID → flat index. */
  paras: ParaInfo[] = []
  flatOf = new Map<string, number>()

  private constructor(ed: Editor) {
    this.ed = ed
    this.spec = JSON.parse(ed.page_spec())
    this.readStructure()
    const n = ed.page_count()
    for (let i = 0; i < n; i++) this.readPage(i)
  }

  static async load(url = '/letter-100.json'): Promise<Engine> {
    await init()
    const json = await (await fetch(url)).text()
    return new Engine(new Editor(json))
  }

  readStructure(): void {
    this.structure = JSON.parse(this.ed.structure())
    this.paras = []
    for (const b of this.structure) {
      if (b.t === 'p') this.paras.push(b)
      else for (const row of b.rows) for (const c of row) this.paras.push(...c.paras)
    }
    this.flatOf = new Map(this.paras.map((p) => [p.id, p.flat]))
  }

  /**
   * Re-read page `i`. Paragraphs whose lines on it changed (or that left or
   * joined it) are added to `touched`.
   */
  readPage(i: number, touched?: Set<string>): void {
    const old = this.pages[i] ?? []
    const lines: Line[] = JSON.parse(this.ed.page(i))
    if (touched) {
      const sig = (ls: Line[]) => {
        const m = new Map<string, string>()
        for (const l of ls) m.set(l.id, (m.get(l.id) ?? '') + JSON.stringify(l))
        return m
      }
      const a = sig(old)
      const b = sig(lines)
      for (const [id, v] of a) if (b.get(id) !== v) touched.add(id)
      for (const id of b.keys()) if (!a.has(id)) touched.add(id)
    }
    for (const l of old) this.paraPages.get(l.id)?.delete(i)
    this.pages[i] = lines
    for (const l of lines) {
      let s = this.paraPages.get(l.id)
      if (!s) {
        s = new Set()
        this.paraPages.set(l.id, s)
      }
      s.add(i)
    }
  }

  /** Apply an engine outcome: re-read dirty pages; returns touched paragraph IDs. */
  absorb(out: Outcome): Set<string> {
    const touched = new Set<string>()
    if (!out.ok) return touched
    if (out.structural) this.readStructure()
    for (let i = out.pages; i < this.pages.length; i++) {
      for (const l of this.pages[i] ?? []) {
        touched.add(l.id)
        this.paraPages.get(l.id)?.delete(i)
      }
    }
    this.pages.length = Math.min(this.pages.length, out.pages)
    for (const i of out.dirty) this.readPage(i, touched)
    return touched
  }

  /** All lines of a paragraph, in order. */
  linesOf(id: string): Line[] {
    const out: Line[] = []
    const pages = [...(this.paraPages.get(id) ?? [])].sort((a, b) => a - b)
    for (const p of pages) for (const l of this.pages[p] ?? []) if (l.id === id) out.push(l)
    return out
  }

  /** Page index a line is on (lines carry no page number). */
  pageOfLine(id: string, s: number): number {
    for (const p of this.paraPages.get(id) ?? []) {
      if (this.pages[p]?.some((l) => l.id === id && l.s === s)) return p
    }
    return 0
  }

  call(f: () => string): Outcome {
    const t = performance.now()
    const out: Outcome = JSON.parse(f())
    timing.engine += performance.now() - t
    return out
  }

  insert(flat: number, off: number, text: string): Outcome {
    return this.call(() => this.ed.insert(flat, off, text))
  }

  delete(fa: number, oa: number, fb: number, ob: number): Outcome {
    return this.call(() => this.ed.delete(fa, oa, fb, ob))
  }

  split(flat: number, off: number, id?: string): Outcome {
    return this.call(() => this.ed.split(flat, off, id))
  }

  text(flat: number): string {
    return this.ed.para_text(flat)
  }

  length(flat: number): number {
    return [...this.text(flat)].length
  }
}
