$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$nodeDir = Join-Path $env:APPDATA 'fnm/node-versions/v22.22.2/installation'
$env:Path = "C:\codeme\git-view\.tools\node_modules\.bin;$nodeDir;$env:USERPROFILE\.cargo\bin;$env:Path"
git rev-parse HEAD
pnpm.cmd install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw 'Install failed' }
pnpm.cmd typecheck
if ($LASTEXITCODE -ne 0) { throw 'Typecheck failed' }
pnpm.cmd lint
if ($LASTEXITCODE -ne 0) { throw 'Lint failed' }
pnpm.cmd test
if ($LASTEXITCODE -ne 0) { throw 'UI tests failed' }
cargo test --manifest-path src-tauri/Cargo.toml --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
$env:VITE_NATIVE_SMOKE = '1'
pnpm.cmd build
if ($LASTEXITCODE -ne 0) { throw 'Smoke frontend build failed' }
cargo build --manifest-path src-tauri/Cargo.toml --features native-smoke,tauri/custom-protocol
if ($LASTEXITCODE -ne 0) { throw 'Native smoke build failed' }
& 'C:\codeme\git-view\scripts\windows-desktop-smoke.ps1'
if ($LASTEXITCODE -ne 0) { throw 'Native desktop smoke failed' }
Remove-Item Env:VITE_NATIVE_SMOKE
