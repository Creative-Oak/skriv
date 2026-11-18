#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Spike 4: a person edits skriv's files in LibreOffice and saves them.

Reads <work>/jobs.json (written by `spike4 prepare`), performs each job's
edits through UNO the way a person would (typing, the Bold command, Enter,
copy and paste through the view), saves, and writes <work>/lo-log.json.

Save paths:
  store    load, edit, save as ODT (File > Save)
  convert  soffice --convert-to odt (no edit)
  docx     load, edit, save as DOCX, close, open the DOCX, save as ODT

The LibreOffice profile is given a user name, so saved files and tracked
changes carry an author, as they would on a person's machine.

Usage: python3 fidelity/scripts/spike4_lo.py <work dir> [--soffice soffice]
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import traceback

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "uno"))
import lo_pipe  # noqa: E402

GIVEN, SURNAME = "Lis", "Larsen"

XCU = f"""<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<item oor:path="/org.openoffice.UserProfile/Data"><prop oor:name="givenname" oor:op="fuse"><value>{GIVEN}</value></prop></item>
<item oor:path="/org.openoffice.UserProfile/Data"><prop oor:name="sn" oor:op="fuse"><value>{SURNAME}</value></prop></item>
</oor:items>
"""


def make_profile():
    p = tempfile.mkdtemp(prefix="skriv-spike4-profile-")
    os.makedirs(os.path.join(p, "user"), exist_ok=True)
    with open(os.path.join(p, "user", "registrymodifications.xcu"), "w") as f:
        f.write(XCU)
    return p


def find(doc, snippet):
    sd = doc.createSearchDescriptor()
    sd.SearchString = snippet
    sd.SearchCaseSensitive = True
    sd.SearchRegularExpression = False
    found = doc.findAll(sd)
    if found.getCount() != 1:
        raise RuntimeError(f"snippet found {found.getCount()} times: {snippet!r}")
    return found.getByIndex(0)


def para_cursor(r):
    """A cursor selecting the paragraph that holds range r."""
    c = r.getText().createTextCursorByRange(r.getStart())
    c.gotoStartOfParagraph(False)
    c.gotoEndOfParagraph(True)
    return c


def dispatch(ctx, doc, command):
    helper = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.DispatchHelper", ctx)
    frame = doc.getCurrentController().getFrame()
    helper.executeDispatch(frame, command, "", 0, ())


def op_insert(ctx, doc, op):
    r = find(doc, op["find"])
    t = r.getText()
    t.insertString(t.createTextCursorByRange(r.getEnd()), op["text"], False)


def op_delete(ctx, doc, op):
    find(doc, op["find"]).setString("")


def op_bold(ctx, doc, op):
    r = find(doc, op["find"])
    doc.getCurrentController().select(r)
    dispatch(ctx, doc, ".uno:Bold")


def op_split(ctx, doc, op):
    from com.sun.star.text.ControlCharacter import PARAGRAPH_BREAK

    r = find(doc, op["find"])
    t = r.getText()
    t.insertControlCharacter(t.createTextCursorByRange(r.getEnd()), PARAGRAPH_BREAK, False)


def op_join(ctx, doc, op):
    r = find(doc, op["find"])
    c = r.getText().createTextCursorByRange(r.getEnd())
    if not c.isEndOfParagraph():
        raise RuntimeError("join snippet does not end its paragraph")
    c.goRight(1, True)
    c.setString("")


def select_paragraph_with_break(r):
    """Select a whole paragraph and the paragraph break after it."""
    c = r.getText().createTextCursorByRange(r.getStart())
    c.gotoStartOfParagraph(False)
    c.gotoEndOfParagraph(True)
    if not c.goRight(1, True):
        raise RuntimeError("paragraph is the last in its text")
    return c


def op_delete_para(ctx, doc, op):
    select_paragraph_with_break(find(doc, op["find"])).setString("")


def transfer(doc, rng):
    ctl = doc.getCurrentController()
    ctl.select(rng)
    return ctl.getTransferable()


def op_dup_para(ctx, doc, op):
    # Select the paragraph and its break, copy, and paste at the start of the
    # next paragraph: the paragraph appears twice, one after the other.
    sel = select_paragraph_with_break(find(doc, op["find"]))
    tr = transfer(doc, sel)
    at = sel.getText().createTextCursorByRange(sel.getEnd())
    doc.getCurrentController().select(at)
    doc.getCurrentController().insertTransferable(tr)


def op_move_para(ctx, doc, op):
    sel = select_paragraph_with_break(find(doc, op["find"]))
    tr = transfer(doc, sel)
    sel.setString("")
    target = find(doc, op["before"])
    at = target.getText().createTextCursorByRange(target.getStart())
    at.gotoStartOfParagraph(False)
    doc.getCurrentController().select(at)
    doc.getCurrentController().insertTransferable(tr)


def op_heading(ctx, doc, op):
    para_cursor(find(doc, op["find"])).setPropertyValue("ParaStyleName", f"Heading {op['level']}")


def cell_position(r):
    cell = r.Cell
    table = r.TextTable
    name = cell.CellName  # e.g. "B3"; merged tables use "B3.1.1" style names
    col = 0
    i = 0
    while i < len(name) and name[i].isalpha():
        col = col * 26 + (ord(name[i].upper()) - 64)
        i += 1
    row = int("".join(ch for ch in name[i:].split(".")[0]))
    return table, row - 1, col - 1


def op_row_insert(ctx, doc, op):
    table, row, _ = cell_position(find(doc, op["find"]))
    table.getRows().insertByIndex(row + 1, 1)


def op_col_delete(ctx, doc, op):
    table, _, col = cell_position(find(doc, op["find"]))
    table.getColumns().removeByIndex(col, 1)


def op_list_add(ctx, doc, op):
    from com.sun.star.text.ControlCharacter import PARAGRAPH_BREAK

    r = find(doc, op["find"])
    t = r.getText()
    c = t.createTextCursorByRange(r.getEnd())
    c.gotoEndOfParagraph(False)
    t.insertControlCharacter(c, PARAGRAPH_BREAK, False)
    t.insertString(c, op["text"], False)


OPS = {
    "insert": op_insert,
    "delete": op_delete,
    "bold": op_bold,
    "split": op_split,
    "join": op_join,
    "delete_para": op_delete_para,
    "dup_para": op_dup_para,
    "move_para": op_move_para,
    "heading": op_heading,
    "row_insert": op_row_insert,
    "col_delete": op_col_delete,
    "list_add": op_list_add,
}


def run_uno(soffice, profile, jobs):
    log = {}
    with lo_pipe.office(soffice, profile) as (ctx, desktop):
        for j in jobs:
            name = os.path.basename(j["out"])
            entry = {"ok": False, "ops": []}
            try:
                doc = lo_pipe.load(desktop, j["in"])
                if doc is None:
                    raise RuntimeError("LibreOffice did not open the file")
                if j["scenario"].get("track"):
                    doc.setPropertyValue("RecordChanges", True)
                for op in j["scenario"]["ops"]:
                    OPS[op["op"]](ctx, doc, op)
                    entry["ops"].append(op["op"])
                if j["save"] == "docx":
                    docx = j["out"][:-4] + ".docx"
                    lo_pipe.store(doc, docx, "MS Word 2007 XML")
                    doc.close(True)
                    doc = lo_pipe.load(desktop, docx)
                lo_pipe.store(doc, j["out"])
                doc.close(True)
                entry["ok"] = True
            except Exception:  # noqa: BLE001
                entry["error"] = traceback.format_exc(limit=3)
            log[name] = entry
    return log


def run_convert(soffice, profile, jobs):
    log = {}
    for j in jobs:
        outdir = tempfile.mkdtemp(prefix="skriv-convert-")
        subprocess.run(
            [soffice, f"-env:UserInstallation=file://{profile}", "--headless", "--norestore",
             "--convert-to", "odt", "--outdir", outdir, j["in"]],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=300, check=False,
        )
        produced = os.path.join(outdir, os.path.basename(j["in"]))
        ok = os.path.exists(produced)
        if ok:
            shutil.move(produced, j["out"])
        shutil.rmtree(outdir, ignore_errors=True)
        log[os.path.basename(j["out"])] = {"ok": ok}
    return log


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("work")
    ap.add_argument("--soffice", default=os.environ.get("SKRIV_SOFFICE", "soffice"))
    args = ap.parse_args()
    jobs = json.load(open(os.path.join(args.work, "jobs.json")))
    profile = make_profile()
    t0 = time.time()
    version = subprocess.run([args.soffice, "--version"], capture_output=True, text=True).stdout.strip()
    log = {"_version": version}
    log.update(run_convert(args.soffice, profile, [j for j in jobs if j["save"] == "convert"]))
    log.update(run_uno(args.soffice, profile, [j for j in jobs if j["save"] != "convert"]))
    log["_seconds"] = round(time.time() - t0, 1)
    with open(os.path.join(args.work, "lo-log.json"), "w") as f:
        json.dump(log, f, indent=1, ensure_ascii=False)
    failed = [k for k, v in log.items() if isinstance(v, dict) and not v.get("ok")]
    print(f"{len(jobs)} jobs, {len(failed)} failed, {log['_seconds']} s, {version}")
    for k in failed:
        print("  FAILED", k, log[k].get("error", "").strip().splitlines()[-1:] if log[k].get("error") else "")
    shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    main()
