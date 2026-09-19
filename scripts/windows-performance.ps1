$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath 'C:\codeme\git-view'
git rev-parse HEAD
git --version
cargo test --manifest-path src-tauri/Cargo.toml --test git_engine acceptance_performance_measurements -- --ignored --nocapture
if ($LASTEXITCODE -ne 0) { throw 'Performance acceptance failed' }
