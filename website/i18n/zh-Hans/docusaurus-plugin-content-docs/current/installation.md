---
sidebar_position: 2
title: 安装
---

# 安装

## 一行命令安装

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.sh | bash
```

该脚本会：
1. 检测操作系统和架构
2. 下载最新的 `valsb` 二进制文件
3. 下载最新的 `sing-box` 内核
4. 将二进制文件放置到正确的系统路径
5. 注册 sing-box 服务

### Windows（以管理员身份运行 PowerShell）

```powershell
irm https://raw.githubusercontent.com/nsevo/val-sing-box-cli/main/scripts/install.ps1 | iex
```

该脚本会：
1. 下载 `valsb.exe` 到 `%APPDATA%\val-sing-box-cli\bin\`
2. 将 bin 目录添加到用户 PATH
3. 运行 `valsb install` 下载 sing-box 并注册 Windows 服务

## 手动安装

从 [GitHub Releases](https://github.com/nsevo/val-sing-box-cli/releases) 下载最新版本。

```bash
# 解压
tar xzf valsb-v*.tar.gz

# 移动到 PATH 目录
sudo mv valsb /usr/local/bin/

# 安装 sing-box 内核并注册服务
valsb install
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
