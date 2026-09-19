$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$nodeDir = Join-Path $env:APPDATA 'fnm/node-versions/v22.22.2/installation'
if (-not (Test-Path (Join-Path $nodeDir 'node.exe'))) { throw 'Expected Node 22 installation is absent.' }
$env:Path = "C:\codeme\git-view\.tools\node_modules\.bin;$nodeDir;$env:Path"
node --version
& (Join-Path $nodeDir 'npm.cmd') install --prefix 'C:\codeme\git-view\.tools' --no-audit --no-fund pnpm@9.12.0
if ($LASTEXITCODE -ne 0) { throw 'pnpm installation failed' }
pnpm.cmd --version
$vswhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$cpp = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $cpp) {
    $install = & $vswhere -latest -products '*' -property installationPath
    if (-not $install) { throw 'Visual Studio installation not found' }
    $setup = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/setup.exe'
    $installer = Start-Process -FilePath $setup -ArgumentList @('modify', '--installPath', ('"' + $install + '"'), '--add', 'Microsoft.VisualStudio.Workload.NativeDesktop', '--includeRecommended', '--passive', '--norestart') -Wait -PassThru
    Write-Output "Visual Studio installer exit: $($installer.ExitCode)"
    if ($installer.ExitCode -notin @(0,3010)) { throw 'Visual Studio C++ workload installation failed' }
}
$probe = Join-Path $env:TEMP ('git-view-probe-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $probe | Out-Null
Set-Content -Path (Join-Path $probe 'main.rs') -Encoding ascii -Value 'fn main() { println!("Git View native compiler OK"); }'
rustc (Join-Path $probe 'main.rs') -o (Join-Path $probe 'probe.exe')
if ($LASTEXITCODE -ne 0) { throw 'Native compiler probe failed' }
& (Join-Path $probe 'probe.exe')
