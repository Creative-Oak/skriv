// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 6: proofing documents that mix Danish and English.
//!
//! - [`detect`]: language detectors behind one trait (lingua, whatlang, and
//!   a vote of the two spelling dictionaries).
//! - [`assign`]: a language per paragraph, with hysteresis: a paragraph
//!   keeps the running language unless detection is confident, and short
//!   paragraphs inherit.
//! - [`spell`]: a Hunspell-format checker (spellbook) holding `da_DK` and
//!   `en_US`, with the word rules a word processor needs (skip numbers,
//!   addresses, all-caps abbreviations).
//! - [`corpus`]: the labelled corpus format and the measurements.
//!
//! Dictionaries are not in git: `crates/proofing/fetch-dicts.sh` downloads
//! them. [`dicts_dir`] finds them; code that needs them skips when absent.

pub mod assign;
pub mod corpus;
pub mod detect;
pub mod lang;
pub mod spell;
pub mod text;

pub use lang::Lang;

use std::path::PathBuf;

/// `crates/proofing/dicts`, or `SKRIV_DICTS` if set, when `da_DK.dic` is there.
pub fn dicts_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("SKRIV_DICTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dicts"));
    dir.join("da_DK.dic").exists().then_some(dir)
}
