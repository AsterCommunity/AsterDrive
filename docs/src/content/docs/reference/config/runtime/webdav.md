---
description: "系统设置的 WebDAV 分组：全站总开关、常见操作系统文件拦截规则；路径前缀和上传硬上限在 config.toml。"
title: "WebDAV（系统设置）"
---

所属入口：`管理 -> 系统设置 -> WebDAV`。其他分组和生效时机见 [系统设置](/reference/config/runtime/)。

这里控制 WebDAV 的全站运行行为：

- **`启用 WebDAV`**
- **`阻止 WebDAV 系统文件`**
- **`WebDAV 系统文件拦截规则`**

关闭后桌面客户端会立刻无法继续通过 WebDAV 访问文件。

默认会阻止 WebDAV 客户端创建常见操作系统元数据文件和目录，例如 `.DS_Store`、`._*`、`.Spotlight-V100`、`.Trashes`、`.fseventsd`、`Thumbs.db`、`desktop.ini`、`$RECYCLE.BIN` 和 `System Volume Information`。这些通常是 Finder、Windows 资源管理器或同步工具自动写出来的文件，放进网盘里多数时候只会污染目录。

拦截规则按 basename 匹配，忽略大小写，支持简单的 `*` 通配符。只有在你明确需要同步这类系统文件时，才建议关闭这一项或删掉对应规则。

:::tip[路径前缀和上传硬上限不在这里]
如果你只是想改 WebDAV 路径前缀或 WebDAV 上传体积硬上限，那不在系统设置里，而是在 [`config.toml` 的 `[webdav]`](/reference/config/webdav/) 里改，改完要重启。
:::
