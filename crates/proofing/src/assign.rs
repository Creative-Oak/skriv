// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A language per paragraph, from per-paragraph detections.
//!
//! Without hysteresis every paragraph gets what the detector says (and
//! keeps the previous language when the detector has nothing). With
//! hysteresis a running language is carried through the document:
//!
//! - a long paragraph switches it only if the detector is confident;
//! - a short paragraph (fewer letters than `min_letters`) inherits it,
//!   unless `short_confidence` is set and the detector clears that bar, in
//!   which case the paragraph gets its own language without moving the
//!   running one;
//! - a short heading takes the language of the paragraph after it, because
//!   a heading belongs to what follows (`headings_look_ahead`).
//!
//! Everything but the heading rule only looks backwards, so editing a
//! paragraph only changes the languages from there on.

use serde::Serialize;

use crate::detect::Detection;
use crate::lang::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Policy {
    pub hysteresis: bool,
    pub min_letters: usize,
    pub switch_confidence: f64,
    pub short_confidence: Option<f64>,
    pub headings_look_ahead: bool,
}

impl Policy {
    /// Every paragraph takes its own detection.
    pub const RAW: Policy = Policy {
        hysteresis: false,
        min_letters: 0,
        switch_confidence: 0.0,
        short_confidence: None,
        headings_look_ahead: false,
    };
}

/// What the assignment needs to know about a paragraph.
#[derive(Debug, Clone, Copy)]
pub struct ParaInfo {
    pub letters: usize,
    pub heading: bool,
}

pub fn assign(paras: &[ParaInfo], dets: &[Detection], default: Lang, p: &Policy) -> Vec<Lang> {
    assert_eq!(paras.len(), dets.len());
    let mut out = Vec::with_capacity(paras.len());
    let mut running = default;
    let mut short = vec![false; paras.len()];
    for (i, (para, det)) in paras.iter().zip(dets).enumerate() {
        if !p.hysteresis {
            running = det.lang.unwrap_or(running);
            out.push(running);
            continue;
        }
        let mut lang = running;
        if para.letters >= p.min_letters {
            if let Some(l) = det.lang
                && l != running
                && det.confidence >= p.switch_confidence
            {
                running = l;
                lang = l;
            }
        } else {
            short[i] = true;
            if let (Some(sc), Some(l)) = (p.short_confidence, det.lang)
                && det.confidence >= sc
            {
                lang = l;
            }
        }
        out.push(lang);
    }
    if p.hysteresis && p.headings_look_ahead {
        for i in (0..paras.len().saturating_sub(1)).rev() {
            if paras[i].heading && short[i] {
                out[i] = out[i + 1];
            }
        }
    }
    out
}

/// Switch errors of `pred` against `truth`: switches `pred` has that
/// `truth` doesn't (spurious) and switches `truth` has that `pred` doesn't
/// (missed). Both sequences start from `default`.
pub fn switch_errors(truth: &[Lang], pred: &[Lang], default: Lang) -> (usize, usize) {
    let (mut spurious, mut missed) = (0, 0);
    let (mut tp, mut pp) = (default, default);
    for (&t, &p) in truth.iter().zip(pred) {
        let ts = t != tp;
        let ps = p != pp;
        if ps && !ts {
            spurious += 1;
        }
        if ts && !ps {
            missed += 1;
        }
        tp = t;
        pp = p;
    }
    (spurious, missed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Lang::*;

    fn d(lang: Lang, c: f64) -> Detection {
        Detection {
            lang: Some(lang),
            confidence: c,
        }
    }

    fn para(letters: usize, heading: bool) -> ParaInfo {
        ParaInfo { letters, heading }
    }

    #[test]
    fn hysteresis_ignores_unsure_and_short_paragraphs() {
        let paras = [
            para(200, false),
            para(200, false),
            para(5, false),
            para(200, false),
        ];
        let dets = [d(Da, 0.9), d(En, 0.55), d(En, 0.99), d(En, 0.95)];
        assert_eq!(assign(&paras, &dets, Da, &Policy::RAW), [Da, En, En, En]);
        let p = Policy {
            hysteresis: true,
            min_letters: 20,
            switch_confidence: 0.8,
            short_confidence: None,
            headings_look_ahead: false,
        };
        assert_eq!(assign(&paras, &dets, Da, &p), [Da, Da, Da, En]);
    }

    #[test]
    fn short_headings_look_ahead() {
        let paras = [para(200, false), para(8, true), para(200, false)];
        let dets = [d(Da, 0.9), Detection::NONE, d(En, 0.95)];
        let p = Policy {
            hysteresis: true,
            min_letters: 20,
            switch_confidence: 0.8,
            short_confidence: None,
            headings_look_ahead: true,
        };
        assert_eq!(assign(&paras, &dets, Da, &p), [Da, En, En]);
    }

    #[test]
    fn counts_spurious_and_missed_switches() {
        // One wrong paragraph in the middle: two spurious switches.
        assert_eq!(switch_errors(&[Da, Da, Da], &[Da, En, Da], Da), (2, 0));
        // A switch detected one paragraph late: one missed, one spurious.
        assert_eq!(switch_errors(&[Da, En, En], &[Da, Da, En], Da), (1, 1));
        // The first paragraph differs from the document default.
        assert_eq!(switch_errors(&[En, En], &[Da, Da], Da), (0, 1));
    }
}
