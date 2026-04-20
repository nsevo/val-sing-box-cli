---
sidebar_position: 2
title: Installation
---

# Installation

`valsb` is a root-only tool. It registers a system service, manages files
under `/etc`, `/var/lib`, `/var/cache` (or `%ProgramData%` on Windows), and
needs root for TUN mode. The installer therefore requires root and the
`valsb` binary will request `sudo` (or trigger a UAC prompt on Windows)
automatically when invoked by a regular user.

## One-liner Install

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | sudo bash
```

This script:
1. Detects your OS and architecture
2. Re-executes itself with `sudo` if not already root
3. Downloads the latest `valsb` and `sing-box` binaries
4. Places binaries under `/usr/local/bin` and `/usr/local/lib/val-sing-box-cli/bin`
5. Registers the sing-box service via the platform-native backend

### Windows (PowerShell, Administrator)

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

This script:
1. Triggers a UAC prompt to elevate to Administrator if needed
2. Installs `valsb.exe` to `%ProgramFiles%\val-sing-box-cli\`
3. Adds the bin directory to the system PATH
4. Runs `valsb install` to download sing-box and register the Windows service

## Manual Install

Download the latest release from [GitHub Releases](https://github.com/nsevo/val-sing-box-cli/releases).

```bash
# Extract the archive
tar xzf valsb-v*.tar.gz

# Move to a directory in your PATH (root-owned)
sudo mv valsb /usr/local/bin/

# Install sing-box kernel and register the service
sudo valsb install
```

## Verify Installation

```bash
valsb version
```

Expected output:

```
  valsb       0.1.0
  sing-box    1.13.8
  platform    linux/amd64
```

## Uninstall

```bash
valsb uninstall --yes
```

This removes all binaries, service files, configs, cache, and data directories — a complete trace-free removal.
