---
description: "AsterDrive WebDAV 协议兼容参考：已实现的方法与能力、PROPFIND / COPY / MOVE 边界、锁与版本控制边界、客户端验收口径和限制速查表。"
title: "WebDAV 协议兼容"
---

:::tip[这一页是协议层的权威清单]
怎么创建 WebDAV 账号、怎么挂载、文件名编码和同名限制的用户侧说明见 [WebDAV 使用](/using/webdav/)；总开关、路径前缀和 `config.toml` 静态字段见 [WebDAV 配置](/reference/config/webdav/)；运行时开关和系统文件拦截见 [WebDAV（系统设置）](/reference/config/runtime/webdav/)。
:::

## 已实现的协议能力

| 类别 | 方法或能力 | 当前行为 |
| --- | --- | --- |
| 能力发现 | `OPTIONS` | 返回已支持方法和 DAV 能力声明 |
| 下载 | `GET`, `HEAD` | 支持 ETag、`Last-Modified`、条件请求和字节 `Range`；范围读取返回 `206` |
| 上传 | `PUT` | 创建或覆盖文件，并执行条件头、锁、配额和存储策略检查 |
| 资源管理 | `MKCOL`, `DELETE`, `COPY`, `MOVE` | 创建目录、删除、复制和移动，支持 `Destination`、`Overwrite` 和相关条件检查 |
| 属性 | `PROPFIND`, `PROPPATCH` | 读取 live properties，并在具体文件或文件夹上保存 dead properties |
| 锁 | `LOCK`, `UNLOCK` | 数据库持久化的 exclusive/shared write lock，支持 `If` 和 `Lock-Token` |

`GET` 直接从文件所属存储驱动流式读取。WebDAV 不绕过存储策略：实际数据仍可以位于本地磁盘、S3-compatible 对象存储、Azure Blob、OneDrive 或远程 follower 节点，具体取决于当前工作空间的存储策略。

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

## 锁和版本控制边界

AsterDrive 支持持久化的 exclusive/shared write lock，也会在移动、复制、删除和覆盖前检查相关锁条件。过期锁会清理，管理员也可在后台清理异常残留锁。

对文件夹创建 `Depth: infinity` 锁后，这个锁会覆盖它的后代资源。客户端操作后代文件或文件夹时，只要按 WebDAV 规则在 `If` 头里提交同一个锁 token，AsterDrive 会用父文件夹锁自己的 href 校验 token，不会把有效 token 当成无权限操作。

AsterDrive 的文件版本历史是产品能力，不构成 RFC 3253 core versioning resource model。当前 WebDAV capability snapshot 不声明 `version-control`，`REPORT` 和 `VERSION-CONTROL` 对已认证资源返回带资源级 `Allow` 的 `405 Method Not Allowed`。

## 客户端兼容性怎么理解

仓库对 WebDAV 有三层检查：

1. Rust 协议回归测试；
2. 固定的 Litmus 0.18 Phase 0 基线；
3. rclone、curl 和 cadaver 真实客户端流程。

回归测试还覆盖 Finder 常见的 `PUT` 形态、特殊文件名、Range、条件请求、锁和属性操作。这表示已经有固定的兼容性检查，不表示所有操作系统、客户端和版本组合都已完整认证。

## 限制速查

| 场景 | 当前结果 | 建议 |
| --- | --- | --- |
| 文件名含 `#` | 支持，URI 中必须是 `%23` | 使用正常 WebDAV 客户端，不手写 raw fragment |
| 同层文件/文件夹同名 | 产品层允许，WebDAV 投影有歧义且优先文件夹 | 需要 WebDAV 同步的目录避免这类同名 |
| collection `PROPFIND Depth: infinity` | `403` + `DAV:propfind-finite-depth` | 列目录使用 `Depth: 1` |
| 挂载根 `PROPPATCH` | `403` | 只对具体文件/文件夹写自定义属性 |
| 跨服务器 `COPY` / `MOVE` | 目标被拒绝 | 先下载再上传，或用客户端同步 |
| 目录级递归锁 | `Depth: infinity` 锁覆盖后代，后代操作可提交父文件夹锁 token | 确认客户端会在 `If` 头里继续携带锁 token |
| RFC 3253 DeltaV | 当前不声明 `version-control`，不开放标准 `REPORT` / `VERSION-CONTROL` | 使用 AsterDrive 网页/API 管理版本历史 |
