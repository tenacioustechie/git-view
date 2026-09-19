$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-Location -LiteralPath 'C:\codeme\git-view'
query user
$taskName = 'GitViewNativeSmoke-' + [guid]::NewGuid().ToString('N')
$report = Join-Path 'C:\codeme\git-view\.tools' ($taskName + '.json')
$script = Join-Path 'C:\codeme\git-view\.tools' ($taskName + '.ps1')
$log = Join-Path 'C:\codeme\git-view\.tools' ($taskName + '.log')
$content = @"
`$env:GIT_VIEW_NATIVE_SMOKE_REPORT = '$report'
Set-Location -LiteralPath `$env:TEMP
& 'C:\codeme\git-view\src-tauri\target\debug\git-view.exe' *> '$log'
exit `$LASTEXITCODE
"@
Set-Content -Path $script -Value $content -Encoding utf8
$identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoProfile -File "' + $script + '"')
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit (New-TimeSpan -Minutes 5)
try {
    Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
    Start-ScheduledTask -TaskName $taskName
    $deadline = (Get-Date).AddSeconds(260)
    do {
        Start-Sleep -Seconds 2
        if (Test-Path $report) {
            $result = Get-Content $report -Raw | ConvertFrom-Json
            if ($result.PSObject.Properties.Name -contains 'passed') {
                Get-Content $report
                Write-Output "Native desktop report: $report"
                if (-not $result.passed) { throw 'Native desktop smoke failed' }
                exit 0
            }
        }
    } while ((Get-Date) -lt $deadline)
    Get-ScheduledTaskInfo -TaskName $taskName | Format-List
    if (Test-Path $log) { Get-Content $log }
    $progressReport = [System.IO.Path]::ChangeExtension($report, 'progress.json')
    if (Test-Path $progressReport) { Get-Content $progressReport }
    throw 'Native desktop smoke timed out'
} finally {
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
