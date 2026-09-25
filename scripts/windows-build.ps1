[CmdletBinding()]
param(
    [switch]$Check,
    [switch]$Install,
    [switch]$Launch,
    [switch]$SkipDeps,
    [switch]$Help
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Show-Usage {
    Write-Output @'
Usage: .\scripts\windows-build.ps1 [options]

Build the unsigned Windows Git View installer from the current checkout.

Options:
  -Check       Run frontend and Rust checks, then test the packaged app.
  -Install     Silently install for the current user after a successful build.
  -Launch      Start the installed app, or the built app without -Install.
  -SkipDeps    Skip pnpm install --frozen-lockfile for a repeat build.
  -Help        Show this help.

Examples:
  .\scripts\windows-build.ps1
  .\scripts\windows-build.ps1 -SkipDeps -Launch
  .\scripts\windows-build.ps1 -Check -Install -Launch
'@
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)][string]$Description,
        [Parameter(Mandatory = $true)][string]$FilePath,
        [string[]]$Arguments = @()
    )
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Test-ReleaseApplication {
    param([Parameter(Mandatory = $true)][string]$Binary)

    Invoke-External -Description 'Release CLI help' -FilePath $Binary -Arguments @('--help')
    Invoke-External -Description 'Release CLI version' -FilePath $Binary -Arguments @('--version')

    $fixture = Join-Path $env:TEMP ('git-view-release-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $fixture | Out-Null
    try {
        Invoke-External -Description 'Release smoke repository setup' -FilePath 'git' -Arguments @('init', '-b', 'main', $fixture)
        $process = Start-Process -FilePath $Binary -ArgumentList @('repo', ('"' + $fixture + '"')) -PassThru
        try {
            $deadline = (Get-Date).AddSeconds(20)
            $ready = $false
            while ((Get-Date) -lt $deadline) {
                Start-Sleep -Milliseconds 250
                $process.Refresh()
                if ($process.HasExited) {
                    throw "Release application exited during startup with code $($process.ExitCode)."
                }
                if ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'Git View') {
                    $ready = $true
                    break
                }
            }
            if (-not $ready) {
                throw 'Release application did not expose its Git View native window within 20 seconds.'
            }
        } finally {
            if (-not $process.HasExited) {
                Stop-Process -Id $process.Id
                $process.WaitForExit()
            }
        }
    } finally {
        Remove-Item -LiteralPath $fixture -Recurse -Force -ErrorAction SilentlyContinue
    }
    Write-Output 'Release CLI and native window smoke checks passed.'
}

if ($Help) {
    Show-Usage
    exit 0
}

if ($PSScriptRoot) {
    $repositoryRoot = Split-Path -Parent $PSScriptRoot
} elseif (Test-Path -LiteralPath 'C:\codeme\git-view\package.json') {
    # windows-remote.py evaluates this file as a script block, where
    # $PSScriptRoot is empty. Preserve the configured build-host fallback.
    $repositoryRoot = 'C:\codeme\git-view'
} elseif (Test-Path -LiteralPath (Join-Path (Get-Location) 'package.json')) {
    $repositoryRoot = (Get-Location).Path
} else {
    throw 'Cannot locate the Git View repository root.'
}

$extraPaths = @(
    (Join-Path $repositoryRoot '.tools\node_modules\.bin'),
    (Join-Path $env:APPDATA 'fnm\node-versions\v22.22.2\installation'),
    (Join-Path $env:USERPROFILE '.cargo\bin')
) | Where-Object { Test-Path -LiteralPath $_ }
if ($extraPaths.Count -gt 0) {
    $env:Path = (($extraPaths -join ';') + ';' + $env:Path)
}

foreach ($command in @('node', 'pnpm.cmd', 'cargo', 'git')) {
    if (-not (Get-Command $command -ErrorAction SilentlyContinue)) {
        throw "Required command is unavailable: $command"
    }
}

$running = @(Get-Process -Name 'git-view' -ErrorAction SilentlyContinue)
if ($running.Count -gt 0) {
    throw 'Quit Git View before building so Windows can replace and test the executable.'
}

Set-Location -LiteralPath $repositoryRoot
git rev-parse HEAD
node --version
pnpm.cmd --version
cargo --version

if (-not $SkipDeps) {
    Invoke-External -Description 'Dependency installation' -FilePath 'pnpm.cmd' -Arguments @('install', '--frozen-lockfile')
}

if ($Check) {
    Invoke-External -Description 'Typecheck' -FilePath 'pnpm.cmd' -Arguments @('typecheck')
    Invoke-External -Description 'Lint' -FilePath 'pnpm.cmd' -Arguments @('lint')
    Invoke-External -Description 'Frontend tests' -FilePath 'pnpm.cmd' -Arguments @('test')
    Invoke-External -Description 'Rust tests' -FilePath 'cargo' -Arguments @('test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked')
    Invoke-External -Description 'Rust formatting' -FilePath 'cargo' -Arguments @('fmt', '--manifest-path', 'src-tauri/Cargo.toml', '--', '--check')
    Invoke-External -Description 'Clippy' -FilePath 'cargo' -Arguments @('clippy', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', '--all-targets', '--', '-D', 'warnings')
}

Invoke-External -Description 'Windows release build' -FilePath 'pnpm.cmd' -Arguments @('tauri', 'build', '--bundles', 'nsis')

$binary = Join-Path $repositoryRoot 'src-tauri\target\release\git-view.exe'
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw "Build finished without the expected executable: $binary"
}
$installer = Get-ChildItem -LiteralPath (Join-Path $repositoryRoot 'src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $installer) {
    throw 'Build finished without an NSIS setup executable.'
}

if ($Check) {
    Test-ReleaseApplication -Binary $binary
}

$launchBinary = $binary
if ($Install) {
    if (-not $env:LOCALAPPDATA) {
        throw 'LOCALAPPDATA is unavailable; cannot select the per-user installation directory.'
    }
    $installPath = Join-Path $env:LOCALAPPDATA 'Git View'
    $installArguments = '/S /D=' + $installPath
    $installProcess = Start-Process -FilePath $installer.FullName -ArgumentList $installArguments -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) {
        throw "Installer failed with exit code $($installProcess.ExitCode)."
    }
    $launchBinary = Join-Path $installPath 'git-view.exe'
    if (-not (Test-Path -LiteralPath $launchBinary -PathType Leaf)) {
        throw "Installer completed without the expected executable: $launchBinary"
    }
    Write-Output "Installed: $installPath"
}

$hash = Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256
Write-Output "Built executable: $binary"
Write-Output "Built installer: $($installer.FullName)"
Write-Output "Installer SHA-256: $($hash.Hash.ToLowerInvariant())"

if ($Launch) {
    Start-Process -FilePath $launchBinary
} else {
    Write-Output "Launch with: & '$launchBinary'"
}
