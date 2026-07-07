---
description: AsterDrive 上传与存储功能地图，覆盖上传模式、Blob、配额、存储策略、策略组、本地存储、S3、Azure Blob、腾讯云 COS、OneDrive、SFTP 和远程节点。
---

# 上传与存储

上传与存储负责把“浏览器或客户端传来的文件”变成“数据库文件记录 + 存储驱动里的对象”。它也是最容易受反向代理、CORS、对象存储和远程节点影响的功能域。

## 能力边界

| 能力 | 说明 | 相关文档 |
| --- | --- | --- |
| 直传小文件 | 小文件直接 POST 到 primary，由服务端写入目标存储 | [上传与大文件](/guide/upload-modes) |
| 分片上传 | 本地分片会话、进度查询、断点续传和 24h session TTL | [上传与大文件](/guide/upload-modes) |
| 对象存储预签名上传 | 浏览器直接 PUT 到 S3-compatible、Azure Blob SAS URL 或腾讯云 COS，服务端最终校验并落账 | [上传与大文件](/guide/upload-modes)、[存储策略](/config/storage) |
| 对象存储 multipart | 浏览器分批上传 part，服务端 complete 后校验内容 | [上传与大文件](/guide/upload-modes)、[S3 / MinIO / R2](/storage/s3-minio-r2)、[Azure Blob Storage](/storage/azure-blob)、[腾讯云 COS](/storage/tencent-cos) |
| Microsoft Graph 存储 | 通过管理员授权把文件写入 OneDrive、SharePoint site drive 或 Microsoft 365 group drive | [OneDrive](/storage/onedrive)、[存储策略](/config/storage) |
| SFTP 存储 | 通过服务端流式读写把文件写到 SSH/SFTP 文件服务器 | [SFTP](/storage/sftp)、[存储策略](/config/storage) |
| 存储策略 | 决定文件最终写到 local、s3、sftp、azure_blob、tencent_cos、one_drive 或 remote | [存储策略](/config/storage) |
| 策略组 | 按用户、团队和文件大小分流到不同存储策略 | [存储策略](/config/storage) |
| 远程节点存储 | primary 把对象写到 follower，再由 follower 写本地或 S3 | [远程节点接入](/guide/remote-nodes)、[远程节点存储策略](/storage/remote-follower) |

## 后端模块

| 模块 | 负责内容 |
| --- | --- |
| `upload_service` | 上传会话、分片、进度、状态转换 |
| `workspace::storage_core` | Blob 去重、文件记录、配额、策略选择和最终落账 |
| `policy_service` | 存储策略、策略组和规则 |
| `storage::traits`、`storage::drivers`、`storage::connectors` | `StorageDriver` 和 `StorageConnector` 抽象、本地、S3-compatible、SFTP、Azure Blob、Tencent COS、OneDrive 和 remote 驱动 |
| `storage::remote_protocol` | primary/follower 内部远程存储协议 |
| `remote::remote_node`、`managed_ingress_profile_service` | 远程节点和接收落点 |
| `task_service::storage_migration` | 存储迁移任务 |

## 关键边界

- 配额检查有事务外 fast-fail 和事务内权威校验两层。
- 文件名唯一性、Blob 引用计数和 session 状态转换都必须用 repo 层原子辅助函数。
- `presigned` 能否工作取决于浏览器能否访问对象存储或 follower `base_url`，不是只看 primary 能不能连通。
- 后台连接测试只证明 AsterDrive 服务端能连到目标后端；`presigned` 上传 / 下载还要另外确认浏览器到对象存储、Azure Blob 或 follower 的网络和 CORS。
- SFTP 是服务端流式后端，重点是 Endpoint、SSH 凭据、基础路径和主机密钥指纹，不涉及浏览器 CORS 或预签名 URL。
- 远程节点 `reverse_tunnel` 适合 `relay_stream`，不适合浏览器直连的 `presigned`。

## 排障方向

- 小文件能传，大文件失败：看反向代理大小、超时、临时目录和分片大小。
- `relay_stream` 成功、`presigned` 失败：优先看 CORS、浏览器网络和 endpoint 可达性。
- 连接测试失败：优先看后台返回的诊断信息；存储类诊断会出现在标准错误响应的 `error.diagnostic.message` 里。
- 远程节点策略失败：先看节点启用状态、传输方式、默认接收落点和协议能力。
- 容量或 Blob 引用异常：用 [运维 CLI](/deployment/ops-cli) 深度检查。
