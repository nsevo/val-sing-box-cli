---
sidebar_position: 4
title: 配置
---

# 配置

valsb 不需要手动配置文件。所有状态通过订阅数据和内部状态文件自动管理。

## 文件位置

运行 `valsb config path` 查看系统上的所有路径：

```
  Config     /etc/val-sing-box-cli
  Data       /var/lib/val-sing-box-cli
  Cache      /var/cache/val-sing-box-cli
  Kernel     /usr/local/lib/val-sing-box-cli/bin/sing-box
  Service    /etc/systemd/system/valsb-sing-box.service
  State      /var/lib/val-sing-box-cli/state.json
```

路径因平台和权限级别（root 或用户）而异。

### 路径约定

| 平台 | Root | 配置目录 | 数据目录 |
|------|------|---------|---------|
| Linux | 是 | `/etc/val-sing-box-cli` | `/var/lib/val-sing-box-cli` |
| Linux | 否 | `~/.config/val-sing-box-cli` | `~/.local/share/val-sing-box-cli` |
| macOS | 否 | `~/Library/Application Support/val-sing-box-cli` | 同上 |
| Windows | 否 | `%APPDATA%\val-sing-box-cli` | 同上 |

## sing-box 配置

sing-box 配置完全来自订阅。valsb 会：

1. 从订阅 URL 获取原始 JSON 配置
2. 注入 `experimental.clash_api`（用于通过 API 切换节点）
3. 注入 `experimental.cache_file`（用于持久化缓存）
4. 写入活跃配置路径

你可以自定义每个分组的节点选择。valsb 将你的选择持久化在状态文件中，并通过修改 selector outbound 的 `default` 字段来应用。

## JSON 输出

所有命令支持 `--json` 参数以获得机器可读输出：

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
