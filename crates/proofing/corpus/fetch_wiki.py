#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 6: fetch the Wikipedia part of the proofing corpus.

Downloads pinned revisions (wiki-manifest.tsv) of Danish and English
Wikipedia articles, keeps headings, body paragraphs and list items, and
writes them to corpus/wiki-cache/ (gitignored). Wikipedia text is
CC BY-SA 4.0; it is not committed. build_wiki.py mixes it into labelled
documents.

    python3 fetch_wiki.py          # fetch the pinned revisions
    python3 fetch_wiki.py --pin    # resolve current revisions into the manifest
"""

import html.parser
import json
import pathlib
import sys
import time
import urllib.parse
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
MANIFEST = HERE / "wiki-manifest.tsv"
CACHE = HERE / "wiki-cache"
UA = "skriv-corpus-fetch/0.1 (spell-checking research; low volume)"

ARTICLES = {
    "da": [
        "Kommunalreformen 2007",
        "Folketinget",
        "Aarhus Kommune",
        "Kommunernes Landsforening",
        "Danmarks Statistik",
        "Vindkraft i Danmark",
        "Danmarks Riges Grundlov",
        "Folkeskolen",
        "Københavns Metro",
        "Storebæltsforbindelsen",
        "Region Midtjylland",
        "Dansk (sprog)",
    ],
    "en": [
        "Municipalities of Denmark",
        "OpenDocument",
        "LibreOffice",
        "General Data Protection Regulation",
        "Wind power in Denmark",
        "Copenhagen Metro",
        "Great Belt Fixed Link",
        "Open-source software",
        "Folketing",
        "Danish language",
        "Spell checker",
        "Statistics Denmark",
    ],
}

STOP_SECTIONS = {
    "referencer", "eksterne henvisninger", "se også", "litteratur", "kilder",
    "noter", "kilder og henvisninger", "henvisninger", "fodnoter",
    "references", "external links", "see also", "notes", "further reading",
    "bibliography", "sources", "citations", "footnotes",
}
SKIP_TAGS = {"sup", "style", "script", "table", "figure", "math", "img", "audio", "video"}
SKIP_CLASSES = {
    "reference", "mw-editsection", "navbox", "infobox", "thumb", "hatnote",
    "noprint", "metadata", "mw-references-wrap", "reflist", "toc", "gallery",
    "mw-empty-elt", "shortdescription", "ambox", "sidebar", "mwe-math-element",
    "mw-cite-backlink", "printfooter", "dablink", "geo", "coordinates",
}
BLOCKS = {"p": "p", "h2": "h", "h3": "h", "h4": "h", "li": "li"}


def get(url):
    for attempt in range(5):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": UA})
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.load(r)
        except Exception as e:  # noqa: BLE001
            print(f"retry {attempt + 1}: {e}", file=sys.stderr)
            time.sleep(2 * (attempt + 1))
    raise SystemExit(f"failed: {url}")


class Extract(html.parser.HTMLParser):
    """Top-level blocks of a MediaWiki parse result, as (kind, text)."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.stack = []  # (tag, skipping)
        self.block = None  # (kind, depth, parts)
        self.blocks = []
        self.stopped = False
        self.list_depth = 0

    def handle_starttag(self, tag, attrs):
        if tag in ("br", "img", "wbr", "hr", "meta", "link", "input"):
            return
        classes = set((dict(attrs).get("class") or "").split())
        parent_skip = self.stack[-1][1] if self.stack else False
        skip = parent_skip or tag in SKIP_TAGS or bool(classes & SKIP_CLASSES)
        if tag in ("ul", "ol", "dl"):
            self.list_depth += 1
        self.stack.append((tag, skip))
        if skip or self.stopped:
            return
        if self.block is None and tag in BLOCKS:
            # Only first-level list items: nested lists stay inside their item.
            if tag == "li" and self.list_depth != 1:
                return
            self.block = (BLOCKS[tag], len(self.stack), [])

    def handle_endtag(self, tag):
        if tag in ("br", "img", "wbr", "hr", "meta", "link", "input"):
            return
        while self.stack:
            t, _ = self.stack.pop()
            if t in ("ul", "ol", "dl"):
                self.list_depth -= 1
            if self.block is not None and len(self.stack) < self.block[1]:
                kind, _, parts = self.block
                text = " ".join("".join(parts).split())
                self.block = None
                if kind == "h" and text.lower() in STOP_SECTIONS:
                    self.stopped = True
                elif text and not self.stopped:
                    self.blocks.append((kind, text))
            if t == tag:
                break

    def handle_data(self, data):
        if self.block is not None and not (self.stack and self.stack[-1][1]):
            self.block[2].append(data)


def pin():
    rows = []
    for lang, titles in ARTICLES.items():
        q = urllib.parse.urlencode({
            "action": "query", "titles": "|".join(titles), "prop": "revisions",
            "rvprop": "ids", "format": "json", "formatversion": "2", "redirects": "1",
        })
        data = get(f"https://{lang}.wikipedia.org/w/api.php?{q}")
        for page in data["query"]["pages"]:
            if "missing" in page:
                print(f"missing: {lang} {page['title']}", file=sys.stderr)
                continue
            rows.append((lang, page["title"], page["revisions"][0]["revid"]))
        time.sleep(1)
    with MANIFEST.open("w", encoding="utf-8") as f:
        f.write("# lang\ttitle\trevision (CC BY-SA 4.0, https://{lang}.wikipedia.org/w/index.php?oldid=REV)\n")
        for lang, title, rev in rows:
            f.write(f"{lang}\t{title}\t{rev}\n")
    print(f"pinned {len(rows)} articles")


def fetch():
    CACHE.mkdir(exist_ok=True)
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        if line.startswith("#") or not line.strip():
            continue
        lang, title, rev = line.split("\t")
        out = CACHE / f"{lang}-{rev}.json"
        if out.exists():
            continue
        q = urllib.parse.urlencode({
            "action": "parse", "oldid": rev, "prop": "text", "format": "json",
            "formatversion": "2", "disableeditsection": "1", "disabletoc": "1",
        })
        data = get(f"https://{lang}.wikipedia.org/w/api.php?{q}")
        ex = Extract()
        ex.feed(data["parse"]["text"])
        blocks = [b for b in ex.blocks if len(b[1]) > 1]
        out.write_text(json.dumps({"lang": lang, "title": title, "rev": int(rev),
                                   "blocks": blocks}, ensure_ascii=False, indent=0),
                       encoding="utf-8")
        chars = sum(len(t) for _, t in blocks)
        print(f"{lang} {title} ({rev}): {len(blocks)} blocks, {chars} chars")
        time.sleep(1)


if __name__ == "__main__":
    pin() if "--pin" in sys.argv else fetch()
