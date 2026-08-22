---
description: "内置存储后端的能力矩阵：部署范围、浏览器直传 / 直连下载、容量观测、存储原生处理、凭据模式，以及 relay_stream 与 presigned 的权威对比。"
title: "存储能力矩阵"
---

:::tip[这一页是速查，不是教程]
按后端做接入的步骤在 [存储后端](/admin/storage-backends/) 各教程里；存储策略和策略组的概念在 [存储策略与策略组](/admin/storage-policies/)。这一页只回答"哪个后端有什么能力"和"上传 / 下载走哪条路"。
:::

## 能力速查

<!-- storage-connectors:matrix:start -->
| 后端 | 部署范围 | 浏览器直传 | 直连下载 | 容量观测 | 存储原生处理 | 凭据模式 |
| --- | --- | --- | --- | --- | --- | --- |
| [本机](/admin/storage-backends/local/) | 单实例本地 | 不支持 | 不支持 | 支持 | 不支持 | 无 connector 凭据 |
| [S3](/admin/storage-backends/s3/) | Primary 间共享 | Presigned | 支持 | 不支持 | 不支持 | 静态密钥 |
| [阿里云 OSS](/admin/storage-backends/alibaba-oss/) | Primary 间共享 | Presigned | 支持 | 不支持 | 不支持 | 静态密钥 |
| [SFTP](/admin/storage-backends/sftp/) | Primary 间共享 | 不支持 | 不支持 | 不支持 | 不支持 | 静态密钥 |
| [Azure Blob](/admin/storage-backends/azure-blob/) | Primary 间共享 | Presigned | 支持 | 不支持 | 不支持 | 静态密钥 |
| [华为云 OBS](/admin/storage-backends/huawei-obs/) | Primary 间共享 | Presigned | 支持 | 不支持 | 不支持 | 静态密钥 |
| [腾讯云 COS](/admin/storage-backends/tencent-cos/) | Primary 间共享 | Presigned | 支持 | 不支持 | 缩略图 + 媒体元数据 | 静态密钥 |
| [远程节点](/admin/storage-backends/remote-follower/) | Primary 间共享 | Presigned | 支持 | 支持 | 不支持 | 无 connector 凭据 |
| [OneDrive](/admin/storage-backends/onedrive/) | Primary 间共享 | Provider direct | 支持 | 支持 | 不支持 | 委托 OAuth |
| [七牛云 Kodo](/admin/storage-backends/qiniu-kodo/) | Primary 间共享 | Presigned | 支持 | 不支持 | 不支持 | 静态密钥 |
<!-- storage-connectors:matrix:end -->

表格展示的是 connector 的静态能力上限；具体策略选项和部署拓扑可以进一步收窄可用路径。例如远程节点的 `presigned` 要求直连模式和浏览器可达的 follower `base_url`，容量观测结果则取决于远程存储目标。

`静态密钥` 和 `委托 OAuth` 描述凭据取得方式，不表示数据库明文格式。所有 connector 自己管理的静态密钥、授权应用 secret 和 OAuth token 都由 `[auth].storage_credential_secret_key` 使用 AES-256-GCM 加密后落库；备份或迁移时必须保留该密钥。详见 [登录与会话](/reference/config/auth/#storage_credential_secret_key)。

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
