#!/usr/bin/env python3
"""Run a UTF-8 PowerShell script on the configured Windows build host."""
import base64
import pathlib
import subprocess
import sys

script = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
encoded = base64.b64encode(script.encode("utf-16le")).decode("ascii")
raise SystemExit(subprocess.call([
    "ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
    "-o", "StrictHostKeyChecking=yes", "git-view-windows",
    "powershell.exe -NoProfile -EncodedCommand " + encoded,
]))
