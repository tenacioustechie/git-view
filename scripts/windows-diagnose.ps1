param(
    [string]$Repository = (Join-Path $PSScriptRoot '..'),
    [switch]$Build
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repositoryPath = (Resolve-Path -LiteralPath $Repository).Path
Set-Location -LiteralPath $repositoryPath
$diagnosticDirectory = Join-Path $repositoryPath '.tools/windows-loader-diagnostics'
New-Item -ItemType Directory -Force -Path $diagnosticDirectory | Out-Null

$summaryPath = Join-Path $diagnosticDirectory 'summary.txt'
$cargoTestLogPath = Join-Path $diagnosticDirectory 'cargo-test.log'
$importsPath = Join-Path $diagnosticDirectory 'git-view-library-test-imports.txt'
$headersPath = Join-Path $diagnosticDirectory 'git-view-library-test-headers.txt'
$manifestPath = Join-Path $diagnosticDirectory 'git-view-library-test.manifest'

Start-Transcript -Path $summaryPath -Force
try {
    Write-Output '== Host =='
    Get-CimInstance Win32_OperatingSystem |
        Select-Object Caption, Version, BuildNumber, OSArchitecture
    rustc --version --verbose
    cargo --version

    if ($Build) {
        cargo test --manifest-path src-tauri/Cargo.toml --locked --no-run
        if ($LASTEXITCODE -ne 0) {
            throw "cargo test --no-run failed with exit code $LASTEXITCODE"
        }
    }

    $vswhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw "vswhere.exe was not found at $vswhere"
    }

    $dumpbin = & $vswhere -latest -products '*' -find 'VC/Tools/MSVC/**/bin/Hostx64/x64/dumpbin.exe' |
        Select-Object -First 1
    if (-not $dumpbin) {
        throw 'dumpbin.exe was not found in the Visual Studio installation'
    }

    $testCandidates = Get-ChildItem 'src-tauri/target/debug/deps/git_view-*.exe' |
        Where-Object BaseName -Match '^git_view-[0-9a-f]+$' |
        Sort-Object LastWriteTime -Descending
    $failedTestName = $null
    if (Test-Path -LiteralPath $cargoTestLogPath) {
        $failedMatches = [regex]::Matches(
            (Get-Content -LiteralPath $cargoTestLogPath -Raw),
            'deps[\\/](git_view-[0-9a-f]+\.exe)'
        )
        if ($failedMatches.Count -gt 0) {
            $failedTestName = $failedMatches[$failedMatches.Count - 1].Groups[1].Value
        }
    }
    $test = if ($failedTestName) {
        $testCandidates | Where-Object Name -EQ $failedTestName | Select-Object -First 1
    } else {
        $testCandidates | Select-Object -First 1
    }
    if (-not $test) {
        throw "The failed git_view library test executable '$failedTestName' was not found"
    }

    Write-Output '== Test executable =='
    Write-Output "Cargo failure selection: $failedTestName"
    Write-Output 'Available git_view test executables:'
    $testCandidates | Select-Object Name, Length, LastWriteTime
    Write-Output 'Selected executable:'
    $test | Select-Object FullName, Length, LastWriteTime
    Get-AuthenticodeSignature -LiteralPath $test.FullName |
        Select-Object Status, StatusMessage, SignerCertificate

    Write-Output '== PE headers =='
    & $dumpbin /headers $test.FullName | Tee-Object -FilePath $headersPath
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin /headers failed with exit code $LASTEXITCODE"
    }

    Write-Output '== Direct dependencies and resolved DLL versions =='
    $dependentOutput = & $dumpbin /dependents $test.FullName
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin /dependents failed with exit code $LASTEXITCODE"
    }
    $dependentOutput
    $dependencyNames = $dependentOutput |
        Select-String '^\s+([^\s]+\.dll)\s*$' |
        ForEach-Object { $_.Matches[0].Groups[1].Value } |
        Sort-Object -Unique
    $resolvedDependencies = foreach ($dependencyName in $dependencyNames) {
        $candidates = @(
            (Join-Path $test.DirectoryName $dependencyName),
            (Join-Path $env:WINDIR "System32/$dependencyName"),
            (Join-Path $env:WINDIR "SysWOW64/$dependencyName")
        )
        $resolved = $candidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
        if ($resolved) {
            $item = Get-Item -LiteralPath $resolved
            [pscustomobject]@{
                Dependency = $dependencyName
                Path = $item.FullName
                FileVersion = $item.VersionInfo.FileVersion
                ProductVersion = $item.VersionInfo.ProductVersion
            }
        } else {
            [pscustomobject]@{
                Dependency = $dependencyName
                Path = '<not found beside executable or in Windows system directories>'
                FileVersion = $null
                ProductVersion = $null
            }
        }
    }
    $resolvedDependencies | Format-Table -AutoSize

    Write-Output '== Imports =='
    & $dumpbin /imports $test.FullName | Out-File -Encoding utf8 -FilePath $importsPath
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin /imports failed with exit code $LASTEXITCODE"
    }
    Write-Output "Full import table written to $importsPath"

    $mt = Get-ChildItem 'C:/Program Files (x86)/Windows Kits/10/bin/*/x64/mt.exe' -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
    if ($mt) {
        Write-Output '== Embedded manifest =='
        & $mt "-inputresource:$($test.FullName);#1" "-out:$manifestPath"
        if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $manifestPath)) {
            Get-Content -LiteralPath $manifestPath
        } else {
            Write-Output "mt.exe did not extract a manifest (exit code $LASTEXITCODE)"
        }
    } else {
        Write-Output 'mt.exe was not found; embedded manifest was not extracted'
    }

    Write-Output '== Direct loader probe =='
    $previousNativePreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    & $test.FullName --list
    $loaderExitCode = $LASTEXITCODE
    $ErrorActionPreference = $previousNativePreference
    $unsignedLoaderExitCode = [BitConverter]::ToUInt32(
        [BitConverter]::GetBytes([int32]$loaderExitCode),
        0
    )
    Write-Output ("Direct loader probe exit code: 0x{0:X8} ({1})" -f $unsignedLoaderExitCode, $loaderExitCode)
} finally {
    Stop-Transcript
}
