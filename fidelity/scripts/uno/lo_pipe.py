# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Drive LibreOffice over a UNO pipe from the system Python (`python3-uno`).
# This is the fallback for installations without LibreOffice's own Python
# script provider (Ubuntu's `libreoffice-script-provider-python` is optional),
# where the macro route in `skriv_uno.py` silently does nothing.

import contextlib
import os
import subprocess
import time
import uuid

import uno
from com.sun.star.beans import PropertyValue
from com.sun.star.connection import NoConnectException


def prop(name, value):
    p = PropertyValue()
    p.Name = name
    p.Value = value
    return p


@contextlib.contextmanager
def office(soffice, profile):
    """Start a headless soffice on a private pipe and yield its Desktop."""
    pipe = f"skriv-{uuid.uuid4().hex[:12]}"
    proc = subprocess.Popen(
        [
            soffice,
            f"-env:UserInstallation=file://{profile}",
            "--headless",
            "--invisible",
            "--norestore",
            "--nologo",
            f"--accept=pipe,name={pipe};urp;StarOffice.ComponentContext",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    local = uno.getComponentContext()
    resolver = local.ServiceManager.createInstanceWithContext("com.sun.star.bridge.UnoUrlResolver", local)
    ctx = None
    for _ in range(240):
        try:
            ctx = resolver.resolve(f"uno:pipe,name={pipe};urp;StarOffice.ComponentContext")
            break
        except NoConnectException:
            if proc.poll() is not None:
                raise RuntimeError("soffice exited before accepting the pipe")
            time.sleep(0.25)
    if ctx is None:
        proc.kill()
        raise RuntimeError("could not connect to soffice")
    desktop = ctx.ServiceManager.createInstanceWithContext("com.sun.star.frame.Desktop", ctx)
    try:
        yield ctx, desktop
    finally:
        with contextlib.suppress(Exception):
            desktop.terminate()
        try:
            proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            proc.kill()


def load(desktop, path, hidden=True):
    return desktop.loadComponentFromURL(uno.systemPathToFileUrl(os.path.abspath(path)), "_blank", 0, (prop("Hidden", hidden),))


def store(doc, path, filter_name="writer8"):
    doc.storeToURL(uno.systemPathToFileUrl(os.path.abspath(path)), (prop("FilterName", filter_name), prop("Overwrite", True)))
