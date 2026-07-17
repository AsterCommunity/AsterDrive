---
description: AsterDrive 文件与工作空间功能地图，覆盖个人空间、团队空间、文件夹、文件记录、回收站、版本和分享。
---

# 文件与工作空间

文件与工作空间是 AsterDrive 的主业务链路。它把“用户看到的文件树”和“数据库里的文件记录、Blob、权限、配额、分享状态”连接起来。

## 能力边界

| 能力 | 说明 | 相关文档 |
| --- | --- | --- |
| 个人空间 | 用户自己的文件、文件夹、回收站、任务和配额 | [用户手册](/guide/user-guide) |
| 团队空间 | 团队文件归属、团队成员角色、团队归档和团队审计 | [团队与权限](/guide/teams-and-permissions) |
| 文件夹树 | 文件夹创建、重命名、移动、复制、递归删除和路径构建 | [常用流程](/guide/core-workflows) |
| 文件记录 | 文件名冲突、版本、Blob 引用、锁状态和属性 | [文件编辑](/guide/editing)、[架构概览](/reference/architecture) |
| 回收站 | 删除、恢复、彻底删除和周期清理 | [用户手册](/guide/user-guide)、[系统设置](/config/runtime) |
| 分享 | 文件/文件夹分享、密码、过期时间、下载次数和分享范围校验 | [分享与公开访问](/guide/sharing) |
| 批量操作 | 批量移动、复制、删除和跨工作空间边界校验 | [用户手册](/guide/user-guide) |

## 后端模块

| 模块 | 负责内容 |
| --- | --- |
| `workspace::scope`、`workspace::models` | 个人空间和团队空间作用域 |
| `file`、`folder` | 文件、文件夹、路径、列表和权限校验 |
| `workspace::storage_core`、`workspace::storage` | 文件记录、Blob、配额和策略落账 |
| `workspace::team` | 团队、成员、角色和归档 |
| `share`、`share_public` routes | 分享创建、公开访问和分享范围 |
| `files::trash`、`content::version`、`files::lock` | 回收站、版本和文件锁 |
| `content::property` | 文件/文件夹扩展属性 |

## 数据边界

- 文件内容不直接存数据库；数据库记录文件、文件夹、Blob、版本、分享和权限关系。
- 个人空间和团队空间共用文件主链路，但工作空间作用域不同。
- 团队空间文件归属团队，不按操作者个人配额落账。
- 分享范围必须回到文件夹树和文件归属校验，不能只靠分享 ID。

## 排障方向

- 文件列表不对：先确认当前工作空间，再看文件夹是否在回收站或团队是否已归档。
- 容量显示异常：看 [运维 CLI](/deployment/ops-cli) 的 `doctor --deep`。
- 分享链接访问异常：看分享是否过期、密码是否正确、文件是否被删除或移出分享范围。
- WebDAV 操作和网页不一致：同时看 WebDAV 账号范围、文件锁和客户端系统文件拦截规则。
