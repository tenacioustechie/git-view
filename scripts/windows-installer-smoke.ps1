$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
$installer = Get-ChildItem 'src-tauri/target/release/bundle/nsis/*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { throw 'Build the release installer first' }
$destination = Join-Path 'C:\codeme\git-view\.tools' ('installed-smoke-' + [guid]::NewGuid().ToString('N'))
$install = Start-Process -FilePath $installer.FullName -ArgumentList @('/S', ('/D=' + $destination)) -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "Installer failed: $($install.ExitCode)" }
$binary = Join-Path $destination 'git-view.exe'
if (-not (Test-Path $binary)) { throw 'Installer did not produce the application executable' }
& $binary --help
if ($LASTEXITCODE -ne 0) { throw 'Installed CLI help failed' }
& $binary --version
if ($LASTEXITCODE -ne 0) { throw 'Installed CLI version failed' }
$taskName = 'GitViewInstalledSmoke-' + [guid]::NewGuid().ToString('N')
$report = Join-Path 'C:\codeme\git-view\.tools' ($taskName + '.json')
$script = Join-Path 'C:\codeme\git-view\.tools' ($taskName + '.ps1')
$content = @"
`$ErrorActionPreference = 'Stop'
`$fixture = Join-Path `$env:TEMP '$taskName'
New-Item -ItemType Directory -Path `$fixture | Out-Null
git init -b main `$fixture | Out-Null
`$process = Start-Process -FilePath '$binary' -ArgumentList @('repo',`$fixture) -PassThru
try {
  Start-Sleep -Seconds 4
  `$process.Refresh()
  if (`$process.HasExited -or `$process.MainWindowHandle -eq 0) { throw 'Installed application did not expose a native window' }
  @{ passed = `$true; windowTitle = `$process.MainWindowTitle; executable = '$binary' } | ConvertTo-Json | Set-Content '$report'
} catch {
  @{ passed = `$false; error = `$_.Exception.Message } | ConvertTo-Json | Set-Content '$report'
} finally {
  if (-not `$process.HasExited) { Stop-Process -Id `$process.Id }
}
"@
Set-Content -Path $script -Value $content -Encoding utf8
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoProfile -File "' + $script + '"')
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 2)
try {
    Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName $taskName
    $deadline = (Get-Date).AddSeconds(60)
    while (-not (Test-Path $report) -and (Get-Date) -lt $deadline) { Start-Sleep -Seconds 2 }
    if (-not (Test-Path $report)) { throw 'Installed desktop smoke timed out' }
    Get-Content $report
    if (-not (Get-Content $report -Raw | ConvertFrom-Json).passed) { throw 'Installed desktop smoke failed' }
} finally {
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
Write-Output "Installed artifact validated at $destination"
