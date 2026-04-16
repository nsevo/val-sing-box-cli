---
sidebar_position: 4
title: Configuration
---

# Configuration

valsb does not require manual configuration files. All state is managed automatically through subscription data and internal state files.

## File Locations

Run `valsb config path` to see all paths on your system:

```
  Config     /etc/val-sing-box-cli
  Data       /var/lib/val-sing-box-cli
  Cache      /var/cache/val-sing-box-cli
  Kernel     /usr/local/lib/val-sing-box-cli/bin/sing-box
  Service    /etc/systemd/system/valsb-sing-box.service
  State      /var/lib/val-sing-box-cli/state.json
```

Paths vary by platform and privilege level (root vs user).

### Path Conventions

| Platform | Root | Config | Data |
|----------|------|--------|------|
| Linux | yes | `/etc/val-sing-box-cli` | `/var/lib/val-sing-box-cli` |
| Linux | no | `~/.config/val-sing-box-cli` | `~/.local/share/val-sing-box-cli` |
| macOS | no | `~/Library/Application Support/val-sing-box-cli` | same |
| Windows | no | `%APPDATA%\val-sing-box-cli` | same |

## sing-box Config

The sing-box configuration is sourced entirely from subscriptions. valsb:

1. Fetches the raw JSON config from the subscription URL
2. Injects `experimental.clash_api` (for node switching via API)
3. Injects `experimental.cache_file` (for persistent cache)
4. Writes it to the active config path

You can customize node selection per-group. valsb persists your choice in the state file and applies it by modifying the `default` field in selector outbounds.

## JSON Output

All commands support `--json` for machine-readable output:

```bash
valsb status --json
```

```json
{
  "ok": true,
  "command": "status",
  "data": {
    "state": "running",
    "kernel_version": "1.13.7",
    "profile": "myconfig",
    "node": "HK-1",
    "exit_ip": "1.2.3.4",
    "location": { "country": "HK", "city": "Hong Kong" }
  }
}
```
