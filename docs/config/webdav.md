# WebDAV

::: tip 这一篇分两层

- **`config.toml` 里的 `[webdav]`** —— 路径前缀和上传体积硬上限，**改完要重启**
- **`管理 -> 系统设置 -> WebDAV`** —— 总开关和系统文件拦截规则，保存后立即影响新请求，不用重启

普通用户用 WebDAV 一般只关心：去个人空间左侧 `WebDAV` 页面创建专用账号，把地址塞进 Finder / Windows / rclone。
:::

## `config.toml` 里的静态配置

```toml
[webdav]
prefix = "/webdav"
payload_limit = 10737418240
xml_payload_limit = 1048576
```

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `prefix` | `"/webdav"` | WebDAV 路径前缀；改完客户端地址也要一起改 |
| `payload_limit` | `10737418240` | WebDAV 文件写入请求体硬上限，默认 10 GiB |
| `xml_payload_limit` | `1048576` | WebDAV XML 类请求体硬上限，默认 1 MiB；用于 `PROPFIND`、`PROPPATCH`、`REPORT`、`LOCK` |

::: warning 这些静态项改完要重启服务
和后台总开关不一样，静态配置只在启动时读一次。
:::

## 后台运行时设置

入口：

```text
管理 -> 系统设置 -> WebDAV
```

这里有三项：

- **启用 WebDAV**：总开关。关闭后桌面客户端会立刻无法继续访问，**不需要重启**
- **阻止 WebDAV 系统文件**：默认开启，用来拦住 Finder、Windows 资源管理器和同步工具自动生成的系统元数据文件
- **WebDAV 系统文件拦截规则**：按文件或目录 basename 匹配，忽略大小写，支持简单的 `*` 通配符

默认拦截这些名字：

- `.DS_Store`
- `._*`
- `.Spotlight-V100`
- `.Trashes`
- `.fseventsd`
- `Thumbs.db`
- `desktop.ini`
- `$RECYCLE.BIN`
- `System Volume Information`

这些文件通常不是用户真正想保存的内容。开启拦截后，客户端尝试通过 WebDAV 创建它们时会收到 `403`，普通文件上传不受影响。

::: tip 什么时候需要改规则
大多数站点保持默认就好。只有在你明确要备份这些系统元数据，或者某个客户端因为拦截规则反复报错影响正常同步时，再按实际客户端行为调整。
:::

## 普通用户的标准用法

1. 个人空间左侧 `WebDAV` 页面创建一个专用账号
2. 设用户名和密码
3. 需要的话限制到根目录下某个文件夹
4. 把地址、用户名、密码填进 Finder、Windows 资源管理器、rclone、Mountain Duck

::: tip 用专用账号，不要复用网页登录密码
WebDAV 专用账号的密码、范围都能单独管理，丢了也不会影响主账号。
:::

## 默认地址

```text
https://你的域名/webdav/
```

`/webdav/` 这个挂载根只作为入口和列表边界存在，不是数据库里的真实文件夹。客户端可以对它做 `PROPFIND` 来列出根目录，但不能对它做 `PROPPATCH` 写自定义 dead properties；这种请求会明确返回 `403 Forbidden`。需要保存自定义属性时，目标必须是具体文件或文件夹。

如果把 `prefix` 改成 `/dav`，客户端地址也改：

```text
https://你的域名/dav/
```

## 上传大文件要看三处

通过 WebDAV 上传大文件时，下面三个上限**取最小值生效**：

1. `webdav.payload_limit`
2. 反向代理的上传大小限制（Nginx `client_max_body_size` / Caddy 等）
3. 存储策略里的单文件大小限制

任何一个卡住，整体就卡住——排查时三处都要看。

`xml_payload_limit` 不限制文件内容上传，它只限制 WebDAV 的 XML 控制请求。除非客户端发出特别大的目录查询、锁请求或属性更新请求，否则一般不用调整。

## 反向代理时不要丢这些

::: warning WebDAV 不只是 GET/PUT
WebDAV 用了一堆扩展方法和头部，反向代理常常默认丢掉。请确保代理层透传：

**头部：** `Authorization`、`Depth`、`Destination`、`Overwrite`、`If`、`Lock-Token`、`Timeout`

**方法：** `PROPFIND`、`PROPPATCH`、`MKCOL`、`MOVE`、`COPY`、`LOCK`、`UNLOCK`
:::

完整反向代理示例见 [反向代理部署](/deployment/reverse-proxy)。
