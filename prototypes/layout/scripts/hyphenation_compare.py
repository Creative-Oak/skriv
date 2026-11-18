#!/usr/bin/env python3
"""Spike 2b: compare hypher's Danish hyphenation with LibreOffice's.

usage: hyphenation_compare.py <hypher-da.tsv> <hyph_da_DK.dic>

LibreOffice hyphenates with libhyphen and the `hyph_da_DK.dic` patterns
(Debian package hyphen-da, from libreoffice-dictionaries). This applies the
same patterns with Liang's algorithm, at 2/2 minimum characters before and
after a break (what our justified test document asks for), and compares
break points word by word with what hypher (TeX hyph-utf8 patterns, via
`layout-bench hyph`) produced.
"""
import sys

def load(path):
    lines = open(path, encoding="latin-1").read().split("\n")
    patterns = {}
    for line in lines[1:]:
        line = line.strip()
        if not line or line.startswith("%") or line.isupper():
            continue
        letters = "".join(c for c in line if not c.isdigit())
        weights = [0] * (len(letters) + 1)
        i = 0
        for c in line:
            if c.isdigit():
                weights[i] = int(c)
            else:
                i += 1
        patterns[letters] = weights
    return patterns

def liang(word, patterns, left=2, right=2):
    w = "." + word.lower() + "."
    points = [0] * (len(w) + 1)
    for i in range(len(w)):
        for j in range(i + 1, len(w) + 1):
            p = patterns.get(w[i:j])
            if p:
                for k, v in enumerate(p):
                    points[i + k] = max(points[i + k], v)
    # points[k] is the weight before w[k]; a break before word[k-1]...
    breaks = []
    for k in range(left, len(word) - right + 1):
        if points[k + 1] % 2 == 1:
            breaks.append(k)
    return breaks

def positions(syllables):
    out, n = [], 0
    for s in syllables.split("-")[:-1]:
        n += len(s)
        out.append(n)
    return out

patterns = load(sys.argv[2])
same = only_lo = only_hy = words = differ = 0
examples = []
for row in open(sys.argv[1], encoding="utf-8"):
    word, syl = row.rstrip("\n").split("\t")
    words += 1
    a = set(liang(word, patterns))
    b = set(positions(syl))
    same += len(a & b)
    only_lo += len(a - b)
    only_hy += len(b - a)
    if a != b:
        differ += 1
        if len(examples) < 15:
            lo = "".join(("-" if i in a else "") + c for i, c in enumerate(word))
            examples.append(f"  LibreOffice {lo:24} hypher {syl}")
print(f"{words} words; identical in {words - differ} ({100 * (words - differ) / words:.1f}%)")
print(f"break points: {same} shared, {only_lo} only LibreOffice, {only_hy} only hypher")
print("\n".join(examples))
