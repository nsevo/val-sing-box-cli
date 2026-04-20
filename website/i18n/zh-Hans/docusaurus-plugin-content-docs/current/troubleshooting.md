---
sidebar_position: 7
title: 故障排除
---

# 故障排除

## 诊断

```bash
valsb doctor
```

运行全面的环境检查：

- **环境** — 用户、操作系统、可用的服务后端
- **内核** — sing-box 是否已安装及其路径
- **路径** — 所有目录的存在性和权限
- **TUN** — TUN 设备是否可用
- **检查** — 配置有效性、服务单元状态

## 常见问题

### `valsb start` 后 "sing-box is not running"

1. 检查服务日志：
   ```bash
   journalctl -u valsb-sing-box.service -n 50
   ```
2. 验证配置：
   ```bash
   valsb doctor
   ```
3. 确保有活跃的订阅：
   ```bash
   valsb sub list
   ```

### "no config found"

需要先添加订阅：

```bash
valsb sub add "https://your-provider.com/sub?format=singbox"
```

### 更新时 "Text file busy"

这是 Linux 上的已知问题，已修复。请更新到最新版本的 valsb。更新器在替换前会先取消链接正在运行的二进制文件。

### "Clash API unreachable"

Clash API 仅在 sing-box 运行时可用。请先启动服务：

```bash
valsb start
```

### 状态显示 "running" 但代理不工作

1. 检查出口 IP：
   ```bash
   valsb status
   ```
2. 尝试切换到其他节点：
   ```bash
   valsb node use
   ```
3. 更新订阅获取最新配置：
   ```bash
   valsb sub update
   ```

### Root 权限

`valsb` 是一个 root 专用工具。普通用户运行任意需要写入系统状态或操作服务的命令时，会自动以 `sudo`（Linux/macOS）或 UAC 提示（Windows）重新启动自身，因此你通常不需要手动加 `sudo`。如果想跳过提示，可以直接在 root shell 中运行，或先执行 `sudo -v` 提前授权。

## 获取帮助

- [GitHub Issues](https://github.com/nsevo/val-sing-box-cli/issues)
- [sing-box 文档](https://sing-box.sagernet.org/)
