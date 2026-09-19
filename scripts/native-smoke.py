#!/usr/bin/env python3
"""Launch a test-only native build with disposable fixtures and a bounded lifetime."""
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

root = pathlib.Path(__file__).resolve().parent.parent
report = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else tempfile.gettempdir() + '/git-view-native-smoke.json')
binary = root / 'src-tauri/target/debug' / ('git-view.exe' if os.name == 'nt' else 'git-view')
env = dict(os.environ, GIT_VIEW_NATIVE_SMOKE_REPORT=str(report))
started = time.time()
with tempfile.TemporaryDirectory(prefix='git-view-invoking-cwd-') as cwd:
    process = subprocess.Popen([str(binary)], cwd=cwd, env=env)
    try:
        code = process.wait(timeout=240)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=15)
        raise SystemExit('Native smoke timed out; no success claimed.')
if not report.exists() or report.stat().st_mtime < started:
    raise SystemExit(f'Native smoke exited {code} without a report.')
result = json.loads(report.read_text())
print(json.dumps(result, indent=2))
raise SystemExit(0 if code == 0 and result.get('passed') else 1)
