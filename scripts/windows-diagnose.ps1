$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
[System.Environment]::OSVersion.Version
$vswhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$dumpbin = & $vswhere -latest -products '*' -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe' | Select-Object -First 1
$test = Get-ChildItem 'src-tauri/target/debug/deps/acceptance-*.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Write-Output $test.FullName
& $dumpbin /dependents $test.FullName
& $dumpbin /imports $test.FullName | Out-File -Encoding utf8 '.tools/acceptance-imports.txt'
Get-Item 'C:/Windows/System32/vcruntime140.dll','C:/Windows/System32/vcruntime140_1.dll','C:/Windows/System32/msvcp140.dll' -ErrorAction SilentlyContinue | Select-Object Name,@{Name='Version';Expression={$_.VersionInfo.FileVersion}}
Get-Content '.tools/acceptance-imports.txt'
