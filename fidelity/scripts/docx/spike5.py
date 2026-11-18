#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 5: DOCX through LibreOffice and skriv, feature by feature.

For every .docx in the corpus directories:

  orig.docx  the file as found
  lo.docx    LibreOffice: open DOCX, save DOCX          (LibreOffice alone)
  lo.odt     LibreOffice: open DOCX, save ODT           (import via LibreOffice)
  sk.docx    lo.odt -> skriv (read, write) -> LibreOffice -> DOCX
  skr.docx   lo.odt -> skriv (regenerate every node from the model) -> LibreOffice -> DOCX

Each DOCX gets a feature inventory (docx_features.py) compared with the
original's, and is validated against ECMA-376 Transitional
(docx_validate.py). Results go to <out>/results.json.

Usage: python3 fidelity/scripts/docx/spike5.py --xsd <dir> --out <dir> <corpus dir>...
"""

import argparse
import concurrent.futures as cf
import glob
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "..", "uno"))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

import docx_features  # noqa: E402
import docx_validate  # noqa: E402


# ------------------------------------------------------------ LibreOffice ---

def lo_batch(args):
    """Run conversions [(in, [(out, filter), ...]), ...] in one soffice; returns {in: error or None}."""
    soffice, jobs = args
    import lo_pipe

    profile = tempfile.mkdtemp(prefix="skriv-s5-")
    done = {}
    try:
        with lo_pipe.office(soffice, profile) as (_ctx, desktop):
            for src, outs in jobs:
                try:
                    doc = lo_pipe.load(desktop, src)
                    if doc is None:
                        raise RuntimeError("not loaded")
                    for out, flt in outs:
                        lo_pipe.store(doc, out, flt)
                    doc.close(True)
                    done[src] = None
                except Exception as e:  # noqa: BLE001
                    done[src] = repr(e)[:200]
    finally:
        shutil.rmtree(profile, ignore_errors=True)
    return done


def run_lo(soffice, jobs, workers, timeout_per_doc=90):
    """Convert with a watchdog: batches of 10; a batch that hangs is retried one by one."""
    results = {}
    batches = [jobs[i:i + 10] for i in range(0, len(jobs), 10)]

    def one(batch):
        # A child process per batch, so a hanging soffice can be killed.
        tmp = tempfile.mktemp(suffix=".json")
        spec = tempfile.mktemp(suffix=".json")
        json.dump({"soffice": soffice, "jobs": batch}, open(spec, "w"))
        try:
            p = subprocess.Popen([sys.executable, __file__, "--child", spec, tmp], start_new_session=True)
            p.wait(timeout=timeout_per_doc * len(batch) + 30)
            return json.load(open(tmp)) if os.path.exists(tmp) else {s: "no result" for s, _ in batch}
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL)
            if len(batch) == 1:
                return {batch[0][0]: "timeout"}
            out = {}
            for j in batch:
                out.update(one([j]))
            return out
        finally:
            for f in (tmp, spec):
                if os.path.exists(f):
                    os.remove(f)

    with cf.ThreadPoolExecutor(workers) as ex:
        for r in ex.map(one, batches):
            results.update(r)
    return results


# ------------------------------------------------------------------ main ---

def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("corpus", nargs="*")
    ap.add_argument("--xsd", required=False)
    ap.add_argument("--out", default="target/spike5")
    ap.add_argument("--soffice", default=os.environ.get("SKRIV_SOFFICE", "soffice"))
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--limit", type=int)
    ap.add_argument("--child", nargs=2)
    a = ap.parse_args()
    if a.child:
        spec = json.load(open(a.child[0]))
        res = lo_batch((spec["soffice"], [(s, [tuple(o) for o in outs]) for s, outs in spec["jobs"]]))
        json.dump(res, open(a.child[1], "w"))
        return

    out = os.path.abspath(a.out)
    work = os.path.join(out, "work")
    os.makedirs(work, exist_ok=True)
    files = []
    for c in a.corpus:
        set_name = os.path.basename(os.path.normpath(c))
        for f in sorted(glob.glob(os.path.join(c, "*.docx"))):
            files.append((set_name, os.path.abspath(f)))
    if a.limit:
        files = files[: a.limit]
    docs = []
    for set_name, f in files:
        d = os.path.join(work, set_name, os.path.basename(f)[:16])
        os.makedirs(d, exist_ok=True)
        docs.append({"set": set_name, "id": os.path.basename(f)[:-5], "orig": f, "dir": d})
    t0 = time.time()

    # 1. LibreOffice: DOCX -> DOCX and DOCX -> ODT.
    jobs = [(x["orig"], [(os.path.join(x["dir"], "lo.docx"), "MS Word 2007 XML"), (os.path.join(x["dir"], "lo.odt"), "writer8")]) for x in docs]
    r1 = run_lo(a.soffice, jobs, a.workers)
    print(f"LibreOffice import: {sum(1 for v in r1.values() if v is None)}/{len(jobs)} in {time.time() - t0:.0f} s", flush=True)

    # 2. skriv in the middle: preserve, and regenerate.
    fid = os.path.join(ROOT, "target", "release", "skriv-fidelity")
    for x in docs:
        x["skriv"] = {}
        lo_odt = os.path.join(x["dir"], "lo.odt")
        if not os.path.exists(lo_odt):
            continue
        for name, extra in (("sk.odt", []), ("skr.odt", ["--regenerate"])):
            p = subprocess.run([fid, "roundtrip", lo_odt, os.path.join(x["dir"], name), *extra], capture_output=True, text=True, timeout=120)
            x["skriv"][name] = None if p.returncode == 0 else p.stderr.strip()[-300:]

    # 3. LibreOffice: skriv's ODT -> DOCX.
    jobs = []
    for x in docs:
        for name in ("sk", "skr"):
            src = os.path.join(x["dir"], f"{name}.odt")
            if os.path.exists(src):
                jobs.append((src, [(os.path.join(x["dir"], f"{name}.docx"), "MS Word 2007 XML")]))
    r2 = run_lo(a.soffice, jobs, a.workers)
    print(f"LibreOffice export: {sum(1 for v in r2.values() if v is None)}/{len(jobs)} in {time.time() - t0:.0f} s", flush=True)

    # 4. Inventories, comparisons, validation.
    validator = docx_validate.Validator(a.xsd) if a.xsd else None
    results = []
    for x in docs:
        rec = {"set": x["set"], "id": x["id"], "lo_import_error": r1.get(x["orig"]), "skriv": x["skriv"]}
        try:
            orig = docx_features.inventory(x["orig"])
        except Exception as e:  # noqa: BLE001
            rec["error"] = f"original unreadable: {e!r}"[:200]
            results.append(rec)
            continue
        rec["census"] = {k: len(v) for k, v in orig.items() if v}
        if validator:
            rec["valid"] = {"orig": validator.validate(x["orig"])}
        for name in ("lo", "sk", "skr"):
            path = os.path.join(x["dir"], f"{name}.docx")
            if not os.path.exists(path):
                rec[name] = None
                continue
            try:
                inv = docx_features.inventory(path)
                rec[name] = docx_features.compare(orig, inv)
            except Exception as e:  # noqa: BLE001
                rec[name] = {"error": repr(e)[:200], "trace": traceback.format_exc()[-400:]}
            if validator:
                rec["valid"][name] = validator.validate(path)
        results.append(rec)
    json.dump(results, open(os.path.join(out, "results.json"), "w"), indent=1, ensure_ascii=False)
    print(f"{len(results)} documents in {time.time() - t0:.0f} s -> {out}/results.json")


if __name__ == "__main__":
    main()
