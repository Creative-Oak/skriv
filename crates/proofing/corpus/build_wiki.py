#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 6: mix the fetched Wikipedia articles into labelled documents.

Each document takes one base article (mostly Danish) and inserts "islands"
of consecutive paragraphs from an article in the other language, the way a
Danish report carries an English abstract, a quoted English passage or an
English appendix. Every paragraph is labelled with the language of the
article it came from. Deterministic (fixed seed); writes
corpus/wiki-build/*.tsv (gitignored, since the text is CC BY-SA).
"""

import json
import pathlib
import random

HERE = pathlib.Path(__file__).resolve().parent
CACHE = HERE / "wiki-cache"
OUT = HERE / "wiki-build"

# (name, default language, base articles, island articles, base budget in chars)
DOCS = [
    ("w01-strukturreform", "da", ["Strukturreformen"], ["Municipalities of Denmark"], 4000),
    ("w02-folketinget", "da", ["Folketinget"], ["Folketing"], 4000),
    ("w03-metro", "da", ["Københavns Metro"], ["Copenhagen Metro"], 4500),
    ("w04-storebaelt", "da", ["Storebæltsforbindelsen"], ["Great Belt Bridge"], 4000),
    ("w05-statistik", "da", ["Danmarks Statistik", "KL"], ["Statistics Denmark"], 4000),
    ("w06-dansk", "da", ["Dansk (sprog)"], ["Danish language"], 4500),
    ("w07-folkeskolen", "da", ["Folkeskolen", "Region Midtjylland"], ["Spell checker"], 4000),
    ("w08-aarhus", "da", ["Aarhus Kommune"], ["Wind power in Denmark"], 4000),
    ("w09-gdpr", "en", ["General Data Protection Regulation"], ["Danmarks Riges Grundlov"], 4000),
    ("w10-odf", "en", ["OpenDocument", "LibreOffice"], ["Danmarks Riges Grundlov"], 4000),
]
ISLAND_SIZES = [1, 1, 1, 2, 2, 3, 4, 6]
MAX_LIST_RUN = 8


def load():
    arts = {}
    for f in CACHE.glob("*.json"):
        d = json.loads(f.read_text(encoding="utf-8"))
        arts[d["title"]] = d
    return arts


def base_blocks(arts, titles, budget):
    out = []
    for title in titles:
        run = 0
        used = 0
        for kind, text in arts[title]["blocks"]:
            run = run + 1 if kind == "li" else 0
            if run > MAX_LIST_RUN:
                continue
            out.append((arts[title]["lang"], kind, text))
            used += len(text)
            if used >= budget / len(titles):
                break
    # Don't end on a heading.
    while out and out[-1][1] == "h":
        out.pop()
    return out


def islands(rng, arts, titles, count):
    pool = []
    for title in titles:
        blocks = arts[title]["blocks"]
        lang = arts[title]["lang"]
        for i, (kind, _) in enumerate(blocks):
            if kind == "p" and len(blocks[i][1]) >= 120:
                pool.append((lang, blocks, i))
    rng.shuffle(pool)
    out = []
    used = set()
    for lang, blocks, i in pool:
        if len(out) == count:
            break
        n = rng.choice(ISLAND_SIZES)
        span = range(i - 1, i + n + 1)
        if any((id(blocks), j) in used for j in span):
            continue
        isl = []
        if i > 0 and blocks[i - 1][0] == "h" and rng.random() < 0.5:
            isl.append((lang, "h", blocks[i - 1][1]))
        for kind, text in blocks[i:i + n]:
            # Glossed examples ("Jeg så {huset}") are in the other language.
            if kind == "h" or len(text) < 60 or "{" in text:
                break
            isl.append((lang, kind, text))
        used.update((id(blocks), j) for j in span)
        out.append(isl)
    return out


def main():
    arts = load()
    OUT.mkdir(exist_ok=True)
    rng = random.Random(6)
    total = 0
    for name, default, base_titles, island_titles, budget in DOCS:
        base = base_blocks(arts, base_titles, budget)
        n_isl = max(3, budget // 1200)
        isl = islands(rng, arts, island_titles, n_isl + 1)
        abstract, isl = isl[0], isl[1:]
        # Insertion points: before a base paragraph or heading that is not
        # inside a list run, so a neutral list never directly follows an island.
        points = [i for i in range(1, len(base)) if base[i][1] in ("p", "h") and base[i - 1][1] != "h"]
        rng.shuffle(points)
        chosen = sorted(points[:len(isl)], reverse=True)
        doc = list(base)
        for at, block in zip(chosen, isl):
            doc[at:at] = block
        if rng.random() < 0.4:
            # An abstract in the other language at the top.
            head = "Abstract" if default == "da" else "Resumé"
            lead = [b for b in abstract if b[1] == "p"][:2]
            doc[0:0] = [(lead[0][0], "h", head)] + lead
        sources = ", ".join(f"{arts[t]['lang']}.wikipedia.org oldid {arts[t]['rev']} ({t})"
                            for t in base_titles + island_titles)
        lines = [f"#default={default}", f"#source=Wikipedia, CC BY-SA 4.0: {sources}"]
        for lang, kind, text in doc:
            lines.append(f"{lang}\t{kind}\t{text}")
        (OUT / f"{name}.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
        chars = sum(len(t) for _, _, t in doc)
        total += chars
        print(f"{name}: {len(doc)} paragraphs, {chars} chars")
    print(f"total {total} chars = {total / 2500:.1f} pages")


if __name__ == "__main__":
    main()
