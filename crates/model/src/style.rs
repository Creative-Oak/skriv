// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Styles, inheritance and the explanation of where a look comes from.
//!
//! ODF already separates named styles (what the author chose) from automatic
//! styles (formatting applied by hand, which the writing application invents a
//! name for). That is exactly design principle 2's "direct formatting is a
//! deviation from a style": an automatic style's own properties are the delta,
//! and its parent is the named style.

use crate::node::{Attrs, NodeMeta, Slot};
use serde::{Deserialize, Serialize};

/// Where a style list lives in the package.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StyleScope {
    /// `office:styles` in styles.xml: named, user-visible styles.
    Named,
    /// `office:automatic-styles` in content.xml: direct formatting.
    ContentAutomatic,
    /// `office:automatic-styles` in styles.xml: used by headers, footers, page layouts.
    StylesAutomatic,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Styles {
    pub named: Vec<StyleEntry>,
    pub content_automatic: Vec<StyleEntry>,
    pub styles_automatic: Vec<StyleEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StyleEntry {
    Style(Style),
    List(ListStyle),
    Opaque(Slot),
}

/// `style:style` or `style:default-style`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub meta: NodeMeta,
    pub default: bool,
    pub attrs: Attrs,
    pub children: Vec<StyleChild>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StyleChild {
    Props(PropertySet),
    Opaque(Slot),
}

/// One `style:*-properties` element: `style:text-properties`,
/// `style:paragraph-properties`, `style:table-column-properties` and so on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PropertySet {
    pub meta: NodeMeta,
    pub name: String,
    pub attrs: Attrs,
    pub children: Vec<Slot>,
}

impl Style {
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("style:name")
    }

    pub fn display_name(&self) -> Option<&str> {
        self.attrs.get("style:display-name").or_else(|| self.name())
    }

    pub fn family(&self) -> Option<&str> {
        self.attrs.get("style:family")
    }

    pub fn parent(&self) -> Option<&str> {
        self.attrs.get("style:parent-style-name")
    }

    pub fn list_style(&self) -> Option<&str> {
        self.attrs.get("style:list-style-name")
    }

    pub fn props(&self, set: &str) -> Option<&PropertySet> {
        self.children.iter().find_map(|c| match c {
            StyleChild::Props(p) if p.name == set => Some(p),
            _ => None,
        })
    }

    pub fn props_mut(&mut self, set: &str) -> &mut PropertySet {
        let i = self
            .children
            .iter()
            .position(|c| matches!(c, StyleChild::Props(p) if p.name == set));
        let i = match i {
            Some(i) => i,
            None => {
                self.children.push(StyleChild::Props(PropertySet {
                    meta: NodeMeta::new(),
                    name: set.to_owned(),
                    attrs: Attrs::default(),
                    children: Vec::new(),
                }));
                self.children.len() - 1
            }
        };
        match &mut self.children[i] {
            StyleChild::Props(p) => p,
            _ => unreachable!(),
        }
    }

    /// Every (property set, attribute, value) this style sets itself.
    pub fn own_properties(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.children.iter().flat_map(|c| match c {
            StyleChild::Props(p) => p
                .attrs
                .iter()
                .map(|a| (p.name.as_str(), a.name.as_str(), a.value.as_str()))
                .collect::<Vec<_>>(),
            StyleChild::Opaque(_) => Vec::new(),
        })
    }
}

/// `text:list-style` or `text:outline-style`. Multilevel numbering lives here,
/// one level style per depth, and paragraph styles link to it by name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListStyle {
    pub meta: NodeMeta,
    pub outline: bool,
    pub attrs: Attrs,
    pub levels: Vec<ListLevelChild>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ListLevelChild {
    Level(ListLevel),
    Opaque(Slot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListLevelKind {
    Number,
    Bullet,
    Image,
    Outline,
}

impl ListLevelKind {
    pub fn element(self) -> &'static str {
        match self {
            ListLevelKind::Number => "text:list-level-style-number",
            ListLevelKind::Bullet => "text:list-level-style-bullet",
            ListLevelKind::Image => "text:list-level-style-image",
            ListLevelKind::Outline => "text:outline-level-style",
        }
    }

    pub fn from_element(name: &str) -> Option<Self> {
        match name {
            "text:list-level-style-number" => Some(ListLevelKind::Number),
            "text:list-level-style-bullet" => Some(ListLevelKind::Bullet),
            "text:list-level-style-image" => Some(ListLevelKind::Image),
            "text:outline-level-style" => Some(ListLevelKind::Outline),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListLevel {
    pub meta: NodeMeta,
    pub kind: ListLevelKind,
    pub attrs: Attrs,
    pub children: Vec<StyleChild>,
}

impl ListLevel {
    pub fn level(&self) -> u8 {
        self.attrs
            .get("text:level")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }

    /// e.g. `1`, `a`, `I`; None for bullets.
    pub fn num_format(&self) -> Option<&str> {
        self.attrs.get("style:num-format")
    }

    /// How many parent levels a number shows ("1.2.3" has 3).
    pub fn display_levels(&self) -> u8 {
        self.attrs
            .get("text:display-levels")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1)
    }
}

impl ListStyle {
    pub fn name(&self) -> Option<&str> {
        self.attrs.get("style:name")
    }

    pub fn level(&self, level: u8) -> Option<&ListLevel> {
        self.levels.iter().find_map(|l| match l {
            ListLevelChild::Level(l) if l.level() == level => Some(l),
            _ => None,
        })
    }
}

/// A property value and the style that supplied it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved<'a> {
    pub value: &'a str,
    pub from_style: &'a str,
    pub automatic: bool,
}

/// Plain-language account of a paragraph's or span's look (design principle 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    /// The named style, by display name.
    pub style: Option<String>,
    /// Properties set by hand on top of it, as (attribute, value).
    pub direct: Vec<(String, String)>,
}

impl Explanation {
    /// "Heading 2 + bold added by hand".
    pub fn sentence(&self) -> String {
        let base = self.style.clone().unwrap_or_else(|| "No style".to_owned());
        if self.direct.is_empty() {
            return base;
        }
        let parts: Vec<String> = self.direct.iter().map(|(k, v)| describe(k, v)).collect();
        format!("{base} + {} added by hand", parts.join(", "))
    }
}

fn describe(attr: &str, value: &str) -> String {
    match (attr, value) {
        ("fo:font-weight", "bold") => "bold".into(),
        ("fo:font-style", "italic") => "italic".into(),
        ("style:text-underline-style", v) if v != "none" => "underline".into(),
        ("fo:font-size", v) => format!("size {v}"),
        ("style:font-name", v) => format!("font {v}"),
        ("fo:color", v) => format!("colour {v}"),
        ("fo:text-align", v) => format!("aligned {v}"),
        ("fo:background-color", v) => format!("highlight {v}"),
        (k, v) => format!("{k}={v}"),
    }
}

/// Attributes that are bookkeeping rather than looks, and so don't count as
/// "added by hand" (`officeooo:rsid` is LibreOffice's edit-session stamp).
fn is_noise(attr: &str) -> bool {
    matches!(attr, "officeooo:rsid" | "officeooo:paragraph-rsid")
}

impl Styles {
    fn lists(&self) -> [(&[StyleEntry], bool); 3] {
        [
            (&self.content_automatic, true),
            (&self.styles_automatic, true),
            (&self.named, false),
        ]
    }

    /// Find a style by family and name. Automatic styles shadow named ones, as
    /// in ODF, because content refers to automatic styles first.
    pub fn find(&self, family: &str, name: &str) -> Option<(&Style, bool)> {
        for (list, automatic) in self.lists() {
            for e in list {
                if let StyleEntry::Style(s) = e
                    && !s.default
                    && s.family() == Some(family)
                    && s.name() == Some(name)
                {
                    return Some((s, automatic));
                }
            }
        }
        None
    }

    pub fn default_style(&self, family: &str) -> Option<&Style> {
        self.named.iter().find_map(|e| match e {
            StyleEntry::Style(s) if s.default && s.family() == Some(family) => Some(s),
            _ => None,
        })
    }

    pub fn list_style(&self, name: &str) -> Option<&ListStyle> {
        for (list, _) in self.lists() {
            for e in list {
                if let StyleEntry::List(l) = e
                    && l.name() == Some(name)
                {
                    return Some(l);
                }
            }
        }
        None
    }

    /// Walk the inheritance chain (style, parent, grandparent, then the family
    /// default) and return the first value for `set`/`attr`.
    pub fn resolve<'a>(
        &'a self,
        family: &str,
        name: &'a str,
        set: &str,
        attr: &str,
    ) -> Option<Resolved<'a>> {
        let mut current = Some(name);
        let mut guard = 0;
        while let Some(n) = current {
            guard += 1;
            if guard > 64 {
                break; // a cycle in a hostile file
            }
            let (style, automatic) = self.find(family, n)?;
            if let Some(v) = style.props(set).and_then(|p| p.attrs.get(attr)) {
                return Some(Resolved {
                    value: v,
                    from_style: style.name().unwrap_or(""),
                    automatic,
                });
            }
            current = style.parent();
        }
        let d = self.default_style(family)?;
        let v = d.props(set)?.attrs.get(attr)?;
        Some(Resolved {
            value: v,
            from_style: "(default)",
            automatic: false,
        })
    }

    /// The list style that numbers a paragraph style, following inheritance.
    pub fn list_style_for_paragraph_style(&self, name: &str) -> Option<&ListStyle> {
        let mut current = Some(name);
        let mut guard = 0;
        while let Some(n) = current {
            guard += 1;
            if guard > 64 {
                return None;
            }
            let (s, _) = self.find("paragraph", n)?;
            if let Some(l) = s.list_style() {
                return self.list_style(l);
            }
            current = s.parent();
        }
        None
    }

    /// Explain a style reference: which named style it is, and what was added by hand.
    pub fn explain(&self, family: &str, name: &str) -> Explanation {
        let Some((style, automatic)) = self.find(family, name) else {
            return Explanation {
                style: Some(name.to_owned()),
                direct: Vec::new(),
            };
        };
        if !automatic {
            return Explanation {
                style: style.display_name().map(str::to_owned),
                direct: Vec::new(),
            };
        }
        let named = style
            .parent()
            .and_then(|p| self.find(family, p))
            .and_then(|(s, _)| s.display_name().map(str::to_owned));
        let direct = style
            .own_properties()
            .filter(|(_, k, _)| !is_noise(k))
            .map(|(_, k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        Explanation {
            style: named,
            direct,
        }
    }

    pub fn iter_all(&self) -> impl Iterator<Item = (StyleScope, &StyleEntry)> {
        self.named
            .iter()
            .map(|e| (StyleScope::Named, e))
            .chain(
                self.content_automatic
                    .iter()
                    .map(|e| (StyleScope::ContentAutomatic, e)),
            )
            .chain(
                self.styles_automatic
                    .iter()
                    .map(|e| (StyleScope::StylesAutomatic, e)),
            )
    }
}
