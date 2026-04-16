---
sidebar_position: 1
slug: /
title: 快速开始
---

# 快速开始

**valsb** 是一个轻量级 CLI 工具，用于跨 Linux、macOS 和 Windows 平台管理 [sing-box](https://sing-box.sagernet.org/) 代理。它处理安装、订阅管理、节点切换、服务生命周期和更新——全部通过一个二进制文件完成。

## 快速上手

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

### 安装后

```bash
# 添加订阅
valsb sub add <你的订阅链接>

# 启动代理
valsb start

# 查看状态
valsb status
```

## 工作原理

valsb 作为 sing-box 的控制平面：

1. **订阅管理** — 从订阅 URL 获取 sing-box JSON 配置
2. **服务管理** — 将 sing-box 注册为系统服务（systemd、launchd、procd、Windows SCM）
3. **节点切换** — 通过 Clash API（热重载）或配置重写切换代理节点
4. **原子更新** — 检查并应用 valsb 和 sing-box 的更新，零中断

## 系统要求

- **Linux**：systemd 或 procd（OpenWrt）
- **macOS**：launchd
- **Windows**：Windows 服务控制管理器（TUN 模式需要管理员权限）
- **sing-box**：由 `valsb install` 自动安装
