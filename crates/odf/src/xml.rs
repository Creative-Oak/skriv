// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! A small XML tree that remembers exactly where every node sits in the source.
//!
//! Byte spans are what make verbatim re-emission possible: an untouched element
//! is written back as `src[start..end]`, so whatever the original producer
//! chose (attribute order, quoting, entity spelling, whitespace) survives.

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::Error;

#[derive(Debug, Clone)]
pub enum Kind {
    Element {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<usize>,
    },
    Text,
    /// Comments, processing instructions, CDATA, doctype.
    Other,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
    /// End of the start tag. Equal to `end` for an empty element (`<x/>`).
    pub start_tag_end: usize,
    /// Start of the end tag (`</x>`). Equal to `end` for an empty element.
    pub end_tag_start: usize,
}

impl Node {
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            Kind::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn attrs(&self) -> &[(String, String)] {
        match &self.kind {
            Kind::Element { attrs, .. } => attrs,
            _ => &[],
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs()
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn children(&self) -> &[usize] {
        match &self.kind {
            Kind::Element { children, .. } => children,
            _ => &[],
        }
    }

    pub fn is_empty_element(&self) -> bool {
        matches!(self.kind, Kind::Element { .. }) && self.start_tag_end == self.end
    }
}

#[derive(Debug, Clone)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: usize,
}

impl Tree {
    pub fn node(&self, i: usize) -> &Node {
        &self.nodes[i]
    }

    pub fn root(&self) -> &Node {
        &self.nodes[self.root]
    }

    /// First child element with this name.
    pub fn child(&self, parent: usize, name: &str) -> Option<usize> {
        self.nodes[parent]
            .children()
            .iter()
            .copied()
            .find(|&c| self.nodes[c].name() == Some(name))
    }

    /// Path of element names, e.g. `["office:body", "office:text"]`, from the root.
    pub fn path(&self, names: &[&str]) -> Option<usize> {
        let mut cur = self.root;
        for n in names {
            cur = self.child(cur, n)?;
        }
        Some(cur)
    }

    /// Decoded text of all text-node children (no element children allowed).
    pub fn text_only(&self, src: &str, i: usize) -> Option<String> {
        let mut s = String::new();
        for &c in self.nodes[i].children() {
            let n = &self.nodes[c];
            match n.kind {
                Kind::Text => s.push_str(&unescape(&src[n.start..n.end])),
                _ => return None,
            }
        }
        Some(s)
    }

    /// Depth-first iterator over element indices under (and including) `i`.
    pub fn descendants(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![i];
        while let Some(n) = stack.pop() {
            if matches!(self.nodes[n].kind, Kind::Element { .. }) {
                out.push(n);
                for &c in self.nodes[n].children().iter().rev() {
                    stack.push(c);
                }
            }
        }
        out
    }
}

pub fn parse(src: &str) -> Result<Tree, Error> {
    // quick-xml skips a byte-order mark without counting it, which would shift
    // every span by three bytes. Parse after it and add it back.
    let bom = if src.starts_with('\u{feff}') { 3 } else { 0 };
    let src = &src[bom..];
    let mut reader = Reader::from_str(src);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = true;

    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut top: Vec<usize> = Vec::new();

    fn attach(nodes: &mut [Node], stack: &[usize], top: &mut Vec<usize>, i: usize) {
        match stack.last() {
            Some(&p) => {
                if let Kind::Element { children, .. } = &mut nodes[p].kind {
                    children.push(i)
                }
            }
            None => top.push(i),
        }
    }

    loop {
        let before = reader.buffer_position() as usize;
        let event = reader
            .read_event()
            .map_err(|e| Error::Xml(format!("at byte {before}: {e}")))?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = src.as_bytes()[..after].ends_with(b"/>");
                let qname = e.name();
                let name: &str = qname.as_ref();
                let name = name.to_owned();
                let mut attrs = Vec::new();
                for a in e.attributes().with_checks(false) {
                    let a = a.map_err(|e| Error::Xml(format!("at byte {before}: {e}")))?;
                    let k: &str = a.key.as_ref();
                    let raw: &str = &a.value;
                    attrs.push((k.to_owned(), unescape(raw)));
                }
                let i = nodes.len();
                nodes.push(Node {
                    kind: Kind::Element {
                        name,
                        attrs,
                        children: Vec::new(),
                    },
                    start: before,
                    end: after,
                    start_tag_end: after,
                    end_tag_start: after,
                });
                attach(&mut nodes, &stack, &mut top, i);
                if !empty {
                    stack.push(i);
                }
            }
            Event::End(_) => {
                let i = stack
                    .pop()
                    .ok_or_else(|| Error::Xml(format!("stray end tag at {before}")))?;
                nodes[i].end = after;
                nodes[i].end_tag_start = before;
            }
            Event::Text(_) | Event::GeneralRef(_) => {
                // quick-xml splits text around entity references; merge them
                // back into one text node per run.
                let parent_children = match stack.last() {
                    Some(&p) => nodes[p].children(),
                    None => &top[..],
                };
                if let Some(&last) = parent_children.last()
                    && matches!(nodes[last].kind, Kind::Text)
                    && nodes[last].end == before
                {
                    nodes[last].end = after;
                    nodes[last].start_tag_end = after;
                    nodes[last].end_tag_start = after;
                    continue;
                }
                let i = nodes.len();
                nodes.push(Node {
                    kind: Kind::Text,
                    start: before,
                    end: after,
                    start_tag_end: after,
                    end_tag_start: after,
                });
                attach(&mut nodes, &stack, &mut top, i);
            }
            Event::Eof => break,
            _ => {
                let i = nodes.len();
                nodes.push(Node {
                    kind: Kind::Other,
                    start: before,
                    end: after,
                    start_tag_end: after,
                    end_tag_start: after,
                });
                attach(&mut nodes, &stack, &mut top, i);
            }
        }
    }
    if !stack.is_empty() {
        return Err(Error::Xml("unclosed element at end of document".into()));
    }
    let root = top
        .iter()
        .copied()
        .find(|&i| matches!(nodes[i].kind, Kind::Element { .. }))
        .ok_or_else(|| Error::Xml("no root element".into()))?;
    if bom > 0 {
        for n in &mut nodes {
            n.start += bom;
            n.end += bom;
            n.start_tag_end += bom;
            n.end_tag_start += bom;
        }
    }
    Ok(Tree { nodes, root })
}

/// Decode the five predefined entities and character references.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(j) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let ent = &rest[1..j];
        let decoded = match ent {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=j]),
        }
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    out
}

/// How a producer spells characters that may or may not be escaped. Older
/// LibreOffice versions write `'` raw; current ones write `&apos;` in both
/// text and attributes. We learn the dialect from each source part so a
/// regenerated element matches its neighbours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialect {
    /// `'` as `&apos;` (text and attributes).
    pub apos: bool,
    /// `"` as `&quot;` in text (attributes always escape it).
    pub quot_text: bool,
}

impl Default for Dialect {
    /// Current LibreOffice.
    fn default() -> Self {
        Dialect {
            apos: true,
            quot_text: true,
        }
    }
}

impl Dialect {
    pub fn detect(src: &str) -> Self {
        // The two travel together in LibreOffice's own history: versions that
        // escape one in text escape both.
        let apos = if src.contains("&apos;") {
            true
        } else {
            !src.contains('\'')
        };
        Dialect {
            apos,
            quot_text: apos || src.contains("&quot;"),
        }
    }
}

/// Escape character data: `&`, `<`, `>`, and `'` if the dialect does.
pub fn escape_text(s: &str, d: Dialect, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' if d.apos => out.push_str("&apos;"),
            '"' if d.quot_text => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// Escape an attribute value for double quotes. Tabs and newlines become
/// character references so attribute-value normalisation can't eat them.
pub fn escape_attr(s: &str, d: Dialect, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' if d.apos => out.push_str("&apos;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xa;"),
            '\r' => out.push_str("&#xd;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_cover_exact_bytes() {
        let src = r#"<?xml version="1.0"?><a x="1"><b>t&amp;u</b><c/>tail</a>"#;
        let t = parse(src).unwrap();
        let a = t.root();
        assert_eq!(
            &src[a.start..a.end],
            r#"<a x="1"><b>t&amp;u</b><c/>tail</a>"#
        );
        let kids: Vec<_> = a
            .children()
            .iter()
            .map(|&i| &src[t.node(i).start..t.node(i).end])
            .collect();
        assert_eq!(kids, ["<b>t&amp;u</b>", "<c/>", "tail"]);
        let b = t.node(a.children()[0]);
        assert_eq!(b.children().len(), 1, "text around an entity is one node");
        assert_eq!(t.text_only(src, a.children()[0]).unwrap(), "t&u");
        let c = t.node(a.children()[1]);
        assert!(c.is_empty_element());
        assert_eq!(&src[b.start_tag_end..b.end_tag_start], "t&amp;u");
    }

    #[test]
    fn attributes_are_decoded() {
        let t = parse(r#"<a v="&lt;&#x41;&#66;&quot;"/>"#).unwrap();
        assert_eq!(t.root().attr("v"), Some("<AB\""));
    }

    #[test]
    fn byte_order_mark_keeps_spans_right() {
        let src = "\u{feff}<?xml version=\"1.0\"?><a><b/><c>t</c></a>";
        let t = parse(src).unwrap();
        let kids: Vec<_> = t
            .root()
            .children()
            .iter()
            .map(|&i| &src[t.node(i).start..t.node(i).end])
            .collect();
        assert_eq!(kids, ["<b/>", "<c>t</c>"]);
    }

    #[test]
    fn mismatched_tags_fail() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
    }

    #[test]
    fn escape_round_trip() {
        let s = "a<b>&\"c\"\t\n";
        for apos in [true, false] {
            let d = Dialect {
                apos,
                quot_text: apos,
            };
            let mut e = String::new();
            escape_attr(s, d, &mut e);
            assert_eq!(unescape(&e), s);
        }
        let mut t = String::new();
        escape_text("x<y&z'\"", Dialect::default(), &mut t);
        assert_eq!(t, "x&lt;y&amp;z&apos;&quot;");
        assert!(!Dialect::detect("<a b='c'/>").apos);
        assert!(Dialect::detect("<a>&apos;</a>").apos);
    }
}
