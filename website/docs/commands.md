---
sidebar_position: 3
title: Commands
---

# Commands

## Service Lifecycle

| Command | Description |
|---------|-------------|
| `valsb start` | Start the sing-box service |
| `valsb stop` | Stop the sing-box service |
| `valsb restart` | Restart the sing-box service |
| `valsb status` | Show service status, active node, and exit IP |

### `valsb status`

Displays the current state of the proxy service including kernel version, active profile, selected node, exit IP, and location.

```bash
valsb status
```

## Subscription Management

| Command | Description |
|---------|-------------|
| `valsb sub add <url>` | Add a subscription |
| `valsb sub list` | List all subscriptions |
| `valsb sub update [target]` | Update subscription(s) |
| `valsb sub remove <target>` | Remove a subscription |
| `valsb sub use <target>` | Switch the active profile |

The `target` can be a profile name, ID, or index number.

### `valsb sub add`

```bash
valsb sub add "https://example.com/sub?format=singbox"
```

The subscription URL must return a full sing-box JSON config. Add `&format=singbox` if your provider supports multiple formats.

## Node Management

| Command | Description |
|---------|-------------|
| `valsb node use` | Interactive node browser |
| `valsb node use <name>` | Switch to a specific node directly |

### Interactive Node Browser

Running `valsb node use` without arguments opens an interactive terminal UI:

- **Group selection** — choose which proxy group to browse
- **Fuzzy search** — start typing to filter nodes by name
- **Live latency** — delays refresh every 5 seconds via Clash API
- **Navigation** — `↑↓` to move, `Enter` to select, `Esc` to go back

```
  Proxy

  > HK-1  120ms *
    HK-2  145ms
    JP-1  89ms
    JP-2  102ms
    US-1  210ms

  up/down navigate  enter select  esc back
```

Type `jp` to instantly filter to Japan nodes.

## Configuration

| Command | Description |
|---------|-------------|
| `valsb config init` | Initialize config directories |
| `valsb config path` | Show all file paths |

## System

| Command | Description |
|---------|-------------|
| `valsb install` | Install sing-box kernel and register service |
| `valsb uninstall --yes` | Remove everything (trace-free) |
| `valsb update` | Check and apply updates for valsb and sing-box |
| `valsb version` | Show version information |
| `valsb doctor` | Diagnose environment and configuration |
| `valsb reload` | Reload sing-box configuration |

## Global Flags

| Flag | Description |
|------|-------------|
| `--json` | Output in JSON format (for scripting) |
| `--verbose` | Enable debug logging |
| `--yes` | Skip confirmation prompts |
| `--config-dir <path>` | Override the config directory |
