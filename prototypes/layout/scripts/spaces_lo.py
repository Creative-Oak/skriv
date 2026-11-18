#!/usr/bin/env python3
"""Spike 2b: inter-word space widths on LibreOffice's justified lines.

usage: spaces_lo.py <justified.pdf>

From `pdftotext -bbox-layout` word boxes: the gap between consecutive words
on a line, relative to the natural space of 12 pt Liberation Serif (3 pt).
A paragraph's last line is excluded by dropping lines whose right edge is
well short of the measure.
"""
import re, subprocess, sys

out = subprocess.run(["pdftotext", "-bbox-layout", sys.argv[1], "-"], capture_output=True, text=True).stdout
ratios = []
for line in re.findall(r'<line [^>]*>(.*?)</line>', out, re.S):
    words = [(float(a), float(b)) for a, b in re.findall(r'<word xMin="([\d.]+)" yMin="[\d.]+" xMax="([\d.]+)"', line)]
    if len(words) < 4 or words[0][0] > 60 or words[-1][1] < 530:
        continue  # not a full body line
    for (a0, a1), (b0, b1) in zip(words, words[1:]):
        gap = b0 - a1
        if gap > 0:
            ratios.append(gap / 3.0)
ratios.sort()
n = len(ratios)
print(f"libreoffice: {n} spaces on justified lines; width / natural: min {ratios[0]:.2f}, mean {sum(ratios)/n:.2f}, "
      f"p95 {ratios[int((n-1)*0.95)]:.2f}, max {ratios[-1]:.2f}; wider than 2x: {sum(1 for r in ratios if r > 2)}")
