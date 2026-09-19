$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$nodeDir = Join-Path $env:APPDATA 'fnm/node-versions/v22.22.2/installation'
$env:Path = "C:\codeme\git-view\.tools\node_modules\.bin;$nodeDir;$env:USERPROFILE\.cargo\bin;$env:Path"
Remove-Item Env:VITE_NATIVE_SMOKE -ErrorAction SilentlyContinue
Remove-Item Env:GIT_VIEW_NATIVE_SMOKE_REPORT -ErrorAction SilentlyContinue
git rev-parse HEAD
git --version
pnpm.cmd tauri build --bundles nsis
if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
& 'src-tauri/target/release/git-view.exe' --help
if ($LASTEXITCODE -ne 0) { throw 'Release CLI help failed' }
& 'src-tauri/target/release/git-view.exe' --version
if ($LASTEXITCODE -ne 0) { throw 'Release CLI version failed' }
Get-ChildItem 'src-tauri/target/release/bundle/nsis/*.exe' | Select-Object FullName,Length
Get-ChildItem 'src-tauri/target/release/bundle/nsis/*.exe' | Get-FileHash -Algorithm SHA256
