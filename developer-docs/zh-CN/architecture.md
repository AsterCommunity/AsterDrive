# AsterDrive 架构概览

本文描述的是当前仓库已经落地的实现，不是早期设计草图。

如果你刚接手这个仓库，建议先看这页，再看 [`module-designs.md`](./module-designs.md)。

## 给新开发者的 60 秒版本

- AsterDrive 现在不是“只有一个运行模式的单体服务”，而是同一套代码支持两种节点模式：
  - `primary`：对外提供主 REST API、公开分享、WebDAV、前端页面，并负责运行时配置和后台任务
  - `follower`：只暴露健康检查和内部对象存储协议，给远端主节点当受管存储节点
- 元数据主要在数据库里，文件内容主要在存储驱动里；两者通过 `files`、`file_blobs`、`file_versions`、`upload_sessions` 等表关联
- 个人空间和团队空间共用同一条文件主链路，只是在 route / service 层通过 `WorkspaceStorageScope` 切换作用域
- 后端主线仍然是：
  `src/api/routes/*` -> `src/services/*` -> `src/db/repository/*` / `src/storage/*`
- WebDAV 不是普通 REST 路由的一个分支，而是独立挂载在 `src/webdav/`
- 运行二进制默认启动 HTTP 服务；启用默认 `cli` feature 时，同一入口还提供 `doctor`、`config`、`database-migrate`、`node enroll` 等运维子命令
- 前端代码在 `frontend-panel/`，生产产物由 primary 节点直接服务
- 配置分两层：
  - 静态配置：`data/config.toml` + `ASTER__...` 环境变量
  - 运行时配置：数据库 `system_config`，产品定义单一数据源是 `src/config/definitions.rs`；共享类型、registry、DB store 和进程内 snapshot 由 `aster_forge_config` / `aster_forge_db` 提供

## 先看哪里

| 你想回答的问题 | 先看哪里 | 为什么 |
| --- | --- | --- |
| 服务怎么启动、怎么区分 primary / follower | `src/main.rs`、`src/config/node_mode.rs`、`src/runtime/startup/` | 这里决定启动模式、运行时状态和节点职责 |
| 运维 CLI 怎么执行 | `src/main.rs`、`src/cli/**` | `cli` feature 下的子命令在进入 HTTP 启动前分派 |
| 主节点挂了哪些路由 | `src/api/primary.rs`、`src/api/routes/` | 这里决定 `/api/v1`、`/health`、`/d`、`/pv`、WebDAV 和前端兜底的注册顺序 |
| 从节点到底暴露什么 | `src/api/follower.rs`、`src/api/routes/internal_storage.rs` | follower 只负责内部存储协议和健康检查 |
| 远端节点反向隧道怎么走 | `src/api/routes/remote_tunnel.rs`、`src/storage/remote_protocol/tunnel/` | primary 暴露 tunnel 控制面，follower 主动连回 primary |
| 一个 REST 接口怎么实现 | 对应 `src/api/routes/**` 文件 | route 层做参数解析、鉴权包装和响应适配 |
| 文件 / 团队 / 分享 / 上传的业务规则在哪 | `src/services/**` | 业务语义集中在 service 层，不应散落在 route 里 |
| 数据怎么查怎么写 | `src/db/repository/**` | repo 层封装数据库访问和跨库兼容细节 |
| 文件内容怎么落盘 / 上对象存储 / 走 OneDrive 或远端节点 | `src/storage/**` | connector descriptor、驱动抽象、具体驱动和远端协议都在这里 |
| WebDAV 为什么和 REST 不一样 | `src/webdav/**` | 这是单独的协议接入层 |
| 团队空间为什么复用个人空间语义 | `src/services/workspace/scope/`、`src/services/workspace/storage/`、`src/services/workspace/models.rs`、`src/services/workspace/storage_core/`、`src/services/files/folder/`、`src/services/files/file/` | scope 切换、上传编排和统一存储核心链路都在这里 |
| 表结构怎么演进 | `migration/`、`src/entities/**` | migration 和 entity 必须一起看 |

追一个具体功能时，最省时间的路径通常是：

1. 先从对应 `src/api/routes/**` 找入口
2. 再跳到 `src/services/**`
3. 最后看 `src/db/repository/**`、`src/storage/**` 或 `src/webdav/**`

## 运行模式与系统边界

### Primary 节点

primary 会注册这些入口：

- REST API：`/api/v1/*`
- 远端节点反向隧道内部接口：`/api/v1/internal/remote-tunnel/*`
- 健康检查：`/health*`
- 公开分享与直链：
  - `/api/v1/s/{token}*`
  - `/d/{token}/{filename}`
  - `/pv/{token}/{filename}`
- WebDAV：默认 `/webdav`
- 前端页面与静态资源：由 `src/api/routes/frontend.rs` 兜底
- 开发态 OpenAPI：
  - `/swagger-ui`
  - `/api-docs/openapi.json`

### Follower 节点

follower 不提供普通用户 API、WebDAV 或前端页面，只注册：

- 健康检查：`/health*`
- 内部对象存储协议：`/api/v1/internal/storage/*`

这条内部协议当前用于主节点和受管远端节点之间的对象写入、对象拼接、对象列举、绑定同步与受管 ingress profile 控制面。

如果远端节点使用 `reverse_tunnel` 或 `auto` 且没有可直连的 `base_url`，follower 不会额外暴露 primary 可直连的入口，而是由 follower 进程里的 tunnel worker 主动连接 primary 的 `/api/v1/internal/remote-tunnel/*`。

## 一个请求如何流转

### Primary 上的普通 REST 请求

1. `src/main.rs` 进入 `run_primary_http_server()`
2. `src/api/primary.rs` 注册 `/api/v1` 下的各模块路由
3. 请求先经过全局中间件：
   - 压缩
   - Request ID
   - 运行时 CORS
   - 安全响应头
4. 命中对应 `src/api/routes/**` handler
5. 受保护接口再经过路由级 JWT 鉴权和限流
6. `src/services/**` 执行业务规则
7. `src/db/repository/**` 负责数据库读写；涉及二进制内容时进入 `src/storage/**`
8. route 层返回统一 JSON，或者直接返回文件流 / SSE / WebDAV / Prometheus 文本响应

例外要记住：

- `/d/...`、`/pv/...`、文件下载、缩略图、分享下载不走统一 JSON 包装
- `GET /api/v1/auth/events/storage` 是 SSE
- `GET /health/metrics` 是 Prometheus text exposition
- 前端兜底路由最后注册，所以 API / WebDAV 必须先于它注册

### Follower 上的内部存储请求

1. `src/api/follower.rs` 只注册 `/api/v1/internal/storage/*`
2. `src/api/routes/internal_storage.rs` 校验内部签名或预签名访问
3. `remote::master_binding` 解析主节点绑定关系和 ingress 策略
4. 通过 `driver_registry` 取得实际存储驱动
5. 请求落到本地 / 对象存储 / 远端驱动能力接口

如果你在查远端节点写入问题，不要先去普通 `files` / `upload` 路由里找。

远端节点有两种传输方式：

- `direct`：primary 直接向 follower 的 `/api/v1/internal/storage/*` 发 HTTP 请求。
- `reverse_tunnel`：primary 把内部存储请求登记到 tunnel registry；follower 通过 `/api/v1/internal/remote-tunnel/poll` / `/complete` 或 `/connect` WebSocket 主动取走请求并回传结果。

`auto` 会根据远端节点是否有非空 `base_url` 选择 direct 或 reverse tunnel。

### WebDAV 请求

WebDAV 不走 `src/api/routes/**`，而是：

1. 由 `crate::webdav::configure()` 在 primary 上挂到配置的 prefix
2. 检查运行时开关 `webdav_enabled`
3. 做 WebDAV 专用 Basic Auth 认证
4. 为请求构造带用户上下文的 `AsterDavFs`
5. 使用数据库锁系统和版本能力
6. 进入自研 WebDAV / DeltaV handler

## 分层结构

```text
┌─────────────────────────────────────────────┐
│ 接入层                                      │
│  - React 前端 / 公开分享页                   │
│  - REST API (primary)                       │
│  - Internal Storage API (follower)          │
│  - WebDAV / DeltaV                          │
├─────────────────────────────────────────────┤
│ 应用层                                      │
│  - 路由、DTO、统一响应、错误码               │
│  - JWT / Admin / Rate Limit / CORS 等中间件 │
├─────────────────────────────────────────────┤
│ 业务层                                      │
│  - auth / profile / team / file / folder    │
│  - upload / batch / share / trash / task    │
│  - policy / config / audit / webdav / wopi  │
│  - workspace scope / storage core           │
├─────────────────────────────────────────────┤
│ 基础设施层                                  │
│  - SeaORM + migration                       │
│  - StorageConnector descriptor / action     │
│  - StorageDriver(Local/S3/SFTP/Azure)       │
│  - StorageDriver(Tencent COS/OneDrive)      │
│  - StorageDriver(Remote)                    │
│  - CacheBackend(Memory / Redis)             │
├─────────────────────────────────────────────┤
│ 数据层                                      │
│  - users / teams / team_members             │
│  - folders / files / file_blobs / versions  │
│  - shares / upload_sessions / tasks         │
│  - webdav_accounts / system_config / locks  │
└─────────────────────────────────────────────┘
```

仓库里的实用判断标准仍然是：

- route 层处理 HTTP / 协议适配
- service 层处理业务语义
- repo 层处理数据库读写
- storage 层处理对象内容

## 关键模块

| 模块 | 当前职责 |
| --- | --- |
| `src/main.rs` | 进程入口、选择节点模式，并把产品 runtime component 交给 `AsterRuntime` 执行 |
| `src/runtime/components.rs` | 构造 primary / follower HTTP、后台任务、mail outbox、audit 和 database component，声明 shutdown 依赖图 |
| `src/runtime/startup/common.rs` | 连接数据库、跑 migration、准备默认策略和运行时配置、加载 policy snapshot / driver registry / cache |
| `src/runtime/startup/primary.rs` | 构造 primary 运行时：`RuntimeConfig`、邮件发送器、SSE 广播、分享下载回滚队列和远端协议运行时 |
| `src/runtime/startup/follower.rs` | 构造 follower 运行时：只保留 follower 需要的共享状态 |
| `src/runtime/tasks.rs` | primary 周期任务注册和关闭；metrics 系统指标任务通过 `MetricsRecorder` 注入 |
| `src/metrics_core.rs` | 始终编译的指标记录 trait 与 `NoopMetrics`，业务层只依赖这层 |
| `src/metrics.rs` | Prometheus 具体实现，仅 `metrics` feature 启用时编译 |
| `src/api/primary.rs` | primary 路由注册 |
| `src/api/follower.rs` | follower 路由注册 |
| `src/api/routes/auth/mod.rs` | 认证、会话、偏好、头像、SSE |
| `src/api/routes/files/` | 文件读写、上传、缩略图、版本、WOPI 启动 |
| `src/api/routes/folders.rs` | 文件夹接口和团队空间聚合入口；团队 `files` 路由挂在这里 |
| `src/api/routes/tags.rs` | 个人和团队工作空间标签库、实体标签绑定与批量标签操作 |
| `src/api/routes/admin/` | 管理后台接口，包括策略、远端节点、用户、团队、分享审计、后台任务、存储迁移、文件 / Blob 可观测、配置、锁、审计 |
| `src/api/routes/share_public.rs` | 公开分享页 API、`/d` 直链、`/pv` 预览直链 |
| `src/api/routes/internal_storage.rs` | follower 内部对象存储协议 |
| `src/api/routes/remote_tunnel.rs` | primary 侧远端节点 reverse tunnel 内部入口 |
| `src/services/` | 业务规则集中层 |
| `src/storage/connectors/` | 存储 connector：descriptor、字段、action、连接测试、上传工作流和凭据需求 |
| `src/storage/drivers/` | 本地、S3-compatible、SFTP、Azure Blob、Tencent COS、OneDrive 和远端驱动 |
| `src/storage/remote_protocol/tunnel/` | reverse tunnel 传输运行时、鉴权、注册表和流式响应 |
| `src/webdav/` | WebDAV 文件系统、认证、锁与 DeltaV 支持 |
| `frontend-panel/` | React 19 + Vite 前端，构建产物由后端服务 |

## 启动流程

### 通用启动步骤

`src/main.rs` 当前的大致顺序是：

1. 安装 panic hook
2. 加载 `.env`
3. 如果启用了 `cli` feature 且传入了 CLI 子命令，先执行对应命令并直接退出
4. 初始化静态配置
5. 初始化日志
6. 清理 runtime 临时目录
7. 根据 `config.server.start_mode` 选择 `primary` 或 `follower`
8. 用 `aster_forge_runtime::AsterRuntime` 组装 HTTP、后台任务、mail outbox、audit 和 database component；启动审计作为 required startup phase 执行

优雅关闭不再由 `main.rs` 手写调用顺序。`src/runtime/components.rs` 声明的依赖图是：

primary 的依赖图是：

```text
background_tasks
mail_outbox    -> depends_on background_tasks
audit_logs     -> depends_on mail_outbox
audit_manager  -> depends_on audit_logs
database       -> depends_on audit_manager
```

follower 没有邮件发送器，因此不会注册假的 mail component；它保持 `audit_logs -> background_tasks`。关闭 primary 时会先停止后台任务，再 drain mail outbox、记录 `server_shutdown`、flush audit buffer，最后关闭全部 reader / writer DB pool。注册顺序不决定关闭顺序，component graph 会在服务启动前完成校验。

邮件 outbox 的共享边界也已收敛到 Forge：

- `MailTemplateCode`、`StoredMailPayload`、`MailOutboxStatus`、dispatch stats 和 retry policy 使用 `aster_forge_mail`。
- `mail_outbox` SeaORM entity、enqueue、claim/retry/sent/failed 状态机和 active count 使用 `aster_forge_db`。
- shutdown drain component 使用 `aster_forge_mail::mail_outbox_component`。
- Drive 只保留模板 payload、模板渲染、SMTP 适配和发送审计 hook。
- 历史 baseline 不回改；后续 migration 将 `template_code` 和 Forge schema contract 对齐，并使用 Forge table/index builder 重建 SQLite 表。

运行时配置的共享边界同样以 Forge 为准：

- `ConfigValueType`、`ConfigSource`、`ConfigVisibility`、`ConfigValue` 使用 `aster_forge_config`。
- `system_config` SeaORM entity 和通用 CRUD/default seed 使用 `aster_forge_db::system_config`。
- `RuntimeConfig` 的进程内快照使用 `aster_forge_config::SyncRuntimeConfig`。
- Drive 保留具体 key、默认值、跨字段校验、媒体处理环境引导、preview registry 修复、权限、audit 和 API 响应结构。
- 已发布的历史 migration 不回改；新的 Drive migration 使用 Forge table/index builder 对齐共享 schema contract。

Prometheus 指标不在 `main.rs` 直接初始化，而是在 `prepare_common()` 中创建 `MetricsRecorder`：

- 启用 `metrics` feature 时初始化 Prometheus registry，并注入 Prometheus recorder
- 未启用时注入 `NoopMetrics`
- 业务层、HTTP middleware、存储驱动 wrapper 和后台任务只依赖 `src/metrics_core.rs` 的 trait，不直接依赖 Prometheus

### `prepare_common()`

`src/runtime/startup/common.rs` 会做所有节点共享的准备：

1. 创建 `MetricsRecorder`，让数据库连接和后续运行时状态都能共享同一个 recorder
2. 连接数据库
3. 执行全部 migration
4. 准备 SQLite 搜索加速能力（若当前后端适用）
5. 确保至少存在一个默认本地存储策略
6. 仅 primary 模式下补种默认策略组
7. 初始化 `auth_cookie_secure` 引导值
8. 写入 `system_config` 默认值
9. 清理废弃的 `node_runtime_mode` 和旧 thumbnail 运行时配置键
10. 重载 `PolicySnapshot`
11. 根据节点模式重载 `DriverRegistry`
12. 初始化缓存后端

### 数据库连接句柄

运行态通过 `DbHandles` 同时保存 writer 和 reader：

- `state.db` / `state.writer_db()` 是 writer。所有事务、写入、读后写、配额权威判断、登录签发 session、refresh token rotation、上传 init/chunk/complete/cancel、依赖 SQLite 单连接模拟锁语义的 repo helper，都必须继续走 writer。
- `state.reader_db()` 是纯读入口。SQLite 文件数据库下它会在 writer 完成 migration 和默认数据初始化后打开独立 reader pool，使用 WAL、`mode=ro` 和 `PRAGMA query_only=ON`；PostgreSQL/MySQL 或内存 SQLite 下它和 writer 指向同一个池。
- reader 查询允许 WAL 快照级别的短暂滞后。只能用于列表、详情、搜索、上传进度、recoverable sessions、presign 查询阶段、auth snapshot cache miss、public runtime snapshot、admin overview 统计这类不会马上做权威写入判断的路径。
- 不要把通用校验 helper 偷偷改成 reader，除非已经确认所有调用方都是纯读。更推荐在 service 入口显式选择 `reader_db()` 或 `writer_db()`，让调用语义能从代码上看出来。

### Primary 特有启动

`src/runtime/startup/primary.rs` 额外准备：

- `RuntimeConfig`
- 运行时邮件发送器
- 存储变更广播通道
- 分享下载回滚队列
- `RemoteProtocolRuntime`，包括 reverse tunnel registry，并注入到 `DriverRegistry`

随后 `src/api/primary.rs` 注册主路由，并在 `src/runtime/tasks.rs` 启动 primary 周期任务。

### Follower 特有启动

`src/runtime/startup/follower.rs` 只保留 follower 需要的共享状态。

随后 `src/api/follower.rs` 仅注册：

- `/api/v1/internal/storage/*`
- `/health*`

`spawn_follower_background_tasks(state)` 当前启动 follower-safe 的通用指标后台任务，并启动 reverse tunnel follower worker；它不会启动 primary 的业务清理任务，也不会启动 `background-task-dispatch`。

## 后台任务

primary 后台工作由 `src/runtime/tasks.rs` 注册，分成一个常驻 worker 和一组周期任务：

- 常驻 worker：`share-download-rollback`
- 周期任务：
  - `mail-outbox-dispatch`
  - `background-task-dispatch`
  - `upload-cleanup`
  - `completed-upload-cleanup`
  - `blob-reconcile`
  - `system-health-check`（包含数据库、缓存和远端节点健康检查）
  - `trash-cleanup`
  - `team-archive-cleanup`
  - `lock-cleanup`
  - `auth-session-cleanup`
  - `external-auth-flow-cleanup`
  - `mfa-flow-cleanup`（MFA 登录 flow、TOTP setup flow 和邮箱验证码）
  - `audit-cleanup`
  - `task-cleanup`
  - `wopi-session-cleanup`

周期任务按运行时配置里的间隔执行。它们只有在有实际结果或失败时才写 `SystemRuntime` 任务记录；空轮询使用 `RuntimeTaskRunOutcome::quiet()` 不灌历史表。`system-health-check` 在连续健康成功时会刷新最近一条成功记录，而不是每轮新增一条噪音记录。

用户可见的 `background_tasks` 记录由 `background-task-dispatch` 派发。当前 dispatcher 按任务类型分四条 lane：

- `Archive`：`archive_compress`、`archive_extract`、`archive_preview_generate`
- `Thumbnail`：`thumbnail_generate`、`image_preview_generate`、`media_metadata_extract`
- `StorageMigration`：`storage_policy_migration`
- `Fallback`：`storage_policy_temp_cleanup`、`trash_purge_all`、`blob_maintenance`、`system_runtime`

前三条 lane 分别有自己的运行时并发配置；Fallback 使用通用 `background_task_max_concurrency`。

dispatcher 认领任务后会为业务执行创建 `TaskExecutionContext`。它同时携带 processing-token lease 和 graceful-shutdown token；这个 token 由 `AsterRuntime` 持有，并通过 HTTP / background-task component 注入 HTTP server、SSE 和后台任务，所以 SIGINT / SIGTERM 到来时，几条链路会一起开始收尾。任务代码、下载轮询、压缩 / 解压的阻塞 worker 都应该通过这个 context 做活跃检查；只有进度写库、runtime metadata 写库和最终状态写库这类底层 helper 直接使用 `TaskLeaseGuard`。服务关闭时，context 会让执行流协作退出，dispatcher 再把仍匹配当前 processing token 的任务释放回 `Retry`，不消耗重试次数。

## CLI 与离线运维入口

`Cargo.toml` 里默认 feature 包含 `cli`，所以默认构建出来的 `aster_drive` 既能直接启动服务，也能执行离线运维子命令。`src/main.rs` 会在 HTTP 服务启动前先解析这些子命令：

| 子命令 | 代码入口 | 当前职责 |
| --- | --- | --- |
| `serve` 或无子命令 | `src/main.rs` | 启动 primary / follower HTTP 服务 |
| `doctor` | `src/cli/doctor.rs`、`src/cli/doctor/**` | 数据库、migration、运行时配置、存储策略和深度一致性审计 |
| `config` | `src/cli/config.rs` | 离线读取、设置、导入、导出、校验 `system_config` |
| `database-migrate` | `src/cli/database_migration.rs`、`src/cli/database_migration/**` | 跨数据库后端迁移，支持 dry-run、verify-only 和断点续传 |
| `node enroll` | `src/cli/node.rs` | follower 用主节点签发的 enrollment token 写入本地 master binding |

这些 CLI 通常直接连接数据库，不经过 HTTP route 层。改这类能力时先看 `src/cli/**` 和对应 service，而不是去 `src/api/routes/**` 里找。

## 配置分层

### 静态配置

静态配置来自：

- `data/config.toml`
- 环境变量 `ASTER__...`

主要控制：

- 监听地址、端口、worker 数
- 节点启动模式
- 数据库连接
- WebDAV 前缀
- 缓存和日志
- follower 受管 local ingress profile 根目录：`server.follower.remote_storage_target_local_root`，默认 `remote-storage-targets`

首次启动会自动创建 `data/config.toml`。配置文件里的相对路径默认相对于 `data/` 解析；兼容旧值时，已经写成 `data/...` 的相对路径会避免二次拼出 `data/data/...`。根目录下的旧 `config.toml` 不再是默认读取位置。

### 运行时配置

运行时配置保存在数据库 `system_config`，由管理员接口热更新。

单一数据源在 `src/config/definitions.rs`，常见键包括：

- `webdav_enabled`
- `webdav_block_system_files_enabled`
- `webdav_block_system_file_patterns`
- `default_storage_quota`
- `trash_retention_days`
- `team_archive_retention_days`
- `max_versions_per_file`
- `auth_cookie_secure`
- `auth_*_ttl_secs`
- `auth_email_code_login_*`
- `public_site_url`
- `cors_*`
- `mail_outbox_dispatch_interval_secs`
- `background_task_dispatch_interval_secs`
- `background_task_dispatch_idle_max_interval_secs`
- `background_task_max_concurrency`
- `background_task_archive_max_concurrency`
- `background_task_thumbnail_max_concurrency`
- `background_task_storage_migration_max_concurrency`
- `background_task_max_attempts`
- `share_download_rollback_queue_capacity`
- `share_stream_session_ttl_secs`
- `maintenance_cleanup_interval_secs`
- `blob_reconcile_interval_secs`
- `remote_node_health_test_interval_secs`
- `task_retention_hours`
- `archive_extract_*`
- `archive_build_*`
- `archive_preview_*`
- `archive_extract_max_staging_bytes`
- `thumbnail_max_source_bytes`
- `thumbnail_max_dimension`
- `image_preview_max_dimension`
- `media_metadata_enabled`
- `media_metadata_max_source_bytes`
- `media_processing_registry_json`
- `wopi_*`

`system_config.category` 只使用 `src/config/definitions.rs` 里登记的分区常量。当前分区口径是：

- `site` / `site.preview`：站点公开入口、品牌和预览应用
- `user.registration_and_login` / `user.avatar`：注册登录和头像
- `auth`：认证 Cookie 和 token TTL
- `mail.config` / `mail.template`：发信配置和邮件模板
- `network`：CORS 等网络访问规则
- `runtime.mail` / `runtime.background_task` / `runtime.maintenance` / `runtime.limits` / `runtime.share_stream`：运行时派发、维护和限制
- `storage`：版本、回收站、团队归档和默认配额等存储保留策略
- `file_processing.archive_extract` / `file_processing.archive_preview` / `file_processing.archive_build` / `file_processing.media`：压缩包和媒体处理
- `webdav` / `audit`：WebDAV 和审计日志

新增分区时必须同步更新允许列表和前端 zh/en i18n。`ALL_CONFIGS` 的单元测试会拒绝未登记分区，也会检查二级分区是否有前端标题和描述文案。

`public_site_url` 是一个历史上保持单数 key 的列表配置。配置类型是 `string_array`，管理 API 暴露为字符串数组，数据库值保存为规范化后的 JSON 数组字符串。生成绝对 URL 时，有请求上下文的路径会优先用当前请求 scheme/Host 在列表里做精确匹配；没有请求上下文或未命中时使用第一项作为回退。这个配置也参与 Cookie 认证写操作的 same-site CSRF 来源判断，但不参与 CORS 放行。

## 改动应该落在哪一层

| 你要改的东西 | 优先落点 |
| --- | --- |
| 新增主节点 REST 接口 | `src/api/routes/**` |
| 新增 follower 内部协议能力 | `src/api/routes/internal_storage.rs`、`src/storage/remote_protocol.rs` |
| 权限、配额、锁、版本、分享范围、团队语义 | `src/services/**` |
| 新增查询、分页、过滤条件 | `src/db/repository/**` |
| 存储 connector descriptor、连接测试、驱动 action、上传策略和对象读写规则 | `src/storage/**` |
| WebDAV 协议行为 | `src/webdav/**` |
| 表字段、索引、默认值 | `migration/` + `src/entities/**` |
| 前端页面、状态管理、SDK 调用 | `frontend-panel/src/**` |

如果你发现复杂业务判断写在 route 层，基本就是代码气味。

## 继续阅读

- [`module-designs.md`](./module-designs.md)
- [`api/index.md`](./api/index.md)
- [`testing.md`](./testing.md)
