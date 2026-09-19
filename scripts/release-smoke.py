#!/usr/bin/env python3
"""Check release CLI exit behavior and startup on a disposable repository."""
import pathlib
import subprocess
import sys
import tempfile
import time

binary = pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='git-view-release-') as tmp:
    cwd = pathlib.Path(tmp)
    for flag, expected in [('--help', 'Usage:'), ('--version', 'Git View 0.1.0')]:
        result = subprocess.run([str(binary), flag], cwd=cwd, capture_output=True, text=True, timeout=10)
        assert result.returncode == 0 and expected in result.stdout, (flag, result)
    result = subprocess.run([str(binary), 'repo', 'missing'], cwd=cwd, capture_output=True, text=True, timeout=10)
    assert result.returncode != 0, 'Invalid explicit path must fail before GUI startup'
    subprocess.run(['git', 'init', '-b', 'main', str(cwd / 'repo')], check=True, capture_output=True)
    process = subprocess.Popen([str(binary), 'repo', 'repo'], cwd=cwd)
    try:
        time.sleep(3)
        assert process.poll() is None, f'Release application exited during startup: {process.returncode}'
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=15)
print('Release CLI help/version/invalid path passed; native process remained running for 3 seconds.')
