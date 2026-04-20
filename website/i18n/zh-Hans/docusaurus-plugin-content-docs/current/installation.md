---
sidebar_position: 2
title: 安装
---

# 安装

`valsb` 是一个 root 专用工具：它会注册系统服务、写入 `/etc`、`/var/lib`、`/var/cache`（Windows 上是 `%ProgramData%`），并且 TUN 模式本身需要 root。安装脚本因此要求 root，普通用户运行 `valsb` 命令时会自动请求 sudo（Windows 下会触发 UAC 提示）。

## 一行命令安装

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | sudo bash
```

该脚本会：
1. 检测操作系统和架构
2. 如果当前不是 root，则使用 `sudo` 重新执行自身
3. 下载最新的 `valsb` 与 `sing-box` 二进制
4. 将二进制放置到 `/usr/local/bin` 与 `/usr/local/lib/val-sing-box-cli/bin`
5. 通过平台原生服务管理器（systemd/launchd/procd）注册服务

### Windows（以管理员身份运行 PowerShell）

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

该脚本会：
1. 如果未提权，会触发 UAC 弹窗以管理员身份重新运行
2. 将 `valsb.exe` 安装到 `%ProgramFiles%\val-sing-box-cli\`
3. 将 bin 目录加入系统 PATH
4. 运行 `valsb install` 下载 sing-box 并注册 Windows 服务

## 手动安装

从 [GitHub Releases](https://github.com/nsevo/val-sing-box-cli/releases) 下载最新版本。

```bash
# 解压
tar xzf valsb-v*.tar.gz

# 移动到 PATH 目录（root 拥有）
sudo mv valsb /usr/local/bin/

# 安装 sing-box 内核并注册服务
sudo valsb install
```

## 验证安装

```bash
valsb version
```

预期输出：

```
  valsb       0.1.0
  sing-box    1.13.8
  platform    linux/amd64
```

## 卸载

```bash
valsb uninstall --yes
```

这将移除所有二进制文件、服务文件、配置、缓存和数据目录——完全无痕卸载。
