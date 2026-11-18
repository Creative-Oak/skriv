// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Spike 4: does skriv's CRDT history survive a file going through
//! LibreOffice or email?
//!
//! "File mode" is what happens when a skriv document leaves skriv as an
//! `.odt` and comes back:
//!
//! 1. [`state`] writes the document with its identity: an `xml:id` on every
//!    paragraph, heading and section, the document ID in `meta:user-defined`,
//!    and an RDF metadata part (`skriv/state.rdf`) holding the CRDT version
//!    the file was written at and, optionally, a snapshot. Spike 1 found that
//!    LibreOffice keeps all of these and drops everything else we might use.
//! 2. When a file comes back, [`noise`] removes what LibreOffice adds to
//!    every file it saves, [`align`] gives the returned document's nodes
//!    their skriv IDs back (LibreOffice drops the IDs of tables, rows,
//!    cells and list items; they are recovered from the paragraphs inside),
//!    and [`apply`] turns the differences into CRDT operations on a branch
//!    forked at the version the file was written at. The branch is a
//!    suggestion attributed to the person who edited the file.
//! 3. [`units`] measures how far two documents are apart, which is how the
//!    spike counts spurious changes: the branch against the same edits made
//!    directly in skriv ([`scenario`]).

pub mod align;
pub mod apply;
pub mod noise;
pub mod scenario;
pub mod state;
pub mod tracked;
pub mod units;

/// Character similarity of two strings, 0..=1 (1 = equal).
pub fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Myers)
        .timeout(std::time::Duration::from_millis(200))
        .diff_chars(a, b)
        .ratio() as f64
}
