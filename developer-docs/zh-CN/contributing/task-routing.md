# 开发任务路由

这张路由表用于把常见任务快速映射到权威文档、代码入口和最低验证，减少每次从全仓库重新侦察。它给出的是起点，不替代沿调用链阅读当前实现。

开始前先读[项目契约](../architecture/project-contract.md)，执行阶段遵循[工程工作流](./engineering-workflow.md)。

## 通用入口

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| 普通 REST API / DTO | [架构概览](../architecture/index.md)、[后端服务所有权](../architecture/backend-service-ownership.md) | `src/api/routes/`、`src/api/dto/`、对应 `src/services/` | focused route/service tests；schema 变化时导出 OpenAPI |
| Service 或领域规则 | [后端服务所有权](../architecture/backend-service-ownership.md)、对应 `design/` | `src/services/<domain>/`、相邻测试 | domain unit tests、相关 integration target、`cargo check` |
| Repository / SQL / migration | [测试与数据库后端](../testing/index.md) | `src/db/repository/`、`crates/aster_drive_model/`、`crates/aster_drive_migration/` | SQLite focused tests；有跨库语义时补 PostgreSQL/MySQL |
| 静态或运行时配置 | [架构概览](../architecture/index.md) | `src/config/`、runtime startup、admin config API/frontend | 默认值、normalize、读写和权限测试；必要时 OpenAPI/frontend tests |
| AsterForge 接入或抽取 | [项目契约](../architecture/project-contract.md) | 当前 Drive adapter、对应 Forge crate/API | 先写“旧模块 -> Forge API -> Drive 保留职责 -> 必测行为”；相关 crate/Drive 编译和集成测试 |
| Code review fixes | [工程工作流](./engineering-workflow.md) | review 引用路径、当前 diff、相邻测试 | 逐条验证真伪；每批 focused compile/test |
| 文档修改 | [文档贡献指南](./documentation.md) | `developer-docs/` 或 `docs/` | 开发者文档跑 `developer-docs:build`；用户文档跑 `docs:build` |

## 文件、工作空间与上传

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| 文件/目录 CRUD、移动、复制、删除、恢复 | [关键模块设计](../architecture/module-designs.md)、[后端服务所有权](../architecture/backend-service-ownership.md) | `src/api/routes/files/`、`src/api/routes/folders.rs`、`src/services/files/`、`src/services/workspace/` | 成功、权限、scope、锁、冲突、事务回滚和 storage side effect |
| 团队与 personal workspace 共用行为 | [架构概览](../architecture/index.md) | `src/services/workspace/scope/`、`storage/`、`storage_core/` | personal/team 双路径、权限和 quota 边界 |
| 上传 init/chunk/complete/cancel | [上传完成契约](../design/upload-finalization-contracts.md)、[关键模块设计](../architecture/module-designs.md) | `src/services/files/upload/`、workspace storage、storage drivers | 各协商模式、失败清理、重试/取消、actual size/hash、quota/version/blob 一致性 |
| 文件锁和结构性 mutation | [资源锁系统](../design/resource-lock-system.md) | `src/services/files/lock/`、workspace storage、`src/webdav/` | owner/token/namespace、父子资源、过期、并发、mutation 和缓存投影边界 |
| 分享、公开下载和预览 | API 对应页面、[项目契约](../architecture/project-contract.md) | `src/api/routes/share_public.rs`、share/preview services | 权限、密码、过期、撤销、下载计数、Range/cache header 和私有资源泄露边界 |

## 存储与远端节点

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| Storage connector / descriptor / 表单字段 | [Descriptor 规范化](../design/storage-descriptor-normalization-contract.md)、[后端服务所有权](../architecture/backend-service-ownership.md) | `crates/aster_drive_storage/`、`src/storage/connectors/`、admin storage policy frontend | validation、descriptor、payload、连接测试、OpenAPI 和前端 focused tests |
| Storage driver / SDK 行为 | [项目契约](../architecture/project-contract.md) | `src/storage/drivers/`、connector runtime construction | 普通请求、错误映射、range、upload/download/delete；presigned 与普通请求分别证明 |
| 对象命名和 OneDrive | [对象命名与 OneDrive](../design/storage-object-naming-and-onedrive-direct-download.md) | storage object key、OneDrive driver/connector | 编码、特殊字符、直链、缓存和回退边界 |
| 远端节点 / storage target / policy ownership | [远端存储目标归属](../design/remote-storage-target-policy-ownership.md) | `src/services/remote/`、storage policy、remote protocol | direct/reverse tunnel/auto、binding、capability、target selection 和失败映射 |
| Internal storage / reverse tunnel wire contract | [内部存储 API](../api/internal-storage.md)、[架构概览](../architecture/index.md) | `src/api/routes/internal_storage.rs`、`remote_tunnel.rs`、`src/storage/remote_protocol/` | 签名、版本兼容、path encoding、超时/取消、流式响应和 transport fallback |

## 协议、安全与运行时

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| WebDAV | [WebDAV API](../api/webdav.md)、[合规测试](../testing/webdav-compliance-testing.md) | `src/webdav/`、相关 files/workspace service | focused unit/integration、协议边界、锁、Range/ETag；需要时 Litmus/真实客户端 |
| WOPI | [WOPI API](../api/wopi.md)、[项目契约](../architecture/project-contract.md) | WOPI route、`src/services/preview/wopi/`、files/workspace service | token、proof、lock、PUT_RELATIVE、rename、版本和错误响应 |
| 认证、MFA、外部认证和 session | [外部认证模块](../design/external-auth.md)、认证 API | auth routes/services、external auth adapter、session repository | 正常/失败、token rotation、browser binding、MFA、限流、凭据泄露边界 |
| 后台任务、runtime 和 shutdown | [架构概览](../architecture/index.md) | `src/runtime/`、task services、Forge adapters | claim/lease、retry、cancellation、shutdown、幂等和多实例边界 |
| 缓存和多实例行为 | [项目契约](../architecture/project-contract.md) | `src/cache/`、对应 authority repository、runtime health | cache miss/fallback/invalidation、权威数据重读、并发和 readiness |

## 前端和生成契约

| 任务 | 先读 | 代码入口 | 最低验证 |
| --- | --- | --- | --- |
| 前端页面、组件或交互 | [`frontend-panel/AGENTS.md`](https://github.com/AsterCommunity/AsterDrive/blob/master/frontend-panel/AGENTS.md) | 相邻 page/component/hook/service/i18n | `bun run check`、focused Vitest；关键用户流程跑 Playwright |
| API schema / generated client | [API 概览](../api/index.md) | Rust DTO/OpenAPI、`frontend-panel/generated/openapi.json`、generated service | OpenAPI export、`bun run generate-api`、检查生成 diff 和前端类型检查 |
| Storage policy 前端 | [Descriptor 规范化](../design/storage-descriptor-normalization-contract.md) | storage policy fields/options/actions | descriptor 驱动字段、payload、连接测试和 transition coverage；不增加 driver 白名单矩阵 |

## 扩大验证的触发条件

出现以下情况时，不停在最低验证：

- 修改公共 trait、AppState/runtime trait、共享 DTO 或 feature gate
- 修改 migration、事务、锁、配额、引用计数或上传完成
- 修改认证、公开访问、WebDAV、WOPI 或 internal storage
- 修改 connector descriptor、driver capability 或生成 API
- 修改跨数据库查询、并发、lease、retry、cancellation 或 shutdown
- focused test 不能覆盖实际用户路径

扩大范围时优先补最相关的集成 target，再考虑 workspace 级检查。最终报告实际运行和未运行的矩阵，不用一条绿色命令替代其他边界。
