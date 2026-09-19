#!/usr/bin/env python3
"""Collect unsigned local artifacts without publishing them."""
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
windows_revision = subprocess.check_output([
    'ssh', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
    'git-view-windows', 'git -C C:/codeme/git-view rev-parse HEAD',
], text=True).strip()
if windows_revision != revision:
    raise SystemExit(f'Source revisions differ: Mac {revision}, Windows {windows_revision}')
out = root / 'artifacts'
out.mkdir(exist_ok=True)
app = root / 'src-tauri/target/release/bundle/macos/Git View.app'
archive = out / 'Git-View_0.1.0_macos-arm64.zip'
subprocess.run(['ditto', '-c', '-k', '--keepParent', str(app), str(archive)], check=True)
installer = out / 'Git-View_0.1.0_windows-x64-setup.exe'
subprocess.run(['scp', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes', 'git-view-windows:C:/codeme/git-view/src-tauri/target/release/bundle/nsis/Git View_0.1.0_x64-setup.exe', str(installer)], check=True)
manifest = {'sourceRevision': revision, 'windowsSourceRevision': windows_revision, 'unsigned': True, 'artifacts': []}
for path in [archive, installer]:
    manifest['artifacts'].append({'file': path.name, 'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
if len(sys.argv) > 1:
    report = out / 'native-windows.json'
    subprocess.run(['scp', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
                    'git-view-windows:' + sys.argv[1], str(report)], check=True)
    manifest['acceptanceReports'] = []
    for name in ['native-macos.json', 'native-windows.json']:
        result = json.loads((out / name).read_text())
        if result.get('passed') is not True:
            raise SystemExit(f'Acceptance report did not pass: {name}')
        manifest['acceptanceReports'].append({'file': name, 'checks': len(result['checks'])})
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print(json.dumps(manifest, indent=2))
