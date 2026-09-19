$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$nodeDir = Join-Path $env:APPDATA 'fnm/node-versions/v22.22.2/installation'
$env:Path = "C:\codeme\git-view\.tools\node_modules\.bin;$nodeDir;$env:USERPROFILE\.cargo\bin;$env:Path"
git rev-parse HEAD
node --version
pnpm.cmd --version
pnpm.cmd install --frozen-lockfile
if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed' }
pnpm.cmd build
if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
cargo build --manifest-path src-tauri/Cargo.toml --locked
if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
