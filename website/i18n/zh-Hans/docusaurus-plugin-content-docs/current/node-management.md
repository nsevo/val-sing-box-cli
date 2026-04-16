---
sidebar_position: 5
title: 节点管理
---

# 节点管理

## 切换节点

### 交互式浏览器

```bash
valsb node use
```

打开多层交互式浏览器：

1. **分组选择** — 如果有多个 selector 分组，先选择一个
2. **节点列表** — 浏览带实时延迟、模糊搜索和键盘导航的节点

### 直接切换

```bash
valsb node use "HK-1"
```

按节点名称直接切换。跨所有 selector 分组工作——valsb 自动查找匹配的节点。

## 模糊搜索

在交互式节点浏览器中，直接输入即可过滤节点：

```
  Proxy
  > jp  (3/21)

  > JP-1  89ms
    JP-2  102ms
    JP-3  115ms

  up/down navigate  enter select  esc back
```

- 输入任意字符实时缩小结果范围
- `Backspace` 编辑过滤内容
- `Esc` 清除过滤器，若过滤器已为空则退出浏览器

## 延迟测试

当 sing-box 服务运行时，节点浏览器自动测试延迟：

- 进入时，spinner 显示初始批量测试进度
- 每 5 秒通过 Clash API 分组延迟测试端点刷新
- 延迟按颜色编码：**绿色** < 150ms，**黄色** < 300ms，**红色** ≥ 300ms

## 节点切换原理

valsb 根据服务是否运行使用两种策略：

1. **Clash API（热重载）** — 如果 sing-box 正在运行，valsb 调用 `PUT /proxies/:group` 即时切换节点，无需重启服务
2. **配置重写** — 修改 selector outbound 中的 `default` 字段并重载配置

优先使用 Clash API 方法，因为它提供零中断切换。
