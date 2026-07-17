# 术语表

这页解释 AsterDrive 文档里反复出现的词。遇到不熟悉的概念时，可以先来这里确认含义，再继续看对应配置或流程。

## 节点与运行模式

| 术语 | 解释 | 相关文档 |
| --- | --- | --- |
| 主控节点 / primary | 默认运行模式。负责登录、前端、管理后台、分享、WebDAV、策略和元数据。 | [服务器配置](/config/server) |
| 从节点 / follower | 远程存储后端。只接收主控签名后的内部对象请求，不给普通用户登录。 | [远程节点](/guide/remote-nodes) |
| 远程节点 | 主控后台里登记的一台 follower 记录，包含节点地址、状态、密钥和远程存储目标。 | [远程节点](/guide/remote-nodes) |
| 远程存储目标 | follower 上真正写入对象的位置，可以是本地目录或 S3 / MinIO；remote 策略可以绑定具体目标。 | [远程节点](/guide/remote-nodes) |
| enroll | 把 follower 绑定到主控的接入动作。通常通过后台生成命令，再到 follower 执行。 | [运维 CLI](/deployment/ops-cli) |

## 存储与上传

| 术语 | 解释 | 相关文档 |
| --- | --- | --- |
| 存储策略 | 定义文件真实落点和上传方式。比如本地目录、S3 / MinIO / Azure Blob / 腾讯云 COS / OneDrive / SFTP、远程节点。 | [存储策略](/config/storage) |
| 策略组 | 决定用户或团队上传时命中哪条存储策略，可以按文件大小分流。 | [存储策略](/config/storage) |
| Blob | 底层文件对象。多个文件记录可以引用同一个 Blob，用于内容去重和版本引用。 | [关于 AsterDrive](/reference/about) |
| 分片上传 | 大文件拆成多个片段上传，失败后尽量续传。 | [上传与大文件](/guide/upload-modes) |
| 对象存储直传 | 浏览器直接把文件传到 S3 / MinIO / 腾讯云 COS，服务端只负责签名和完成确认。 | [上传与大文件](/guide/upload-modes) |
| 服务端转发 | 浏览器先把文件传给 AsterDrive，再由服务端写到 S3 / MinIO / 腾讯云 COS。 | [上传与大文件](/guide/upload-modes) |

## 配置

| 术语 | 解释 | 相关文档 |
| --- | --- | --- |
| `config.toml` | 启动配置。决定监听地址、数据库、日志、WebDAV 前缀、节点模式等。 | [配置总览](/config/) |
| 系统设置 | 后台热改的全站规则。包括公开站点地址、注册、Cookie、邮件、回收站、WOPI、审计等。 | [系统设置](/config/runtime) |
| 公开站点地址 | AsterDrive 对外可访问的 HTTP(S) 来源，用于分享、邮件、WebDAV、WOPI 回调等。 | [系统设置](/config/runtime) |
| CORS | 浏览器跨源访问规则。只有明确跨源调用 API 时才需要放行。 | [系统设置](/config/runtime) |
| 反向代理 | Caddy、Nginx、Traefik 等公网入口，负责 HTTPS、域名、上传大小、WebDAV 方法透传。 | [反向代理](/deployment/reverse-proxy) |

## 访问协议与外部服务

| 术语 | 解释 | 相关文档 |
| --- | --- | --- |
| WebDAV | 让 Finder、Windows、rclone 或同步工具以文件协议方式访问 AsterDrive。 | [WebDAV](/config/webdav) |
| WOPI | Office 在线预览/编辑协议。AsterDrive 提供文件接口，OnlyOffice / Collabora 等服务负责打开文件。 | [文件编辑](/guide/editing) |
| 预览应用 | 后台配置的文件打开方式，可以是内置预览、外部 URL 模板或 WOPI 应用。 | [系统设置](/config/runtime) |
| 审计日志 | 记录关键用户和管理员动作，用于排查、追踪和日常检查。 | [管理后台](/guide/admin-console) |

## 运维

| 术语 | 解释 | 相关文档 |
| --- | --- | --- |
| `doctor` | 运维 CLI 子命令，用于部署检查、配置检查和深度一致性检查。 | [运维 CLI](/deployment/ops-cli) |
| 数据库迁移 | 把业务数据从一个数据库后端迁到另一个后端，比如 SQLite 到 PostgreSQL。 | [运维 CLI](/deployment/ops-cli) |
| 前端资源缓存 | 浏览器或代理缓存旧页面资源导致升级后页面异常的常见原因。 | [前端资源缓存](/deployment/frontend-assets) |
| 回滚 | 升级失败后退回旧版本，通常需要旧二进制/镜像和升级前备份配合。 | [升级与版本迁移](/deployment/upgrade) |
