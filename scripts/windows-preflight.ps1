$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath 'C:\codeme\git-view'
Get-ChildItem (Join-Path $env:APPDATA 'fnm/node-versions') -Recurse -Filter node.exe | Select-Object -ExpandProperty FullName
$vswhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
if (Test-Path $vswhere) {
    & $vswhere -all -products '*' -format json
    & $vswhere -all -products '*' -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/link.exe'
}
Get-Command winget.exe,fnm.exe -ErrorAction SilentlyContinue | Select-Object Name,Source
