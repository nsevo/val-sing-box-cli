---
sidebar_position: 7
title: Troubleshooting
---

# Troubleshooting

## Diagnostics

```bash
valsb doctor
```

Runs a comprehensive check of your environment:

- **Environment** — user, OS, available service backends
- **Kernel** — whether sing-box is installed and its path
- **Paths** — existence and permissions of all directories
- **TUN** — whether TUN device is available
- **Checks** — configuration validity, service unit status

## Common Issues

### "sing-box is not running" after `valsb start`

1. Check the service logs:
   ```bash
   journalctl -u valsb-sing-box.service -n 50
   ```
2. Validate the configuration:
   ```bash
   valsb doctor
   ```
3. Ensure you have an active subscription:
   ```bash
   valsb sub list
   ```

### "no config found"

You need to add a subscription first:

```bash
valsb sub add "https://your-provider.com/sub?format=singbox"
```

### "Text file busy" during update

This was a known issue on Linux, now fixed. Update to the latest valsb version. The updater unlinks the running binary before replacing it.

### "Clash API unreachable"

The Clash API is only available when sing-box is running. Start the service first:

```bash
valsb start
```

### Service status shows "running" but proxy doesn't work

1. Check your exit IP:
   ```bash
   valsb status
   ```
2. Try switching to a different node:
   ```bash
   valsb node use
   ```
3. Update your subscription to get fresh configs:
   ```bash
   valsb sub update
   ```

### TUN mode requires root

On Linux, TUN mode requires running as root:

```bash
sudo valsb start
```

On Windows, run PowerShell as Administrator.

## Getting Help

- [GitHub Issues](https://github.com/nsevo/val-sing-box-cli/issues)
- [sing-box Documentation](https://sing-box.sagernet.org/)
