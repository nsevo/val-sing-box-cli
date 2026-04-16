#!/usr/bin/env pwsh
#
# val-sing-box-cli installer for Windows
#
# Usage:
#   irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
#
# Environment variables:
#   $v               Override valsb version to install (default: latest)
#   $env:VALSB_VERSION  Alternative version override
#

$ErrorActionPreference = 'Stop'

$AppName = 'val-sing-box-cli'
$BinName = 'valsb'
$ValsbRepo = 'nsevo/val-sing-box-cli'
$InstallerUrl = "https://raw.githubusercontent.com/$ValsbRepo/main/scripts/install.ps1"

# ── Logging ───────────────────────────────────────────────────────────

function Write-Info  { param([string]$Msg) Write-Host "  [info]  $Msg" -ForegroundColor Cyan }
function Write-Ok    { param([string]$Msg) Write-Host "  [ok]    $Msg" -ForegroundColor Green }
function Write-Warn  { param([string]$Msg) Write-Host "  [warn]  $Msg" -ForegroundColor Yellow }
function Write-Fatal { param([string]$Msg) throw $Msg }
function Get-DelegatePrincipal {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    return @{
        Account = $identity.Name
        Sid = $identity.User.Value
    }
}
function Confirm-ElevationRequest {
    param([string[]]$Reasons)

    Write-Host ''
    Write-Host '  Administrator privileges are required for this step:' -ForegroundColor Yellow
    foreach ($reason in $Reasons) {
        Write-Host "    - $reason" -ForegroundColor Yellow
    }
    Write-Host ''

    $response = Read-Host '  Continue and request Administrator privileges? [Y/n]'
    if ([string]::IsNullOrWhiteSpace($response)) {
        return $true
    }

    switch ($response.Trim().ToLowerInvariant()) {
        'y' { return $true }
        'yes' { return $true }
        'n' { return $false }
        'no' { return $false }
        default { return $true }
    }
}
function Test-IsAdministrator {
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = [Security.Principal.WindowsPrincipal]::new($identity)
        return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } catch {
        return $false
    }
}
function Invoke-ElevatedInstaller {
    param(
        [string]$ResolvedVersion,
        [hashtable]$DelegatePrincipal
    )

    $hostPath = (Get-Process -Id $PID).Path
    if ([string]::IsNullOrWhiteSpace($hostPath)) {
        $hostPath = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    }

    $escapedUrl = $InstallerUrl -replace "'", "''"
    $command = @"
`$ErrorActionPreference = 'Stop'
`$ProgressPreference = 'SilentlyContinue'
"@

    if (-not [string]::IsNullOrWhiteSpace($ResolvedVersion)) {
        $escapedVersion = $ResolvedVersion -replace "'", "''"
        $command += "`r`n`$env:VALSB_VERSION = '$escapedVersion'"
    }
    if ($DelegatePrincipal) {
        $escapedAccount = $DelegatePrincipal.Account -replace "'", "''"
        $escapedSid = $DelegatePrincipal.Sid -replace "'", "''"
        $command += "`r`n`$env:VALSB_DELEGATE_ACCOUNT = '$escapedAccount'"
        $command += "`r`n`$env:VALSB_DELEGATE_SID = '$escapedSid'"
    }
    $command += "`r`n`$script = Invoke-RestMethod -Uri '$escapedUrl' -UseBasicParsing -Headers @{ 'User-Agent' = 'valsb-installer' }"
    $command += "`r`n& ([ScriptBlock]::Create(`$script))"
    $encodedCommand = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $arguments = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-EncodedCommand', $encodedCommand)

    Write-Info 'Requesting Administrator privileges...'
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
        'AMD64' { return @{ Raw = $rawArch; Normalized = 'amd64' } }
        'X64'   { return @{ Raw = $rawArch; Normalized = 'amd64' } }
        default { Write-Fatal "Unsupported architecture: $rawArch (only amd64 is supported)" }
    }
}

try {

# ── Architecture detection ────────────────────────────────────────────

$ArchInfo = Get-InstallerArchitecture
$RawArch = $ArchInfo.Raw
$Arch = $ArchInfo.Normalized
$DelegatePrincipal = Get-DelegatePrincipal

# ── Version resolution ────────────────────────────────────────────────

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
    if (-not (Confirm-ElevationRequest -Reasons @(
                'Windows service registration needs Administrator access to the Service Control Manager.',
                'sing-box may need elevated privileges later for TUN/network interface setup.'
            ))) {
        Write-Warn 'Administrator approval declined. Re-run the installer when you are ready to continue.'
        return
    }
    Invoke-ElevatedInstaller -ResolvedVersion $Version -DelegatePrincipal $DelegatePrincipal
    return
}

# ── Paths (must match src/platform/paths.rs user_dirs for Windows) ────

$DataDir = Join-Path $env:APPDATA $AppName
$BinDir  = Join-Path $DataDir 'bin'
$ValsbExe = Join-Path $BinDir "$BinName.exe"

if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
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

# ── Add to PATH ───────────────────────────────────────────────────────

$UserPath = [System.Environment]::GetEnvironmentVariable('Path', [System.EnvironmentVariableTarget]::User)
if ([string]::IsNullOrWhiteSpace($UserPath)) {
    $NewUserPath = $BinDir
} elseif (($UserPath -split ';' | Where-Object { $_ }) -contains $BinDir) {
    $NewUserPath = $null
} else {
    $NewUserPath = "$UserPath;$BinDir"
}

if ($NewUserPath) {
    [System.Environment]::SetEnvironmentVariable('Path', $NewUserPath, [System.EnvironmentVariableTarget]::User)
    $env:Path += ";$BinDir"
    Write-Ok 'Added to PATH (restart your terminal for it to take effect)'
} else {
    Write-Ok 'PATH already configured'
}

# ── Run valsb install (download sing-box + register service) ──────────

Write-Info 'Installing sing-box kernel...'
try {
    $env:VALSB_DELEGATE_ACCOUNT = $DelegatePrincipal.Account
    $env:VALSB_DELEGATE_SID = $DelegatePrincipal.Sid
    & $ValsbExe install
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
        Write-Fatal "valsb install exited with code $LASTEXITCODE"
    }
} catch {
    Write-Fatal "valsb install failed: $_"
}

# ── Done ──────────────────────────────────────────────────────────────

Write-Host ''
Write-Host '  Installation complete!' -ForegroundColor Green
Write-Host ''
Write-Host '  Next steps:'
Write-Host "    1. Add a subscription:  " -NoNewline; Write-Host 'valsb sub add "<url>"' -ForegroundColor White
Write-Host "    2. Start the service:   " -NoNewline; Write-Host 'valsb start' -ForegroundColor White
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
