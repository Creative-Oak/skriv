// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Both paginators compiled to wasm32, with an integer-only API so a page
//! can drive them without generated glue:
//!
//! - `init(engine) -> pages`: 0 = own, 1 = own (greedy), 2 = Typst; lays
//!   out the embedded 100-page document (add 10 for the justified variant).
//! - `pick(page, in_table) -> para`: choose where to type (as natively).
//! - `keystroke(para) -> 1`: type one character in the middle of `para` and
//!   re-lay out; JavaScript times the call.
//! - `hash_lo()`, `hash_hi()`: the layout hash, for determinism checks.
//!
//! The input is prepared natively (`layout-bench export`) because building
//! it from ODT mints UUID v7 IDs, which need a clock and randomness.

use std::cell::RefCell;

use layout_contract::input::Doc;
use layout_contract::{EditHint, Engine, Pages};

static INPUT: &[u8] = include_bytes!("../../../../target/spike2b/letter-100.json");
static JUSTIFIED: &[u8] = include_bytes!("../../../../target/spike2b/letter-100-justified.json");

struct State {
    doc: Doc,
    engine: Box<dyn Engine>,
    pages: Pages,
    offsets: Vec<usize>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn engine(kind: u32) -> Box<dyn Engine> {
    match kind {
        #[cfg(feature = "own")]
        0 => Box::new(layout_own::OwnEngine::new()),
        #[cfg(feature = "own")]
        1 => {
            let mut e = layout_own::OwnEngine::new();
            e.breaker = layout_own::Breaker::Greedy;
            Box::new(e)
        }
        #[cfg(feature = "typst")]
        2 => Box::new(layout_typst::TypstEngine::new()),
        _ => panic!("engine {kind} not built"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn init(kind: u32) -> u32 {
    // kind + 10: the justified, hyphenated variant.
    let doc: Doc =
        serde_json::from_slice(if kind >= 10 { JUSTIFIED } else { INPUT }).expect("input");
    let mut engine = engine(kind % 10);
    let pages = engine.layout(&doc, &EditHint::Full).clone();
    let n = pages.pages.len() as u32;
    let offsets = doc.paras().iter().map(|p| p.char_len() / 2).collect();
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            doc,
            engine,
            pages,
            offsets,
        })
    });
    n
}

#[unsafe(no_mangle)]
pub extern "C" fn pick(page: u32, in_table: u32) -> u32 {
    STATE.with(|s| {
        let s = s.borrow();
        let st = s.as_ref().expect("init first");
        layout_contract::target(&st.doc, &st.pages, page as usize, in_table != 0) as u32
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn keystroke(para: u32) -> u32 {
    const TEXT: &[char] = &['Æ', 'b', 'l', 'e', 'ø', 'e', 'n', ' ', 'p', 'å', ' '];
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let st = s.as_mut().expect("init first");
        let p = para as usize;
        let off = st.offsets[p];
        st.doc
            .para_mut(p)
            .unwrap()
            .insert(off, TEXT[off % TEXT.len()]);
        st.offsets[p] += 1;
        let pages = st.engine.layout(&st.doc, &EditHint::Para(p));
        u32::from(layout_contract::locate(pages, p, off + 1).is_some())
    })
}

/// Hash of the layout made by `init` (before any typing).
fn hash() -> u64 {
    STATE.with(|s| layout_contract::layout_hash(&s.borrow().as_ref().expect("init first").pages))
}

#[unsafe(no_mangle)]
pub extern "C" fn hash_lo() -> u32 {
    hash() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn hash_hi() -> u32 {
    (hash() >> 32) as u32
}

/// One number about the layout made by `init`, for comparing with a native
/// dump: field 0 = lines on the page, then per line: 1 y, 2 height,
/// 3 range start, 4 range end, 5 first caret, 6 last caret, 7 para.
#[unsafe(no_mangle)]
pub extern "C" fn info(page: u32, line: u32, field: u32) -> f64 {
    STATE.with(|s| {
        let s = s.borrow();
        let p = &s.as_ref().expect("init first").pages.pages[page as usize];
        if field == 0 {
            return p.lines.len() as f64;
        }
        let l = &p.lines[line as usize];
        match field {
            1 => f64::from(l.y),
            2 => f64::from(l.height),
            3 => l.range.start as f64,
            4 => l.range.end as f64,
            5 => f64::from(l.carets[0]),
            6 => f64::from(*l.carets.last().unwrap()),
            _ => l.para as f64,
        }
    })
}
