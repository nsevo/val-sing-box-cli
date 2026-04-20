#!/usr/bin/env pwsh
#
# val-sing-box-cli installer for Windows (Administrator-only)
#
# valsb is a root-only tool that registers a Windows service and writes its
# state under %ProgramData%. This installer therefore *requires*
# Administrator and will trigger a UAC prompt to re-launch itself when
# started from a normal PowerShell window.
#
# Usage:
#   irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
#
# Environment variables:
#   $v / $env:VALSB_VERSION   Override valsb version to install (default: latest)
#

$ErrorActionPreference = 'Stop'

$AppName = 'val-sing-box-cli'
$BinName = 'valsb'
$ValsbRepo = 'nsevo/val-sing-box-cli'
$InstallerUrl = "https://raw.githubusercontent.com/$ValsbRepo/main/scripts/install.ps1"

function Write-Info  { param([string]$Msg) Write-Host "  [info]  $Msg" -ForegroundColor Cyan }
function Write-Ok    { param([string]$Msg) Write-Host "  [ok]    $Msg" -ForegroundColor Green }
function Write-Warn  { param([string]$Msg) Write-Host "  [warn]  $Msg" -ForegroundColor Yellow }
function Write-Fatal { param([string]$Msg) throw $Msg }

function Test-IsAdministrator {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = [Security.Principal.WindowsPrincipal]::new($identity)
        return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch {
        return $false
    }
}

# Re-launch the installer with Administrator privileges via UAC.
# Used when the user starts the script from an unelevated session.
function Invoke-ElevatedInstaller {
    param([string]$ResolvedVersion)

    $hostPath = (Get-Process -Id $PID).Path
    if ([string]::IsNullOrWhiteSpace($hostPath)) {
        $hostPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    }

    $escapedUrl = $InstallerUrl -replace "'", "''"
    $command = "`$ErrorActionPreference = 'Stop'`r`n`$ProgressPreference = 'SilentlyContinue'"
    if (-not [string]::IsNullOrWhiteSpace($ResolvedVersion)) {
        $escapedVersion = $ResolvedVersion -replace "'", "''"
        $command += "`r`n`$env:VALSB_VERSION = '$escapedVersion'"
    }
    $command += "`r`n`$script = Invoke-RestMethod -Uri '$escapedUrl' -UseBasicParsing -Headers @{ 'User-Agent' = 'valsb-installer' }"
    $command += "`r`n& ([ScriptBlock]::Create(`$script))"
    $encodedCommand = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $encodedCommand)

    Write-Info 'valsb installer requires Administrator; requesting UAC prompt...'
    try {
        $proc = Start-Process -FilePath $hostPath -Verb RunAs -ArgumentList $arguments -Wait -PassThru
    } catch {
        Write-Fatal "Administrator elevation failed: $_"
    }

    if ($proc.ExitCode -ne 0) {
        Write-Fatal "Elevated installer exited with code $($proc.ExitCode)"
    }

    Write-Ok 'Elevated install completed'
}

function Get-InstallerArchitecture {
    $rawArch = $env:PROCESSOR_ARCHITEW6432
    if ([string]::IsNullOrWhiteSpace($rawArch)) {
        $rawArch = $env:PROCESSOR_ARCHITECTURE
    }

    if ([string]::IsNullOrWhiteSpace($rawArch)) {
        try {
            $rawArch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        } catch {
        }
    }

    if ([string]::IsNullOrWhiteSpace($rawArch)) {
        Write-Fatal 'Failed to detect Windows architecture'
    }

    switch ($rawArch.ToUpperInvariant()) {
        'AMD64' { return 'amd64' }
        'X64'   { return 'amd64' }
        default { Write-Fatal "Unsupported architecture: $rawArch (only amd64 is supported)" }
    }
}

try {

$Arch = Get-InstallerArchitecture

$Version = if ($v) { $v }
           elseif ($env:VALSB_VERSION) { $env:VALSB_VERSION }
           else { $null }

if (-not $Version) {
    Write-Info 'Resolving latest valsb version...'
    try {
        $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$ValsbRepo/releases/latest" -Headers @{ 'User-Agent' = 'valsb-installer' }
        $Version = $Release.tag_name -replace '^v', ''
    } catch {
        Write-Fatal "Failed to fetch latest release: $_"
    }
}

Write-Info "Version: valsb $Version ($Arch)"

if (-not (Test-IsAdministrator)) {
    Invoke-ElevatedInstaller -ResolvedVersion $Version
    return
}

# ── Paths (must match src/platform/paths.rs windows()) ────────────────

$ProgramFilesRoot = if ($env:ProgramFiles) { $env:ProgramFiles } else { 'C:\Program Files' }
$ProgramDataRoot  = if ($env:ProgramData)  { $env:ProgramData }  else { 'C:\ProgramData' }

$BinDir   = Join-Path $ProgramFilesRoot $AppName
$DataDir  = Join-Path $ProgramDataRoot  $AppName
$ValsbExe = Join-Path $BinDir "$BinName.exe"

foreach ($dir in @($BinDir, $DataDir)) {
    if (-not (Test-Path $dir)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
}

# ── Download ──────────────────────────────────────────────────────────

$ArchiveName = "$BinName-v$Version-windows-$Arch.zip"
$DownloadUrl = "https://github.com/$ValsbRepo/releases/download/v$Version/$ArchiveName"
$TempZip = Join-Path $env:TEMP $ArchiveName

Write-Info "Downloading $ArchiveName..."
try {
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $TempZip -UseBasicParsing
} catch {
    Write-Fatal "Download failed: $_"
}

# ── Extract ───────────────────────────────────────────────────────────

$TempExtract = Join-Path $env:TEMP "$BinName-extract-$([System.IO.Path]::GetRandomFileName())"
try {
    Expand-Archive -Path $TempZip -DestinationPath $TempExtract -Force

    $ExePath = Get-ChildItem -Path $TempExtract -Filter "$BinName.exe" -Recurse | Select-Object -First 1
    if (-not $ExePath) {
        Write-Fatal "$BinName.exe not found in archive"
    }

    Copy-Item -Path $ExePath.FullName -Destination $ValsbExe -Force
} finally {
    Remove-Item -Path $TempZip -Force -ErrorAction SilentlyContinue
    Remove-Item -Path $TempExtract -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Ok "Installed valsb to $ValsbExe"

Write-Info 'Verifying valsb binary...'
try {
    & $ValsbExe version | Out-Null
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
        Write-Fatal "Installed valsb failed to start (exit code $LASTEXITCODE). This usually means a required Windows runtime dependency is missing."
    }
} catch {
    Write-Fatal "Installed valsb failed to start: $_"
}

# ── Add to system PATH ────────────────────────────────────────────────

$MachinePath = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::Machine)
if ([string]::IsNullOrWhiteSpace($MachinePath)) {
    $NewMachinePath = $BinDir
} elseif (($MachinePath -split ';' | Where-Object { $_ }) -contains $BinDir) {
    $NewMachinePath = $null
} else {
    $NewMachinePath = "$MachinePath;$BinDir"
}

if ($NewMachinePath) {
    [System.Environment]::SetEnvironmentVariable('Path', $NewMachinePath, [System.EnvironmentVariableTarget]::Machine)
    $env:Path += ";$BinDir"
    Write-Ok 'Added to system PATH (restart your terminal for it to take effect)'
} else {
    Write-Ok 'PATH already configured'
}

# ── Run valsb install (download sing-box + register service) ──────────

Write-Info 'Installing sing-box kernel and registering service...'
try {
    & $ValsbExe install
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
        Write-Fatal "valsb install exited with code $LASTEXITCODE"
    }
} catch {
    Write-Fatal "valsb install failed: $_"
}

Write-Host ''
Write-Host '  Installation complete!' -ForegroundColor Green
Write-Host ''
Write-Host '  valsb is a root-managed CLI: future commands will request UAC automatically when needed.' -ForegroundColor DarkGray
Write-Host ''
Write-Host '  Next steps:'
Write-Host '    1. Add a subscription:  ' -NoNewline; Write-Host 'valsb sub add "<url>"' -ForegroundColor White
Write-Host '    2. Start the service:   ' -NoNewline; Write-Host 'valsb start' -ForegroundColor White
Write-Host ''
} catch {
    $message = $_.Exception.Message
    if (-not $message) {
        $message = "$_"
    }
    $line = $_.InvocationInfo.ScriptLineNumber
    $position = $_.InvocationInfo.PositionMessage

    Write-Host ''
    Write-Host "  [error] $message" -ForegroundColor Red
    if ($line) {
        Write-Host "  [line]  $line" -ForegroundColor DarkYellow
    }
    if ($position) {
        Write-Host $position -ForegroundColor DarkYellow
    }
    Write-Host '  The PowerShell session was kept open so you can read the error.' -ForegroundColor Yellow
    return
}
