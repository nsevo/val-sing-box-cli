---
sidebar_position: 3
title: 命令参考
---

# 命令参考

## 服务生命周期

| 命令 | 说明 |
|------|------|
| `valsb start` | 启动 sing-box 服务 |
| `valsb stop` | 停止 sing-box 服务 |
| `valsb restart` | 重启 sing-box 服务 |
| `valsb status` | 查看服务状态、活跃节点和出口 IP |

### `valsb status`

显示代理服务的当前状态，包括内核版本、活跃配置、选中节点、出口 IP 和位置。

```bash
valsb status
```

## 订阅管理

| 命令 | 说明 |
|------|------|
| `valsb sub add <url>` | 添加订阅 |
| `valsb sub list` | 列出所有订阅 |
| `valsb sub update [target]` | 更新订阅 |
| `valsb sub remove <target>` | 移除订阅 |
| `valsb sub use <target>` | 切换活跃配置 |

`target` 可以是配置名称、ID 或索引号。

### `valsb sub add`

```bash
valsb sub add "https://example.com/sub?format=singbox"
```

订阅 URL 必须返回完整的 sing-box JSON 配置。如果提供商支持多种格式，请添加 `&format=singbox`。

## 节点管理

| 命令 | 说明 |
|------|------|
| `valsb node use` | 交互式节点浏览器 |
| `valsb node use <name>` | 直接切换到指定节点 |

### 交互式节点浏览器

不带参数运行 `valsb node use` 会打开交互式终端 UI：

- **分组选择** — 选择要浏览的代理分组
- **模糊搜索** — 直接输入即可按名称过滤节点
- **实时延迟** — 每 5 秒通过 Clash API 刷新延迟
- **键盘导航** — `↑↓` 移动，`Enter` 选择，`Esc` 返回

```
  Proxy

  > HK-1  120ms *
    HK-2  145ms
    JP-1  89ms
    JP-2  102ms
    US-1  210ms

  up/down navigate  enter select  esc back
```

输入 `jp` 即可快速过滤到日本节点。

## 配置

| 命令 | 说明 |
|------|------|
| `valsb config init` | 初始化配置目录 |
| `valsb config path` | 显示所有文件路径 |

## 系统

| 命令 | 说明 |
|------|------|
| `valsb install` | 安装 sing-box 内核并注册服务 |
| `valsb uninstall --yes` | 移除所有内容（无痕卸载） |
| `valsb update` | 检查并应用 valsb 和 sing-box 的更新 |
| `valsb version` | 显示版本信息 |
| `valsb doctor` | 诊断环境和配置 |
| `valsb reload` | 重载 sing-box 配置 |

## 全局参数

| 参数 | 说明 |
|------|------|
| `--json` | 以 JSON 格式输出（用于脚本） |
| `--verbose` | 启用调试日志 |
| `--yes` | 跳过确认提示 |
| `--config-dir <path>` | 覆盖配置目录 |
