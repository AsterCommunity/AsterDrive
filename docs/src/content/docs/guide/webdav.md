---
description: AsterDrive WebDAV 的连接方式、已实现协议能力、工作空间映射以及文件名、同名资源、属性、锁和 DeltaV 限制。
title: "WebDAV 功能与限制"
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

挂载前的开关、路径前缀、大小上限和系统文件拦截规则，见 [WebDAV 配置](/config/webdav/)。

## 已实现的协议能力

| 类别 | 方法或能力 | 当前行为 |
| --- | --- | --- |
| 能力发现 | `OPTIONS` | 返回已支持方法和 DAV 能力声明 |
| 下载 | `GET`, `HEAD` | 支持 ETag、`Last-Modified`、条件请求和字节 `Range`；范围读取返回 `206` |
| 上传 | `PUT` | 创建或覆盖文件，并执行条件头、锁、配额和存储策略检查 |
| 资源管理 | `MKCOL`, `DELETE`, `COPY`, `MOVE` | 创建目录、删除、复制和移动，支持 `Destination`、`Overwrite` 和相关条件检查 |
| 属性 | `PROPFIND`, `PROPPATCH` | 读取 live properties，并在具体文件或文件夹上保存 dead properties |
| 锁 | `LOCK`, `UNLOCK` | 数据库持久化的 exclusive/shared write lock，支持 `If` 和 `Lock-Token` |
| 最小 DeltaV | `VERSION-CONTROL`, `REPORT` | 支持文件的 `DAV:version-tree`，从 AsterDrive 文件版本生成版本树 |

`GET` 直接从文件所属存储驱动流式读取。WebDAV 不绕过存储策略：实际数据仍可以位于本地磁盘、S3-compatible 对象存储、Azure Blob、OneDrive 或远程 follower 节点，具体取决于当前工作空间的存储策略。

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

## `PROPFIND` 和属性边界

- 缺省 `Depth` 按 `infinity` 解析。
- 对文件夹发送 `Depth: infinity` 会返回 `403 Forbidden` 和 `DAV:propfind-finite-depth`，服务端不会做无界递归枚举。
- 文件上的 `Depth: infinity` 按单资源处理。
- `/webdav/` 是虚拟挂载根，不是数据库中的真实文件夹。它支持 `PROPFIND`，但对根的 `PROPPATCH` 返回 `403 Forbidden`。
- 自定义 dead properties 只保存在具体文件或文件夹上；`DAV:` 保护命名空间中的属性由服务端控制。

客户端要列出目录时应使用 `Depth: 1`。不要把 WebDAV 挂载当作一次请求就能遍历整棵工作空间的无限递归 API。

普通 WebDAV 客户端会自动生成正确的 XML，不需要手动处理下面这些规则。只有自己写脚本或接协议客户端时需要留意：

- `prop`、`allprop`、`propname` 和 `include` 必须属于 `DAV:` 命名空间，不能只写一个同名但没有命名空间的元素；
- 空请求体按 `allprop` 处理；只要发送了非空 XML，请求体就必须明确选择 `prop`、`allprop` 或 `propname` 之一；
- `include` 只能出现一次，而且只和 `allprop` 一起使用；
- 已经有有效选择项时，其他扩展元素会按 WebDAV 规则忽略，不会因为服务端不认识就让整个请求失败；只有未知元素、没有有效选择项的请求仍会被拒绝。

这意味着手写请求时应该声明 `xmlns="DAV:"`（或给对应元素使用绑定到 `DAV:` 的前缀）。如果普通客户端突然无法列目录，先抓取实际请求体，确认反向代理没有改写 XML。

## `COPY` / `MOVE` 边界

- `Destination` 必须位于当前 WebDAV 服务的同一 origin，并且仍在当前 WebDAV 路径前缀下。
- 跨 WebDAV 服务器的 `COPY` / `MOVE` 不在当前范围。
- `COPY` 接受 `Depth: 0` 或缺省 / `infinity`，明确拒绝 `Depth: 1`。
- 对文件夹使用 `COPY Depth: 0` 只复制文件夹本身和 dead properties，不复制子项。
- 请求会检查 ETag 条件、`If` / `Lock-Token` 以及 `Overwrite`。

## 锁和 DeltaV 限制

AsterDrive 支持持久化的 exclusive/shared write lock，也会在移动、复制、删除和覆盖前检查相关锁条件。过期锁会清理，管理员也可在后台清理异常残留锁。

对文件夹创建 `Depth: infinity` 锁后，这个锁会覆盖它的后代资源。客户端操作后代文件或文件夹时，只要按 WebDAV 规则在 `If` 头里提交同一个锁 token，AsterDrive 会用父文件夹锁自己的 href 校验 token，不会把有效 token 当成无权限操作。

当前已知边界：

- DeltaV 只实现最小子集：`VERSION-CONTROL` 和文件的 `REPORT DAV:version-tree`。AsterDrive 自身的版本历史不等于完整 RFC 3253 版本控制服务器。
- `REPORT version-tree` 只支持文件，不支持文件夹。

## 客户端兼容性怎么理解

仓库对 WebDAV 有三层检查：

1. Rust 协议回归测试；
2. 固定的 Litmus 0.18 Phase 0 基线；
3. rclone、curl 和 cadaver 真实客户端流程。

回归测试还覆盖 Finder 常见的 `PUT` 形态、特殊文件名、Range、条件请求、锁和属性操作。这表示已经有固定的兼容性检查，不表示所有操作系统、客户端和版本组合都已完整认证。

上线前建议用你实际采用的客户端验证：

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

完整代理示例见 [反向代理](/deployment/reverse-proxy/)。

## 限制速查

| 场景 | 当前结果 | 建议 |
| --- | --- | --- |
| 文件名含 `#` | 支持，URI 中必须是 `%23` | 使用正常 WebDAV 客户端，不手写 raw fragment |
| 同层文件/文件夹同名 | 产品层允许，WebDAV 投影有歧义且优先文件夹 | 需要 WebDAV 同步的目录避免这类同名 |
| collection `PROPFIND Depth: infinity` | `403` + `DAV:propfind-finite-depth` | 列目录使用 `Depth: 1` |
| 挂载根 `PROPPATCH` | `403` | 只对具体文件/文件夹写自定义属性 |
| 跨服务器 `COPY` / `MOVE` | 目标被拒绝 | 先下载再上传，或用客户端同步 |
| 目录级递归锁 | `Depth: infinity` 锁覆盖后代，后代操作可提交父文件夹锁 token | 确认客户端会在 `If` 头里继续携带锁 token |
| 完整 DeltaV | 只支持最小文件版本树子集 | 将完整版本管理放在 AsterDrive 网页/API 中 |
