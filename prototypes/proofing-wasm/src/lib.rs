// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The proofing crate in wasm32 with an integer-only API, so a page or
//! Node can drive it without generated glue. Strings and dictionary files
//! go in through `alloc` and linear memory.
//!
//! - `alloc(len) -> ptr`, `free(ptr, len)`
//! - `load(lang, aff_ptr, aff_len, dic_ptr, dic_len) -> 1 | 0`: lang 0 = da, 1 = en
//! - `check(lang, ptr, len) -> flags`: misspelled words in a paragraph
//! - `suggest(lang, ptr, len) -> n`: suggestions for one word
//! - `detect(kind, ptr, len) -> 0 da | 1 en | -1 none`: kind 0 = lingua
//!   (Danish and English), 1 = whatlang (Danish and English), 2 = the
//!   dictionary vote; `confidence()` returns the last confidence.

#![allow(unsafe_code)] // `no_mangle` exports and raw pointers from JavaScript.

use std::cell::RefCell;

use skriv_proofing::Lang;
use skriv_proofing::detect::{Detection, Detector};
use skriv_proofing::spell::{self, Dictionary, Speller};

thread_local! {
    static DICTS: RefCell<[Option<Dictionary>; 2]> = const { RefCell::new([None, None]) };
    static SPELLER: RefCell<Option<Speller>> = const { RefCell::new(None) };
    static CONF: RefCell<f64> = const { RefCell::new(0.0) };
    #[cfg(feature = "lingua")]
    static LINGUA: skriv_proofing::detect::Lingua = skriv_proofing::detect::Lingua::new(false, false);
    #[cfg(feature = "whatlang")]
    static WHATLANG: skriv_proofing::detect::Whatlang = skriv_proofing::detect::Whatlang::new(true);
}

fn lang(l: u32) -> Lang {
    if l == 0 { Lang::Da } else { Lang::En }
}

/// # Safety
/// `ptr..ptr+len` must be memory from `alloc` holding UTF-8.
unsafe fn str_at<'a>(ptr: *const u8, len: usize) -> &'a str {
    unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) }
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` and `len` must come from one `alloc` call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
    unsafe { drop(Vec::from_raw_parts(ptr, 0, len)) };
}

/// # Safety
/// The pointers must be `alloc` memory holding UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn load(
    l: u32,
    aff: *const u8,
    aff_len: usize,
    dic: *const u8,
    dic_len: usize,
) -> u32 {
    let (aff, dic) = unsafe { (str_at(aff, aff_len), str_at(dic, dic_len)) };
    let Ok(d) = spell::load_dictionary(aff, dic) else {
        return 0;
    };
    DICTS.with(|ds| {
        let mut ds = ds.borrow_mut();
        ds[lang(l).index()] = Some(d);
        if ds[0].is_some() && ds[1].is_some() {
            let [da, en] = std::mem::take(&mut *ds);
            SPELLER.with(|s| *s.borrow_mut() = Some(Speller::new(da.unwrap(), en.unwrap())));
        }
    });
    1
}

fn with_speller<R>(f: impl FnOnce(&Speller) -> R) -> R {
    SPELLER.with(|s| f(s.borrow().as_ref().expect("load both dictionaries first")))
}

/// # Safety
/// `ptr..ptr+len` must be `alloc` memory holding UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn check(l: u32, ptr: *const u8, len: usize) -> u32 {
    let text = unsafe { str_at(ptr, len) };
    with_speller(|s| s.check_text(text, lang(l)).len() as u32)
}

/// # Safety
/// `ptr..ptr+len` must be `alloc` memory holding UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn suggest(l: u32, ptr: *const u8, len: usize) -> u32 {
    let word = unsafe { str_at(ptr, len) };
    with_speller(|s| s.suggest(word, lang(l)).len() as u32)
}

/// # Safety
/// `ptr..ptr+len` must be `alloc` memory holding UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn detect(kind: u32, ptr: *const u8, len: usize) -> i32 {
    let text = unsafe { str_at(ptr, len) };
    let d: Detection = match kind {
        #[cfg(feature = "lingua")]
        0 => LINGUA.with(|d| d.detect(text)),
        #[cfg(feature = "whatlang")]
        1 => WHATLANG.with(|d| d.detect(text)),
        2 => with_speller(|s| skriv_proofing::detect::DictVote { speller: s }.detect(text)),
        _ => return -2,
    };
    CONF.with(|c| *c.borrow_mut() = d.confidence);
    d.lang.map_or(-1, |l| l.index() as i32)
}

#[unsafe(no_mangle)]
pub extern "C" fn confidence() -> f64 {
    CONF.with(|c| *c.borrow())
}
