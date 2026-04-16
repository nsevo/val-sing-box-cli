---
sidebar_position: 2
title: Installation
---

# Installation

## One-liner Install

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
```

This script:
1. Detects your OS and architecture
2. Downloads the latest `valsb` binary
3. Downloads the latest `sing-box` kernel
4. Places binaries in the correct system paths
5. Registers the sing-box service

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

This script:
1. Downloads `valsb.exe` to `%APPDATA%\val-sing-box-cli\bin\`
2. Adds the bin directory to your user PATH
3. Explains why Administrator access is needed, asks for confirmation, then prompts for approval when Windows service registration is needed
4. Runs `valsb install` to download sing-box and register the Windows service

## Manual Install

Download the latest release from [GitHub Releases](https://github.com/nsevo/val-sing-box-cli/releases).

```bash
# Extract the archive
tar xzf valsb-v*.tar.gz

# Move to a directory in your PATH
sudo mv valsb /usr/local/bin/

# Install sing-box kernel and register the service
valsb install
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
