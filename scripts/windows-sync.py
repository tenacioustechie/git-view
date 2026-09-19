#!/usr/bin/env python3
"""Synchronize a committed checkpoint only, preserving dirty Windows checkouts."""
import base64
import pathlib
import subprocess
import tempfile

revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
with tempfile.TemporaryDirectory(prefix="git-view-sync-") as tmp:
    bundle = pathlib.Path(tmp) / "source.bundle"
    subprocess.run(["git", "bundle", "create", str(bundle), "HEAD"], check=True)
    subprocess.run(["scp", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes", str(bundle), "git-view-windows:C:/codeme/git-view-source.bundle"], check=True)
    script = r'''$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath 'C:\codeme\git-view'
$generatedChanges = @(git status --porcelain --untracked-files=all -- src-tauri/gen)
if ($generatedChanges.Count -gt 0) {
    $backup = Join-Path '.tools' ('generated-schema-backup-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $backup | Out-Null
    Copy-Item -Recurse 'src-tauri/gen' $backup
    git restore --worktree -- src-tauri/gen
    if ($LASTEXITCODE -ne 0) { throw 'Could not restore generated tracked schemas after backing them up.' }
}
$changes = @(git status --porcelain --untracked-files=all | Where-Object { $_ -notmatch '^\?\? (\.tools/|src-tauri/gen/)' })
if ($changes.Count -gt 0) { $changes; throw 'Windows checkout has changes; preserve and use a separate worktree.' }
git fetch C:/codeme/git-view-source.bundle HEAD
if ($LASTEXITCODE -ne 0) { throw 'Bundle fetch failed' }
git switch -C git-view-build REVISION
if ($LASTEXITCODE -ne 0) { throw 'Build branch switch failed' }
git rev-parse HEAD
git status --short --branch
'''.replace('REVISION', revision)
    encoded = base64.b64encode(script.encode('utf-16le')).decode('ascii')
    subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "StrictHostKeyChecking=yes", "git-view-windows", "powershell.exe -NoProfile -EncodedCommand " + encoded], check=True)
print("Windows source checkpoint:", revision)
