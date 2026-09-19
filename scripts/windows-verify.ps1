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
$env:GIT_VIEW_NATIVE_SMOKE_REPORT = Join-Path $env:TEMP ('git-view-native-' + (git rev-parse --short HEAD) + '-' + [guid]::NewGuid().ToString('N') + '.json')
$smoke = Start-Process -FilePath 'C:\codeme\git-view\src-tauri\target\debug\git-view.exe' -WorkingDirectory $env:TEMP -PassThru
if (-not $smoke.WaitForExit(240000)) { Stop-Process -Id $smoke.Id; throw 'Native WebView smoke timed out' }
if (-not (Test-Path $env:GIT_VIEW_NATIVE_SMOKE_REPORT)) { throw 'Native smoke produced no report' }
Get-Content $env:GIT_VIEW_NATIVE_SMOKE_REPORT
$report = Get-Content $env:GIT_VIEW_NATIVE_SMOKE_REPORT -Raw | ConvertFrom-Json
if (-not $report.passed) { throw 'Native smoke reported failure' }
Write-Output "Native report: $env:GIT_VIEW_NATIVE_SMOKE_REPORT"
Remove-Item Env:VITE_NATIVE_SMOKE
Remove-Item Env:GIT_VIEW_NATIVE_SMOKE_REPORT
