# Focused rerun after renderer/native-harness changes. Run windows-verify.ps1
# first for backend changes or the full integration gate.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$nodeDir = Join-Path $env:APPDATA 'fnm/node-versions/v22.22.2/installation'
$env:Path = "C:\codeme\git-view\.tools\node_modules\.bin;$nodeDir;$env:USERPROFILE\.cargo\bin;$env:Path"
git rev-parse HEAD
pnpm.cmd lint
if ($LASTEXITCODE -ne 0) { throw 'Lint failed' }
pnpm.cmd test
if ($LASTEXITCODE -ne 0) { throw 'UI tests failed' }
$env:VITE_NATIVE_SMOKE = '1'
pnpm.cmd build
if ($LASTEXITCODE -ne 0) { throw 'Smoke frontend build/typecheck failed' }
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --features native-smoke -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Native/test Clippy failed' }
cargo build --manifest-path src-tauri/Cargo.toml --locked --features native-smoke,tauri/custom-protocol
if ($LASTEXITCODE -ne 0) { throw 'Native smoke build failed' }
& 'C:\codeme\git-view\scripts\windows-desktop-smoke.ps1'
if ($LASTEXITCODE -ne 0) { throw 'Native desktop smoke failed' }
Remove-Item Env:VITE_NATIVE_SMOKE
