---
sidebar_position: 5
title: Node Management
---

# Node Management

## Switching Nodes

### Interactive Browser

```bash
valsb node use
```

Opens a multi-layer interactive browser:

1. **Group selection** — if multiple selector groups exist, pick one first
2. **Node list** — browse nodes with real-time latency, fuzzy search, and keyboard navigation

### Direct Switch

```bash
valsb node use "HK-1"
```

Switches directly by node name. Works across all selector groups — valsb finds the matching node automatically.

## Fuzzy Search

In the interactive node browser, just start typing to filter nodes instantly:

```
  Proxy
  > jp  (3/21)

  > JP-1  89ms
    JP-2  102ms
    JP-3  115ms

  up/down navigate  enter select  esc back
```

- Type any characters to narrow down results in real time
- `Backspace` to edit the filter
- `Esc` clears the filter, or exits the browser if the filter is already empty

## Latency Testing

When the sing-box service is running, the node browser automatically tests latency:

- On entry, a spinner shows while the initial batch test runs
- Every 5 seconds, delays refresh via the Clash API group delay test endpoint
- Latency is color-coded: **green** < 150ms, **yellow** < 300ms, **red** ≥ 300ms

## How Node Switching Works

valsb uses two strategies depending on whether the service is running:

1. **Clash API (hot-reload)** — if sing-box is running, valsb calls `PUT /proxies/:group` to switch the node instantly without restarting the service
2. **Config rewrite** — modifies the `default` field in the selector outbound and reloads the config

The Clash API method is preferred as it provides zero-downtime switching.
