// Spike 6: load the dictionaries into the wasm build and time checking.
// Node: `node bench.mjs <wasm> <dicts-dir>`. Browser: chromium.cjs calls run().

export async function run(wasmBytes, files, corpusTexts, typos, log) {
  const enc = new TextEncoder()
  let t = performance.now()
  const module = await WebAssembly.compile(wasmBytes)
  // lingua's wasm-bindgen exports bring imports we never call: stub them.
  const imports = {}
  for (const imp of WebAssembly.Module.imports(module)) {
    imports[imp.module] ??= {}
    imports[imp.module][imp.name] = () => {
      throw new Error(`unexpected import ${imp.module}.${imp.name}`)
    }
  }
  const instance = await WebAssembly.instantiate(module, imports)
  const w = instance.exports
  const r = { compile_instantiate_ms: performance.now() - t }

  const put = (bytes) => {
    const p = w.alloc(bytes.length)
    new Uint8Array(w.memory.buffer, p, bytes.length).set(bytes)
    return [p, bytes.length]
  }
  for (const [lang, name] of [
    [0, 'da_DK'],
    [1, 'en_US'],
  ]) {
    t = performance.now()
    const [ap, al] = put(files[`${name}.aff`])
    const [dp, dl] = put(files[`${name}.dic`])
    if (!w.load(lang, ap, al, dp, dl)) throw new Error(`load ${name}`)
    w.free(ap, al)
    w.free(dp, dl)
    r[`load_${name}_ms`] = performance.now() - t
  }
  r.memory_mb_after_load = w.memory.buffer.byteLength / 1048576

  // Paragraphs of the handwritten corpus, with their languages.
  const paras = corpusTexts.map(([lang, text]) => [lang, ...put(enc.encode(text)), text])
  let words = 0
  for (const [, , , text] of paras) words += text.split(/\s+/).filter(Boolean).length
  t = performance.now()
  let flags = 0
  for (let k = 0; k < 3; k++) for (const [lang, p, l] of paras) flags += w.check(lang, p, l)
  r.check_words_per_second = (3 * words) / ((performance.now() - t) / 1000)
  r.flags_per_pass = flags / 3

  // Detection: first call (lingua loads its models lazily), then per paragraph.
  for (const [kind, name] of [
    [0, 'lingua'],
    [1, 'whatlang'],
    [2, 'dict_vote'],
  ]) {
    const [, p, l] = paras.find((x) => x[3].length > 200)
    t = performance.now()
    const got = w.detect(kind, p, l)
    r[`${name}_first_call_ms`] = performance.now() - t
    if (got === -2) continue
    t = performance.now()
    for (const [, pp, ll] of paras) w.detect(kind, pp, ll)
    r[`${name}_us_per_paragraph`] = ((performance.now() - t) * 1000) / paras.length
  }
  r.memory_mb_after_detect = w.memory.buffer.byteLength / 1048576

  // Per keystroke: re-check the median paragraph and re-detect it.
  const sorted = [...paras].sort((a, b) => a[3].length - b[3].length)
  for (const [label, para] of [
    ['median', sorted[sorted.length >> 1]],
    ['p95', sorted[Math.floor(sorted.length * 0.95)]],
  ]) {
    const [lang, p, l, text] = para
    const s = []
    for (let i = 0; i < 200; i++) {
      const t0 = performance.now()
      w.check(lang, p, l)
      w.detect(0, p, l)
      s.push(performance.now() - t0)
    }
    s.sort((a, b) => a - b)
    r[`keystroke_${label}_chars`] = text.length
    r[`keystroke_${label}_p50_ms`] = s[100]
    r[`keystroke_${label}_p95_ms`] = s[190]
  }

  // Suggestions for misspelled words, per language.
  for (const [lang, name] of [
    [0, 'da'],
    [1, 'en'],
  ]) {
    const st = []
    for (const [l, word] of typos) {
      if (l !== lang) continue
      const [p, n] = put(enc.encode(word))
      const t0 = performance.now()
      w.suggest(l, p, n)
      st.push(performance.now() - t0)
    }
    st.sort((a, b) => a - b)
    r[`suggest_${name}_n`] = st.length
    r[`suggest_${name}_p50_ms`] = st[st.length >> 1]
    r[`suggest_${name}_p95_ms`] = st[Math.floor(st.length * 0.95)]
    r[`suggest_${name}_max_ms`] = st[st.length - 1]
  }
  for (const [k, v] of Object.entries(r)) log(`${k}: ${typeof v === 'number' ? v.toFixed(2) : v}`)
  return r
}

export function loadInputs(fs, path, dictsDir) {
  const files = {}
  for (const f of ['da_DK.aff', 'da_DK.dic', 'en_US.aff', 'en_US.dic']) {
    files[f] = new Uint8Array(fs.readFileSync(path.join(dictsDir, f)))
  }
  const corpusDir = path.join(dictsDir, '..', 'corpus')
  const texts = []
  for (const f of fs.readdirSync(path.join(corpusDir, 'handwritten')).sort()) {
    for (const line of fs
      .readFileSync(path.join(corpusDir, 'handwritten', f), 'utf8')
      .split('\n')) {
      if (!line || line.startsWith('#')) continue
      const [lang, , raw] = line.split('\t')
      texts.push([lang === 'da' ? 0 : 1, raw.replace(/\{\{(da|en):/g, '').replace(/\}\}/g, '')])
    }
  }
  const typos = []
  for (const line of fs.readFileSync(path.join(corpusDir, 'typos.tsv'), 'utf8').split('\n')) {
    if (!line || line.startsWith('#')) continue
    const [lang, wrong, , cls] = line.split('\t')
    if (cls === 'realword' || cls === 'split') continue
    typos.push([lang === 'da' ? 0 : 1, wrong])
  }
  return { files, texts, typos }
}

if (typeof process !== 'undefined' && process.argv[1]?.endsWith('bench.mjs')) {
  const fs = await import('node:fs')
  const path = await import('node:path')
  const { files, texts, typos } = loadInputs(fs, path, process.argv[3])
  const r = await run(fs.readFileSync(process.argv[2]), files, texts, typos, console.log)
  console.log(JSON.stringify(r))
}
