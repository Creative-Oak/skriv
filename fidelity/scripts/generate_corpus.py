#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
"""Generate fidelity/corpus/generated/: one LibreOffice-authored ODT per feature.

The documents are built through the UNO API by a macro running inside
soffice (uno/skriv_generate.py), so they are genuine LibreOffice output rather
than hand-written XML. Content is our own; licence CC0-1.0.

Usage: python3 fidelity/scripts/generate_corpus.py [--soffice PATH]
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "corpus", "generated")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--soffice", default=os.environ.get("SKRIV_SOFFICE", "soffice"))
    args = ap.parse_args()
    out = os.path.abspath(OUT)
    os.makedirs(out, exist_ok=True)
    for f in os.listdir(out):
        if f.endswith(".odt"):
            os.remove(os.path.join(out, f))
    profile = tempfile.mkdtemp(prefix="skriv-gen-profile-")
    try:
        subprocess.run([args.soffice, f"-env:UserInstallation=file://{profile}", "--headless", "--terminate_after_init"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=300, check=False)
        scripts = os.path.join(profile, "user", "Scripts", "python")
        os.makedirs(scripts, exist_ok=True)
        shutil.copy(os.path.join(HERE, "uno", "skriv_generate.py"), scripts)
        subprocess.run(
            [args.soffice, f"-env:UserInstallation=file://{profile}", "--headless", "--invisible", "--norestore",
             "vnd.sun.star.script:skriv_generate.py$run?language=Python&location=user"],
            env=dict(os.environ, SKRIV_OUT=out), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            timeout=600, check=False,
        )
    finally:
        shutil.rmtree(profile, ignore_errors=True)
    log_path = os.path.join(out, "generate.log.json")
    if not os.path.exists(log_path):
        print("the generator macro did not run", file=sys.stderr)
        return 1
    log = json.load(open(log_path))
    os.remove(log_path)
    failed = [e for e in log if not e["ok"]]
    for e in log:
        print(("ok    " if e["ok"] else "FAIL  ") + e["generator"])
    for e in failed:
        print(e["error"], file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
