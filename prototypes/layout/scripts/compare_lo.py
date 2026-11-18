#!/usr/bin/env python3
"""Spike 2b: compare an engine's line breaks with LibreOffice's.

usage: compare_lo.py <libreoffice.pdf> <lines-ENGINE.tsv> [--detail]

LibreOffice's lines come from `pdftotext -bbox-layout` (header, footer and
footnote lines dropped); the engine's from `layout-bench lines`. Reports:
pages; body lines; the share of LibreOffice's lines the engine reproduces
exactly, in order (difflib on line texts); and the share of pages that
start with the same line.
"""
import difflib, re, subprocess, sys, collections

def lo_lines(pdf):
    out = subprocess.run(["pdftotext", "-bbox-layout", pdf, "-"], capture_output=True, text=True).stdout
    pages = []
    for page in re.findall(r"<page[^>]*>(.*?)</page>", out, re.S):
        lines = []
        for ym, body in re.findall(r'<line xMin="[\d.]+" yMin="([\d.]+)"[^>]*>(.*?)</line>', page, re.S):
            text = " ".join(re.findall(r">([^<]*)</word>", body)).strip()
            y = float(ym)
            if y < 60 or y > 750:  # header, footnotes and footer
                continue
            lines.append(text)
        pages.append(lines)
    return pages

TABLE = set()

def engine_lines(tsv):
    """Body lines per page; table-cell lines are left out (pdftotext merges
    cells side by side into one line, so they can't be compared)."""
    pages = collections.OrderedDict()
    for row in open(tsv, encoding="utf-8"):
        pg, para, text = row.rstrip("\n").split("\t")
        text = " ".join(text.split())
        pages.setdefault(int(pg), [])
        if para.startswith("T"):
            TABLE.add(text)
            continue
        pages[int(pg)].append(text)
    return list(pages.values())

def norm(t):
    """List labels ("1.", "•") and footnote numbers are drawn by LibreOffice
    but aren't in the paragraph text the engines lay out; soft hyphens are
    dropped."""
    t = " ".join(t.replace("\u00ad", "").split())
    t = re.sub(r"^(\d+\.|•)\s+", "", t)
    t = re.sub(r"\s+-$", "-", t)  # a justified line's hyphen after stretched space
    return "<citation>" if t.isdigit() else t

en = engine_lines(sys.argv[2])
lo = [[l for l in p if not any(t and t in l for t in TABLE)] for p in lo_lines(sys.argv[1])]
a = [norm(l) for p in lo for l in p]
b = [norm(l) for p in en for l in p]
sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
same = sum(m.size for m in sm.get_matching_blocks())
starts = sum(1 for x, y in zip(lo, en) if x and y and norm(x[0]) == norm(y[0]))
print(f"pages LO {len(lo)} engine {len(en)}; lines LO {len(a)} engine {len(b)}; "
      f"identical lines {same}/{len(a)} ({100*same/max(1,len(a)):.1f}%); "
      f"pages starting alike {starts}/{min(len(lo), len(en))}")
if "--detail" in sys.argv:
    for i, (x, y) in enumerate(zip(lo, en)):
        if not x or not y or norm(x[0]) != norm(y[0]):
            print(f"first page starting differently: {i+1}")
            for k in range(max(0, i - 1), i + 1):
                print(f"  LO p{k+1} last:   {lo[k][-2:]}\n  eng p{k+1} last:  {en[k][-2:]}")
            print(f"  LO p{i+1} first: {lo[i][:2]}\n  eng p{i+1} first: {en[i][:2]}")
            break
