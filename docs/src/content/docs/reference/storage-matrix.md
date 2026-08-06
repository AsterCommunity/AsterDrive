---
description: "八种存储后端的能力矩阵：浏览器直传 / 直连下载、容量观测、存储原生处理、凭据落库方式，以及 relay_stream 与 presigned 的权威对比。"
title: "存储能力矩阵"
---

:::tip[这一页是速查，不是教程]
按后端做接入的步骤在 [存储后端](/admin/storage-backends/) 各教程里；存储策略和策略组的概念在 [存储策略与策略组](/admin/storage-policies/)。这一页只回答"哪个后端有什么能力"和"上传 / 下载走哪条路"。
:::

## 能力速查

| 后端 | 浏览器直传 / 直连下载 | 容量观测 | 存储原生处理 | 凭据落库 |
| --- | --- | --- | --- | --- |
| `local` | 不支持，由 AsterDrive 读写本地磁盘 | 支持（文件系统） | 不支持 | 无凭据 |
| `s3` | `presigned` 上传 + 下载 | 不支持 | 不支持 | 明文 |
| `azure_blob` | `presigned`（SAS URL）上传 + 下载 | 不支持 | 不支持 | 明文 |
| `tencent_cos` | `presigned` 上传 + 下载 | 不支持 | COS 数据万象（按策略开关） | 明文 |
| `asterdrive.storage.huawei_obs` | `presigned` 上传 + 下载（需 OBS CORS） | 不支持 | 不支持 | AES-256-GCM 加密 |
| `one_drive` | `frontend_direct` 上传、Graph 直接下载 | 支持（Graph quota） | 不支持 | AES-256-GCM 加密 |
| `sftp` | 不支持，服务端流式读写 | 不支持 | 不支持 | 明文 |
| `remote` | `presigned` 需直连 + 浏览器可达的 follower `base_url` | 跟随远程存储目标 | 不支持 | 明文 |

OneDrive 凭据的加密主密钥是 `config.toml` 里的 `[auth].storage_credential_secret_key`，迁移备份时必须保留；见 [登录与会话](/reference/config/auth/#storage_credential_secret_key)。

## `relay_stream` vs `presigned`

这一节是上传 / 下载方式的**唯一权威说明**，各后端教程只讲自己的开启条件和差异。

| 方式 | 数据路径 | 优点 | 代价 |
| --- | --- | --- | --- |
| `relay_stream` | 浏览器 ↔ AsterDrive ↔ 存储后端 | 浏览器不直连后端，不踩 CORS；内网后端可用；便于排查 | 流量经过 AsterDrive，占用节点带宽和连接 |
| `presigned` / 直连 | 浏览器 ↔ 存储后端（AsterDrive 只签发地址） | 卸载 AsterDrive 带宽；大文件和高并发更稳 | 浏览器必须能访问后端；要配 CORS、HTTPS 证书和暴露响应头 |

建议顺序：新后端先用 `relay_stream` 跑通上传、下载、预览、分享，确认稳定后再按教程切 `presigned`。

切到 `presigned` 前要确认：

- 浏览器能直接访问对象存储 endpoint 或 follower `base_url`（通常是真实 HTTPS 域名）
- 后端 CORS 允许 AsterDrive 站点的来源，并暴露下载和 Range 所需响应头
- 公开分享、图片预览、PDF / 视频 Range 请求在新方式下都验证过一遍

OneDrive 是例外：它的 `frontend_direct` 直传跨域支持由 Microsoft 提供，不需要在 AsterDrive 或对象存储侧额外配 CORS。
