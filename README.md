# val-sing-box-cli

A CLI tool to manage [sing-box](https://github.com/SagerNet/sing-box) on Linux, macOS, Windows, and OpenWrt.

sing-box is a proxy kernel. The operations around it are simple: install, load a config, run as a service, switch nodes, check status. Most GUI clients wrap these into an Electron app or a platform-specific shell that weighs more than the kernel itself, pulls in a rendering engine, generates its own config layer, and runs a background process whether you need it or not.

`valsb` skips all of that. It talks to sing-box directly, uses the provider config as-is, and manages the service through the OS-native backend (systemd, launchd, procd, Windows SCM). One static binary, a few hundred KB, no runtime dependency. It works the same over SSH on a headless server, inside a terminal on your desktop, or in a script on a router.

## Install

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

## Quick Start

```bash
valsb sub add "https://your-provider.example/sub?format=sing-box"
valsb start
valsb status
valsb node use
```

## Preview

```text
  State      running
  Kernel     sing-box 1.13.7
  Backend    systemd system
  Profile    valconfig

  Node       HK-HKG-DC23-001
  Exit IP    203.0.113.1
  Location   HK · HKG
```

## Platforms

| OS | Arch | Service Backend |
|---|---|---|
| Linux | amd64, arm64 | systemd (user / system) |
| OpenWrt | amd64, arm64 | procd |
| macOS | amd64, arm64 | launchd |
| Windows | amd64 | Windows Service |

## Documentation

Full command reference, configuration guide, and troubleshooting:

**[https://nsevo.github.io/val-sing-box-cli/](https://nsevo.github.io/val-sing-box-cli/)**

## Build From Source

Requires Rust 1.85+

```bash
git clone https://github.com/nsevo/val-sing-box-cli.git
cd val-sing-box-cli
cargo build --release
```

## License

MIT
