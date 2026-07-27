---
description: "AsterDrive WebDAV 使用：创建专用账号、挂载地址、文件名 URL 编码、同名文件/文件夹限制、客户端验收清单和反向代理注意事项。"
title: "WebDAV 使用"
---

:::tip[先说结论]
AsterDrive 的 WebDAV 是个人或团队工作空间的协议视图，不是另外一套文件系统。通过 WebDAV 上传、移动、复制和删除的资源，会继续使用 AsterDrive 的工作空间、存储策略、配额、历史版本和审计链路。
:::

## 连接前先准备账号

WebDAV 默认挂载地址是：

```text
https://你的域名/webdav/
```

连接步骤：

1. 在需要连接的个人或团队工作空间中创建 WebDAV 专用账号。
2. 保存创建时返回的用户名和密码；明文密码只显示一次。
3. 把挂载地址、用户名和密码填入 WebDAV 客户端。
4. 如果账号设置了根文件夹，客户端只会看到该文件夹及其子项。

WebDAV 挂载使用 **Basic Auth 和 WebDAV 专用凭据**。网页登录的 Bearer JWT 不是 WebDAV 挂载凭据，也不需要把网页登录密码交给客户端。

个人账号只进入对应的个人空间；团队账号只进入对应的团队空间，并继续受团队成员身份、角色和工作空间权限约束。

挂载前的开关、路径前缀、大小上限和系统文件拦截规则，见 [WebDAV 配置](/reference/config/webdav/)；协议层已实现的方法、属性、锁和 DeltaV 边界，见 [WebDAV 协议兼容](/reference/webdav-compat/)。

## 文件名必须按 URL 规则编码

WebDAV 路径是 URI，不是直接把操作系统文件名拼到字符串后面。文件名中的保留字符需要由客户端做 percent-encoding。

例如，Windows 允许的文件名：

```text
report#draft.txt
```

在 WebDAV URL 中应表示为：

```text
/webdav/report%23draft.txt
```

`#` 在 URI 中用来开始 fragment。下面的写法不表示文件名中的 `#`：

```text
/webdav/report#draft.txt
```

常见 WebDAV 客户端会在发送前移除真正的 fragment，并把文件名中的 `#` 编码为 `%23`。AsterDrive 已覆盖 `%23` 文件名的上传和下载往返。刻意发送带 raw `#fragment` 的非标准 request-target 时，底层 HTTP 解析器可能在 AsterDrive 处理前就截断 fragment；不要使用这种形式表示文件名。该解析边界由 [GitHub #424](https://github.com/AsterCommunity/AsterDrive/issues/424) 跟踪。

## 文件和文件夹同名时的限制

:::caution[WebDAV 只有一个 URI 命名空间]
AsterDrive 产品模型当前允许同一父目录下的文件和文件夹同名；WebDAV 中的一个 href 却只能稳定表示一个资源。这两种模型不完全对等。
:::

假设同一父目录下同时存在：

```text
report        # 文件
report/       # 文件夹
```

在 WebDAV 视图里，`/report` 和 `/report/` 不适合当作两个可独立管理的资源标识。当既有同名冲突已经存在时，AsterDrive WebDAV 的路径解析优先返回文件夹，同名文件在 WebDAV 视图中可能被遮蔽。

WebDAV 写入会尽量保持这个单一命名空间：

- 目标已是文件时，`MKCOL` 返回 `405 Method Not Allowed`；
- 目标已是文件夹时，`MKCOL` 也返回 `405 Method Not Allowed`；
- 目标已是文件夹时，`PUT` 返回 `405 Method Not Allowed`；
- `COPY` / `MOVE` 将目标 href 当作一个资源，并按 `Overwrite` 语义处理已存在目标。

如果同名对象是通过网页、REST API 或老版本创建的，WebDAV 不会自动重命名或删除它们。这类目录是一个有损投影：文件可能在 WebDAV 客户端中不可达。需要稳定同步的目录，应避免在同一层创建同名文件和文件夹。

## 上线前用真实客户端验收

仓库里的协议回归测试和兼容性基线见 [WebDAV 协议兼容](/reference/webdav-compat/)。上线前建议用你实际采用的客户端验证：

1. 根目录和两层子目录可以列出；
2. 普通文件和包含空格、中文、`#` 的文件可以上传、下载和重命名；
3. 大文件限制、Range 下载和断线后重试符合预期；
4. 复制、移动、删除和覆盖行为符合预期；
5. 同一文件在多客户端打开时，锁和冲突提示可以接受。

## 反向代理别破坏 WebDAV

WebDAV 不只用 `GET` 和 `PUT`。反向代理必须透传扩展方法和相关请求头，特别是：

- 方法：`PROPFIND`、`PROPPATCH`、`MKCOL`、`COPY`、`MOVE`、`LOCK`、`UNLOCK`、`REPORT`、`VERSION-CONTROL`；
- 头部：`Authorization`、`Depth`、`Destination`、`Overwrite`、`If`、`Lock-Token`、`Timeout`。

反向代理还可能有自己的请求体上限、超时、缓冲和路径编码规则。遇到“小文件正常，大文件失败”“可以下载，不能创建目录”“特殊文件名变了”时，同时对照直连 AsterDrive 和经过代理的结果。

完整代理示例见 [反向代理](/deploy/reverse-proxy/)。
