// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The skriv semantic document model.
//!
//! ODF-shaped, but not ODF XML: it knows nothing about bytes, namespaces or
//! zip files, and nothing about any editor framework (ADR 0002). Every block
//! and inline that can be referenced carries a stable [`NodeId`]. What the
//! model does not understand is carried in [`Slot`]s so it can be written back
//! unchanged.
//!
//! Spike 1 scope: paragraphs, headings, styles with inheritance, direct
//! formatting as automatic-style deltas, list styles with multilevel numbering,
//! sections, tables with explicit column widths, frames and images, fields,
//! comments, tracked changes and bookmarks.

pub mod content;
pub mod document;
pub mod node;
pub mod style;

pub use content::*;
pub use document::*;
pub use node::*;
pub use style::*;

#[cfg(test)]
mod tests;
