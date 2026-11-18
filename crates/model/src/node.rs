// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Things every node carries: a stable ID, where it came from, whether it has
//! been edited, its attributes, and slots for what we don't understand.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A stable identifier. Minted IDs are UUID v7 with a letter prefix so they are
/// valid XML NCNames (an `xml:id` may not start with a digit). IDs that arrive
/// in an imported file (`xml:id="id123"`) are kept as they are.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl NodeId {
    pub const PREFIX: &'static str = "sk-";

    pub fn mint() -> Self {
        NodeId(format!("{}{}", Self::PREFIX, uuid::Uuid::now_v7()))
    }

    pub fn is_minted(&self) -> bool {
        self.0.starts_with(Self::PREFIX)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which source part a node was read from. Byte offsets are into that part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PartKind {
    Content,
    Styles,
    Meta,
}

/// Where a node's XML sits in the source part. `start..end` is the whole
/// element; `start..start_tag_end` is its start tag (equal to `end` for an
/// empty element).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub part: PartKind,
    pub start: usize,
    pub end: usize,
    pub start_tag_end: usize,
}

impl Origin {
    pub fn is_empty_element(&self) -> bool {
        self.start_tag_end == self.end
    }
}

/// Per-node bookkeeping for "projection with preservation" (ADR 0006).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeMeta {
    pub id: NodeId,
    /// True when `id` came from an `xml:id` in the source rather than being minted.
    pub id_from_source: bool,
    /// None for nodes created in the app.
    pub origin: Option<Origin>,
    /// Set when the node itself has been edited. A dirty node's tags are
    /// regenerated from the model; its clean children are still copied verbatim.
    pub dirty: bool,
    /// Write `id` out as `xml:id` even though the node is otherwise untouched.
    pub stamp_id: bool,
    /// Computed by [`crate::propagate_dirty`] before a write: some node below
    /// this one is dirty or stamped, so its start and end tags can be copied
    /// but its children must be visited.
    #[serde(default)]
    pub descendants_dirty: bool,
    /// The node's own tags are unchanged but its list of children is not
    /// (one was deleted or moved away). Such a node can't be copied whole;
    /// [`crate::propagate_dirty`] treats it like one with a dirty descendant.
    #[serde(default)]
    pub children_changed: bool,
}

impl NodeMeta {
    pub fn imported(origin: Origin, source_id: Option<&str>) -> Self {
        match source_id {
            Some(id) => NodeMeta {
                id: NodeId(id.to_owned()),
                id_from_source: true,
                origin: Some(origin),
                dirty: false,
                stamp_id: false,
                descendants_dirty: false,
                children_changed: false,
            },
            None => NodeMeta {
                id: NodeId::mint(),
                id_from_source: false,
                origin: Some(origin),
                dirty: false,
                stamp_id: false,
                descendants_dirty: false,
                children_changed: false,
            },
        }
    }

    pub fn new() -> Self {
        NodeMeta {
            id: NodeId::mint(),
            id_from_source: false,
            origin: None,
            dirty: true,
            stamp_id: false,
            descendants_dirty: false,
            children_changed: false,
        }
    }

    /// True when the node can be written out by copying its source bytes.
    pub fn is_verbatim(&self) -> bool {
        !self.dirty && !self.stamp_id && !self.descendants_dirty && self.origin.is_some()
    }

    /// True when this node itself, not just something below it, must be written differently.
    pub fn is_changed(&self) -> bool {
        self.dirty || self.stamp_id || self.origin.is_none()
    }
}

impl Default for NodeMeta {
    fn default() -> Self {
        Self::new()
    }
}

/// One attribute, by qualified name as written (`fo:font-weight`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attr {
    pub name: String,
    pub value: String,
}

/// Attribute pairs whose order LibreOffice's ODF import depends on, as
/// (must come first, must come after). XML says attribute order is
/// insignificant; LibreOffice 26.8.1.1 disagrees. Spike 1 found this by
/// reversing attribute order and bisecting rendering differences. Each pair
/// below was involved, e.g. `fo:country` before `fo:language` changes which
/// locale a weak character (an apostrophe) gets.
///
/// This table is a minimal start and is known to be incomplete: some
/// orderings that respect it still render differently. The real fix is for
/// new attributes to follow LibreOffice's own serialisation order (a
/// follow-up). Existing attributes always keep their source order.
pub const ORDER_SENSITIVE: &[(&str, &str)] = &[
    ("fo:language", "fo:country"),
    ("style:language-asian", "style:country-asian"),
    ("style:language-complex", "style:country-complex"),
    ("fo:font-style", "style:font-style-asian"),
    ("fo:font-style", "style:font-style-complex"),
    ("style:text-rotation-angle", "style:text-rotation-scale"),
];

/// Attributes in their original order. The model reads and writes the ones it
/// understands through typed accessors on each node; everything else rides
/// along untouched. Keeping order means a regenerated start tag usually comes
/// out the same as the original.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attrs(pub Vec<Attr>);

impl Attrs {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    }

    /// Set an attribute. An existing one keeps its position; a new one goes
    /// at the end, except where LibreOffice's import depends on order (see
    /// [`ORDER_SENSITIVE`]), where it goes before the attribute it must precede.
    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some(a) = self.0.iter_mut().find(|a| a.name == name) {
            a.value = value;
            return;
        }
        let attr = Attr {
            name: name.to_owned(),
            value,
        };
        let before = ORDER_SENSITIVE
            .iter()
            .filter(|(first, _)| *first == name)
            .filter_map(|(_, then)| self.0.iter().position(|a| a.name == *then))
            .min();
        match before {
            Some(i) => self.0.insert(i, attr),
            None => self.0.push(attr),
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        let i = self.0.iter().position(|a| a.name == name)?;
        Some(self.0.remove(i).value)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Attr> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Attrs {
    fn from(pairs: [(&str, &str); N]) -> Self {
        Attrs(
            pairs
                .iter()
                .map(|(n, v)| Attr {
                    name: (*n).to_owned(),
                    value: (*v).to_owned(),
                })
                .collect(),
        )
    }
}

/// The format a preserved payload belongs to. A slot is only ever re-emitted
/// into the format it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotFormat {
    Odf,
}

/// Something we read but do not model: an unknown element, a comment, a run of
/// whitespace between blocks. Kept as the exact source text and written back
/// verbatim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub format: SlotFormat,
    /// The element's qualified name, or None for text, comments and the like.
    pub name: Option<String>,
    pub raw: String,
}

impl Slot {
    pub fn odf(name: Option<&str>, raw: impl Into<String>) -> Self {
        Slot {
            format: SlotFormat::Odf,
            name: name.map(str::to_owned),
            raw: raw.into(),
        }
    }

    pub fn is_whitespace(&self) -> bool {
        self.name.is_none() && self.raw.chars().all(char::is_whitespace)
    }
}
