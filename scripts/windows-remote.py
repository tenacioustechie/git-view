#!/usr/bin/env python3
"""Run a UTF-8 PowerShell script on the configured Windows build host."""
import base64
import pathlib
import subprocess
import sys

script = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
# Keep the SSH command below Windows' command-line limit. Send the script through
# stdin; only the fixed UTF-8 bootstrap travels as an encoded command argument.
bootstrap = "[Console]::InputEncoding = [System.Text.UTF8Encoding]::new(); $gitViewScript = [Console]::In.ReadToEnd(); & ([ScriptBlock]::Create($gitViewScript))"
encoded = base64.b64encode(bootstrap.encode("utf-16le")).decode("ascii")
raise SystemExit(subprocess.run([
    "ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10",
    "-o", "StrictHostKeyChecking=yes", "git-view-windows",
    "powershell.exe -NoProfile -NonInteractive -EncodedCommand " + encoded,
], input=script.encode("utf-8")).returncode)
