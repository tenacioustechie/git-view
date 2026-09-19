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
    for args in [('repo', 'missing'), ('group', '--scan', 'missing'), ('group', '--scan', '.', '--depth', '11')]:
        result = subprocess.run([str(binary), *args], cwd=cwd, capture_output=True, text=True, timeout=10)
        assert result.returncode != 0, f'Invalid explicit arguments must fail before GUI startup: {args}'
    subprocess.run(['git', 'init', '-b', 'main', str(cwd / 'repo')], check=True, capture_output=True)
    for args in [('repo', 'repo'), ('group', '--scan', '.', '--depth', '2')]:
        process = subprocess.Popen([str(binary), *args], cwd=cwd)
        try:
            time.sleep(3)
            assert process.poll() is None, f'Release application exited during startup: {process.returncode}'
            assert not list(cwd.glob('*.json')), 'Discovery must not create a project before review'
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
print('Release CLI help/version/invalid arguments passed; repository and unsaved discovery startup each remained running for 3 seconds.')
