#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Summarise spike5.py's results.json as Markdown tables.

Usage: python3 fidelity/scripts/docx/spike5_report.py <results.json> [set ...]
"""

import collections
import json
import sys

sys.path.insert(0, __import__("os").path.dirname(__file__))
from docx_features import FEATURES  # noqa: E402

PATHS = [
    ("lo", "LibreOffice DOCX→DOCX"),
    ("lo2", "LibreOffice DOCX→ODT→DOCX"),
    ("sk", "…→ODT→skriv→ODT→… (preserving)"),
    ("skr", "…→ODT→skriv→ODT→… (regenerated)"),
]


def pct(a, b):
    return f"{100 * a / b:.1f}%" if b else "–"


def main():
    r = json.load(open(sys.argv[1]))
    sets = sys.argv[2:] or sorted({x["set"] for x in r})
    for s in sets:
        docs = [x for x in r if x["set"] == s]
        ok = [x for x in docs if "census" in x]
        print(f"\n## Corpus `{s}`: {len(docs)} files, {len(ok)} readable\n")
        # Census.
        print("| Feature | Documents with it | Instances |")
        print("|---|---|---|")
        for f in FEATURES:
            n = sum(1 for x in ok if x["census"].get(f))
            k = sum(x["census"].get(f, 0) for x in ok)
            if n:
                print(f"| {f} | {n} ({pct(n, len(ok))}) | {k} |")
        # Conversion failures.
        print()
        for p, label in PATHS:
            missing = sum(1 for x in ok if not isinstance(x.get(p), dict) or "error" in (x.get(p) or {}))
            print(f"- {label}: {len(ok) - missing}/{len(ok)} produced a readable DOCX")
        errs = collections.Counter((x.get("lo_import_error") or "") for x in docs if x.get("lo_import_error"))
        for e, n in errs.items():
            print(f"  - LibreOffice import failed ({n}): {e}")
        sk_errs = collections.Counter(v for x in docs for v in (x.get("skriv") or {}).values() if v)
        for e, n in sk_errs.most_common(5):
            print(f"  - skriv round trip failed ({n}): {e[:160]}")
        # Validity.
        print("\n| Validates against ECMA-376 Transitional | " + " | ".join(["original"] + [lbl for _, lbl in PATHS]) + " |")
        print("|---|" + "---|" * (1 + len(PATHS)))
        row = []
        for p in ["orig"] + [p for p, _ in PATHS]:
            have = [x for x in ok if x.get("valid", {}).get(p) is not None]
            good = sum(1 for x in have if not x["valid"][p])
            row.append(f"{good}/{len(have)} ({pct(good, len(have))})")
        print("| files | " + " | ".join(row) + " |")
        first = collections.Counter()
        for x in ok:
            for p, _ in PATHS:
                for part, n, msg in x.get("valid", {}).get(p) or []:
                    if not x["valid"].get("orig"):
                        first[(p, part.split("/")[-1].rstrip("0123456789.xml") or part, msg.split(":", 1)[-1][:120])] += 1
        if first:
            print("\nNew validation errors (in files whose original validates), most common:\n")
            for (p, part, msg), n in first.most_common(8):
                print(f"- {p}, {part}: {msg.strip()} ({n})")
        # Fidelity per feature.
        print("\n| Feature (instances kept) | " + " | ".join(lbl for _, lbl in PATHS) + " |")
        print("|---|" + "---|" * len(PATHS))
        for f in FEATURES:
            cells = []
            any_total = False
            for p, _ in PATHS:
                kept = total = docs_lost = 0
                for x in ok:
                    v = x.get(p)
                    if not isinstance(v, dict) or "error" in v:
                        continue
                    kept += v[f]["kept"]
                    total += v[f]["total"]
                    if v[f]["kept"] < v[f]["total"]:
                        docs_lost += 1
                any_total |= total > 0
                cells.append(f"{pct(kept, total)} ({total - kept} lost, {docs_lost} docs)")
            if any_total:
                print(f"| {f} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
