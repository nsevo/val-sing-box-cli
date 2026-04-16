---
sidebar_position: 1
slug: /
title: Getting Started
---

# Getting Started

**valsb** is a lightweight CLI tool that manages [sing-box](https://sing-box.sagernet.org/) proxy across Linux, macOS, and Windows. It handles installation, subscription management, node switching, service lifecycle, and updates — all from a single binary.

## Quick Start

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

### After Installation

```bash
# Add a subscription
valsb sub add <your-subscription-url>

# Start the proxy
valsb start

# Check status
valsb status
```

## How It Works

valsb acts as a control plane around sing-box:

1. **Subscription** — fetches sing-box JSON configs from subscription URLs
2. **Service** — registers and manages sing-box as a system service (systemd, launchd, procd, Windows SCM)
3. **Node switching** — switches proxy nodes via Clash API (hot-reload) or config rewrite
4. **Updates** — checks and applies updates for both valsb and sing-box atomically

## Requirements

- **Linux**: systemd or procd (OpenWrt)
- **macOS**: launchd
- **Windows**: Windows Service Control Manager (requires Administrator for TUN mode)
- **sing-box**: automatically installed by `valsb install`
