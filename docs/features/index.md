---
description: AsterDrive 功能地图，按身份访问、文件工作空间、上传存储、预览处理、系统运维梳理后端能力、管理入口和相关文档。
---

# 功能地图

这一组页面按 AsterDrive 的后端能力组织，不按读者任务组织。

如果你是普通用户，优先看 [用户手册](/guide/user-guide) 和 [常用流程](/guide/core-workflows)。如果你在管理实例、排障、写二开功能，功能地图更适合用来定位“这个能力归哪个模块管、该看哪几页”。

## 功能分区

| 功能域 | 负责什么 | 主要入口 |
| --- | --- | --- |
| [身份与访问](./auth-access) | 登录、会话、MFA、Passkey、外部认证、WebDAV 专用账号、公开访问边界 | [登录与会话](/config/auth)、[外部认证](/config/external-auth)、[WebDAV](/config/webdav) |
| [文件与工作空间](./files-workspaces) | 个人空间、团队空间、文件夹、文件记录、回收站、版本、分享 | [用户手册](/guide/user-guide)、[团队与权限](/guide/teams-and-permissions)、[分享与公开访问](/guide/sharing) |
| [上传与存储](./upload-storage) | 上传模式、Blob、配额、存储策略、策略组、本地 / S3 / Azure Blob / COS / OneDrive / 远程节点 | [上传与大文件](/guide/upload-modes)、[存储策略](/config/storage)、[存储后端总览](/storage/) |
| [预览与处理](./preview-processing) | 缩略图、媒体信息、压缩包预览、WOPI、文件编辑、分享流播放 | [在线预览与 WOPI](/guide/preview-and-wopi)、[文件编辑](/guide/editing)、[系统设置](/config/runtime) |
| [系统与运维](./runtime-operations) | 启动配置、运行时配置、后台任务、邮件、监控、审计、CLI、备份升级 | [配置总览](/config/)、[部署概览](/deployment/)、[运维 CLI](/deployment/ops-cli) |

## 后端模块速查

| 模块 | 功能域 | 说明 |
| --- | --- | --- |
| `auth_service`、`mfa_service`、`passkey_service`、`external_auth_service` | 身份与访问 | 用户登录、安全验证和外部身份绑定 |
| `file`、`folder`、`team_service`、`share`、`files::trash`、`content::version` | 文件与工作空间 | 文件主链路、团队空间、分享、回收站和版本 |
| `upload_service`、`workspace::storage`、`storage_policy::policy`、`storage::*` | 上传与存储 | 上传会话、存储策略选择、Blob 写入和驱动抽象 |
| `thumbnail`、`media::processing`、`media::metadata`、`files::archive::preview`、`preview::wopi` | 预览与处理 | 文件派生结果、在线打开和预览能力 |
| `config_service`、`task_service`、`mail::sender`、`audit_service`、`ops::health`、`readiness_service` | 系统与运维 | 热配置、后台任务、邮件、审计和健康检查 |

## 怎么用这组页面

- 查“用户怎么操作”：回到 [使用指南](/guide/)。
- 查“管理员在哪里配置”：看 [管理后台](/guide/admin-console) 和 [配置总览](/config/)。
- 查“这个功能由哪些后端模块承接”：从当前功能地图进入对应功能域。
- 查“部署和事故处理”：看 [部署运维](/deployment/) 和 [故障排查](/deployment/troubleshooting)。
