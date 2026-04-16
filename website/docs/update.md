---
sidebar_position: 6
title: Update
---

# Update

## Checking for Updates

```bash
valsb update
```

This command:

1. Checks the latest versions of both `valsb` and `sing-box` from GitHub Releases
2. Compares with the currently installed versions
3. Shows a diff if updates are available
4. Asks for confirmation before proceeding

```
  [ok]     Version check complete
  valsb      0.0.9 → 0.1.0
  sing-box   1.13.7 → 1.13.8

  ? Proceed with update? yes
```

## Atomic Update Process

The update process is designed to minimize downtime:

1. **Download first** — all new binaries are downloaded to temporary files before any changes
2. **Replace binaries** — only after successful download, the old binaries are atomically replaced
3. **Restart service** — if sing-box was running, it is restarted with the new binary

On Unix systems, the running `valsb` binary is replaced by unlinking the old file first, then copying the new one. This avoids the "Text file busy" error.

## Non-interactive Mode

```bash
valsb update --yes
```

Skips the confirmation prompt. Useful for automation and cron jobs.

## JSON Output

```bash
valsb update --json
```

Returns structured JSON with version information and update results.
