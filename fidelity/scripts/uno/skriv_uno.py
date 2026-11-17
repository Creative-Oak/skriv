# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# A Python macro that runs *inside* soffice (installed into a throwaway user
# profile by lo_preservation.py). LibreOffice's standalone Python can't be
# started on every machine, but macros always run, and they see UNO directly.
#
# Reads a job list from the JSON file named by $SKRIV_JOB:
#   [{"in": "/abs/a.odt", "out": "/abs/b.odt", "edit": true}, ...]
# Loads each file hidden, optionally types a word at the start of the first
# paragraph (a user editing in LibreOffice), and stores it as ODT.

import json
import os
import traceback

import uno
from com.sun.star.beans import PropertyValue


def _prop(name, value):
    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


def run(*_args):
    ctx = uno.getComponentContext()
    desktop = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
    jobs = json.load(open(os.environ["SKRIV_JOB"]))
    log = []
    for job in jobs:
        try:
            doc = desktop.loadComponentFromURL(
                uno.systemPathToFileUrl(job["in"]), "_blank", 0, (_prop("Hidden", True),)
            )
            if job.get("edit"):
                text = doc.getText()
                cursor = text.createTextCursorByRange(text.getStart())
                text.insertString(cursor, "Edited in LibreOffice. ", False)
            doc.storeToURL(
                uno.systemPathToFileUrl(job["out"]),
                (_prop("FilterName", "writer8"), _prop("Overwrite", True)),
            )
            doc.close(True)
            log.append({"in": job["in"], "ok": True})
        except Exception:
            log.append({"in": job["in"], "ok": False, "error": traceback.format_exc()})
    with open(os.environ["SKRIV_JOB"] + ".log", "w") as f:
        json.dump(log, f, indent=1)
    desktop.terminate()
