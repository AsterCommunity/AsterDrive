# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [v0.4.0-beta.2] - 2026-07-16

### Release Highlights

**AsterDrive `v0.4.0-beta.2` 是 `0.4.0` 系列的第二个 beta，主线是认证与访问链路的安全加固，以及公开分享导航体验的完善。** 外部认证（OAuth2 / OIDC）登录流程绑定浏览器 session（HttpOnly cookie + SHA-256 绑定哈希），从协议层阻断 CSRF 与 session fixation；远程节点注册 token 收紧为一次性原子兑换，重放不再泄露 bootstrap 凭据；WebDAV Basic 认证加入分层限流（per-IP + 用户名退避）与统一错误边界，不再暴露凭据、归属、禁用、团队访问等失败细节；首次启动在完成系统初始化前禁止普通注册，并通过数据库 initialization lock 保证只有一个初始管理员。与此同时，公开分享落地路由化的子文件夹导航、侧边栏目录树与已登录会话探测；workspace 路由合并为单一组件，切换工作区时保留当前视图。

- **外部认证 browser binding** — OAuth2 / OIDC 登录流程绑定 HttpOnly cookie，SHA-256 校验，拒绝未绑定的 legacy flow
- **注册 token 一次性兑换** — 远程节点注册 token 原子 claim，重放被拒，bootstrap 凭据不泄露
- **WebDAV 认证限流与凭据边界** — Basic auth 分层限流（per-IP + 用户名退避）+ 统一 401，隐藏失败细节
- **强制 setup 才能注册** — 系统初始化前禁止普通注册，数据库锁保证唯一初始管理员
- **公开分享导航** — 路由化子文件夹导航、侧边栏目录树、公开页已登录会话探测
- **workspace 路由统一** — 合并 personal / team 路由，切换工作区保留当前视图

### Added

- **外部认证 browser binding（安全）**
  - `external_auth_login_flows` 新增 `browser_binding_hash` 列（32 字节随机 secret 的 SHA-256）
  - OAuth / OIDC 流程发起时生成 secret，哈希入库，原文以 HttpOnly cookie 下发
  - 签发 session token 前校验绑定 cookie 与 flow 匹配；成功或失败均清除 cookie
  - 拒绝 `browser_binding_hash` 为 NULL 的 legacy 未绑定 flow
  - 覆盖绑定校验、篡改检测与并行流程测试

- **WebDAV 认证限流与凭据边界**
  - Basic 认证应用 auth-tier 限流，新增 per-IP 限流（信任代理 client IP 解析）与跨轮换 IP 的用户名失败退避
  - 限流时返回带 `Retry-After` 的 WebDAV 兼容 401
  - WebDAV 账号连接测试受 actor ownership 检查保护，复用 auth 限流 tier
  - 凭据 / 归属 / 禁用 / 团队访问失败统一收敛为 auth 错误，不暴露差异

- **首次启动 setup 强制**
  - 完成系统初始化前禁止普通注册，新增 `validation.system_not_initialized` 错误码与前端文案
  - 数据库 initialization lock 保证并发 setup 只创建一个初始管理员，setup 重试干净回滚

- **远程节点注册 token 一次性兑换**
  - 按 token hash 原子 claim 可兑换 token，重放被拒且不暴露 bootstrap 凭据
  - 保留 replaced / completed / expired / redeemed 终态错误；已 redeemed token 在节点 bootstrap 时视为已配置

- **外部认证 provider 创建默认值后端化**
  - provider kind schema 新增 `create_defaults`（display_name / options / scopes / enabled / auto_provision / auto_link_verified_email / require_email_verified）与 Microsoft tenant 默认值
  - 新增 `issuer_url_supported` 布尔位，替代前端多 flag 推断 issuer 字段可见性的逻辑
  - 前端切换 provider 时按后端默认值重建表单，移除 isGitHub / Google / Microsoft / Qq 等前端 provider 专属分支

- **公开分享路由化导航**
  - 新增规范的公开分享子文件夹路由与路由校验，按路由加载子文件夹内容、面包屑、排序与刷新；新增 scoped 祖先查询端点
  - 公开文件夹分享新增侧边栏目录树（懒加载、面包屑水合、请求去重、token 变更取消 stale 响应、分支级重试）
  - 公开页对已登录用户做 optional session probe（`/auth/me`），不强制跳转登录
  - `ShareAccess` extractor（const generic flags）替代手动 cookie 检查，收敛下载次数与归档下载 feature gating

- **workspace 路由统一**
  - `PersonalWorkspaceRoute` 与 `TeamWorkspaceRoute` 合并为单一 `WorkspaceRoute`，共享同一 route element，工作区切换不再 unmount 页面
  - 新增 `workspaceSwitchPath`，跨工作区切换时保留 workspace 无关视图（搜索 / 分享 / 任务 / 回收站 / 分类）的 query 与 hash；team id 校验收紧为严格整数正则

- **PDF 虚拟滚动精度**
  - 并发预加载所有 PDF 页面尺寸（最多 8 worker）后再暴露虚拟滚动条，消除等高估算导致的布局抖动
  - 每页初始尺寸使用真实宽高比；测量高度跨页导航保留，仅旋转时刷新

### Changed

- **admin 日报统计 SystemSetup** — `SystemSetup` 审计 action 计入 admin 日报 new_users 指标
- **测试账号创建 helper 抽取** — `setup_test_account_via_api` / `create_test_account_via_api` / `create_test_account_at_api_endpoint` 复用，替换各测试文件重复注册样板
- **内部：前端工具链迁移 TypeScript 7** — tsgo / native-preview 脚本改为增量 TypeScript 7 tsc 检查，新增 tsconfig incremental build info，刷新前端依赖与 Biome schema（纯工具链，无行为变化）
- **内部：CI / 构建维护** — SeaORM migration 与锁定依赖升级、Rust toolchain 组件、CI 拆分 format / clippy job、新增 OpenAPI 与生成 SDK drift check、收紧 cargo audit 触发并视 warning 为失败、支持显式 `ASTER_BUILD_TIME` 与隔离的 frontend fallback embed（纯维护）

### Fixed

- **音乐播放源刷新丢播放位置** — 重放同一 prepared source 不重载音频；记录已加载 source key 避免冗余加载；刷新后按 metadata 恢复当前播放时间
- **视频预览无法填充全屏** — 全屏展开状态传入视频预览，新增可复用 video frame，移除 aspect-ratio 约束，保留 native fallback 尺寸
- **绝对 HTTP 资源 URL 被误判** — 绝对 HTTP(S) URL 视为浏览器可寻址资源，视频预览初始化保留绝对 stream session URL
- **公开分享导航状态抖动** — 保留 compact dropdown 拖拽目标的面包屑 source index；目录树展开状态移到 toggle 按钮；刷新 / 排序时取消 stale load-more；忽略 stale 导航与文件分享响应；离线时公共 auth probe 保持登出

### Security

- **外部认证 CSRF / session fixation 防护** — OAuth2 / OIDC 登录流程绑定浏览器 session（HttpOnly cookie + SHA-256 哈希），拒绝未绑定的 legacy flow
- **远程节点注册 token 重放** — 原子 claim 强制一次性兑换，重放不泄露 bootstrap 凭据
- **WebDAV 认证探测面收敛** — 分层限流 + 统一 auth 错误边界，隐藏凭据 / 归属 / 禁用 / 团队访问失败差异
- **未授权首注册** — 系统初始化完成前禁止普通注册，数据库锁保证唯一初始管理员

### Database Migrations

- `m20260716_000001_bind_external_auth_login_flows`
  - 为 `external_auth_login_flows` 新增 `browser_binding_hash` 列，支撑外部认证 browser binding
  - 现有未绑定 flow（NULL 哈希）在登录时被拒绝，需重新发起

### Statistics

- 179 files changed, 10296 insertions(+), 2947 deletions(-)
- 14 commits
- 1 个数据库 migration

### Notes

- 本版本是 `0.4.0` 系列第二个 beta，建议先在测试环境验证外部认证登录（OAuth2 / OIDC）、WebDAV Basic 认证限流、远程节点注册与首次 setup 流程
- 1 个数据库 migration 会在启动时自动执行；进行中的 legacy 未绑定 flow 会被拒绝并需要重新发起登录
- 升级后客户端（浏览器）需支持 cookie，外部认证登录依赖 browser binding cookie 完成校验
- WebDAV 客户端在多次失败后会收到带 `Retry-After` 的 401，应遵守退避而非立即重试
- 多实例部署下，一次性注册 token 兑换依赖同一份权威数据库

## [v0.4.0-beta.1] - 2026-07-14

### Release Highlights

**AsterDrive `v0.4.0-beta.1` 是 `0.4.0` 系列的首个 beta，主线是把可复用基础能力迁移到 AsterForge 共享 crates，并补齐多实例运行所需的配置同步、运行时租约与任务调度基础设施。** API、数据库、缓存、审计、邮件、外部认证、验证、通用工具、Actix 中间件、指标和任务运行时等公共能力不再由 AsterDrive 维护平行实现；产品层继续保留文件、工作空间、分享、上传、存储策略、远端节点、WebDAV 与 WOPI 等 AsterDrive 领域编排。与此同时，本版本新增基于 Redis pub/sub 的跨实例运行时配置同步，落地 runtime lease、scheduled task 与后台任务 dedupe contract，并完成计划在 `0.4.0` 移除的旧 ingress profile API 兼容路由清理。CORS 白名单改为字符串数组，并支持 Chrome / Edge、Firefox 和 Safari Web Extension origin。

- **AsterForge 共享能力迁移** — API contract、数据库、缓存、配置、审计、邮件、外部认证、验证、工具、加密、指标、运行时、任务和 Actix middleware 统一使用 `aster_forge_*` crates，删除 AsterDrive 内部重复实现
- **跨实例运行时配置同步** — 新增 `[config_sync]`，可通过 Redis pub/sub 通知其他实例从权威数据库重新加载运行时配置；单实例默认关闭
- **多实例任务运行基础** — 新增 runtime lease、scheduled task 与后台任务 dedupe key，运行时组件通过显式依赖图完成启动和优雅关闭
- **CORS 扩展来源支持** — `cors_allowed_origins` 改为字符串数组，支持 HTTP(S) 与浏览器扩展 origin，旧逗号分隔配置会在启动时迁移
- **旧远端存储兼容路由移除** — 管理 API 统一使用 `/storage-targets` / `/storage-target-drivers`，follower 内部协议统一使用 `/targets`

### Added

- **跨实例运行时配置同步**
  - 新增静态配置组 `[config_sync]`，支持 `disabled` 和 `redis` 两种 backend
  - 管理 API 与 `aster_drive config set` / `delete` / `import` 在数据库写入成功后发布 reload 通知
  - 其他实例收到通知后从 writer database 重新加载完整运行时配置，Redis 只传递通知，不保存配置值
  - 新增配置 reload / mutation 指标与订阅 worker 日志
  - primary、follower 与测试 runtime state 统一接入 `ConfigSyncRuntime`
  - 新增中英文多实例配置同步文档与生产检查项

- **多实例运行时和任务调度基础**
  - 新增 `runtime_leases` 表，用于多实例运行时能力的租约协调
  - 新增 `scheduled_tasks` 表及 name / next-run 索引
  - `background_tasks` 新增可选 `dedupe_key` 与唯一索引，支持任务去重查询
  - task execution context 增加显式 lease renewal timeout
  - runtime component graph 明确 HTTP、background task、mail outbox、audit、database 等组件的关闭依赖关系

- **浏览器扩展 CORS origin**
  - 支持 `chrome-extension://`、`moz-extension://` 和 `safari-web-extension://`
  - 管理后台按数组逐项编辑允许的 origin
  - 新增完整 origin、通配符与 credential 组合校验
  - 新增旧逗号分隔值迁移、浏览器扩展 origin 和前端表单测试

- **构建 fallback 资源**
  - 自动识别并刷新包含 `Frontend Not Built` 标记的旧 fallback 目录
  - fallback 输出补齐 CSS、service worker 与 web manifest
  - fallback 页面结构与正常前端构建产物保持一致

### Changed

- **核心基础能力迁移到 AsterForge**
  - 本地 `api-docs-macros` workspace crate 迁移到 `aster_forge_api_docs_macros`
  - allocator、cache、logging、panic、metrics、Actix observability 和 middleware 使用 Forge 实现
  - `NullablePatch`、分页结构和公共 API error bridge 使用 `aster_forge_api`
  - `DbHandles`、transaction、retry、pagination、sort、search query 和 index migration helper 使用 `aster_forge_db`
  - audit log、system config、mail outbox 使用 Forge 共享数据库 contract 与 runtime component
  - external OAuth2 / OIDC driver registry 使用 `aster_forge_external_auth`
  - filename、email、display text、URL、path、number、ID、HTTP validator、hash 与 crypto helper 使用 Forge 共享实现
  - task execution、lease、step、retry、lane 与 registry contract 使用 `aster_forge_tasks`
  - 删除相应的产品内薄封装、重复类型和转发 facade

- **运行时生命周期组件化**
  - primary / follower startup 使用 `AsterRuntime` component graph 装配运行时能力
  - audit 改用 Forge buffered batch writer
  - mail outbox、background task、audit 与 database 的关闭顺序通过依赖关系表达
  - config loader 返回结构化 `ConfigLoadReport`，由启动入口统一输出，避免 JSON 模式下混入非结构化 stderr

- **CORS 配置改为数组**
  - `cors_allowed_origins` 的运行时配置类型由 `string` 改为 `string_array`
  - 管理 API 与前端按 JSON 字符串数组读写
  - 旧逗号分隔 origin 列表会在启动时规范化为 JSON 数组
  - `["*"]` 允许任意 origin，但仍禁止与跨域 credential 同时启用

- **缓存配置字段统一**
  - `[cache].redis_url` 更名为 `[cache].endpoint`
  - 配置示例、用户文档和测试统一使用 `endpoint`

- **数据库 migration 基础能力**
  - migration crate 使用 `aster_forge_db` 提供的跨数据库 index helper
  - runtime lease 与 scheduled task 表加入数据库复制顺序
  - 修复 system config migration rollback 中 MySQL 不兼容的字符串默认值
  - schema drift 测试覆盖 runtime lease 与 scheduled task 表

- **系统健康与任务错误表达**
  - admin health component 新增结构化 `details`
  - remote node health counter 从 `usize` 改为 `u64`
  - Forge task core error 映射到稳定的产品 API error code，不再统一退化为内部错误

- **依赖升级**
  - `aes-gcm` 0.10 → 0.11
  - AWS SDK、`azure_core`、`rand`、`russh`、`sea-orm`、`criterion`、`tokio-tungstenite` 等依赖升级
  - 测试 RSA key generation 迁移到 `rsa 0.10.0-rc` 与 `rand 0.10`

- **文档与术语同步**
  - “ingress target / 接收落点”统一为 “remote storage target / 远程存储目标”
  - 外部认证开发文档改为 AsterForge shared driver / registry 边界
  - 存储能力列表补充 SFTP
  - 内部协议文档明确当前版本 v5、最低兼容版本 v4
  - `/public/frontend-config` 明确为公开品牌与登录入口配置的当前接口

### Removed

- 移除管理 API 中已废弃的远端存储 ingress profile 兼容路由：
  - `GET /api/v1/admin/remote-nodes/{id}/ingress-profile-drivers`
  - `GET /api/v1/admin/remote-nodes/{id}/ingress-profiles`
  - `POST /api/v1/admin/remote-nodes/{id}/ingress-profiles`
  - `PATCH /api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}`
  - `DELETE /api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}`
- 移除 follower internal storage protocol 中的 `/ingress-profiles` 路由别名，统一使用 `/targets`
- 移除对应 OpenAPI operation 与生成的前端 API client 方法
- 移除 AsterDrive 内部重复维护的 cache、middleware、external-auth driver、mail sender、配置 contract、数据库 helper、通用 utils、allocator 和 API docs macro 实现

### Fixed

- 旧 fallback 前端资源存在时能够被构建脚本识别并重新生成，不再继续使用不完整的 fallback 目录
- CORS wildcard 与 credential 的组合在管理前端和后端配置 normalization 两侧都得到校验
- 首次生成配置文件时，JSON 日志模式不再混入非结构化配置报告
- migration index rename / drop helper 在 MySQL 上保持幂等
- 数据库迁移复制流程包含 `runtime_leases` 与 `scheduled_tasks`
- system config migration rollback 不再生成 MySQL 不兼容的 `DEFAULT ""`

### Database Migrations

- `m20260712_000001_align_forge_audit_contract`
  - 将现有 `audit_logs` 对齐 Forge contract，保留已有审计数据
  - 扩展 IP address 字段并补齐 system user 默认语义
- `m20260712_000002_add_forge_audit_query_indexes`
  - 增加 Forge audit query indexes
- `m20260712_000003_align_forge_system_config_contract`
  - 将 `system_config` 对齐 Forge contract 并保留已有配置
- `m20260712_000004_align_forge_mail_outbox_contract`
  - 将 `mail_outbox` 对齐 Forge contract 并保留已有邮件记录
- `m20260713_000001_runtime_leases`
  - 新增 `runtime_leases`
- `m20260713_000002_background_task_dedupe_key`
  - 为 `background_tasks` 新增 `dedupe_key` 与唯一索引
- `m20260713_000003_scheduled_tasks`
  - 新增 `scheduled_tasks` 及调度索引

### Configuration Changes

- 新增 `[config_sync]`：
  - `backend = "disabled"`：默认值，适合单实例部署
  - `backend = "redis"`：通过 Redis pub/sub 同步运行时配置 reload 通知
  - `endpoint = ""`：Redis URL
  - `topic = "aster_drive.config_reload"`：同一实例组必须保持一致
- `[cache].redis_url` 更名为 `[cache].endpoint`
- `cors_allowed_origins` 从逗号分隔字符串改为 JSON 字符串数组；已有旧值会在启动时自动迁移
- `server.follower.managed_ingress_local_root` 配置别名继续保留兼容；新配置仍应使用 `server.follower.remote_storage_target_local_root`

### Statistics

- 582 files changed, 12840 insertions(+), 23524 deletions(-)
- 22 commits
- 7 个数据库 migration
- 新增静态配置组：1（`config_sync`）
- 移除旧远端存储兼容 API：管理端 5 个路由、follower internal protocol 4 个路由

### Notes

- 本版本是 `0.4.0` 系列首个 beta，建议先在测试环境验证数据库迁移、外部认证、邮件发送、后台任务与远端节点链路
- 7 个数据库 migration 会在启动时自动执行；audit、system config 与 mail outbox migration 会保留已有数据
- 单实例部署无需 Redis，保持 `[config_sync].backend = "disabled"` 即可
- 多实例部署启用 config sync 时，所有实例必须连接同一份权威数据库、同一 Redis 服务并使用相同 topic
- Redis pub/sub 不补发离线期间的消息；实例启动时会从数据库全量加载配置
- 使用旧 `/ingress-profiles` 管理 API 或 internal storage protocol 别名的客户端必须迁移到 `/storage-targets`、`/storage-target-drivers` 和 `/targets`
- `managed_ingress.*` 错误码继续保留历史前缀，用于现有客户端与日志兼容
- 当前 remote internal protocol 为 v5，最低兼容版本仍为 v4

## [v0.3.2] - 2026-07-08

### Release Highlights

**AsterDrive `v0.3.2` 是 `0.3.1` 之后的快速热修复版本，主线是前端公共认证路由的初始化与 token refresh 行为收敛。** 在邀请注册、密码重置、邮箱验证、OIDC callback 等公共页面上，应用启动时不再发起无谓的 bootstrap auth 检查，401 错误时也不再尝试 token refresh，从而减少无效 API 调用并避免把公共流程错误地重定向到登录页；同时补全了多个认证相关页面、回收站页面和错误页的 i18n namespace 加载，分享视图页面补齐 tasks namespace，并新增对应测试覆盖。

- **公共认证路由跳过初始化检查** — `/invite/:token`、`/reset-password` 不再触发 bootstrap auth check
- **公共端点跳过 token refresh** — 密码重置、邮箱验证、OIDC callback、邀请相关端点 401 时不再尝试刷新 token
- **URL 匹配只比较 pathname** — 修复 query/fragment 导致公共端点未被识别的问题
- **i18n namespace 补全** — 强制改密、重置密码、邀请注册、团队详情、回收站、错误页加载正确 namespace；分享视图补充 tasks

### Fixed

- 前端公共认证页面（邀请注册、密码重置）启动时不再执行 bootstrap auth check
- 公共 auth 端点 401 错误时不再尝试 token refresh
- token refresh skip 匹配改用 pathname，避免 query/fragment 干扰
- 补全 ForcePasswordChangePage / ResetPasswordPage / InviteRegisterPage / AdminTeamDetailPage / TrashPage / ErrorPage 的 i18n namespace 加载
- ShareViewPage 增加 tasks namespace

### Statistics

- 6 files changed, 133 insertions(+), 13 deletions(-)
- 1 commit

## [v0.3.1] - 2026-07-08

### Release Highlights

**AsterDrive `v0.3.1` 是 `0.3.0` 系列的架构整理与性能打磨版本，主线是服务层领域模块化重组、WebDAV 性能与 HTTP 条件请求合规性提升、以及上传 finalize 路径契约化。** 服务层从扁平的 `*_service` 文件重组为按领域嵌套的模块树（`auth` / `files` / `share` / `user` / `mail` / `remote` / `workspace` / `preview` / `webdav` / `media` 等），纯路径迁移不改 API 与 schema；WebDAV 落地 RFC 9110 条件请求（`If-Match` / `If-None-Match` / `If-Modified-Since` / `If-Unmodified-Since`）与 `Last-Modified` 响应，range 读取与大型 PROPFIND 显著提速，读写 DB 分离修复写后读一致性；上传 finalize 路径引入 `VerifiedUploadedBlob` 等不可变值对象，把 size / policy / path 校验与清理计划收敛到类型层。配套把 "managed ingress profile" 重命名为语义更清晰的 "remote storage target"，远程存储策略可绑定到具体 target；本版本还顺带合并了贡献者提交的 SFTP 存储后端。

- **服务层领域模块化重组** — 扁平 `*_service` 文件按领域嵌套为模块树，纯路径迁移，无 API / schema 变化
- **WebDAV 性能与合规性** — RFC 9110 条件请求与 `Last-Modified` 响应、range 读取与大型 PROPFIND 显著提速、读写 DB 分离修复写后读一致性、下载审计窗口合并
- **上传 finalize 路径契约化** — `VerifiedUploadedBlob` / `VerifiedTempStoreBlob` 等不可变值对象收敛校验与清理计划，固化 complete 重试幂等与配额超限不扣配额不变量
- **远程存储 target 化** — `managed ingress profile` 重命名为 `remote storage target`，远程策略绑定具体 target，旧 API 路径与配置 key 保留兼容（deprecated）
- **SFTP 存储后端** — 基于 `russh`，支持连接池、host key 强制固定（TOFU）、高效 range 读；纯服务端中继上传，不支持 presigned / multipart

### Added

- **远程节点 driver descriptor 发现**
  - 新增 `ManagedIngressDriverDescriptor` 与字段 descriptor（Text / Secret / Boolean / Number + required / secret / label_key / placeholder / help_key），内置 local + S3 两个 driver 描述
  - 新 API 从远程节点拉取支持的 driver 描述，管理后台按当前 driver 动态渲染字段、显示 driver 专属 help / placeholder，选远程不支持的 driver 时给出校验错误
  - 远程内部协议版本 v4 → v5；`RemoteStorageCapabilities` 新增 `managed_ingress` 能力字段；v4 兼容层保留（`MIN_SUPPORTED_PROTOCOL_VERSION` 仍为 v4）

- **远程存储策略绑定 target**
  - `storage_policies` 表新增 `remote_storage_target_key` 列与索引；策略创建 / 更新时校验 target 已 applied 且无错误，非远程策略拒绝带 target key
  - 内部 / presigned 请求按策略指定的 target 路由，而非总走 binding 默认
  - 管理后台策略对话框新增 target 二级选择（loading / empty / error / hint），支持在策略表单内直接快速创建 target

- **下载 disposition 参数 + resource-handle 端点**
  - `DownloadQuery` 新增可选 `disposition`（`inline` / `attachment`），非法值返回校验错误；贯穿 personal / team / share 下载链路（空值 = attachment 旧行为，向后兼容）
  - 新端点 `POST /api/v1/files/{id}/resource-handle` 与 team 对应端点，返回 `FileResourceHandle`；`Auto` 模式下为不可渲染图片（HEIC / RAW / TIFF）选派生 WebP 表示、HTML 等沙箱 MIME 强制同源

- **WebDAV HTTP 条件请求合规**
  - 新增 `src/utils/http_validators.rs` 实现 RFC 9110 的 `If-Match`（强 ETag）/ `If-None-Match`（弱 ETag）/ `If-Modified-Since` / `If-Unmodified-Since` 求值与 header 优先级；GET / HEAD 响应携带 `Last-Modified`

- **WebDAV 下载审计合并**
  - 新配置项 `webdav_download_audit_coalesce_window_secs`（默认 30s）；同一账号 + 文件 + 请求类型（full / ranged）+ 客户端指纹（IP + User-Agent 的 SHA256）在窗口内的重复下载合并为一条审计记录，设为 0 表示每次都记录

- **SFTP 存储后端**
  - 新增 `DriverType::Sftp`（持久化字符串 `"sftp"`，sea-orm 以字符串存储，老库无需 migration），基于 `russh 0.62.1` + `russh-sftp 2.3.0`
  - 连接池（默认 4，空闲 TTL 60s，获取超时 30s，RAII lease 归还）、强制 host key 固定（TOFU，未配置指纹直接拒绝并回传服务器实际 `SHA256:` 指纹）、高效 range 读（SFTP seek + take）
  - 上传走服务端中继流式，**不支持** presigned / multipart / 浏览器直传——能力边界与 S3 不同

### Changed

- **服务层重组为领域模块**（纯内部，无 API / schema 变化）
  - `src/services/` 下扁平 `*_service` 文件 → 按领域嵌套模块树，绝大部分是 `use` 路径迁移
  - 新增 `developer-docs/{en,zh-CN}/backend-service-ownership.md` 服务层责任边界文档

- **术语统一：managed ingress profile → remote storage target**
  - API 端点 `/ingress-profiles` → `/storage-targets`，旧路径保留为 deprecated 别名（计划于 `0.4.0` 移除）
  - 配置项 `server.follower.managed_ingress_local_root` → `server.follower.remote_storage_target_local_root`，loader 同时接受新旧两个键；默认值 `managed-ingress` → `remote-storage-targets`
  - 保留兼容项：API 错误码仍为 `managed_ingress.*`；审计日志 entity type / action 仍为 `RemoteIngressProfile`；wire protocol capabilities 仍用 `managed_ingress` 字段名

- **`max_file_size` 从 target 层上移到 policy 层**
  - 删除 `remote_storage_targets.max_file_size` 列；改从请求的 `max_file_size` 签名查询参数读取（值来源于存储策略），负值 clamp 到 0
  - target 创建表单移除 max_file_size，target 卡片移除 revision 显示

- **WebDAV 读写 DB 分离与缓存精细化**
  - `resolve_path_cached_in_scope` 拆 read / write 两变体：写操作（PUT / MKCOL / COPY / MOVE / DELETE / LOCK）走 `writer_db()`，读操作走 `reader_db()`，避免写后读命中过期缓存
  - 引入 `CacheInvalidationTargets` 支持 prefix + 定向 key 删除；文件夹变更改为定向删 key 而非全前缀刷；PROPFIND 只请求标准 live property 时跳过 dead property 与锁发现加载

- **WebDAV range 读与 PROPFIND 路径优化**
  - 下载路径一次解析 file + blob + meta，消除重复 metadata 查询；流式块大小 16 KiB → 64 KiB
  - `read_dir` 去 N+1：直接用 `file` 记录构造目录项；ETag 改用结构化 `file_etag`，不再依赖 blob hash
  - 团队成员变更触发的 WebDAV auth 缓存失效改为 fire-and-forget（带重试 + 日志），不再阻塞成员操作

- **上传 finalize 路径契约化**（纯内部，无 API / 行为变化）
  - 新增 `VerifiedUploadedBlob` / `VerifiedTempStoreBlob` / `VerifiedPreuploadedNondedupStoreBlob` 不可变值对象，把 size / policy / path 校验与清理计划收敛到类型层
  - 新增回归测试固化不变量：complete 重试不重复扣配额、配额超限时 session 置 Failed 且不扣配额（错误码 `E032`）

- **前端 admin / 文件操作改造为插件可扩展**
  - 新增 admin settings action / editor / invalidation / save-transaction 四个 registry；文件操作 `fileActionRegistry`（声明式 action descriptor，支持 builtin + plugin action）
  - `AdminPoliciesPage` 从 1595 行减到 235 行（抽 5 个 controller hook + 3 个工具模块）；12 个 controller 迁移到通用 `useManagedListQueryState` / `useManagedAdminList`，减少约 800 行样板

- **S3 remote target 编辑强制输入 access_key**
  - 新增 `src/storage/field_contract.rs` 统一 required / secret / boolean 字段语义；编辑 S3 remote target 时每次必须输入 `access_key`，`secret_key` 未变更时保留原值
  - 前端从硬编码 `driver_type === "s3"` 改为查 descriptor `secretKeyField?.secret === true`

### Fixed

- **WebDAV 写后读一致性** — 写操作走 `writer_db()` 权威解析，新增测试覆盖 PUT / COPY / MOVE / LOCK / DELETE 后立即 GET / PROPFIND
- **音乐播放 metadata 刷新打断播放** — track metadata 按 resource path 而非全对象做 key；跨域重定向资源跳过 range probe
- **WebDAV 团队归档 / 恢复后 auth 缓存陈旧** — 归档 / 恢复团队时失效 WebDAV auth 缓存
- **k6 基准测试 token 复用** — `refreshSession` 原本一直复用旧 refresh token；`mixed-ramp.js` / `soak-mixed.js` 每个 VU 改为单独 `login()`

### Security

- **SFTP host key 强制固定** — 从盲信任意 host key 收紧到 TOFU + pin，防 MITM；未配置指纹时拒绝连接并回传服务器实际 `SHA256:` 指纹引导管理员确认
- **S3 remote target 编辑强制 access_key** — 公开凭证每次必须输入，仅 `secret_key` 在未变更时保留原值
- **负值 `max_file_size` 被拒** — `normalize_storage_policy_max_file_size` 拒绝负值
- **remote target 相对路径逃逸检测** — `normalize_relative_local_target_path` 检测路径逃逸段

### Database Migrations

- `m20260704_000001_rename_managed_ingress_profiles_to_remote_storage_targets`（表 / 列改名，可回滚）
- `m20260704_000002_add_remote_storage_target_key_to_storage_policies`（加列 + 索引）
- `m20260705_000001_drop_remote_storage_target_max_file_size`（删 `max_file_size` 列，改走策略查询参数）

### Configuration Changes

- `server.follower.managed_ingress_local_root` → `server.follower.remote_storage_target_local_root`（loader 同时接受新旧 key，旧 key deprecated，计划于 `0.4.0` 移除）
- 默认值 `managed-ingress` → `remote-storage-targets`
- 新增 `webdav_download_audit_coalesce_window_secs`（默认 30s，设为 0 表示每次下载都记录审计）

### Statistics

- 916 files changed, 45826 insertions(+), 17459 deletions(-)
- 30 commits（含若干 `[skip ci]` 仓库迁移 / 测试镜像固定）
- 3 个数据库 migration（自动执行，可回滚）
- 新增后端驱动：1（SFTP）

### Notes

- 从 `v0.3.0` 升级：3 个 DB migration 在启动时自动执行；API 旧路径 `/ingress-profiles` 与旧配置 key 保留兼容（deprecated），建议尽快迁移到 `/storage-targets` / `remote_storage_target_local_root`
- SFTP 策略首次配置需走 host key TOFU 确认流程：未配 `sftp_host_key_fingerprint` 时连接测试会返回服务器实际 `SHA256:` 指纹，确认后写入即可
- SFTP 后端为纯服务端中继，不支持 presigned / multipart / 浏览器直传，能力边界与 S3 不同
- 远程内部协议 v4 → v5：`MIN_SUPPORTED_PROTOCOL_VERSION` 仍为 v4，旧 follower 靠兼容层继续工作；primary + follower 混合部署建议一并升级
- Docker 用户建议使用 `v0.3.1` / `stable` / `latest` 镜像标签

## [v0.3.0] - 2026-06-23

### Release Highlights

**AsterDrive `0.3.0` 正式发布。** 在 `v0.3.0` beta / RC 线（Azure Blob 与 OneDrive 驱动、统一 `StorageConnector` 抽象、对象存储术语统一与凭据脱敏、内联预览走预签名直链、审计日志无限期保留等）基础上，本版本收口公开分享预览的缓存稳定性与 CI 构建流水线，把 `0.3.0` 系列从 RC 阶段推进到稳定发布。

- **预览资源缓存身份稳定化** — 公开分享预览响应返回 canonical ETag（blob hash），前端按 stable identity 复用 blob / text 缓存，跨过期 preview token 共享同一份资源
- **条件请求 no longer 触发 CORS 预检** — `If-None-Match` 条件请求留在 same-origin，不再 302 到预签名对象存储 URL
- **CI 前端构建独立化** — GitHub Actions 把前端构建拆成独立 `build-frontend` job，下游 build / integration-backends 通过 artifact 下载产物，避免重复构建与 Rust cache 污染
- **依赖升级** — `react-image-crop` 11.0.10 → 11.1.2，`@typescript/native-preview` 升至 `7.0.0-dev.20260622.1`

### Added

- **预览资源 stable identity 类型**
  - 前端新增 `ResourceRequest` 类型区分 `cacheKey` / `etag` / `requestPath`，新增 `resourceCacheKey` / `resourceRequestPath` / `resourceCanonicalEtag` 辅助函数集中处理
  - `useBlobUrl` / `useTextContent` 按 stable identity 缓存，提供 canonical ETag 时跳过 `If-None-Match` 条件请求
  - 各预览组件（`BlobImagePreview` / `PdfPreview` / `CsvTablePreview` / `JsonPreview` / `MarkdownPreview` / `TextCodePreview` / `XmlPreview` / `FilePreviewBody` / `ImagePreviewPanel`）接受 `ResourcePath`（string | `ResourceRequest`）
  - `PreviewLinkInfo` 新增 canonical etag 字段（blob hash），公开分享预览响应携带稳定身份
  - 新增 `resourceRequest` / `useBlobUrl` / `useTextContent` 单元测试覆盖 cache key 回落与条件重验证逻辑

### Changed

- **条件请求 same-origin 收敛**
  - `file_service::download::build` 在 presigned redirect 前拒绝条件请求（`If-None-Match`），避免浏览器跨 origin 携带条件触发 CORS 预检
  - `preview_link_service` 在响应中携带 canonical ETag，供前端复用缓存

- **CI 前端构建独立 job**
  - `rust.yml` 新增 `build-frontend` job 单独构建前端并上传 artifact，`build` / `integration-backends` 改为 `needs: build-frontend` 并通过 `download-artifact` 拉取 `frontend-panel/dist`
  - 移除原先散落在 `build` / `integration-backends` 中的 `Setup bun` 与 `Build frontend` 步骤，避免 Rust cache 被前端产物污染

- **预览组件路径类型统一**
  - 前端预览组件从内联类型定义改为导入公共 `ResourcePath` 类型，测试中的 mock helper 收敛到 `resourceCacheKey` / `resourceCanonicalEtag` 工具

- **依赖升级**
  - `react-image-crop` 11.0.10 → 11.1.2
  - `@typescript/native-preview` 升至 `7.0.0-dev.20260622.1`

### Fixed

- `OverviewRecentEventsSection` 表格列改为固定宽度 + `max-w-0` 截断，修复内容过长时列宽抖动

### Notes

- 本版本为 `0.3.0` 系列正式发布版
- 从 `v0.3.0-rc.2` 升级到 `v0.3.0` 没有新增数据库 migration
- 生产配置 schema 未新增必需项
- Docker 用户建议使用 `v0.3.0`、`stable` 或 `latest` 镜像标签；`edge` 继续保留给后续预发布版本
- 统计数据：31 files changed, 923 insertions(+), 174 deletions(-)
- 本次范围共 2 个提交

## [v0.3.0-rc.2] - 2026-06-22

### Release Highlights

**AsterDrive `0.3.0-rc.2` 是 0.3.0 发布候选线的预览体验与审计保留补强版本，主线是内联预览走对象存储预签名直链与审计日志无限期保留。** Inline 预览（图片 / 视频 / 音频 / PDF / Markdown / 代码 / 表格 / JSON / XML 等）在策略允许且 MIME 不需要 same-origin CSP sandbox 时直接 302 到对象存储预签名 URL，不再由服务端统一加 CSP 与缓存头转流，降低服务端带宽与转发延迟；公开分享预览（preview link）下载路径切换到统一下载出口以同样享受预签名。前端 `apiUrl` 把后端 API origin 的 URL 从「外部资源」中排除并新增 `shouldSendResourceCredentials`——预签名 URL 落在对象存储 origin 时不会携带会话凭据，避免 CORS 预检失败与凭据外泄；各预览组件 `path` 改为可空，加载期间显示 loading 占位。审计日志保留期 `audit_log_retention_days` 设为 `0` 时跳过自动清理，实现永久保留。

- **Inline 预览走预签名直链** — 策略允许且 MIME 不需 same-origin sandbox 时 inline 预览直接 302 到对象存储预签名 URL，preview link 同步切换到统一下载出口
- **CORS 安全的凭据处理** — 后端 API origin 的 URL 不再被前端误判为外部资源，新增 `shouldSendResourceCredentials` 集中判断；预签名 URL 属于对象存储 origin 时不携带会话凭据
- **审计日志无限期保留** — `audit_log_retention_days = 0` 跳过自动清理，i18n 文案补充「设为 0 表示永久保留」
- **存储操作审计文案补全** — 新增「管理员触发存储操作」i18n 文案（en / zh）

### Added

- **审计日志无限期保留**
  - `audit_service::cleanup_expired` 在 `retention_days <= 0` 时跳过清理并返回 0，管理员可将 `audit_log_retention_days` 设为 `0` 实现永久保留
  - `settings-operations` i18n 文案补充「设为 0 表示永久保留，不自动清理」
  - 新增 `audit_action_admin_trigger_storage_action` i18n 文案（en / zh）覆盖存储操作审计
  - 新增 `test_audit_cleanup_retention_zero_keeps_logs` 集成测试断言 365 天前的记录在 retention=0 时仍被保留

### Changed

- **Inline 预览走预签名直链**
  - `file_service::download::build` 中 `should_presign` 从「仅 Attachment」改为「除需 same-origin CSP sandbox 的 inline MIME 类型外均走 presigned redirect」；`build_presigned_redirect_outcome` 接受 `disposition` 参数，content-disposition 不再硬编码 Attachment
  - `preview_link_service::download_file` 由直接调用 `build_stream_outcome_with_disposition_and_range` 改为 `build_download_outcome_with_disposition_and_range`，公开分享预览同样享受预签名
  - 前端新增 `useContentPreviewResourcePath` hook：存在 `previewLinkFactory` 时优先解析 preview link 路径，否则回落 downloadPath
  - 前端各预览组件（`PdfPreview` / `BlobImagePreview` / `MusicPreview` / `VideoPreview` / `MarkdownPreview` / `CsvTablePreview` / `XmlPreview` / `JsonPreview` / `TextCodePreview`）从 `downloadPath` 切换到 `contentPreviewPath`，`path` 改为可空并在加载期间显示 loading
  - `tests/test_upload.rs` 中 direct-link inline presigned 测试断言改为 302 redirect + `Cache-Control: no-store` + `Location` 携带 `response-content-disposition`

- **前端资源凭据判断收紧**
  - `apiUrl.ts` 新增 `isConfiguredApiUrl` 判断 URL 是否属于后端 API origin；`isExternalResourceUrl` 不再把后端 API URL 当作外部资源
  - 新增 `shouldSendResourceCredentials(path)`：仅在「非外部资源且非公开资源」时携带会话凭据，`authenticatedResource` 改用该判断
  - 预签名 URL 落在对象存储 origin（与 API origin 不同）时不会被携带凭据，避免 CORS 预检失败与凭据泄露

- **S3 / MinIO / R2 文档补全**
  - `docs/storage/s3-minio-r2.md` 与 `docs/en/storage/s3-minio-r2.md` 补全内联预览走预签名直链所需的 CORS 与凭据配置说明

### Statistics

- 35 files changed, 1641 insertions(+), 116 deletions(-)
- 3 commits

## [v0.3.0-rc.1] - 2026-06-22

### Release Highlights

**AsterDrive `0.3.0-rc.1` 是 0.3.0 系列的发布候选版本，主线是存储策略术语统一与凭据安全加固。** 将面向 S3 命名的策略字段统一为通用对象存储术语（`s3_upload_strategy` → `object_storage_upload_strategy` 等，serde alias 保留旧名向后兼容），Microsoft Graph 的 client secret / token 用 `secrecy::SecretString` 包装并在所有持有凭据的类型上手动实现 `Debug` 确保日志脱敏；reverse tunnel stream lane 在离线 / 关闭 / 超时时自动回退 poll 模式而非直接失败。

- **存储策略术语统一（Object Storage，向后兼容）** — S3 专属字段名改为通用对象存储命名，旧名通过 serde alias 与前端 legacy fallback 继续可用
- **凭据安全加固** — Microsoft Graph client secret / token 用 `SecretString` 包装，相关 entity 与 provider 手动实现 `Debug` 保证日志脱敏
- **Reverse tunnel 可靠性** — stream lane 离线 / 关闭 / 超时自动回退 poll 请求
- **OneDrive / Azure Blob 文档补完** — admin API 与存储后端文档补齐新驱动说明

### Changed

- **存储策略术语统一（Breaking，向后兼容）**
  - `StoragePolicyOptions` JSON 字段 `s3_upload_strategy` / `s3_download_strategy` → `object_storage_upload_strategy` / `object_storage_download_strategy`，旧名通过 `#[serde(alias = "...")]` 与前端 legacy fallback 继续可用
  - enum 类型 `S3UploadStrategy` / `S3DownloadStrategy` → `ObjectStorageUploadStrategy` / `ObjectStorageDownloadStrategy`（Rust + OpenAPI + 前端 types），枚举值 `relay_stream` / `presigned` 不变
  - connector capability `s3_transfer_strategy` → `object_storage_transfer_strategy`
  - 前端管理面板文案从"S3 上传 / 下载方式"改为"对象存储上传 / 下载方式"

- **OneDrive 授权请求体收紧**
  - `POST /admin/policies/{id}/storage-authorization/start` 仅接受 `{ "provider": "microsoft_graph" }`，Client ID / Secret / tenant / scopes 必须先保存到 `application_config.microsoft_graph`

### Security

- 新增 `secrecy = "0.10"` 依赖，Microsoft Graph `client_secret` / `refresh_token` / `access_token` 用 `SecretString` 包装，仅在调用 Microsoft Graph 时通过 `expose_secret()` 取出
- `storage_policy` / `managed_follower` / `master_binding` / `managed_ingress_profile` entity 与所有 Microsoft Graph token provider / request 类型手动实现 `Debug`，日志中 `access_key` / `secret_key` 显示为 `***REDACTED***`，每个类型带单元测试断言不泄露明文

### Fixed

- reverse tunnel stream lane 在 `reverse tunnel is offline` / `lane closed` / `response channel closed` / 流式等待超时时自动回退 poll 请求，此前直接失败
- RenameDialog 重命名提交期间禁用按钮并用 ref 守卫，防止用户快速重复点击触发多次重命名请求

### Statistics

- 109 files changed, 1808 insertions(+), 691 deletions(-)
- 3 commits

## [v0.3.0-beta.2] - 2026-06-21

### Release Highlights

**AsterDrive `0.3.0-beta.2` 是 0.3.0 beta 线的稳定性与错误处理打磨版本，主线是连接器抽象的健壮性收口与 OneDrive 授权流程修正。** 全面将 panic 路径（`expect`/`unwrap`）替换为 `Result` 错误传播，统一存储连接测试的诊断返回（从成功响应 payload 迁移到 error metadata）；OneDrive 授权改为"先保存凭据再授权"，draft 策略连接测试支持复用已保存凭据；各 service 缓存逻辑下沉到独立 cache 子模块。

- **错误处理健壮性收口** — `expect`/`unwrap` 全面替换为 `Result`，multipart 读取失败映射 BAD_REQUEST，connector 注册缺失改为显式错误而非静默回退 local
- **存储连接测试 API 契约统一** — 诊断信息从成功响应 payload 迁移到 `ApiErrorInfo.diagnostic`，前端从 `ApiError.diagnostic` 读取
- **OneDrive 授权流程修正** — 授权请求只发 provider type，要求先保存凭据变更才能发起授权
- **draft 策略凭据复用** — 连接测试可选 `policy_id`，空白凭据字段复用已保存凭据（S3 / Azure Blob / Tencent COS）
- **cache 模块抽取** — 各 service 缓存逻辑下沉到独立 cache 子模块

### Added

- **draft 策略凭据复用**
  - 连接测试端点新增可选 `policy_id`，S3 / Azure Blob / Tencent COS 在凭据字段为空时复用已保存凭据，管理员改配置时无需重输敏感信息

### Changed

- **错误处理重构**
  - CORS、加密、shutdown handler、CLI 序列化、时间/时区运算、内存 mail sender mutex 中毒等 panic 路径替换为 `Result` 传播与日志降级
  - signal handler 安装失败不再 panic；archive preview / offline download / WOPI discovery client 构建失败由静默 fallback 改为错误传播
  - `connector_or_local` 静默回退 local 改为 `connector_or_registered` 显式报错；故意不可达分支加 `#[allow(clippy::expect_used)]` 标注不变式

- **存储连接测试 API 契约统一**
  - 移除 `StoragePolicyProbeResult` 与 `probe_connection*` 端点，改用标准错误响应携带 `ApiErrorInfo.diagnostic`
  - 新增 `ApiErrorDiagnostic` schema，对外暴露 message / kind，api_code / retryable 内部保留
  - 前端连接测试从 `ApiError.diagnostic` 读取失败原因

- **OneDrive 授权流程**
  - 授权请求不再携带 draft Microsoft Graph 凭据，仅发送 provider type，后端复用已保存 application_config
  - 凭据变更后必须先保存才能发起授权

- **连接器 descriptor 配置**
  - driver-type 条件分支改为显式 input struct（`ObjectStorageConnectorDescriptorInput` / `ObjectStorageFieldDescriptorInput`）
  - descriptor UI 逻辑从集中函数下沉到各 connector；multipart ETag 要求改为显式 input 字段

- **cache 模块抽取**
  - admin / auth / folder / passkey / preview link / share stream / stream ticket / workspace scope / WebDAV auth / WebDAV path resolver 的缓存操作下沉到独立 cache 子模块，补全作用域与失效测试
  - `share_stream` marker 编码移入 cache 模块，`reserve_count_marker` / `store_count_marker` 改为 `Result` 传播

- **依赖升级**
  - actix-web 4.13.0 → 4.14.0、actix-multipart 0.7.2 → 0.8.0、actix-http 3.12.1 → 3.13.0、actix-multipart-derive 0.7.0 → 0.8.0
  - derive_more 统一为 2.1.1、foldhash 0.2.0、impl-more 0.3.1；parse-size 替换为 bytesize 2.4.0
  - 前端 `@typescript/native-preview` 升至 `7.0.0-dev.20260621.1`

### Fixed

- multipart 上传读取失败（`UploadFieldReadFailed` / `AvatarUploadReadFailed`）映射为 BAD_REQUEST，此前为 500
- offline download 循环在 shutdown 信号下不再静默中断，改为返回 transient 错误
- WOPI discovery client 构建失败不再回退 `reqwest::Client::new`，改为传播错误

### Statistics

- 114 files changed, 3665 insertions(+), 1347 deletions(-)
- 4 commits

## [v0.3.0-beta.1] - 2026-06-21

### Release Highlights

**AsterDrive `0.3.0-beta.1` 是 0.3.0 系列的第一个 beta 版本，主线是存储后端扩展与连接器抽象统一。** 本版本新增 Azure Blob Storage 与 OneDrive（含 SharePoint site / Group drive）两类云盘驱动，引入统一的 `StorageConnector` 抽象让六个驱动以一致的 descriptor 暴露能力与表单字段；OneDrive 凭据走 Microsoft Graph OAuth + PKCE，token 用 HKDF 派生密钥 + AES-256-GCM 加密落库。缓存后端精简为 memory / Redis 两种并移除已失效的 `cache.enabled` 开关，Redis 引入本地 memory 二级回退；存储策略连接探测改为结构化诊断端点，失败返回 200 + 脱敏原因。同时补全审计日志结构化详情、Tencent COS CORS 一键配置、PWA precache 重构与 toast 视觉重做。

- **Azure Blob Storage 驱动** — Block Blob CRUD、SAS URL 预签名上传/下载、分块上传
- **OneDrive 驱动 + OAuth 凭据管理** — personal / work_or_school / SharePoint site / group drive，PKCE 流程，>250MiB 分块续传
- **StorageConnector 统一抽象** — 六驱动统一 descriptor、字段与上传工作流，新增 `/admin/policies/storage-drivers`
- **缓存后端精简与 Redis 二级回退（Breaking）** — 移除 `cache.enabled`，Redis 故障时回落 memory
- **存储策略诊断端点** — 结构化 `StoragePolicyDiagnostic`，SAS / account key 脱敏
- **审计日志结构化** — 文件 / 用户 / 策略 / 会话操作补全上下文
- **Tencent COS CORS 自动配置** — 从 `public_site_url` 派生多 origin，一键应用
- **PWA precache 与 toast 重做** — glob 化预缓存清单 + 预算告警，toast 主题化

### Added

- **Azure Blob Storage 驱动**
  - 新增 `azure_blob` driver type，完整 CRUD、SAS URL 预签名上传 / 下载、Block Blob 分块上传
  - `InitUploadResponse` 新增 `presigned_require_etag` 字段；`presigned_put_requires_etag` trait 方法区分驱动（Azure Blob = false，S3 / remote = true）
  - `AzureBlobConfigError`：生产环境 SAS `spr` 协议限 https，loopback / Azurite 放行 https + http
  - 前端表单字段改为 Storage Account Name / Key，预签名请求附带 `x-ms-blob-type: BlockBlob` 头

- **OneDrive 驱动与 OAuth 凭据管理**
  - 新增 `onedrive` driver type + `OneDriveAccountMode`（personal / work_or_school / sharepoint_site / group_drive）
  - 7 个 `onedrive_*` 策略字段；Microsoft Graph OAuth + PKCE 授权流程
  - token 与 client secret 用 `auth.storage_credential_secret_key` 经 HKDF 派生 + AES-256-GCM 加密落库
  - >250MiB 走 Graph 原生分块上传 session
  - 驱动根动态解析（personal / work / site / group）
  - 前端凭据面板含授权状态徽章、授权 / 回调入口

- **StorageConnector 统一抽象与驱动 descriptor**
  - 引入 `StorageConnector` trait + `StorageConnectorDescriptor`，六驱动统一暴露 capabilities / fields / upload workflows / actions
  - 新增 `GET /admin/policies/storage-drivers` 端点；前端用 TTL 缓存的 descriptor 取代 driver-type 字符串判断
  - 新增 `ProviderResumableUploadDriver` trait（OneDrive / Graph 原生断点续传）
  - 新增 DB 表 `storage_connector_application_configs`，Microsoft Graph app 配置从 policy key 字段迁移到 connector application config

- **存储策略诊断端点**
  - 连接探测返回结构化 `StoragePolicyDiagnostic`（api_code / kind / message / retryable）
  - 探测失败改为 200 + `ok:false` 而非 4xx
  - SAS token、account key 在响应中脱敏

- **Tencent COS CORS 自动配置**
  - 新增 `POST /admin/policies/action` 与 `POST /admin/policies/{id}/action`，`StoragePolicyActionType` 可扩展动作枚举
  - `configure_tencent_cos_cors` 从 `public_site_url` 派生多 origin，仅替换 ID 为 `asterdrive-presigned-access` 的规则
  - 草稿动作支持复用已保存 policy 凭据
  - 触发审计日志

- **审计日志结构化详情**
  - 用户管理、存储策略、团队、会话撤销等操作补全审计快照与展示文案
  - 文件 / 文件夹操作补全 location 与 transfer 路径细节
  - 标签批量操作、MFA 挑战、passkey 登录、WebDAV、WOPI 改为结构化详情
  - 新增 `ShareDeleteAuditDetails`、`UserMfaManageAuditDetails`、`ExternalAuthUnlinkAuditDetails` 等 struct

### Changed

- **缓存后端精简（Breaking）**
  - 移除 `cache.enabled` 配置项与 noop cache backend，仅保留 memory / Redis
  - CacheBackend trait 新增 `take_bytes` / `delete_many`
  - Redis 引入二级回退：Redis 失败或熔断时落本地 MemoryCache，恢复后清理影子条目
  - 抽出 `RedisClient` trait + `FakeRedisClient` 测试替身

- **存储策略前端判断重构**
  - 移除 `isS3CompatibleDriver` / `isOneDriveDriver` / `isObjectStorageDriver`，改 field-presence 检测
  - `POLICY_OPTION_SERIALIZERS` 替换为 `buildPolicyOptionsFallback`，仅写非默认值

- **上传会话字段统一（DB 迁移自动处理）**
  - 重命名 `s3_temp_key` → `object_temp_key`、`s3_multipart_id` → `object_multipart_id`
  - `S3_MULTIPART_MIN_PART_SIZE` → `OBJECT_MULTIPART_MIN_PART_SIZE`
  - OneDrive 凭据存储从 `storage_policies.access_key` / `secret_key` 迁移到 `storage_connector_application_configs`，旧字段自动清空（代码兼容旧字段回退读取）

- **PWA precache 与 toast 重做**
  - vite-plugin-pwa 改用 glob 通配 + 关键资源校验，移除手动 precache 清单与 forbidden 列表
  - 预缓存预算软告警：entries ≤450、raw size ≤5MB；排除 admin / file browser / music player / PDF / office 等大模块
  - `pwaWarmup` 区分 user / admin 路由独立追踪
  - Toaster 主题化（oklch、backdrop-blur、4.2s 时长、i18n.dir 方向）

- **审计日志穷举化**
  - `detail_message` 通配分支改为穷举 match，防止新增 action 被静默遗漏
  - `mfa_factor_repo::list_for_user` 泛化为 `ConnectionTrait`，支持事务内调用

- **依赖升级**
  - Rust：sea-orm 2.0.0-rc.41、aws-sdk-s3 1.137、h2 0.4.15
  - 前端：@base-ui/react 1.6、axios 1.18、react-router-dom 7.18、tailwindcss 4.3.1、biome 2.5、@types/node 26、@typescript/native-preview 7.0.0-dev.20260620.1

### Fixed

- **PWA precache 大小写敏感漏排除**
  - 大小写敏感文件系统漏排除补全：新增 `assets/**/*admin*`、`assets/**/*musicPlayer*` 小写 / camelCase 变体

### Security

- OneDrive OAuth token 与 Client Secret 用 HKDF 派生密钥 + AES-256-GCM 加密落库，API 与审计只暴露 `client_secret_configured` 布尔状态
- 存储策略连接诊断在响应中脱敏 SAS token 与 account key
- OneDrive 创建 / 更新策略时清空 legacy `storage_policies.access_key` / `secret_key` 字段，避免明文 secret 长期驻留

### Database Migrations

- 新增 `m20260612_000001`：`storage_policy_credential`、`storage_policy_authorization_flow` 表（OneDrive OAuth token 加密存储）
- 新增 `m20260620_000001`：3 个 JSON 列 backfill `{}` 并强制 NOT NULL（跨 MySQL / SQLite / PostgreSQL）

### Configuration Changes

- **移除 `cache.enabled`（Breaking）** — 缓存开关已失效，仅保留 memory / Redis 两种 backend；已有配置中的 `cache.enabled` 字段不再生效
- 新增 `auth.storage_credential_secret_key` — OneDrive / 后续 OAuth 驱动的凭据加密主密钥（缺失时启动自动生成）
- 新增 OneDrive 策略字段：`onedrive_account_mode`、`onedrive_tenant`、`onedrive_site_id`、`onedrive_drive_id`、`onedrive_group_id`、`onedrive_scopes` 等

### Statistics

- 316 files changed, 36108 insertions(+), 3381 deletions(-)
- 9 commits

## [v0.3.0-alpha.5] - 2026-06-13

### Release Highlights

**AsterDrive `0.3.0-alpha.5` 是 0.3.0 系列的第五个预发布版本，聚焦安全加固、文件夹级存储策略绑定、激活流程防枚举以及只读浏览/分享归档下载体验。** 本版本将公开分享密码 Cookie、公共直链、预览链接和分享流式播放会话从 `auth.jwt_secret` 拆分到专用 HMAC 密钥，并在 WebDAV 路径、XML 解析、上传大小校验、分享下载并发等多处加固边界检查；新增文件夹级存储策略绑定（管理员独占）与策略继承解析；激活邮件重发改为防账号枚举的统一响应；文件浏览器引入只读模式并支持分享归档下载；上传重试、缩略图懒加载与 i18n 分包拆分进一步优化前端性能。

- **认证与公开链接密钥隔离（Breaking）** — 新增 `share_cookie_secret`、`direct_link_secret`，拆分直链/预览/流式播放 token 验签
- **文件夹级存储策略绑定** — 管理员可绑定/清除文件夹策略，上传走最近祖先策略，完整继承解析
- **激活重发防枚举流程** — 登录与激活重发统一响应、强制时延、按结果细分指标
- **分享归档下载与只读浏览** — `FileBrowserProvider` 只读模式、`/s/{token}/archive-download` 端点、下载限额与中断回滚
- **多模块安全加固** — XXE 防护、WebDAV 路径规范化、锁数量限额、分享 Cookie 绑定客户端、上传实际大小校验
- **schema drift 检测** — SeaORM 实体与迁移产物的列定义跨 SQLite/Postgres/MySQL 一致性校验
- **前端性能优化** — i18n 按需分包、登录后 PWA 预热路径、缩略图视窗内拉取、上传重试去抖

### Added

- **文件夹级存储策略绑定**
  - 新增 `PUT /api/v1/admin/folders/{id}/policy` 端点（管理员独占），设置或清除文件夹策略绑定
  - 策略继承解析：文件夹继承最近祖先的显式策略，回落至用户/团队策略组
  - 上传服务沿文件夹层级解析有效策略
  - 审计日志新增 `folder_policy_change` 动作（记录原策略 ID 和新策略 ID）
  - 访问控制：仅管理员可设置/清除文件夹策略；普通 PATCH 请求拒绝 `policy_id` 字段
  - 校验：拒绝不可用策略、锁定/已删除文件夹和断裂的层级链
  - 前端 `FolderPolicyDialog` 组件（策略选择、继承可视化、有效策略提示）
  - 仅管理员可见的右键菜单入口与对话框预加载

- **激活重发防枚举流程**
  - 新增 `ActivationResendRequestPanel` 组件（登录表单内入口、不暴露账号是否存在）
  - 邮箱字段自动从登录标识符填充
  - 后端引入 `RegisterActivationResendOutcome` 枚举（Sent / EmailNotFound / AlreadyActive / AccountDisabled / Cooldown / EmailPolicyRejected）
  - 通过 `record_auth_event` 按 outcome 发出结构化指标，外部仅看到通用 200 响应
  - 邮件策略拒绝时返回通用 200 而非 400，避免泄漏账号状态
  - `apply_auth_mail_response_floor` 在 `setup`/`register`/`login` 强制最小响应时延，缓解时序枚举攻击
  - 登录失败统一收敛为 `auth.credentials_failed`（凭据错误、待激活、账号禁用同响应）
  - 引入 `LoginFailureReason` 枚举保留内部上下文用于指标，不写入 API 响应

- **分享归档下载（共享 ZIP）**
  - 新增 `POST /s/{token}/archive-download` 与 `GET /s/{token}/archive-download/{ticket}` 端点
  - `stream_ticket_service` 新增 `SharedArchiveDownload` 票据类型
  - `task_service::archive::selection` 新增 `prepare_shared_archive_download`、`stream_shared_archive_download`
  - 新增 `archive_download_user_enabled`、`archive_download_share_enabled` 运行时配置开关
  - 新增 API 错误码 `ArchiveDownloadUserDisabled`、`ArchiveDownloadShareDisabled`
  - 分享下载额度与归档下载共用：创建/消费票据时预留与回滚下载计数，客户端中断流式传输时自动回滚

- **文件浏览器只读模式**
  - `FileBrowserContextValue` 新增 `readOnly`、`selectionEnabled` 标志
  - `FileBrowserContextValue` 新增 `getThumbnailPath` 回调（只读场景注入缩略图）
  - 只读模式下抑制选择、拖拽、排序和右键菜单，可独立通过 `selectionEnabled` 解锁
  - 删除/标签/移动按钮在没有对应 handler 时自动隐藏
  - `useFileBrowserBatchActions` 新增 `allowDelete`、`allowTagManagement` 选项
  - 移除 `ReadOnlyFileCollection`/`ReadOnlyFileGrid`/`ReadOnlyFileTable`，用 `FileBrowserProvider` + `readOnly` 模式替代
  - `ShareFolderView` 改用 `FileBrowserProvider` + 只读模式，归档下载走 `shareService.streamArchiveDownload`

- **schema drift 检测**
  - 新增 `tests/test_schema_drift.rs::test_entity_columns_match_migrated_database_schema`
  - 跨 SQLite（PRAGMA）、PostgreSQL/MySQL（information_schema.columns）做架构内省
  - 通过宏化注册表收集 42 个实体的列定义并与迁移结果比对
  - 使用 `BTreeSet` 保证比对顺序确定性

- **管理员系统信息端点**
  - 新增 `GET /api/v1/admin/system-info`（认证后才暴露构建版本与构建时间）
  - 前端管理"关于"页展示后端构建时间，缺失/无效时回落到本地化的"未知"
  - 新增 `formatBuildTime` 校验时间戳并复用 `formatDateTime`

- **WebDAV 单用户锁数量上限**
  - 新增 `webdav.max_active_locks_per_user` 配置（默认 1024）
  - 新增 `count_active_by_owner` 仓库函数（统计未过期与无超时锁）
  - 引入 `DavLockPreflightError`、`DavLockSystem::prepare_lock` 钩子，事务内复核避免 TOCTOU
  - 超限返回 HTTP 507 Insufficient Storage（`webdav.lock_limit_exceeded`）
  - 增加用户行级锁（`lock_by_id`）确保并发场景下额度判断准确

### Changed

- **认证与公开链接密钥隔离（Breaking）**
  - 新增 `auth.share_cookie_secret`、`auth.direct_link_secret`，并将公开分享密码验证 Cookie、公共直链、预览链接和分享流式播放会话从 `auth.jwt_secret` 拆分到专用 HMAC 密钥
  - 已有 `data/config.toml` 在启动时会自动补齐缺失的 `auth.jwt_secret`、`auth.share_cookie_secret`、`auth.direct_link_secret` 和 `auth.mfa_secret_key`，不会覆盖已有值
  - 由于 direct link / preview / stream token 不再接受 `auth.jwt_secret` 验签，升级前生成的公共直链、预览链接和分享流式播放会话 token 会失效，需要重新生成
  - 分享密码验证 Cookie 会因 `auth.share_cookie_secret` 切换而失效，用户需要重新输入分享密码

- **下载文件名编码（Content-Disposition）**
  - `DownloadDisposition` 抽离到独立 `download_headers` 模块
  - 改为 RFC 5987 `filename*=UTF-8''<percent-encoded>` 格式，使用 actix-web 的 `ContentDisposition` 构造器
  - 净化 `\r`、`\n`、`\0` 等控制字符防止头部注入
  - 覆盖所有下载端点（直链、预览链接、归档流）

- **分享密码 Cookie 客户端绑定**
  - 引入 `ShareCookieBinding`：将 Cookie MAC 绑定到 user agent SHA256 哈希 + IP 子网（IPv4 /24、IPv6 /64）
  - HMAC 结构：`share_verified:{token}:ua:{hash}:ip:{subnet}`
  - 合并重复的 cookie 校验为 `check_share_cookie` / `check_share_cookie_ignoring_download_limit`
  - 同子网与同 user agent 内 Cookie 仍有效，阻断跨客户端/跨网络重放

- **公共健康检查信息收敛**
  - `/health` 与 `/ready` 不再裸返回 `version`、`build_time`，避免未认证客户端获取构建元数据
  - 存储就绪失败响应移除 `error` 字段防止驱动细节泄漏
  - 健康检查接口直接返回 `HealthResponse`，不再包裹 `ApiResponse`
  - 构建元数据移至上文新增的认证后 `/admin/system-info` 端点

- **多部分/直传上传实际大小校验**
  - `UploadedMultipartPart` 携带 part number + size 元数据
  - `list_uploaded_parts` 重命名为 `list_uploaded_part_details`，返回带大小信息
  - S3 完成流程：拉取每个分片的实际大小、校验序号连续、对比声明总大小，不一致直接 `AbortMultipartUpload`
  - 直传流式上传完成后回读 blob 元数据，比对声明大小并对照策略上限，复核配额
  - 校验失败清理 preuploaded blob，防止客户端绕过配额

- **WebDAV 路径规范化与 XXE 防护**
  - 引入 `PathEscape` 错误，先做百分号解码、再做 `.` / `..` 段折叠
  - 拒绝带前置 `..` 段（含 `%2e%2e` 等编码变体）逃逸挂载根的路径
  - 集合资源保留尾部斜杠
  - 在 PROPFIND / PROPPATCH / LOCK / REPORT 解析前调用 `reject_xml_dtd_or_entity`，检测 `<!DOCTYPE`、`<!ENTITY`
  - 触发时返回 403 + `<no-external-entities/>` 错误体（带 `xmlns:D` 命名空间）

- **WebDAV 锁错误模型**
  - 引入 `DavLockError`（`Conflict`/`LimitExceeded`/`Backend`）取代 `Result<DavLock, DavLock>`，使锁冲突与额度耗尽返回不同响应
  - `Backend` 用于数据库失败，映射 HTTP 500
  - 增加结构化 tracing：包含路径、实体类型/ID 与错误细节
  - 锁额度校验从 `prepare_lock` no-op 修正为实际生效，刷新 timeout 前先校验 scope
  - 默认无 Timeout 头时使用 `MAX_LOCK_DURATION_SECS`（7 天）替代"无限"语义，并拒绝可能溢出 chrono 的超大值

- **WebDAV 缓存认证复核**
  - `CachedWebdavAuth` 新增 `account_id`，命中缓存时通过 DB 直接复核
  - `validate_cached_scope` 替换为 `validate_cached_account`：复核账号 username、user/team/folder ID 与启用状态
  - 账号被禁用或字段变化时立即让缓存失效并返回 `AuthForbidden`
  - 校验 cached 密码哈希，防止旧凭据继续命中

- **WOPI 安全增强**
  - 新增 `X-WOPI-Token` 头作为首选认证途径，query 参数 fallback 兼容旧客户端
  - 所有 WOPI 响应附加 `Cache-Control: no-store`，避免 token 通过浏览器缓存泄漏
  - `access_token` query 参数由必填改为可选
  - 默认 WOPI access token TTL 从 60 分钟降为 15 分钟

- **分享下载计数原子性**
  - `increment_download_count` 成功后立即重读 share 记录，避免使用 stale 内存值做 limit 检查
  - 用直接 `>=` 重读后的计数器替代 `saturating_add(1)` 比较
  - 重载失败时记录 warning 并回落缓存失效
  - 调试日志补充 `share_id`、`download_count`、`max_downloads`

- **管理后台量纲单位输入控件**
  - 新增 `AdminNumberUnitInput` 通用数值 + 单位下拉控件
  - 配额输入由硬编码 MB 替换为 `AdminStorageQuotaInput`（字节 → TB）
  - 系统设置中的缩放数值字段统一使用共享的单位组件
  - 系统设置说明文案移到 help trigger tooltip 后面，降低视觉噪音
  - 缩放数值控件加入实时校验：拒绝超 `Number.MAX_SAFE_INTEGER` 的换算结果、保留无效草稿与显示单位
  - `AdminTeamDetailDialog` 配额状态合并为单一 `quotaDraftOveride`，修正 save/delete/restore 时的 stale draft 行为
  - 配额校验接受 0 与正整数；非正乘数与负向换算被 schema 拒绝

- **前端缩略图与认证缓存隔离**
  - 持久化用户缓存仅保留 profile、preferences、token 过期时间，剥离 `id`/`email`/`role`/`storage`
  - 读取既有缓存时迁移并清除敏感字段
  - 缩略图缓存命名空间从用户 ID 派生改为 sessionStorage 中的 session UUID
  - 会话过期时清理命名空间

- **前端 i18n 与 PWA 预热重排**
  - 拆分 i18n 加载为 authenticated shell 与完整 bundle，shell 仅含 core/files/tasks/share/search/errors/offline
  - 登录成功路径直接预热 shell i18n 与文件浏览器路由
  - 用户路由预热在登录后路径触发后跳过，避免重复
  - 预览引擎按需在进入文件浏览器时预热
  - `App` 鉴权检查与 shell i18n 全部 ready 后再渲染认证路由，避免未翻译闪烁
  - Service Worker 静态资源缓存策略由 StaleWhileRevalidate 改为 CacheFirst，离线场景更稳

- **前端文件浏览器 / 上传 / 缩略图体验**
  - `useEnteredViewport` 增加 `trackVisibility`/`isInViewport`；`FileThumbnail` 仅在视窗内拉取
  - 持久化缩略图按 ETag 做后台 revalidation；离开视窗时清理 blob URL 释放内存
  - `useUploadAreaUploads` 增加 `retryingTaskIdsRef` 集合，去抖并发重试触发
  - `summarizeUploadTasks` 进度分母改用 `progressCount`，按尺寸加权
  - `UploadPanel` 引入 `taskRowKey`、向 virtualizer 传 `getItemKey`，状态变化时不复用旧行
  - `FolderPolicyDialog` 与 `FileBrowserPage` 多个 UI 状态迁移到 `useReducer`，加入 `targetKey` 哨兵丢弃过期异步结果
  - 升级到 `@testing-library/user-event` 模拟真实交互

- **路由与延迟加载**
  - 引入 `localizedLazyPage` helper，在渲染 lazy 页面前预加载所需 i18n 命名空间
  - `AdminRoute` 渲染前预加载 `admin`、`core` 命名空间，失败降级并 warn
  - 调整 `/external-auth/links` 路由顺序，避免被 `/{kind}/{provider}` 通配吞掉

- **token 生成方案**
  - `new_share_token()` 由 8 字符 base62 改为 32 字符 UUID v4 hex
  - 移除自定义字符集与未使用的 `rand::RngExt` 引用

- **窗口跳转安全属性**
  - 分享视图下载与文件夹下载的 `window.open` 全部传入 `noopener,noreferrer` 三参数

- **依赖更新**
  - `nom-exif` 3.6.0 → 3.6.1，`aws-smithy-types` 1.4.9 → 1.5.0，`aws-smithy-eventstream` 0.60.20 → 0.60.21
  - `block-buffer`、`cc`、`memchr`、`regex`、`regex-syntax`、`rust_decimal`、`smallvec`、`time`/`time-core`/`time-macros`、`uuid`、`zerocopy`/`zerocopy-derive` 等小幅更新
  - 移除未使用的 `powerfmt` 依赖
  - `Cargo.toml` 新增 `AptS-1543` 作为作者

### Fixed

- **分享归档下载并发与回滚**
  - 创建/消费归档票据时预留并回滚分享下载计数
  - 客户端中断或下游流失败时将下载计数回滚到 0，错误日志补充 `share_id`
  - 缓存校验后的文件夹 ID，避免重复鉴权
  - `FileCard` checkbox 的 `onChange` fallback 防止未定义 handler 报错

- **管理员配额草稿**
  - `quotaValueToBytes` 比较实际字节值而非显示字符串，避免误判变化
  - `AdminNumberUnitInput` 在 `invalidState` 时为数字输入和单位选择器都加 destructive 边框
  - 单位变化 handler 改为基于 units 数组匹配，避免空值类型不安全比较
  - 草稿无效时保留显示而非静默丢弃
  - `UserDetailDialogBody` 拆分为 Content / Footer / Profile / Security 子组件，提高可读性

- **路由防护与并发刷新**
  - `test_concurrent_refresh_same_token_has_single_winner` 中为获胜 token 种入 CSRF token
  - `routeGuards.test.tsx` 补充 `ensureI18nNamespaces` 与 logger mock，新增管理员 locale 加载失败用例

- **杂项修复**
  - 远端驱动 `list_uploaded_part_details` 过滤无效 part number（≤0）
  - S3 `list_parts` 翻页缺 `next_part_number_marker` 时正确处理
  - MFA 测试断言更新为 `401 UNAUTHORIZED` + `auth.credentials_failed`，与未验证邮箱登录现行行为一致
  - 分享下载并发测试隔离到独立的 SQLite 文件，并使用 pool-size-1 的连接避免共享态干扰

### Security

- 公开分享密码 Cookie / 公共直链 / 预览链接 / 分享流式播放会话切换到独立 HMAC 密钥
- 分享密码 Cookie 绑定 user agent 哈希与 IP 子网，阻断跨客户端重放
- 多部分与直传上传校验实际上传大小、对照策略上限、按真实大小判额
- WebDAV 路径规范化阻断目录穿越（含百分号编码 `..` 变体）
- WebDAV XML 端点拒绝 DTD/ENTITY，缓解 XXE
- WebDAV 单用户活跃锁数量限额，避免资源滥用
- 登录与激活重发统一错误码 + 响应时延下限，缓解账号枚举
- 健康检查不再泄漏版本与构建时间，存储就绪不再泄漏驱动错误细节
- WOPI 优先头部认证、响应 `Cache-Control: no-store`、默认 TTL 缩短到 15 分钟
- 持久化的用户缓存剥离 ID/email/role/storage 等敏感字段
- 缩略图缓存命名空间从用户 ID 切换为 session UUID
- 共享 RFC 5987 编码净化 `\r`/`\n`/`\0`，防 Content-Disposition 头注入
- `window.open` 调用统一附加 `noopener,noreferrer`，防 tabnabbing 与 referrer 泄漏
- WebDAV 缓存认证命中时复核账号状态与密码哈希

### Testing

- 新增 schema drift 集成测试，覆盖 SQLite/PostgreSQL/MySQL
- 新增激活重发的活动账号、禁用账号、邮件策略黑名单场景集成测试，验证跳过场景不发邮件
- 新增 WebDAV 锁额度、路径穿越（多种 `..` 编码）、Timeout 边界、XXE 与 cached auth 复核测试
- 新增分享下载计数原子性测试：32 并发对 `max_downloads=1` 仅一例预留
- 新增分享归档下载中断回滚集成测试：下载计数回 0、后续下载可正常进行
- 新增直传上传策略边界、metadata 大小溢出 `i64::MAX` 测试
- 新增 password 生成保证四类字符的回归测试
- 新增 `FolderPolicyDialog` 关闭、stale 结果丢弃、保存失败保留对话框、空策略列表等覆盖
- 新增 i18n 命名空间预加载、admin 命名空间加载失败的路由测试
- 新增 `AdminSettingsConfigRows` 部分更新时配置保留的测试
- 修复 `AdminTeamDetailDialog` 测试在 `waitFor` 中加入值断言以避免竞态

### Database Migrations

无新增迁移。

### Configuration Changes

- 新增 `auth.share_cookie_secret`、`auth.direct_link_secret`（缺失时启动自动补齐）
- 新增 `archive_download_user_enabled`、`archive_download_share_enabled` 运行时开关
- 新增 `webdav.max_active_locks_per_user`（默认 1024）
- WOPI 默认 access token TTL 由 60 分钟降为 15 分钟

### Statistics

- 249 files changed, 11957 insertions(+), 2361 deletions(-)
- 10 commits

---

## [v0.3.0-alpha.4] - 2026-06-11

### Release Highlights

**AsterDrive `0.3.0-alpha.4` 聚焦 PWA 启动性能审计、API 错误码细粒度分类、Service Worker 缓存优化和数据库类型约束统一。** 本版本新增启动性能监测工具链，可自动生成性能报告和 Web Vitals 指标；API 错误码按模块（search, policy, storage, tasks）分层细化，提升调试和文档准确度；Service Worker 缓存策略重构以支持精细粒度版本控制；运行时数据库类型约束统一为 `DatabaseConnection`，移除 `ConnectionTrait` 泛型冗余。同时更新安全策略文档支持分支标识为 `master`，修复文档示例。

- **PWA 启动性能审计** — 自动化性能测试脚本、Web Vitals 指标收集、HTML 报告生成
- **API 错误码细粒度分类** — search / policy / storage / tasks 模块独立错误码集
- **Service Worker 缓存优化** — 支持版本级别缓存隔离和更新策略
- **数据库类型约束统一** — `ConnectionTrait` 泛型移除，使用 `DatabaseConnection` trait object
- **文档和配置更新** — 安全策略分支引用更新、Cargo.toml 优化

### Added

- **PWA 启动性能审计工具**
  - 新增 `frontend-panel/scripts/audit-startup.mjs` 自动化审计脚本
  - Web Vitals 指标收集（LCP、FID、CLS、FCP、TTFB）
  - HTML 报告生成，支持分数评级和详细指标
  - 性能基线和阈值检测

- **API 错误码细粒度分类（Breaking）**
  - `search` 模块：`search.invalid_query`、`search.query_timeout` 等
  - `policy` 模块：`policy.not_found`、`policy.driver_type_mismatch` 等
  - `storage` 模块：`storage.quota_exceeded`、`storage.access_denied` 等
  - `tasks` 模块：`tasks.not_found`、`tasks.invalid_state` 等
  - 文档同步更新，所有错误码映射到细分类别

- **Service Worker 缓存版本隔离**
  - 版本级别缓存键支持（cache name 包含应用版本）
  - PWA 路由预热阶段增强
  - 缓存更新和清理策略优化

### Changed

- **数据库访问类型统一（Breaking on internal API）**
  - `ConnectionTrait` 泛型完全移除
  - 所有服务层和 API 路由使用 `DatabaseConnection` trait object
  - 零成本抽象，行为完全一致
  - 影响 40+ 个文件

- **文档和配置更新**
  - `SECURITY.md` 支持分支从 `main` 更新为 `master`
  - `CONTRIBUTING.md` 示例仓库 URL 更新
  - `Cargo.toml` release 配置：`strip = false` → `strip = true`

### Fixed

- **启动性能测试覆盖**
  - 新增 `frontend-panel/src/lib/pwaWarmupLoaders.test.ts` 单元测试
  - 改进初始化路由加载逻辑验证

### Database Migrations

无新增迁移。

### Statistics

- 150 files changed, 3676 insertions(+), 1022 deletions(-)
- 6 commits

---

## [v0.3.0-alpha.3] - 2026-06-10

### Release Highlights

**AsterDrive `0.3.0-alpha.3` 是 `0.3.0-alpha.2` 的发布流水线修正版。二者在应用代码、数据库迁移、运行时配置和用户可见功能上等价；`alpha.3` 仅用于重新发布完整的 GitHub Release 资产。** `0.3.0-alpha.2` 的首次发布触发了 GitHub immutable release 限制，导致部分归档资产未能上传完成；本版本通过先创建 draft release、上传全部资产后再发布，修复该发布流程问题。由于本次修正只影响 GitHub Release 发布流程，Docker 镜像或镜像内版本元数据可能仍标识为 `0.3.0-alpha.2`，这与 `0.3.0-alpha.3` 的应用层内容等价。

### Changed

- **发布流程修复**
  - GitHub Release 现在先以 draft 形式创建并上传全部归档资产，再发布为正式 release / prerelease
  - 避免 immutable release 仓库中 release 已发布后继续上传 assets 导致失败
  - `v0.3.0-alpha.3` 与 `v0.3.0-alpha.2` 的应用层变更等价，完整功能变更见 `v0.3.0-alpha.2`
  - Docker 镜像或镜像内版本元数据可能仍显示为 `0.3.0-alpha.2`，属于发布标识差异，不代表功能或代码差异

## [v0.3.0-alpha.2] - 2026-06-10

### Release Highlights

**AsterDrive `0.3.0-alpha.2` 是 0.3.0 系列的第二个预发布版本，重点提升存储策略管理、用户安全控制和文件浏览体验。** 本版本引入 S3 兼容存储驱动自动提升机制与路径样式配置，支持更灵活的对象存储集成；新增强制密码修改流程，管理员可要求用户在首次登录或特定场景下强制更新密码；标签系统增强了创建能力与实时事件通知；文件浏览器完成 UI 重构，预览对话框、音乐播放器、过滤工具栏全面升级。同时修复了用户登录统计遗漏 passkey 和外部认证的问题，并优化了 CI 发布流程。

- **S3 存储策略增强** — 驱动提升、path-style 配置、存储向导重构
- **强制密码修改** — 首次登录强制改密、管理员手动触发、完整审计流
- **标签管理提升** — 用户可直接创建标签、存储事件通知、UI 优化
- **文件浏览器重构** — 统一过滤工具栏、预览对话框改版、音乐播放器增强
- **可配置图片尺寸** — 缩略图和预览图尺寸可通过系统配置动态调整

### Added

- **强制密码修改流程**
  - 新增 `user.must_change_password` 字段（迁移 `m20260610_000001_add_user_must_change_password`）
  - 受限 token 机制：登录时若需强制修改密码，签发带 `password_change: true` 的受限 token
  - 受限 token 只能访问 `/api/v1/auth/password/change` 和 `/api/v1/auth/logout`
  - 密码修改增强：拒绝新旧密码相同、成功后自动清除 `must_change_password` 标志
  - 管理员用户创建增强：密码可选（留空生成 24 字符临时密码），返回 `generated_password`
  - 管理员可手动触发/清除用户强制修改密码要求
  - 前端新增 `ForcePasswordChangePage`、`GeneratedPasswordDialog`、`UserSecurityActionsSection`
  - 路由守卫：`LoginGuard` 和 `ProtectedRoute` 检测受限 token 并跳转到强制修改页面
  - 国际化支持（中英）
  - 完整测试覆盖（受限 token、临时密码、审计脱敏）

- **可配置缩略图和预览尺寸**
  - 新增配置项：`thumbnail_max_dimension`（默认 400px）和 `image_preview_max_dimension`（默认 1600px）
  - 非默认尺寸使用尺寸特定缓存路径（如 `1-d320`、`1-d2048`）
  - 所有派生渲染路径（vips_cli、ffmpeg_cli、lofty、storage_native）传递配置尺寸
  - 配置校验：范围 1–16384，默认值使用默认缓存路径

- **标签管理增强**
  - 标签库管理器内联创建：搜索查询无匹配时显示"创建标签"按钮，支持 Enter 快捷键
  - 标签内联颜色编辑：编辑器中新增颜色选择器，导出 `TAG_COLOR_PALETTE` 供复用
  - 存储变更事件：新增 `tag.created`、`tag.updated`、`tag.deleted`、`tag.assignment_changed` 事件
  - 前端实时订阅：`SearchBrowserPage` 和 `CategoryBrowserPage` 订阅标签事件，影响显示文件时重新加载
  - UI 改进：对话框滚动布局修复、关闭动画期间保留草稿状态
  - 新增 `affected_parent_ids_for_entities()` 辅助函数（分块查询，每批 500）

- **锁定状态变更通知和分享刷新**
  - 新增 `lock.created`、`lock.deleted` 存储事件
  - `ShareDialog` 新增 `onShareCreated` 回调（页面分享创建后刷新文件浏览器列表）
  - 修复 `onShareCreated` 同步抛出回归：用 `.then()` 包装，异常被 `.catch()` 捕获

- **在线归档压缩开关**
  - 新增 `archive_compress_enabled` 配置键（默认 true）
  - 标志关闭时返回 `archive_compress.disabled`（HTTP 403）
  - 国际化支持

- **S3 兼容驱动提升和路径样式控制**
  - 新增 `POST /api/v1/admin/policies/{id}/promote-s3-driver` 端点
  - 驱动提升守卫：显式白名单（S3 → TencentCos）、活动上传会话检查、存储桶不可变性验证
  - S3 路径样式控制：`StoragePolicyOptions` 新增 `s3_path_style` 字段（默认 true）
  - 移除 Cloudflare R2 特定逻辑：不再重写 R2 URL 或拒绝 `.r2.dev`
  - 前端 UI：创建向导检测腾讯 COS 端点时显示驱动建议横幅，编辑表单显示提升面板
  - 新增 `S3PathStyleField` 开关（仅对通用 `s3` 驱动可见）
  - 表单稳定性改进：用 `useRef` 替换 `useState`+`useEffect` 避免陈旧状态
  - 添加深度相等检查 `policyFormValueEquals` 检测未保存更改

- **UI/UX 重构和增强**
  - 新增 `AdminFilterToolbar` 可折叠组件（带切换按钮和活动过滤器徽章）
  - 新增 `useRetainedDialogValue` Hook（关闭动画期间保留对话框内容）
  - 全局搜索过滤：过滤器折叠在可切换内联按钮后，标签选项隐藏在二级选择器
  - 搜索从对话框改为全页面，新增 `/search` 和 `/teams/:id/search` 路由
  - 管理侧边栏导航重新排序
  - 个人资料设置视图重构：使用 `SettingsRow` 布局、`usePendingAction` Hook
  - 安全页面改进：每个面板添加 `descriptionKey`，大屏两列布局
  - MFA 动作简化：移除自定义动作组件，用标准 `animate-in`/`fade-in` 类替换
  - 关于页面重设计：两列网格布局、色条装饰、构建详情网格、四个功能卡片
  - 设置 UI 密度收紧：减少间距（space-y-10 → space-y-6）、缩小导航宽度
  - 保存栏动画改进：基于 CSS transition、`latestVisibleStateRef` 冻结退出时内容
  - 文件浏览器重设计：文件卡片左对齐、元文本行显示大小、文件夹琥珀色图标容器
  - 文件预览增强：新增 `FilePreviewFileSummary` 组件、预览表面组件系统（`PreviewSurface` 系列）

### Changed

- **CI/CD 优化**
  - GitHub Release 发布流程改进：二进制文件打包成归档文件（tar.gz/zip）后再上传
  - Linux/macOS target 使用 `.tar.gz`，Windows target 使用 `.zip`
  - Release Notes 更新下载链接和校验说明

- **测试覆盖增强**
  - E2E 测试：适配 UI 变更，新增文件浏览器过滤器交互测试
  - 新增单元测试：标签创建、强制密码修改、认证资源、预览组件

- **依赖更新**
  - `wasm-bindgen` 升级到 0.2.123
  - 新增 `audit.toml` 告警抑制配置

### Fixed

- **用户登录统计修复**
  - 修复：管理服务中的登录次数统计现在正确包含 passkey 和外部认证登录
  - 之前只统计密码登录，导致 WebAuthn 或 OIDC 登录用户统计不准确

### Database Migrations

- `m20260610_000001_add_user_must_change_password` — 在 `user` 表新增 `must_change_password` 字段（默认 false）

### Configuration Changes

- 新增配置项：
  - `thumbnail_max_dimension` — 缩略图最大尺寸（默认 400px，范围 1–16384）
  - `image_preview_max_dimension` — 预览图最大尺寸（默认 1600px，范围 1–16384）
  - `archive_compress_enabled` — 在线归档压缩开关（默认 true）
- 存储策略支持 `s3_path_style` 配置（S3 兼容存储）

---

**统计数据**：
- 339 files changed, 15,023 insertions(+), 3,282 deletions(-)
- 8 commits

## [v0.3.0-alpha.1] - 2026-06-09

### Release Highlights

**AsterDrive `0.3.0-alpha.1` 是 0.3.0 系列的预发布版本，聚焦 API 错误码协议统一、用户邀请流程、文件标签体系和运行时架构解耦。** 本版本将后端双轨制的 `ErrorCode`（数值）与 `ApiSubcode`（字符串）合并为单一的字符串 `ApiErrorCode`，作为前端与文档的唯一稳定错误码来源；新增用户邀请系统，管理员可通过邮件发送一次性注册链接；上线文件/文件夹标签系统，支持工作区作用域、批量操作与搜索集成；运行时状态拆分为可组合的 trait 体系，为多运行时场景铺路。UI 侧同步完成 action menu、z-index 体系、分类浏览页与全页面搜索重构。

- **统一 API 错误码协议** — 合并 `ErrorCode`/`ApiSubcode` 为字符串 `ApiErrorCode`，内部存储协议升至 v4，向后不兼容
- **用户邀请系统** — 管理员邮件邀请、一次性注册链接、状态追踪、撤销、可定制邮件模板
- **文件/文件夹标签系统** — 工作区作用域、批量绑定、搜索过滤、可视化颜色管理
- **运行时 trait 架构** — `PrimaryAppState`/`FollowerAppState` 改为组合式 trait，提升可测性与多运行时扩展能力
- **API 显式状态访问** — 全部 API 路由统一使用 `state.get_ref()`，移除隐式 Deref，提升类型安全
- **前端体验工程化** — action menu、语义化 z-index token、分类浏览页、全页面搜索、内联确认 UI

### Added

- **用户邀请系统**
  - 新增 `user_invitations` 表，支持 pending / accepted / expired / revoked 状态流转
  - 邀请仓库、服务层、token 生成与校验逻辑
  - 管理端 API：创建、列表、撤销邀请
  - 公开 API：校验与接受邀请
  - 邀请专属错误码（invalid、expired、revoked、accepted）
  - 邀请邮件模板可定制（HTML + 主题，支持中英）
  - 自动过期与撤销机制 + 审计日志覆盖
  - 前端 `InviteUserDialog`、`UserInvitationsTable`、`InviteRegisterPage` 组件
  - `LoginPage` 集成邀请流程，错误码国际化
  - 接受邀请时已登录用户状态的正确处理

- **文件/文件夹标签系统**
  - 新增 `tags` 表，支持 personal/team 工作区作用域，name 规范化索引
  - 标签 CRUD：创建、重命名、改色、删除
  - 标签绑定端点：附加/移除文件和文件夹的标签
  - 批量标签操作（跨多文件/多文件夹）
  - 标签过滤集成到现有文件/文件夹搜索
  - 标签生命周期与绑定操作的审计日志
  - 前端 `TagChips` 组件（颜色编码 + 溢出处理）
  - 前端 `TagManagerDialog`（单/批量管理）+ `TagLibraryManagerDialog`（工作区级别管理）
  - 标签接入文件浏览器（卡片/表格行/右键菜单/批量操作）
  - 标签显示与管理接入文件/文件夹信息弹窗
  - 全局搜索新增 any/all 匹配模式的标签过滤
  - 文件浏览器工具栏与右键菜单新增标签库管理入口
  - 中英文翻译
  - 重命名动作语义：`copy` → `copy_to`、`move` → `move_to`
  - API 端点：
    - `GET /api/v1/tags`、`POST /api/v1/tags`、`PATCH /api/v1/tags/:id`、`DELETE /api/v1/tags/:id`
    - `GET/PUT /api/v1/tags/:entity_type/:entity_id` 实体标签查询与替换
    - `PUT/DELETE /api/v1/tags/:tag_id/:entity_type/:entity_id` 单条附加/移除
    - `PUT/DELETE /api/v1/tags/:tag_id/batch` 批量附加/移除
    - 团队工作区下镜像端点 `/api/v1/teams/:team_id/tags`

- **邮件审计日志**
  - 新增审计动作 `mail_send` 与 `mail_delivery_failed`
  - 新增审计实体类型 `mail`
  - 记录邮件投递尝试（模板、收件人、错误详情）
  - 覆盖 outbox dispatcher 与直发场景（MFA、配置测试）
  - 审计字段增强：可选 IP、User-Agent
  - 敏感字段（收件人名、主题、错误）UTF-8 安全截断至 1024 字符
  - 前端 i18n 邮件审计条目支持（中英）

- **前端 UI 体验**
  - 新增 `ManagerDialogShell` 通用对话框骨架（固定头/可滚中/固定底）
  - `AdminTableList` 新增工具栏、分页、过滤空态支持
  - 文件浏览器新增每条目 action menu（`FileBrowserItemActionMenu`）
  - 全局搜索 header 新增激活过滤 chip 条，单条移除
  - 新增 `usePendingAction` Hook 防止异步重复提交
  - 分类浏览页：视频/音频缩略图生成、文件位置跳转（"Go to file location"）
  - 侧边栏分类链接从触发搜索改为直接导航
  - 分类视图支持无限滚动（每页 100）
  - 搜索 API 新增 `sort_by`/`sort_order` 参数
  - 分类浏览页新增文件信息面板，状态与列表同步

### Changed

- **API 错误码协议统一（Breaking）**
  - 移除后端 `error_code.rs`（数值 `ErrorCode`）与 `subcode.rs`（`ApiSubcode`）
  - 全部 `*_with_subcode` 助手函数重命名为 `*_with_code` 变体
  - `AsterError` 用 `api_error_code_override()` 替代 `api_error_subcode()`
  - `ApiResponse.code` 字段从数值 `ErrorCode` 改为字符串 `ApiErrorCode`
  - OpenAPI schema 移除 `ApiSubcode` 和 `ErrorCode`，仅保留 `ApiErrorCode`
  - `ApiErrorInfo` 响应契约移除 `subcode` 和 `internal_code` 字段
  - 内部存储协议版本从 v3 升级到 v4，最小支持版本同步升至 v4（向后不兼容）
  - `StoragePolicyCleanupRemoteNodeSnapshot` 新增 `last_capabilities` 字段（serde default）
  - 前端全部从 `ErrorCode`/`ApiSubcode` 迁移到 `ApiErrorCode` 字符串
  - `ApiError` 构造器简化为 `(code, message)`，移除旧 subcode 包装
  - `useApiError` 移除 subcode 分类逻辑，统一走 `error.code`
  - 集成测试全部改为字符串 code 断言（`"success"`、`"auth.token_missing"`）
  - 公共 API 示例统一使用 `code: "success"` 与字符串错误码

- **运行时架构重构（Breaking on internal API）**
  - `PrimaryAppState`/`FollowerAppState` 引入 trait 体系：
    - `SharedRuntimeState` 统一访问 config / db / cache / storage / policy / metrics / mail
    - 专用 trait：`TaskRuntimeState`、`MailRuntimeState`、`StorageChangeRuntimeState`、`RemoteProtocolRuntimeState`
  - 服务层参数从具体类型改为 trait bound（`impl SharedRuntimeState` 等）
  - 字段访问改为方法调用（`state.config` → `state.config()`）
  - `PrimaryRuntimeState` 拆分为 4 个专用 trait + `TaskRuntimeState`
  - 40+ 个服务函数接受 `SharedRuntimeState` 或具体子 trait
  - `web::Data<T>` 提供 blanket impl 保持 API 兼容
  - `TaskRuntimeState` 新增 `wake_background_task_dispatcher`
  - 健康检查改用 `RemoteProtocolRuntimeState` 进行远程节点测试
  - 影响 208 个文件，零成本抽象，行为完全一致

- **API 显式状态访问**
  - 全部 API 路由用 `state.get_ref()` 替代隐式 Deref
  - 中间件显式调用运行时 config
  - 主节点/从节点健康检查显式状态访问
  - WebDAV 与远程 tunnel 客户端同步更新
  - 移除 `PrimaryAppState` 的隐式 `Deref` 实现
  - 影响 44 个文件

- **前端架构改进**
  - `AppLayout` 和 `TopBar` 移除 `actions` prop，搜索按钮移至 `HeaderControls` 作为 `mobileSearchAction`
  - `EditShareDialog`、`TagLibraryManagerDialog`、`TagManagerDialog` 迁移到 `ManagerDialogShell`
  - 管理页（Tasks / Teams / Users / Invitations / External Auth）改用 `AdminTableList` + 拆分表头/行
  - 主题切换：用自定义分层动画替代 View Transition API，跨浏览器一致
  - 主题切换新增光泽叠加层，卸载时清理防内存泄漏
  - 搜索从对话框改为全页面结果浏览器，支持 Enter 提交
  - 搜索 header 新增 `onSubmitSearch` 回调与 `searchReady` 状态
  - 文件 store 移除 `searchQuery`、`searchFiles`、`searchFolders`、`search()`、`clearSearch()`
  - 路由新增 `/search` 和 `/teams/:id/search`
  - 破坏性操作改用内联确认 UI（团队成员移除、WebDAV 账号删除、存储策略连接测试、远程节点 ingress profile 删除、文件预览未保存改动、团队归档）
  - 文件卡片 action menu 在桌面端隐藏，让出空间给状态指示
  - 账户菜单下拉尺寸与间距针对移动视口优化
  - 上传面板展开/收起状态与底部 padding 联动系统
  - 删除对话框形式的 `GlobalSearchResultRow` / `GlobalSearchResultsPanel`

- **主题/UI 体系**
  - 引入语义化 z-index token 系统（`--z-fixed`、`--z-dialog`、`--z-dropdown`、`--z-popover`、`--z-tooltip`、`--z-alert-dialog`、`--z-toast`）
  - 固定 chrome 元素（批量操作栏、侧边栏、上传面板、音乐播放器）统一为 `--z-fixed`
  - 全部分层堆叠顺序：fixed (40) < dialog (50) < dropdown/popover (60) < tooltip (65) < alert-dialog (70) < toast (80)
  - 文件浏览器选择工具栏改为 absolute 定位覆盖层，`bg-card` 背景
  - 上传拖拽覆盖与设置保存条统一用 CSS 变量 token

- **其他工程化**
  - jemalloc 配置按平台拆分，Linux 优化设置
  - tunnel 在线检测改为完全依赖心跳时间戳
  - mock auth server 配置为单 worker 防竞态
  - 外部认证测试中的 reqwest 模式整合
  - 移除重复的数据库后端断言
  - `.cargo/audit.toml` 移除已修复的 RUSTSEC-2026-0097
  - GitHub Actions 升级 codecov-action 到 v7
  - MSRV 从 1.91.1 升至 1.94.0

- **批量移动性能优化**
  - 新增 `find_by_names_in_parent`、`find_by_names_in_team_parent` 等仓库方法
  - 批量移动用 `load_target_file_name_map`/`load_target_folder_name_map` 做批量名查重
  - 数据库查询从 O(n) 降为 O(1) 的冲突检测
  - 批量移动新增 Unicode 归一化（NFC/NFD）支持，防止误冲突
  - 归一化查询变体生成，NFD 兜底查找
  - k6 性能测试新增分片上传时序指标（init / chunk / complete / client gap）
  - 修复 k6 API 成功码检查，同时支持字符串 `"success"` 和数值 0

### Fixed

- 前端防止重复提交（文件/文件夹创建对话框 + `usePendingAction`）
- 前端认证错误处理：502/503/504 网关错误保留缓存 auth 状态，401/403/token 错误才强制登出
- `ApiError` 类新增 `status` 字段并贯穿错误链
- `readHttpStatus` 直接从 error 对象提取 status
- refresh token 失败设置 `isAuthStale` 触发重试
- 修复清理失败时的孤立存储对象问题
- 改进删除操作的原子性和一致性
- 隧道心跳在轮询周期间的可靠性
- k6 客户端代码格式化与成功码解析
- 主题切换在组件卸载时的资源清理

### Security

- 外部认证 URL 配置校验与专门化检查
- 本地邮箱策略防止非预期注册
- 邀请链接使用安全 token 哈希
- 接受邀请前验证状态
- 邀请端点禁止 token refresh 尝试
- 一次性邀请自动更新状态
- 邮件审计敏感字段 UTF-8 安全截断

### Testing

- 新增组件测试：邀请对话框、邀请表格、ManagerDialogShell、FileBrowserItemContextMenu、FileInfoDialog、FileThumbnail
- 新增 z-index 分层与 token 使用验证套件
- 新增底部覆盖偏移与 z-index 分配的覆盖测试
- 新增排序搜索结果测试（文件按 size desc、文件夹按 name desc）
- 新增重复点击场景下的提交保护测试
- 新增 502/503/504 网关错误在 auth 检查与 token 刷新中的边界测试
- 新增 ApiError 状态保留测试
- 新增 `isSessionAuthFailure` 多种状态码测试
- 新增邮件审计字段 UTF-8 截断单元测试
- 新增隧道心跳跨轮询周期测试
- e2e 测试统一通过 `E2eApiResponse<T>` + `expectApiSuccess` 助手 + `E2E_API_SUCCESS_CODE` 常量
- e2e 搜索流程更新为"先提交再导航到结果页"
- 批量移动新增 Unicode 归一化冲突检测测试 + 索引验证测试
- 批量空请求错误码精确匹配测试（BadRequest）
- OpenAPI 测试验证所有 `ApiResponse` schema 引用 `ApiErrorCode`
- 迁移测试新增 `seed_user_invitation_fixture` 与 `seed_tag_fixture` 断言
- OIDC 测试断言更新为 `bad_request` 而非旧 `wopi.public_site_url_required`

### Documentation

- **API 错误码 v4 迁移**
  - 公共 API 示例移除旧数值错误码、`error.code`/`error.subcode`/`error.internal_code`
  - 响应示例统一 `code: "success"` + 字符串错误码
  - 移除错误码范围表与数值-字符串映射表
  - 错误处理文档聚焦顶层 `code` 字段
  - 内部存储协议 v4 文档化（向后不兼容）
  - 部署/排错文档统一指向 `code` 字段
  - GitHub Actions workflow 触发改为发布版本
  - 日志文档引用 API 响应的 `code` 字段
  - 错误码契约说明合并为单一权威来源

### Notes

- 本版本为 `0.3.0` 首个预发布版本（`alpha.1`）
- **Breaking Change**：API 错误码协议
  - 公共错误码改为字符串 `ApiErrorCode`，原 `ErrorCode`（数值）与 `ApiSubcode` 已移除
  - `ApiErrorInfo` 移除 `subcode` 和 `internal_code` 字段
  - 内部存储协议 v3 → v4，**最低支持版本升至 v4**，v3 节点无法与 v4 主/从节点互通
- **Breaking Change**：内部 API
  - 运行时 trait 体系替代具体类型参数（影响 service 层内部调用，不影响 HTTP API）
  - API 路由显式 `state.get_ref()` 替代隐式 Deref（编译期错误，不影响运行时）
- **Breaking Change**：移除的废弃端点
  - `/api/v1/public/branding` 已移除，统一改用 `/public/frontend-config`
- **Breaking Change**：缩略图能力响应
  - `PublicThumbnailSupport.extensions` 扁平字段移除，改为 `image_thumbnail.extensions` / `audio_thumbnail.extensions` 能力字段
- 新增数据库迁移：
  - `m20260607_000001_add_user_invitations` — 用户邀请表
  - `m20260608_000001_add_tags` — 标签系统表
- 预发布版本建议在测试环境使用，不推荐生产部署
- 客户端集成需同步更新：
  - 解析 `code` 字段为字符串而非数值
  - 弃用 `error.subcode` / `error.internal_code` 解析逻辑
  - 缩略图能力查询改用 `image_thumbnail.extensions` / `audio_thumbnail.extensions`
  - 邀请流程使用 `/api/v1/invitations` 系列端点
  - 标签操作使用 `/api/v1/tags` 系列端点

---

**统计数据**：
- 675 files changed, 29,067 insertions(+), 11,921 deletions(-)
- 20 commits
- Rust Edition 2024, MSRV 1.94.0

## [v0.2.7] - 2026-06-06

### Release Highlights

**AsterDrive `0.2.7` 聚焦企业级登录、图像预览、WebDAV 协议合规和存储驱动多样化。** 本版本新增 GitHub、Google、Microsoft、QQ 四大 OAuth2/OIDC 提供商支持，用户可通过单点登录快速接入；完整实现图像全屏预览、缩放、旋转和 AVIF 原生支持；WebDAV 协议大幅改进 RFC 4918 合规性，支持多活锁定、递归冲突检测和共享锁；新增腾讯云 COS 存储驱动，支持原生媒体处理和缩略图生成；企业功能继续强化，支持邮箱策略、Passkey 登录控制和团队级策略组迁移。

- **OAuth2/OIDC 外部认证** — 新增 GitHub、Google、Microsoft、QQ 四大提供商，支持单点登录和 Microsoft 租户管理
- **完整图像预览系统** — 全屏查看、缩放、旋转、导航，原生 AVIF 支持和浏览器能力检测
- **WebDAV RFC 4918 合规** — 多活锁定、递归冲突检测、共享锁支持、If header 处理完善
- **腾讯云 COS 驱动** — 原生媒体元数据和缩略图生成支持，S3 寻址风格配置
- **企业认证策略** — 本地邮箱黑名单/白名单、Passkey 登录控制、策略组团队扩展
- **性能与稳定性** — jemalloc 内存管理、缩略图缓存优化、并发限制、心跳隔离

### Added

- **OAuth2/OIDC 外部认证**
  - 新增 GitHub OAuth provider，自动提取已验证主邮箱
  - 新增 Google OIDC provider，支持标准配置
  - 新增 Microsoft Entra provider，支持租户管理和自定义配置
  - 新增 QQ Connect OAuth2 provider
  - 提供商特定选项支持，Microsoft 租户值规范化
  - 防止专门化提供商的 URL 配置覆盖
  - 审计日志记录外部认证相关操作
  - 前端支持提供商配置表单和设置预检

- **图像预览系统**
  - 全屏图像预览器，支持缩放、平移、旋转
  - 图像预览导航，支持前后切换
  - 原生 AVIF 格式支持
  - 浏览器渲染能力检测，HEIF/HEIC 优雅降级
  - 图像预览策略配置
  - 每次预览的缩略图能力检测
  - 延迟生成优化，缓存命中前不处理
  - 简化预览状态管理和缩放逻辑

- **WebDAV 协议增强**
  - RFC 4918 完整合规实现
  - 多活锁定支持和有效期修剪
  - 递归操作锁定冲突检测
  - 共享 WebDAV 锁支持
  - If header Not 关键字不区分大小写处理
  - 集中化 HTTP 响应构建器
  - 请求源辅助函数提取
  - 上下文结构整合

- **存储驱动扩展**
  - 腾讯云 COS 驱动实现
    - 原生媒体元数据支持
    - 原生缩略图生成
    - 私有 URL 寻址
  - S3 寻址风格配置（虚拟托管、路径风格），支持腾讯 COS 兼容
  - 远程存储驱动模块化（提取子模块结构）
  - Blob 迁移多部分上传支持
  - 文件和文件夹详情存储使用量跟踪

- **企业认证策略**
  - 本地邮箱白名单/黑名单支持
  - Passkey 登录政策控制
  - 策略组迁移扩展到团队分配

- **性能与运维**
  - jemalloc 内存分配器支持（可选 feature）
  - 最大并发限制和缩略图缓存大小校验
  - 缩略图元数据预检查移除，优化范围读取
  - 心跳独立任务化，防止 SQLite 死锁
  - 存储操作可取消上下文
  - 文件复制逻辑提取和优化

### Changed

- 根 crate 版本从 `0.2.6` 升级到 `0.2.7`
- **WebDAV 架构重构**
  - HTTP 响应构建器集中化（`webdav/responses.rs`）
  - 协议处理独立模块化（`webdav/protocol.rs`）
  - 请求源和上下文结构提取
- **缩略图处理**
  - 移除元数据预检查，直接读缓存
  - 范围读取优化和 Bytes 类型改进
- **存储驱动**
  - 远程驱动分子模块化（`remote/protocol.rs`、`remote/client.rs` 等）
  - Blob 迁移函数签名简化
  - 维护任务禁止缓存预热
- **任务调度**
  - 心跳逻辑移至独立后台任务
  - 防止 SQLite 连接死锁
- **类型安全**
  - 改进流处理和类型安全
  - 外部认证提供商类型完善

### Fixed

- 修复 WebDAV DAV 命名空间前缀声明（RFC 4918 PROPFIND）
- 修复图像预览逻辑和 Microsoft OIDC 遗留发行者处理
- 防止专门化 OAuth 提供商意外的 URL 配置覆盖
- 修复清理失败时的孤立存储对象问题
- 改进删除操作的原子性和一致性

### Security

- 外部认证 URL 配置校验和专门化检查
- 本地邮箱策略防止非预期注册

### Testing

- 新增 WebDAV 完整协议测试（3929+ 行）
- 新增 OAuth2/OIDC 集成测试（覆盖所有提供商）
- 新增存储迁移测试（911+ 行）
- 新增任务管理测试（431+ 行）
- 新增 WebDAV 锁系统专用测试
- 前端新增 admin 设置、预览和外部认证配置测试

### Documentation

- **新增文档**
  - 容量规划与部署建议（`deployment/capacity-planning.md`）
  - 功能指南模块化（认证、文件、预览、上传、运维）
  - 本地存储详细指南
  - 腾讯云 COS 配置与使用
  - 架构设计完整文档
  - jemalloc profiling 指南
- **更新文档**
  - API 文档同步所有新提供商和存储驱动
  - 外部认证文档完全重写（提供商安装、配置、故障排查）
  - WebDAV 和 WOPI 文档反映 RFC 合规改进
  - 配置文档新增认证和存储驱动选项

### Notes

- 本版本为 `0.2.7` 功能与生态扩展版本
- 新增数据库迁移：
  - `m20260604_000001_allow_shared_webdav_locks` — 共享 WebDAV 锁支持
  - `m20260606_000001_add_external_auth_provider_options` — 外部认证提供商选项
- **Breaking Change**：API 端点重命名
  - 策略组迁移端点：`POST /admin/policy-groups/{id}/migrate-users` → `POST /admin/policy-groups/{id}/migrate-assignments`
  - 原因：端点从只迁移用户扩展为同时迁移用户和团队绑定
  - 请求类型：`MigratePolicyGroupUsersReq` → `MigratePolicyGroupAssignmentsReq`
  - 响应类型：`PolicyGroupUserMigrationResult` → `PolicyGroupAssignmentMigrationResult`
  - 响应新增 `affected_teams` 字段
- **Breaking Change**：外部认证 API 调整
  - 新增 4 个 OAuth2/OIDC 提供商类型（GitHub, Google, Microsoft, QQ）
  - 提供商配置新增 `options` 字段
  - Microsoft 提供商支持租户配置（`tenant_id` 规范化）
- **Breaking Change**：WebDAV 协议改进
  - 多活锁定和共享锁数据库模式变更
  - 资源允许多个共享锁，需要使用 `lock_token` 显式释放
  - WOPI 预览生命周期管理改进
- **Breaking Change**：邮箱策略校验
  - 本地邮箱白名单/黑名单只接受 ASCII 域名
  - 拒绝 Unicode 域名（含 punycode）
  - 邮箱验证要求必须只有一个 `@` 分隔符
  - 改进的容错处理（自动 trim 空白、跳过非法条目）
- **Breaking Change**：Passkey 政策
  - 新增 `passkey_login_policy` 配置项
  - 旧数据库缺少该字段时默认为启用状态
- 强类型 API 客户端建议重新生成，以同步外部认证、存储驱动、WebDAV 和策略组迁移接口
- Docker 用户可使用 jemalloc profiling 变体进行内存性能分析
- 自定义客户端实现需要更新对 `migrate-users` 端点的引用，并处理新增的 `affected_teams` 字段

---

**统计数据**：
- 529 files changed, 49,170 insertions(+), 8,301 deletions(-)
- 46 commits
- Rust Edition 2024, MSRV 1.94.0

## [v0.2.6] - 2026-06-02

### Release Highlights

**AsterDrive `0.2.6` 聚焦 aria2 离线下载引擎、后台任务优雅关闭、自定义配置可见性和从节点审计可观测性。** 本版本新增 aria2 外部下载引擎支持，具备断点续传、多连接并发和内置引擎 fallback 能力；后台任务系统引入优雅关闭机制，服务重启时给予 30 秒宽限期让任务安全退出；自定义配置支持 private / authenticated / public 三级可见性控制，前端可按需暴露配置；从节点存储操作补齐完整审计日志和追踪覆盖。

- **aria2 离线下载引擎** — 新增 aria2 外部引擎支持，RPC 调用、断点续传、多连接、速度限制、探测和 fallback 到内置引擎
- **后台任务优雅关闭** — 服务关闭时发送取消信号，任务支持检查点同步检测和异步可中断睡眠，30 秒宽限期后强制终止
- **自定义配置可见性控制** — 系统配置新增 visibility 字段，支持 private / authenticated / public 三级可见度，公开 API 带缓存和 Vary 头
- **从节点审计日志** — 新增 8 个 follower 专属审计动作，覆盖绑定同步、对象读写删和 Ingress Profile 管理
- **WOPI RSA 安全重构** — 生产环境 `rsa` 替换为 `ring`，公钥增加约束校验，测试密钥运行时生成
- **开发者文档英文版** — 新增完整英文 REST API、架构、模块设计和测试文档
- **管理后台版本徽章彩蛋** — ↖(^ω^)↗

### Added

- **aria2 离线下载引擎**
  - 新增 `Aria2` 下载引擎，通过 RPC 调用外部 aria2 进程（`aria2.addUri`、`aria2.tellStatus`）
  - 支持断点续传：持久化 `gid` 和 `processing_token` 到 `runtime_json`，重启后恢复
  - 支持多连接下载：`split` 分片数、`max_connection_per_server` 单服务器最大连接数
  - 支持最低速度限制：`lowest_speed_limit_bytes_per_sec`，低于阈值自动重试
  - 新增 RPC 探测功能：`probe_aria2_rpc` 测试连通性并返回 aria2 版本
  - 新增引擎注册表架构：`offline_download_engine_registry_json`，支持多引擎优先级排序和链式 fallback
  - aria2 失败时自动降级到内置引擎，并清理 aria2 runtime 状态
  - Docker Compose 新增可选 `aria2` 服务（`p3terx/aria2-pro`），使用 `--profile aria2` 启动
  - 新增 `offline_download_engine_registry_json`、`offline_download_aria2_rpc_url`、`offline_download_aria2_rpc_secret` 等配置项
  - 前端新增 `OfflineDownloadEngineRegistryEditor` 组件，支持可视化引擎管理、启用禁用、优先级拖拽和 RPC 连通性测试
  - 新增 `offline_download` 文档（中英文），覆盖引擎配置和 Docker 部署
- **后台任务优雅关闭**
  - 新增 `TaskExecutionContext`，统一封装 `TaskLeaseGuard` 和 `shutdown_token`
  - 提供 `ensure_active()` 同步检查点、`sleep_or_shutdown()` 异步可中断睡眠、`shutdown_requested()` 异步等待
  - 压缩任务（`archive/compress.rs`）在 `spawn_blocking` 前后调用 `context.ensure_active()`
  - 任务派发器每轮循环检查 `shutdown_token.is_cancelled()`
  - 任务执行器外层 `select!` 同时监控业务流程和心跳/lease
  - 系统周期任务（`tasks.rs`）所有 worker 监听 `shutdown_token.cancelled()`
  - 关闭时 `release_task_for_shutdown()` 将正在执行的任务 lease 释放回 `Retry` 状态，避免标记为失败
  - 新增 `TaskWorkerShutdownRequested` 错误码，区分正常关闭、lease 丢失和续约超时
- **自定义配置可见性控制**
  - `system_config` 表新增 `visibility` 字段（`private` / `authenticated` / `public`），默认 `private`
  - 新增 `idx_system_config_visibility` 索引
  - 内置配置不允许修改 visibility，仅 `custom.*` 自定义配置可改
  - 敏感配置值在 API 响应中脱敏为 `***REDACTED***`
  - 新增 `GET /api/v1/public/custom-config`：匿名返回 `public`，认证返回 `public` + `authenticated`
  - 匿名响应 `Cache-Control: public, max-age=60`，认证响应 `Cache-Control: private, max-age=60`
  - 新增 `Vary` 头处理公开配置响应
  - 新增 5 个集成测试和 E2E 测试覆盖
- **从节点审计日志**
  - 新增 8 个 follower 专属审计动作：`FollowerBindingSync`、`FollowerObjectRead`、`FollowerObjectWrite`、`FollowerObjectDelete`、`FollowerObjectCompose`、`FollowerIngressProfileCreate`、`FollowerIngressProfileUpdate`、`FollowerIngressProfileDelete`
  - 从节点启动时初始化 `global_audit_log_manager`
- **开发者文档英文版**
  - 新增 `developer-docs/en/` 完整英文文档，覆盖 REST API（admin、auth、batch、files、folders、health、public、shares、tasks、teams、trash、webdav、wopi）、架构、模块设计和测试指南
  - 原有中文文档迁移到 `developer-docs/zh-CN/`
- **管理后台版本徽章彩蛋**
  - ↖(^ω^)↗
- **测试覆盖**
  - 新增 task dispatch、archive validation、offline download 路径测试
  - 新增离线下载路径长度和权限修复测试
  - 新增公开自定义配置可见性集成测试和 E2E 测试
  - 新增从节点网络拓扑部署文档

### Changed

- 根 crate 版本从 `0.2.5` 升级到 `0.2.6`
- **WOPI RSA 安全重构**
  - 生产依赖 `rsa` 移除，`ring` 加入
  - WOPI proof 验签使用 `ring::signature::RSA_PKCS1_2048_8192_SHA256`
  - 新增 RSA 公钥约束校验：模数 2048-8192 位、奇数、指数 3 以上奇数
  - 测试保留 `rsa 0.9` 仅用于测试密钥运行时生成（dev-dependencies）
- **依赖升级**
  - `jsonwebtoken` 从 `rust_crypto` 切换到 `aws_lc_rs`
  - `sea-orm` 从 `2.0.0-rc.38` 升级到 `2.0.0-rc.40`
- **敏感配置脱敏**
  - `SystemConfig` 序列化时敏感值自动替换为 `***REDACTED***`
  - 审计日志中的敏感值同样脱敏
  - 审计日志记录 `visibility` 和 `prior_visibility` 变更
- **压缩任务错误处理**
  - 归档压缩工作流错误处理改进，使用 `TaskExecutionContext` 统一上下文

### Fixed

- 修复 aria2 输出目录权限问题
- 修复迁移时间戳从 `000001` 到 `000002` 的修正
- 修复静态 RSA 测试密钥，改为运行时生成（减少测试文件大小和密钥泄露风险）

### Notes

- 本版本为 `0.2.6` 功能增强版本
- 新增数据库迁移：
  - `m20260601_000001_add_system_config_visibility` — `system_config` 表新增 `visibility` 字段
  - `m20260601_000002_add_background_task_runtime_json` — `background_tasks` 表新增 `runtime_json` 字段
- **Breaking Change**：API 变更
  - 新增 `GET /api/v1/public/custom-config` 公开自定义配置接口
  - `SystemConfig` 响应中敏感值可能显示为 `***REDACTED***`
- **Breaking Change**：依赖变更
  - 生产构建不再依赖 `rsa` crate，改为 `ring`
- 新增运行时配置项：
  - `offline_download_engine_registry_json` — 引擎注册表
  - `offline_download_aria2_rpc_url` / `offline_download_aria2_rpc_secret` / `offline_download_aria2_request_timeout_secs` / `offline_download_aria2_split` / `offline_download_aria2_max_connection_per_server` / `offline_download_aria2_lowest_speed_limit_bytes_per_sec` — aria2 专用配置
- Docker 用户如需 aria2 离线下载，使用 `docker compose --profile aria2 up -d`
- 强类型 API 客户端建议重新生成，以同步公开自定义配置和离线下载引擎接口

---

**统计数据**：
- 209 files changed, 15,033 insertions(+), 2,223 deletions(-)
- 41 commits
- Rust Edition 2024

## [v0.2.5] - 2026-06-01

### Release Highlights

**AsterDrive `0.2.5` 聚焦离线下载、审计日志结构化展示和管理后台设置体验优化。** 本版本新增 HTTP/HTTPS 链接离线下载后台任务，支持速率限制、并发控制和安全校验；审计日志引入结构化展示层，支持可配置的动作范围过滤和分组展示；管理后台设置页重构分类元数据，运行时配置默认折叠以提升浏览效率。

- **离线下载** — 新增 HTTP/HTTPS 链接导入后台任务，支持个人空间和团队空间，内置速率限制、并发控制、URL 安全校验和文件大小限制
- **审计日志结构化展示** — 新增 `AuditPresentation` 结构化展示类型，支持按动作分组、可配置的动作范围过滤，审计响应新增 `presentation` 字段
- **管理后台设置页重构** — 分类元数据提取为独立模块和查找表，设置页导航和加载逻辑拆分，运行时配置区块默认折叠（后台任务除外）
- **认证错误码增强** — 新增注册被禁用时的独立结构化错误码
- **文档与项目规范更新** — README 添加产品截图，项目提交语言统一为英文

### Added

- **离线下载（HTTP/HTTPS 链接导入）**
  - 新增 `POST /api/v1/tasks/offline-download` 和 `POST /api/v1/teams/{team_id}/tasks/offline-download` 接口
  - 新增 `OfflineDownload` 后台任务类型，支持流式下载、断点续传、进度跟踪
  - 新增 URL 安全校验：强制 HTTPS（本地开发除外）、域名黑名单、端口限制、协议白名单
  - 新增速率限制：按用户/团队级别限制并发下载数和请求频率
  - 新增速度限制和并发控制配置：支持全局和每任务级别的带宽与并发数限制
  - 新增 `offline_download` 审计动作类型，记录下载发起者和目标 URL
  - 新增 `task_service/offline_download.rs`（1052 行）和 `spec/offline_download.rs`（65 行）
  - 前端任务展示层新增离线下载专用摘要和图标映射
- **审计日志结构化展示层**
  - 新增 `AuditPresentation` 类型，支持按动作分组、计数和嵌套详情展示
  - 审计日志响应新增 `presentation` 字段（可选结构化展示数据）
  - 新增 `audit_log_recorded_actions` 运行时配置，支持自定义审计记录的动作范围
  - 新增 `audit_service/presentation.rs`（298 行），实现审计展示格式化逻辑
  - 新增 `server_start`、`server_shutdown` 审计动作类型
  - 前端审计格式化库扩展展示字段解析（`lib/audit.ts`，131 行改动）
  - 新增审计展示层和配置字段的完整文档（中英文）
  - 新增审计展示边界处理：缺失枚举组、数组参数兼容性修复
- **管理后台设置体验优化**
  - 运行时配置区块默认折叠，仅后台任务保持展开，减少页面视觉噪音
  - 新增 `AdminSettingsLoadedContent` 组件，分离配置加载内容展示
  - 新增 `adminSettingsCategoryMetadata.ts`（228 行）和测试（211 行），集中维护分类元数据

### Changed

- 根 crate 版本从 `0.2.4` 升级到 `0.2.5`
- **管理后台设置架构重构**
  - 分类元数据从分散定义 consolidated 为统一查找表（`adminSettingsCategoryMetadata.ts`）
  - 设置页数据加载逻辑拆分为 `useAdminSettingsData` 和独立内容组件
  - 配置项 schema 新增 `options` 字段，支持下拉选项类型
- **配置模块整理**
  - 重命名设置分类，拆分文件处理相关配置到独立模块
  - `config/admin` 和 `config/settings` 相关结构清理
- **审计日志查询增强**
  - 审计查询支持 `presentation` 字段序列化和反序列化
  - 审计动作枚举扩展 `server_start`、`server_shutdown`、`offline_download`
- **文档更新**
  - README 和 README.zh 添加产品截图展示
  - 项目提交语言统一切换为英文
- **任务调度**
  - 任务调度 lane 逻辑更新，支持 `offline_download` 任务类型分通道调度
  - 任务注册表和类型系统扩展离线下载规范

### Fixed

- 修复审计展示中缺失枚举组导致格式化失败的问题
- 修复审计展示中数组参数处理不当的问题
- 修复管理后台设置页测试中的异步数据断言稳定性

### Notes

- 本版本为 `0.2.5` 功能增强版本
- 没有新增数据库 migration
- **Breaking Change**：API 变更
  - 审计日志响应新增 `presentation` 字段（可选）
  - `AuditAction` 枚举新增 `server_start`、`server_shutdown`、`offline_download`
  - 新增 `POST /api/v1/tasks/offline-download` 和团队版本接口
  - 配置 schema 新增 `options` 字段
- 强类型 API 客户端建议重新生成，以同步离线下载接口、审计展示字段和新增审计动作
- 新增运行时配置项：
  - `audit_log_recorded_actions` — 控制审计记录的动作范围
  - 离线下载相关速率限制和并发控制配置

---

**统计数据**：
- 177 files changed, 7,627 insertions(+), 1,444 deletions(-)
- 17 commits
- Rust Edition 2024

## [v0.2.4] - 2026-05-31

### Release Highlights

**AsterDrive `0.2.4` 聚焦通用 OAuth2 外部认证、团队 WebDAV 账号、后台任务规范系统和前端架构重构。** 本版本新增通用 OAuth2 外部认证 provider，支持 Logto、Keycloak 等标准 OIDC 提供商接入；WebDAV 新增团队工作空间账号支持和 Range 请求；后台任务系统引入类型安全的规范层，统一任务创建、编解码和展示逻辑；前端路由系统和多个管理页面完成组件化拆分重构。

- **通用 OAuth2 外部认证** — 新增 Generic OAuth2 provider，支持 PKCE、公开客户端、多种客户端认证方式，默认 scopes 包含 openid 以兼容 Logto 等提供商
- **团队 WebDAV 账号** — 团队工作空间支持独立 WebDAV 账号管理，含创建/删除/审计日志
- **WebDAV 功能增强** — 支持 HTTP Range 请求，修复 Finder 持锁 PUT 误判问题，模块拆分提升可维护性
- **后台任务规范系统** — 引入 `BackgroundTaskSpec` trait 和 `TypedTaskCreate` builder，统一任务类型声明、payload 编解码和展示逻辑
- **前端架构重构** — 路由系统组件化拆分，团队管理/分享/WebDAV/外部认证页面提取 controller hook
- **依赖升级** — rsa 0.10、xmltree 替代 quick-xml

### Added

- **通用 OAuth2 外部认证 Provider**
  - 新增 `GenericOAuth2` provider driver（711 行），支持手动配置授权、令牌和用户信息端点
  - 支持 PKCE 流程、公开客户端认证（无 client_secret）、ClientSecretPost 认证方式
  - 默认 scopes 包含 `openid email profile`，兼容 Logto 等 OIDC 提供商
  - 新增 URL 校验模块 `url.rs`，统一 HTTPS 强制和 localhost 豁免逻辑
  - 前端新增 OAuth2 图标资源和配置表单
  - 新增通用 OAuth2 provider 配置文档（中英文）
  - 新增 OAuth2 集成测试（490+ 行）
- **团队 WebDAV 账号**
  - 新增 `GET/POST/DELETE /api/v1/teams/{team_id}/webdav-accounts` 接口
  - 新增 `WebdavAccountTable`、`WebdavAccountRow`、`WebdavCreateAccountDialog` 等前端组件
  - 团队 WebDAV 账号审计日志独立记录
  - 新增 WebDAV 账号集成测试（491 行）
- **后台任务规范系统**
  - 新增 `BackgroundTaskSpec` trait，统一任务类型、payload/result 编解码、steps、lane、max attempts 声明
  - 新增 `TypedTaskCreate` builder，类型安全的任务创建接口
  - 新增 `TaskPresentation` 类型，支持结构化任务状态展示消息
  - 新增 `src/services/task_service/spec/` 模块和 `registry.rs`（257 行）
  - 新增 `presentation.rs`（538 行），后端直接输出展示文本
- **WebDAV 功能增强**
  - 支持 HTTP Range 请求（部分内容下载）
  - WebDAV 模块拆分为 locks/props/resources/transfer/file/fs 子模块
  - 新增 WebDAV 集成测试（785 行）
- **前端组件**
  - 新增 `WorkspaceOutlet`、`AdminRoute`、`LoginGuard`、`ProtectedRoute` 路由组件
  - 新增 `MyShareCard`、`MyShareStatusBadge`、`MySharesSelectionBar` 分享组件
  - 新增 `useAdminExternalAuthPageController`（743 行）外部认证页面控制器
  - 工作区切换器路由切换后恢复下拉菜单展开状态

### Changed

- 根 crate 版本从 `0.2.3` 升级到 `0.2.4`
- **前端路由系统重构** — 从单一路由文件拆分为多个专用路由组件
- **团队管理页面重构** — `TeamManageDialog.tsx` 从 658 行拆分为 view/shell/actions/state 等模块
- **我的分享页面重构** — `MySharesPage.tsx` 从 381 行拆分为多个展示组件
- **WebDAV 账号页面重构** — 从 462 行简化为 267 行，提取公共组件
- **外部认证页面重构** — 提取 controller hook，简化视图层
- **模块文件结构重构** — 20+ 个单文件模块转换为目录模块（cli、types、runtime/startup 等）
- **任务展示逻辑** — 使用后端结构化展示消息替代前端解析，增强容错性
- 重命名 `MfaFactorMethod` 为 `MfaPersistentFactorMethod`，语义更清晰
- 统一归档格式检测逻辑，移除基于 MIME 的宽松匹配
- 依赖升级：rsa 0.9→0.10、xmltree 替代 quick-xml、aws-sdk-s3 1.134、nom-exif 3.6

### Fixed

- 修复 Finder 持锁 PUT 被误判为他人锁定的问题
- 修复 WebDAV 账户管理页面当前用户标识显示
- 修复 WebDAV 团队账户功能边界检查
- 增强 OAuth2 错误响应的诊断信息
- 修复工作区搜索键盘事件处理
- 修复团队管理分页和导航问题

### Notes

- 本版本为 `0.2.4` 功能增强版本
- 新增数据库迁移：
  - `m20260530_000001_add_webdav_account_team_scope`
- **Breaking Change**：API 变更
  - 任务信息新增 `presentation` 字段（结构化展示消息）
  - 外部认证 provider 新增 `GenericOAuth2` 类型
  - 团队新增 WebDAV 账号管理接口
- **Breaking Change**：依赖变更
  - `rsa` 升级到 0.10（API 不兼容）
  - `quick-xml` 替换为 `xmltree`
- 强类型 API 客户端建议重新生成，以同步外部认证和团队 WebDAV 接口

---

**统计数据**：
- 270 files changed, 16,436 insertions(+), 9,368 deletions(-)
- 40 commits
- Rust Edition 2024

## [v0.2.3] - 2026-05-29

### Release Highlights

**AsterDrive `0.2.3` 聚焦远程存储反向隧道、Blob 维护任务和归档能力收口。** 本版本新增反向隧道传输模式，支持无公网 IP 的远程节点通过主动连接方式接入；新增 Blob 维护后台任务，支持孤立对象清理、引用计数协调和健康状态检查；归档服务继续聚焦 ZIP 预览与解包，并保留后续扩展更多格式的抽象边界；任务展示逻辑全面重构，新增 688 行任务展示模块，支持 20+ 种系统任务的运行时名称映射和 70+ 种状态的国际化；远程存储协议传输层重构，统一请求/响应编码和流式帧处理；数据库查询优化，使用 SeaORM 查询构建器替代手动 SQL 拼接。

- **反向隧道传输模式** — 远程节点支持 Direct/ReverseTunnel/Auto 三种传输模式，无公网 IP 节点可通过反向隧道主动连接主节点
- **归档能力收口** — 继续支持 ZIP 预览和在线解包，7z 支持在开发期评估后未纳入本次发布，避免 `crc64fast` i686 构建失败及 FFI/GPL 路线风险
- **Blob 维护任务** — 新增 `BlobMaintenance` 后台任务类型，支持扫描、检查、协调引用、清理孤立对象
- **任务展示重构** — 新增 `taskPresentation.ts` 模块（688 行），支持运行时任务名称映射和状态国际化
- **远程存储协议重构** — 传输层重构，新增 `transport.rs` 和 `runtime.rs`，统一请求/响应编码和流式帧处理
- **数据库查询优化** — 使用 SeaORM 查询构建器替代手动 SQL 拼接，提高安全性和可维护性
- **管理员文件信息增强** — 新增创建者信息、Blob 引用计数、健康状态、上传者信息
- **前端页面重构** — 远程节点页面逻辑提取为 controller hook（637 行），任务页面、管理员文件页面、管理员任务页面全面重构

### Added

- **反向隧道传输模式**
  - 新增 `/internal/remote-tunnel` API 端点（poll/complete/connect）
  - 新增 `RemoteNodeTransportMode` 枚举（Direct/ReverseTunnel/Auto）
  - 新增隧道客户端实现（1456 行），支持多通道流式传输、自动重连、背压处理
  - 新增隧道服务器实现（1160+ 行测试），包含认证、帧编码、注册表管理、持久化轮询
  - 支持 WebSocket 和 HTTP 长轮询两种传输方式
  - 数据库新增 `managed_followers.transport_mode/tunnel_last_error/tunnel_last_seen_at` 字段
  - 前端新增 `TransportModeSelector.tsx` 组件，支持无障碍性
  - 新增 `useAdminRemoteNodesPageController.ts` hook（637 行），远程节点页面逻辑提取
- **归档格式能力声明**
  - 新增 `ArchiveFormat` 抽象，统一 ZIP 预览与解包格式管理，为未来接入更多格式保留边界
  - 前端新增 `archivePreviewFormatCapabilities.ts`，集中维护归档预览格式能力
  - 过滤不支持的格式（如 RAR、7z）预览选项，避免前端暴露不可用入口
- **Blob 维护任务**
  - 新增 `BackgroundTaskKind::BlobMaintenance` 任务类型
  - 新增 `blob_maintenance.rs` 服务（767 行），支持扫描、检查、协调引用、清理孤立对象
  - 批量处理（1000 条/批）、进度跟踪、事务支持
  - 新增 `POST /admin/files/blobs/maintenance` API 端点
  - 新增 `AdminFileBlobHealth` 枚举（Healthy/Orphan/RefCountMismatch/CleanupClaimed）
  - 数据库新增 `storage_migration_checkpoints.renamed_opaque_blobs` 字段
- **任务展示增强**
  - 新增 `taskPresentation.ts` 模块（688 行），运行时任务名称映射和状态国际化
  - 支持 20+ 种系统任务的显示名称映射
  - 支持 70+ 种状态的国际化文本
  - 新增 `tasks/common.json` 和 `tasks/status-kind.json` 国际化文件（中英文）
  - 新增 `steps.rs` 模块（44 行），统一的步骤状态管理接口
- **存储策略增强**
  - 新增 `StoragePolicySummaryFields.tsx` 组件（165 行），存储策略摘要展示
  - 新增 `S3DownloadStrategyField.tsx` 和 `S3UploadStrategyField.tsx`，S3 策略字段分离
- **管理员文件信息增强**
  - `AdminFileInfo` 新增 `created_by` 字段（创建者用户摘要）
  - `AdminFileBlobInfo` 新增 `file_ref_count/version_ref_count/actual_ref_count` 字段（引用计数）
  - `AdminFileBlobInfo` 新增 `health` 字段（Blob 健康状态）
  - `AdminFileBlobInfo` 新增 `uploader_count/uploaders` 字段（上传者信息）
  - `AdminFileBlobReferenceFile` 新增 `created_by_*` 字段
- **用户身份组件**
  - 新增 `UserIdentityGroup.tsx` 组件（49 行），用户身份展示

### Changed

- 根 crate 版本从 `0.2.2` 升级到 `0.2.3`
- **远程存储协议重构**
  - 新增 `transport.rs`（770 行），统一的请求/响应编码和流式帧处理
  - 新增 `runtime.rs`（179 行），异步任务管理和连接生命周期管理
  - 重构 `client.rs`（536 行改动），支持多种传输模式和改进的错误处理
  - 增强 `errors.rs`（72 行改动），新增隧道相关错误类型
- **归档服务重构**
  - 提取 `format.rs`（格式管理）、`io.rs`（I/O 操作）、`scan.rs`（扫描逻辑）
  - 重构 `zip_scan/` 模块，优化 Zip 扫描性能
  - 改进 `archive_preview_service/`，保留 ZIP 原始清单缓存重建和旧版缓存兼容
- **数据库查询优化**
  - 使用 SeaORM 查询构建器替代手动 SQL 拼接（`apply/copy.rs`）
  - 优化 Blob 查询性能（`blob/lookup.rs`，177 行改动）
- **任务服务重构**
  - 存储迁移任务支持 opaque key 重命名计数（`storage_migration.rs`，210 行改动）
  - 任务分发支持新的 blob 维护任务类型（`dispatch/execute.rs`，41 行改动）
  - 提取解压暂存逻辑（`archive/extract/staging.rs`），归档解包流程继续复用 ZIP 安全校验和暂存导入路径
- **前端页面重构**
  - `AdminRemoteNodesPage.tsx` 从 588 行简化为 72 行，逻辑提取为 controller hook
  - `TasksPage.tsx` 改进任务展示逻辑和国际化支持（137 行改动）
  - `AdminFilesPage.tsx` 新增 Blob 健康状态展示（846 行改动）
  - `AdminTasksPage.tsx` 支持新任务类型和改进的过滤排序（577 行改动）
  - `AdminOverviewPage.tsx` 新增后台任务部分和系统健康状态横幅（146 行改动）
- **配置和文档更新**
  - 运行时配置文档新增隧道相关配置说明（`runtime.md`，40+ 行改动）
  - 存储驱动配置更新（`storage.md`，43 行改动）
  - API 文档新增 Blob 维护和远程隧道 API 文档

### Fixed

- 修复归档预览旧版缓存兼容性问题，处理 `zip_utf8` 字段别名和缺失字段
- 修复反向隧道流式传输错误处理和流中止逻辑
- 修复存储迁移 blob 摘要构建，使用 SeaORM 查询构建器替代手动 SQL

### Notes

- 本版本为 `0.2.3` 功能增强版本
- 7z 在线预览与在线解包在 `0.2.3` 开发期经过评估，最终未纳入本次发布：
  - 纯 Rust 方案选择少，当前候选依赖会间接触发 `crc64fast` i686 构建失败
  - FFI/xz 绑定路线存在 GPL 许可风险
  - `.7z` 文件仍会作为普通压缩包文件类型显示，但不会暴露归档预览或在线解包入口
  - issue #206 已标记为 `not planned`，后续只有在依赖许可、跨平台构建和维护成本都可控时才重新评估
- 新增数据库迁移：
  - `m20260528_000001_add_storage_migration_opaque_rename_count`
  - `m20260529_000001_add_remote_node_transport`
- **Breaking Change**：数据库 Schema 变更
  - `managed_followers` 表新增 `transport_mode/tunnel_last_error/tunnel_last_seen_at` 字段
  - `storage_migration_checkpoints` 表新增 `renamed_opaque_blobs` 字段
  - 必须运行数据库迁移后才能启动
- **Breaking Change**：API 变更
  - 新增 `blob_maintenance` 任务类型，客户端需要更新任务类型枚举
  - `RemoteNodeInfo` 新增 `transport_mode` 字段（默认为 "direct"）
  - `AdminFileInfo` 新增 `created_by` 字段（可选）
  - `AdminFileBlobInfo` 新增多个引用计数和健康状态字段

---

**统计数据**：
- 271 files changed, 23,475 insertions(+), 3,416 deletions(-)
- 33 commits
- Rust Edition 2024, MSRV 1.91.1

## [v0.2.2] - 2026-05-28

### Release Highlights

**AsterDrive `0.2.2` 聚焦存储策略迁移、管理员可观测性、错误码体系重构和前端性能优化。** 本版本新增完整的存储策略数据迁移工作流，支持断点续传和失败恢复；管理员后台新增文件与 Blob 可观测页面，可多维度筛选、排序和查看存储使用情况；引入 `ApiErrorCode` 替代 `ApiSubcode` 作为稳定错误标识，改善客户端错误处理体验；优化前端启动性能，延迟加载非关键配置和 SSE 连接；任务卡片重构为摘要+展开详情的两段式布局，改善大量任务场景下的可用性。

- **存储策略数据迁移** — 新增完整迁移工作流（选择源/目标策略 → 预检查 → 创建任务 → 断点续传 → 完成），支持大规模数据迁移的断点续传和失败恢复
- **管理员可观测页面** — 新增文件与 Blob 可观测页面，支持多维度筛选、排序、分页，迁移对话框增加"检查计划"按钮展示预检查结果
- **错误码体系重构** — 引入 `ApiErrorCode` 替代 `ApiSubcode`，响应新增 `code` 字段，前端优先读取 `error.code`，向后兼容 `error.subcode`
- **前端性能优化** — 非关键配置延迟到空闲时加载，SSE 连接增加初始延迟，上传会话恢复延迟执行，文件夹树切换优先复用缓存
- **任务卡片重构** — 改为摘要+展开详情的两段式布局，关键信息一目了然，详情按需展开
- **Metrics 镜像构建** — Docker 构建矩阵新增 `metrics` 变体，镜像标签统一加 `-metrics` 后缀
- **刷新令牌错误处理优化** — 新增过期令牌重用检测，多标签页会话管理更加稳定
- **文档域名迁移** — 所有文档链接从 `asterdrive.docs.esap.cc` 迁移至 `drive.astercosm.com`

### Added

- **存储策略数据迁移**
  - 新增完整的存储迁移工作流：选择源/目标策略 → 预检查 → 创建任务 → 断点续传 → 完成
  - 后端新增 `StoragePolicyMigration` 任务类型，独立并发通道（StorageMigration lane）
  - 数据库新增 `storage_migration_checkpoints` 表，支持断点续传和失败恢复
  - 迁移结果包含详细统计：迁移/跳过/失败对象数及字节数
  - 新增 `POST /admin/storage-migrations`、`POST /admin/storage-migrations/dry-run`、`POST /admin/storage-migrations/resume` 接口
  - 新增 RustFS S3 端到端迁移、断点续传、跨批次合并集成测试
- **管理员可观测页面**
  - 新增 `/admin/files` 和 `/admin/file-blobs` 页面，支持多维度筛选、排序、分页
  - 后端新增 `admin_file_service` 模块，提供文件与 Blob 的反向引用查询
  - 存储迁移对话框增加"检查计划"按钮，展示预检查结果（源数据统计、目标容量、去重预估）
  - 任务详情对话框支持从检查点恢复失败的迁移任务
  - 新增 `GET /admin/files`、`GET /admin/file-blobs` 接口
- **错误码体系**
  - 新增 `ApiErrorCode` 枚举（654 行），覆盖所有现有 `ApiSubcode` 值
  - `ApiErrorInfo` 响应新增 `code` 字段，前端优先读取 `error.code`，向后兼容 `error.subcode`
  - 新增 `RefreshTokenStale` 和 `RefreshTokenReuseDetected` 错误码
  - 登录失败统一返回通用错误消息，避免泄露用户存在性
- **任务卡片**
  - 任务卡片改为两段式布局：摘要（summary）+ 展开详情
  - 新增 `summaryParts` 函数生成结构化摘要（文本+图标芯片）
  - 新增 `TaskSummaryChip` 组件展示文件名、策略等关键信息
  - 新增 `taskIcon` 函数为每种任务类型映射对应图标
  - 进度、步骤详情、时间戳等移入可折叠展开面板
- **Metrics 镜像**
  - Docker 构建矩阵新增 `metrics` 变体，启用 `server,cli,metrics` features
  - 每个变体添加 `suffix` 字段，metrics 镜像标签统一加 `-metrics` 后缀
  - 构建缓存 scope 和 registry ref 加入变体维度，避免缓存冲突
  - `publish-manifest` 任务改为矩阵策略，分别为 default 和 metrics 发布多架构 manifest

### Changed

- 根 crate 版本从 `0.2.1` 升级到 `0.2.2`
- 标记 `ApiSubcode` 为 0.3.0 废弃，保留过渡期兼容性
- 前端错误处理逻辑优先检查 `error.code` 而非 `error.subcode`
- 刷新令牌过期或被重用时返回独立错误码，前端自动同步会话状态
- 跨标签页刷新协调增加心跳检测和过期接管逻辑
- 存储迁移前检查目标路径是否已被引用，避免误删已存在的 blob 对象
- 重构 `copy_blob_streaming` 和 `cleanup_unmoved_target_object`，统一通过 `target_object_is_referenced` 守卫清理操作
- 非关键公共配置（预览应用、缩略图、媒体数据）延迟到空闲时加载
- SSE 连接增加 1500ms 初始延迟，避免页面加载期间抢占网络资源
- 上传会话恢复延迟 600ms 执行，降低初始渲染压力
- 在 fileStore 中缓存 `lastFolderContents`，文件夹树切换时优先复用已有数据
- MFA 状态请求添加缓存机制，支持 force 强制刷新及变更后自动失效
- 跨标签刷新锁兼容无 updatedAt 字段的旧版锁记录
- 路由守卫仅在未认证时显示加载态，避免已登录用户重新检查时闪屏
- 文件夹树控制器移除 setTimeout，直接复用 store 缓存快照
- 迁移对话框在 dry-run 加载中时禁用提交按钮
- AdminTaskTable 行添加 aria-expanded/aria-controls 无障碍属性
- 文件/blob 行添加键盘 Enter/Space 交互支持
- 所有文档链接从 `asterdrive.docs.esap.cc` 迁移至 `drive.astercosm.com`

### Fixed

- 修复存储迁移前未检查目标路径是否已被引用，可能误删已存在的 blob 对象的问题
- 修复刷新令牌过期或被重用时的错误处理不完善的问题
- 修复多标签页场景下会话管理不稳定的问题
- 修复 E2E 测试使用 heading/cell 角色查询不稳定的问题
- 修复重命名对话框输入逻辑，改用全选后逐字输入
- 修正归档任务创建时传入文件名 stem 而非完整文件名的问题
- 修正刷新令牌错误码从 E012 改为 E019

### Security

- 登录失败统一返回通用错误消息，避免泄露用户存在性
- 存储迁移前检查目标路径引用，避免误删已被引用的对象

### Notes

- 本版本为 `0.2.2` 功能与稳定性维护版本
- 新增数据库迁移：
  - `m20260528_000001_add_storage_migration_checkpoints`
- **Breaking Change**：引入 `ApiErrorCode` 替代 `ApiSubcode`
  - `ApiErrorInfo` 响应新增 `code` 字段
  - 前端需要优先检查 `error.code` 而非 `error.subcode`
  - `ApiSubcode` 标记为 0.3.0 废弃，保留过渡期兼容性
  - 旧客户端仍可工作但会收到废弃警告
- API 新增接口：
  - `POST /admin/storage-migrations` - 创建存储迁移任务
  - `POST /admin/storage-migrations/dry-run` - 预检查迁移计划
  - `POST /admin/storage-migrations/resume` - 恢复失败的迁移任务
  - `GET /admin/files` - 查询文件列表
  - `GET /admin/file-blobs` - 查询 Blob 列表
- Docker 镜像新增 `-metrics` 变体，用户可选择拉取以启用 metrics 功能
- 强类型 API 客户端建议重新生成，以同步错误码、存储迁移和管理员可观测接口
- 统计数据：145 files changed, 11,903 insertions(+), 890 deletions(-)
- 本次范围共 14 个提交

## [v0.2.1] - 2026-05-26

### Release Highlights

**AsterDrive `0.2.1` 聚焦账号安全、团队容量管理、上传恢复隔离和文档体系完善。** 本版本新增邮箱验证码 MFA 登录方式，管理员可在团队创建和编辑流程中直接配置存储配额；上传会话恢复按前端实例隔离，避免多标签页或多浏览器实例互相抢占恢复任务；同时补齐完整英文文档、现代化 VitePress 文档站主题，并同步更新 API、配置和用户文档。

- **邮箱验证码 MFA 登录** — 在 TOTP 和恢复码之外新增邮箱验证码二次验证方式，支持发送冷却、有效期、TOTP fallback 策略和专用邮件模板
- **团队存储配额管理** — 管理员创建或编辑团队时可直接设置存储配额，团队详情页展示配额与用量进度
- **上传会话实例隔离** — 上传会话记录前端实例 ID，可恢复上传列表按浏览器实例过滤，减少多标签页恢复冲突
- **英文文档体系与文档站改版** — 新增完整英文文档目录，VitePress 支持中英文站点、深色模式、现代化视觉变量和主题切换动画
- **配置与邮件投递保护** — 邮箱验证码 MFA 启用前校验 SMTP 配置，邮件配置失效时自动关闭相关 MFA 能力，降低用户被锁定风险
- **开发者文档同步** — API、运行时配置、WebDAV、认证、上传、团队、错误码和架构文档同步补齐新能力说明

### Added

- **邮箱验证码 MFA**
  - 新增 `email_code` MFA challenge 方法，向用户已验证邮箱发送 8 位数字一次性验证码
  - 新增 `POST /api/v1/auth/mfa/challenge/email-code/send` 发送邮箱验证码接口
  - 新增邮箱验证码有效期和重发冷却运行时配置
  - 新增是否允许已启用 TOTP 用户使用邮箱验证码 fallback 的运行时配置
  - 新增邮箱验证码登录邮件主题与 HTML 模板
  - 新增 `mfa_email_codes` 数据表、实体、仓库和清理 / 消费逻辑
  - 审计日志新增邮箱验证码发送动作
- **团队存储配额**
  - 管理后台创建团队对话框支持设置团队存储配额
  - 管理后台团队详情编辑流程支持修改团队存储配额
  - 团队详情 Overview 区域新增配额数值和使用进度展示
  - 后端管理员创建 / 更新团队 API 支持可选 `storage_quota` 字段
- **上传会话恢复隔离**
  - 上传会话新增 `frontend_client_id` 字段
  - 上传初始化请求和上传会话查询支持传入前端实例 ID
  - 前端按浏览器实例生成并持久化上传客户端 ID
  - 上传面板新增已取消上传任务状态展示
- **文档与文档站**
  - 新增完整英文文档体系，覆盖配置、部署、运维、用户指南、存储和故障排查
  - VitePress 配置新增中英文 locale、导航、描述和 Open Graph 信息
  - 文档站新增品牌色、阴影、网格背景、深色模式变量和首页视觉优化
  - 文档站新增导航栏、下拉菜单、搜索框、侧边栏和主题切换动画
  - 支持基于 View Transition API 的主题切换圆形扩散动画，并兼容 `prefers-reduced-motion`

### Changed

- 根 crate 版本从 `0.2.0-hotfix.1` 升级到 `0.2.1`
- 上传初始化内部参数统一封装为 `InitUploadParams`，个人空间和团队空间上传初始化共享同一参数模型
- 上传会话恢复查询默认按当前前端实例过滤，旧客户端不传 `frontend_client_id` 时保留原有兼容行为
- 系统配置写入改为事务化保存，联动配置和审计日志记录保持原子性
- 配置审计日志记录实际存储的归一化值，而不是原始输入值
- 开启邮箱验证码 MFA 时会校验 SMTP 邮件投递配置是否完整
- 邮件投递配置被修改为不可用状态时，会自动关闭邮箱验证码 MFA 登录能力
- 邮件模板渲染上下文新增 `{{lang}}` 变量，登录验证码邮件可输出正确 HTML 语言标签
- 管理后台设置页改为分页加载完整配置列表，避免超过单页上限后部分配置项不可见
- 团队配额输入和展示统一走存储配额解析工具，支持更精确地从 bytes 转换为 MB
- 登录页 MFA challenge 面板支持 TOTP、恢复码和邮箱验证码方法切换
- TOTP 验证码前端校验收紧为 6 位纯数字
- 缩略图和 blob URL hook 增加当前用户命名空间隔离与竞态保护
- README 与关于页面更新产品定位，强调自托管文件基础设施和 Docker 优先快速启动
- 开发者 API 文档同步补齐邮箱验证码 MFA、上传实例隔离、团队配额、WebDAV 系统文件拦截和错误码说明
- 配置文档同步补齐邮箱验证码 MFA、邮件模板、运行时配置和 WebDAV 系统文件保护配置

### Fixed

- 修复团队配额为 `0` 时在编辑对话框中被误判为存在未保存变更的问题
- 修复团队配额输入在小数、负值、非数字和溢出场景下校验不一致的问题
- 修复邮箱验证码生成和邮件发送之间的事务边界，避免哈希或入库失败后仍可能发送不可用验证码
- 修复过期邮箱验证码仍可能被消费的问题
- 修复管理员设置页因配置分页限制导致邮件模板等配置项不可见的问题
- 修复前端重复定义邮件投递配置就绪判断的问题
- 修复 blob URL 在快速切换资源时可能产生孤立 object URL 的竞态问题
- 修复缩略图缓存未按当前用户隔离可能导致状态串扰的问题
- 修正 README / 关于页面中的测试错误码断言说明

### Security

- 邮箱验证码 MFA 默认关闭，必须由管理员显式开启
- 邮箱验证码只保存哈希值，发送新验证码会使旧的未消费验证码失效
- 单用户同一时间只允许存在一条未消费邮箱验证码记录，通过数据库唯一索引约束
- 邮箱验证码有效期不会超过当前 MFA 登录 flow 剩余时间
- 邮箱验证码 MFA 启用前强制检查邮件投递配置，避免用户进入无法完成验证的登录路径
- 邮件投递配置变为不可用时自动关闭邮箱验证码 MFA，降低配置变更导致账号锁定的风险
- WebDAV 文档补齐系统文件拦截配置说明，明确可拦截 `.DS_Store`、`Thumbs.db` 等系统文件写入

### Notes

- 本版本为 `0.2.1` 功能与文档维护版本
- 新增数据库迁移：
  - `m20260526_000001_add_upload_session_frontend_client`
  - `m20260526_000002_add_mfa_email_codes`
- 新增运行时配置项：
  - `auth_email_code_login_enabled`
  - `auth_email_code_login_allow_totp_fallback`
  - `auth_email_code_login_ttl_secs`
  - `auth_email_code_login_resend_cooldown_secs`
- 新增邮件模板配置项：
  - `mail_template_login_email_code_subject`
  - `mail_template_login_email_code_html`
- API 枚举扩展：
  - `MfaChallengeMethodType` 新增 `email_code`
  - `MfaChallengeRequestMethod` 新增 `email_code`
  - `ApiSubcode` 新增邮箱验证码 MFA 相关子码
- 上传相关客户端如需启用实例隔离，应在初始化上传和查询上传会话时传入稳定的 `frontend_client_id`
- 强类型 API 客户端建议重新生成，以同步 MFA 方法、错误子码、团队配额和上传会话字段
- 统计数据：168 files changed, 15,701 insertions(+), 737 deletions(-)
- 本次范围共 10 个提交

## [v0.2.0-hotfix.1] - 2026-05-25

### Release Highlights

**`0.2.0` 系列第一个热修复。** 本版本细分认证错误码语义，将原来笼统的 `AuthFailed` (2000) 拆分为 `TokenMissing` (2007)、`CredentialsFailed` (2008) 和 `MfaFailed` (2009) 三个独立错误码，让前端能更精确地处理认证失败场景。

- **认证错误码细分** — 缺少 token、凭证错误和 MFA 验证失败分别返回独立错误码，前端可按语义触发刷新或重定向
- **SSE 断线重连前刷新会话** — 存储变更事件流断开后先刷新 access token 再重连，减少因 token 过期导致的连续重连失败
- **上传分块认证失败自动刷新** — 分块上传因 token 认证失败时先刷新会话再立即重试，无需等待退避延迟
- **侧边栏拖拽手柄无障碍语义修正** — 将 resize handle 从 `<input type="range">` 改为 `<hr>` + ARIA separator，匹配实际交互语义

### Changed

- 后端认证中间件缺少 token 时返回 `TokenMissing` (2007)，不再混入凭证错误
- MFA 相关错误（验证码错误、流程过期、尝试次数超限、因子未启用、恢复码已用）统一映射到 `MfaFailed` (2009)
- 凭证错误（密码错误、分享密码错误等）返回 `CredentialsFailed` (2008)
- 前端 `isTokenAuthError` 匹配 `TokenMissing`，HTTP 拦截器在 `TokenMissing` 时触发 token 刷新重试
- 存储变更事件流在认证初始化（`isChecking`）期间不建立连接，避免 bootstrap 阶段的无效 SSE 请求
- 存储变更事件流断线后先刷新 access token，刷新导致会话清除时不再重连
- 分块上传因 token 认证失败时先刷新会话再立即重试，跳过指数退避延迟
- 侧边栏宽度调整手柄从 `role=slider` 改为 `role=separator`，修正无障碍语义

### Fixed

- 修复 SSE 事件流因 access token 过期而连续重连失败后最终放弃的问题
- 修复上传分块因 token 过期失败后仍需等待退避延迟的问题
- 修复 MFA 验证失败错误码被映射到通用 `AuthFailed` 而非独立 `MfaFailed` 的问题
- 修复缺少 token 的请求被错误归类为凭证错误的问题

### Notes

- 本版本为 `0.2.0` 系列第一个热修复版本
- API 错误码新增 2007 (TokenMissing)、2008 (CredentialsFailed)、2009 (MfaFailed)；`AuthFailed` (2000) 仍保留但不再由当前代码路径产生
- 自定义客户端如果按 `code == 2000` 判断认证失败，建议改为匹配 2000-2009 范围或按具体子码处理
- 没有新增数据库 migration
- 统计数据：28 files changed, 430 insertions(+), 72 deletions(-)

## [v0.2.0] - 2026-05-25

### Release Highlights

**AsterDrive `0.2.0` 正式发布。** 在 `v0.2.0-rc.1` 的账号安全、MFA、监控指标、SQLite 读写分离、媒体元数据和归档预览基础上，本版本继续收口前端体验、移动端布局、文档预览 iframe 权限和测试覆盖，把 `0.2.0` 系列从 RC 阶段推进到稳定发布。

- **正式版稳定性收口** — 根 crate 版本升级到 `0.2.0`，前端包版本与产品名同步为 `asterdrive-panel` / `0.2.0`
- **文档预览权限增强** — 受信任文档查看器 iframe 支持剪贴板、全屏、画中画、自动播放和安全弹窗逃逸，Office / Google 等在线预览交互更完整
- **文件夹树交互打磨** — 侧栏文件夹树新增平滑展开 / 收起动画，根目录支持独立折叠，并补齐键盘与 ARIA 语义
- **移动端布局修复** — 全面适配动态视口高度和底部安全区域，修复短视口下侧边栏目录树被压缩或无法滚动的问题
- **MFA 设置体验增强** — 安全设置中的 MFA 绑定流程加入步骤切换、Presence 和高度测量动画，减少面板跳动
- **测试覆盖补齐** — 新增短视口侧边栏 E2E、文件夹树动画生命周期和 MFA 动画组件测试

### Added

- 文件夹树新增展开 / 收起过渡动画，子树高度与图标状态同步过渡
- 根目录行新增独立展开 / 收起控制，不再需要通过导航动作影响折叠状态
- MFA 设置流程新增步骤切换动画、Presence 动画和动态高度测量组件
- 新增短视口侧边栏 E2E 测试，覆盖移动端浏览器地址栏和小屏布局下的滚动可用性
- 新增文件夹树动画、根目录折叠和 MFA 动画相关单元测试

### Changed

- 文档查看器 iframe 权限按受信任场景细分，Office / Google 等文档预览获得更完整的交互能力
- 前端布局从固定 `vh` 逐步调整为 `dvh` 与安全区域适配，改善移动端地址栏变化时的可用空间计算
- 侧边栏拆分为导航、快捷分类、容量展示、内容区和拖拽调整等子组件，降低主组件复杂度
- 文件夹树控制逻辑迁移到 reducer 与 controller hook，主组件只保留渲染编排
- WOPI 预览会话管理改为资源订阅模式，预览生命周期和清理路径更可控
- 分享创建 / 编辑对话框状态管理迁移到 reducer，减少表单状态分散和重复更新逻辑

### Fixed

- 修复短视口和移动端场景下侧边栏目录树滚动区域被内容挤压的问题
- 修复文件夹树收起状态下仍可能保留可交互子内容的无障碍问题
- 修复密码输入框缺少 `autocomplete="new-password"` 导致的浏览器自动填充语义问题
- 修复部分预览、分享和 MFA 交互测试中的异步状态边界

### Security

- 细化 iframe sandbox 策略：外部 Web App 预览继续使用更收紧的沙箱权限，只有受信任文档查看器获得同源、顶层导航和弹窗逃逸能力
- 文档预览 iframe `allow` 策略显式限定剪贴板、全屏、画中画和自动播放能力，减少无意扩权

### Notes

- 本版本为 `0.2.0` 系列正式发布版
- 从 `v0.2.0-rc.1` 升级到 `v0.2.0` 没有新增数据库 migration
- 生产配置 schema 未新增必需项；`src/config/loader.rs` 仅补齐测试配置中的 auth 示例
- Docker 用户建议使用 `v0.2.0`、`stable` 或 `latest` 镜像标签；`edge` 继续保留给后续预发布版本
- 统计数据：55 files changed, 2,720 insertions(+), 1,060 deletions(-)
- 本次范围共 5 个提交

## [v0.2.0-rc.1] - 2026-05-24

### Release Highlights

**`0.2.0` 系列进入 RC 阶段。** 本版本把账号安全、多因素认证、监控指标、数据库连接模型和媒体/归档预览继续收口；前端同步补齐 MFA 登录与安全设置，压缩包编码兼容、媒体元数据展示和分享页加载体验也做了集中打磨。

- **MFA 多因素认证** — 新增 TOTP、恢复码、登录二次验证和管理员重置能力
- **Prometheus 指标体系** — 引入 `MetricsRecorder`，覆盖 API、数据库、存储、上传、运行时与 WOPI 等关键链路
- **SQLite 读写分离** — 引入 `DbHandles` 和 reader pool，修复只读连接权限校验引发的一致性问题
- **归档预览编码兼容** — ZIP 清单缓存升级至 v2，支持自动/手动选择 GB18030、UTF-8、CP437 等文件名编码
- **媒体元数据增强** — 扩展 RAW / TIFF / GPS / 音视频元数据提取与公开媒体能力接口
- **前端质量收口** — 文件预览、分享页、信息面板、MFA、上传和团队/远端节点补齐大量 Vitest / E2E 覆盖

### Added

- **多因素认证（MFA）**
  - 新增 TOTP 因子绑定、验证、禁用和删除流程
  - 新增恢复码生成、展示、复制、下载和重新生成流程
  - 登录流程支持 `mfa_required` challenge，密码登录与外部认证均可进入二次验证
  - 管理后台用户详情支持重置用户 MFA
  - 新增 `mfa_factors`、`mfa_recovery_codes`、`mfa_login_flows`、`mfa_totp_setup_flows` 表
- **监控指标**
  - 新增 `MetricsRecorder` trait 和 Prometheus recorder
  - 覆盖 HTTP API、数据库查询、存储驱动、上传、后台任务和 WOPI 等指标
  - 新增监控部署文档、Grafana dashboard 和生产检查项
- **媒体与预览能力**
  - 新增公开媒体数据能力接口及前端缓存
  - RAW 图片元数据支持提取基础 EXIF 与 GPS 信息
  - TIFF 原始格式增加 EXIF fallback 解析
  - ZIP 归档预览新增文件名编码选择
- **前端体验**
  - 登录页新增 MFA challenge 面板
  - 安全设置新增 MFA 管理区块
  - 分享页拆分密码面板、控制器和无限滚动加载逻辑
  - 文件信息面板扩展媒体元数据展示

### Changed

- **数据库连接模型**
  - `AppState` 移除冗余 `db` 字段，统一通过 `writer_db()` / reader handles 访问数据库
  - SQLite 引入读写分离连接池，减少读请求对写连接的占用
- **归档预览架构**
  - ZIP 原始扫描与显示层限制签名拆分
  - 归档清单缓存升级到 v2，记录编码、兼容性提示和更细的错误分类
  - 前端压缩包预览拆成状态模型、内容组件和交互控制
- **媒体元数据与预览**
  - 媒体元数据提取支持 range 读取，降低远程存储场景下的读取成本
  - 文件预览、音乐播放器、分享播放队列和信息面板统一读取后端媒体能力
- **前端结构与质量**
  - Shell、分享视图、文件信息面板、预览对话框等模块继续拆分 controller / hook / view
  - 多处补齐 `aria-label`、`aria-expanded` 与屏幕阅读器辅助文本
  - 移除不再使用的前端依赖并升级 Vite、Vitest、Base UI、Hono、shadcn 等依赖

### Fixed

- 修复 MFA 登录、恢复码、TOTP 设置和异常状态处理中的多项边界问题
- 修复下载指标记录与存储驱动缓存失效问题
- 修复使用只读数据库连接做权限校验时可能导致的一致性问题
- 修复部分媒体文件在元数据提取、预览和详情展示中的兼容问题
- 修复压缩包预览在非 UTF-8 文件名、编码探测和错误展示上的兼容性问题

### Security

- 新增 MFA secret 加密配置与 TOTP 密钥保护
- Web 应用嵌入式预览 iframe 增加 `sandbox` 限制
- `SECURITY.md` 扩展安全政策、报告流程和支持版本说明
- 指标文档明确 `/health/metrics` 需要内网或白名单保护

### Notes

- 本版本为 `0.2.0` 系列第一个 RC 版本
- 新增数据库迁移：`m20260523_000001_add_mfa`
- 新增配置项：`[auth].mfa_secret_key`；替换该密钥会导致已启用 MFA 的认证器密钥无法解密，升级前必须备份配置和数据库
- 登录 API 响应调整为带 `status` 的 tagged enum；自定义客户端需要处理 `mfa_required` 分支
- Prometheus 指标需要启用 `metrics` feature 后重新编译，并谨慎暴露 `/health/metrics`
- Docker 默认引导 `ffprobe` CLI，用于媒体元数据能力探测
- 统计数据：707 files changed, 40,624 insertions(+), 10,506 deletions(-)
- 本次范围共 39 个提交

## [v0.2.0-beta.3] - 2026-05-21

### Release Highlights

**`0.2.0` 系列继续补齐媒体元数据与错误语义。** 本版本把 blob 级媒体元数据提取接入后台任务和数据库缓存，前端文件详情页与分享页能展示更完整的 EXIF / 音视频信息，同时扩展 API subcode，并收口健康检查和国际化结构。

- **Blob 级媒体元数据缓存** — 新增 `blob_media_metadata` 表、仓库和提取服务，按 blob hash 缓存图片 / 音频 / 视频元数据
- **文件详情元数据展示** — 文件信息面板和分享页同步接入更完整的媒体信息展示，覆盖图片 EXIF、音频标签和视频基础信息
- **API subcode 收口** — 扩展稳定机器可读错误子码，并同步 OpenAPI、前端错误映射和类型定义
- **健康检查轻量化** — 就绪探针从写入测试改为轻量 `readiness_check`，减少健康探测副作用
- **前端结构整理** — 多语言资源按模块拆分，媒体相关前端组件和配置文案同步收口

### Added

- 新增 blob 级媒体元数据迁移、仓库和提取服务
- 新增媒体元数据后台任务，用于异步提取和缓存
- 管理后台新增媒体元数据开关及相关配置项
- 前端补齐媒体元数据渲染、缩略图辅助和文件详情展示逻辑
- `ApiSubcode` 枚举和 OpenAPI 定义同步扩展

### Changed

- 文件信息、预览、音乐播放器和分享页的媒体展示逻辑进一步拆分和整理
- `health_service` 的 ready 检查改为更轻量的实现
- 前端 i18n 从单文件资源拆分为模块化目录结构
- 媒体处理相关配置文案和页面结构同步调整

### Fixed

- 修复媒体元数据缓存、解析和缩略图处理中的多项边界问题
- 修复部分媒体文件在预览和详情页中的兼容性问题

### Notes

- 本版本新增数据库迁移：`m20260520_000001_add_blob_media_metadata`
- 升级前建议备份数据库与数据目录
- 统计数据：409 files changed, 19,731 insertions(+), 7,744 deletions(-)
- 本次范围共 20 个提交

## [v0.2.0-beta.2] - 2026-05-19

### Release Highlights

**`0.2.0` 系列继续补齐媒体体验与认证安全细节。** 本版本把音频预览升级为全局音乐播放器，新增图片预览派生接口，优化分享流播放会话、上传进度、多标签页刷新协作，以及 OIDC / Passkey / 存储策略相关边界。

- **全局音乐播放器** — 音频预览升级为可跨页面保活的播放队列，支持上一首 / 下一首、循环 / 单曲 / 随机、音量、进度、媒体会话和元数据解析
- **图片预览 WebP 派生物** — 新增个人、团队与分享页图片预览接口，支持 1600px WebP 派生缓存、ETag / 304 和 HEIF 后端预览降级
- **分享流播放会话可配置** — 分享音频 / 视频 Range 流会话 TTL 改为运行时配置，默认 3 小时，支持 5 分钟到 24 小时范围校验
- **上传体验增强** — 上传任务显示平滑速度，direct / presigned / chunked / multipart 请求统一跟踪和取消，减少取消后残留请求
- **认证刷新更稳** — 多标签页 access token 刷新增加 localStorage 协调；刷新 token 复用检测加入同客户端短窗口判定，降低并发刷新误杀
- **公开站点与 Passkey 配置收口** — setup 可从请求 Origin 初始化 `public_site_url`，管理端支持一键填入当前地址，Passkey 缺少站点 URL 时返回明确配置错误

### Added

- **全局音乐播放器**
  - 新增 `MusicPlayerHost`、音乐播放器 store 与队列构建工具
  - 支持音频文件列表队列播放、分享页音乐播放、后台面板入口和播放详情
  - 支持 `music-metadata` 解析标题、歌手、专辑和封面，并接入浏览器 Media Session
  - 播放分享文件时可自动刷新临近过期的流播放 session
- **图片预览接口**
  - 新增 `/api/v1/files/{id}/image-preview`
  - 新增 `/api/v1/teams/{team_id}/files/{id}/image-preview`
  - 新增 `/api/v1/s/{token}/image-preview` 与分享文件夹内文件图片预览接口
  - 图片预览统一输出 WebP，缓存路径按处理器与版本隔离
- **分享流播放配置**
  - 新增 `share_stream_session_ttl_secs` 运行时配置
  - 管理后台新增对应配置项文案和校验
- **上传速度显示**
  - 上传任务项新增速度展示
  - direct、presigned、chunked 与 presigned multipart 上传均记录已上传字节与速度
- **多标签页刷新协调**
  - 前端新增跨标签页刷新锁，避免多个标签页同时刷新 access token
  - 同步 peer 刷新结果后会补齐本地 session 过期时间
- **文档**
  - 补充 Passkey、外部认证、ZIP 预览、流播放 session、内部存储协议、分享、远程节点和错误处理文档
  - 文档站新增 CNAME 与 robots.txt

### Changed

- **音频 / 视频预览架构**
  - 预览组件中的视频流工厂泛化为媒体流工厂
  - 旧的 blob 媒体预览拆分为图片预览、音乐预览和视频预览
  - 音频预览不再只嵌入单个 `<audio>`，改为把文件载入全局播放器
- **媒体处理**
  - 内置 image 管线、vips_cli、ffmpeg_cli 和 storage_native 处理器支持图片预览派生物
  - vips / ffmpeg 日志不再输出本地输入输出路径，降低路径泄露风险
  - 缩略图和图片预览 ETag 均带处理器 namespace 与版本
- **上传取消**
  - 前端统一登记上传 XHR，请求取消时可中止同任务的所有在途上传请求
  - presigned multipart 取消逻辑调整，非 assembling 状态立即删除 session 并中止远端 multipart 上传
- **公开站点配置**
  - 管理端检测 `public_site_url` 前会读取最新配置；多来源配置不再弹单值修复弹窗，而是跳转到设置页
  - 设置页字符串数组配置支持把当前访问地址直接加入 `public_site_url`
- **存储策略校验**
  - S3-compatible 策略创建、更新和连接测试必须提供非空 `access_key` / `secret_key`
  - 本地与远程策略保持原有连接字段行为
- **依赖**
  - 前端新增 `music-metadata`
  - 更新 Hono、ip-address、react-arborist、tsgo preview 等依赖

### Fixed

- **刷新 token 并发误杀**
  - 同一客户端在短宽限窗口内重复提交刚轮换的 refresh token 时返回 stale token，不再直接吊销全部会话
  - 不同客户端、缺少客户端证据或超出宽限窗口的复用仍按疑似泄露处理并吊销会话
- **审计日志 IP**
  - 登录、刷新、登出、会话吊销和改密审计按可信代理配置解析 `X-Forwarded-For`
  - 未受信代理来源的伪造转发头会被忽略
  - 支持带端口 IPv4 与方括号 IPv6 的转发地址解析
- **Passkey / OIDC 配置**
  - Passkey 登录在缺少 `public_site_url` 时返回明确配置错误
  - OIDC provider slug、issuer normalize 和回调边界补充测试覆盖
- **上传清理**
  - presigned multipart 取消后会等待远端 multipart abort 可见，减少 RustFS / S3 临时上传残留
- **图片预览兼容性**
  - HEIF / HEIC 图片优先使用后端派生预览，浏览器不支持原始格式时不再直接显示失败

### Security

- **刷新 token 复用检测精细化**
  - 保留 refresh token 复用吊销全部会话的安全策略
  - 同客户端短时间并发刷新被识别为 stale refresh，避免正常多标签页并发触发错误吊销
- **可信代理审计 IP**
  - 审计日志只在 peer 命中可信代理 CIDR / IP 时采用 `X-Forwarded-For`
  - 未受信客户端无法通过伪造 header 污染登录和会话审计 IP
- **S3 策略凭证校验**
  - 防止创建或测试缺少 access key / secret key 的 S3-compatible 存储策略

### Notes

- 本版本为 `0.2.0` 系列第二个 beta 版本，主要聚焦媒体体验、分享流播放和认证稳定性
- 没有新增数据库 migration
- 分享音频 / 视频播放链接默认有效期从 30 分钟调整为 3 小时，可通过 `share_stream_session_ttl_secs` 修改
- 自定义客户端如要使用图片预览，可优先请求新的 `image-preview` 接口，并按 ETag 处理 `304 Not Modified`
- Docker / 生产环境升级前仍建议备份数据库与数据目录

---

**统计数据**：
- 167 files changed, 11,410 insertions(+), 590 deletions(-)
- 9 commits

---

## [v0.2.0-beta.1] - 2026-05-18

### Release Highlights

**AsterDrive 进入 `0.2.0` 系列！** 在 `v0.1.0` 稳定版基础上，本版本聚焦企业级登录、归档预览、远程存储协议和搜索/审计可观测性的能力扩展，并完成多处安全与性能层面的收口。

- **OIDC 单点登录** — 完整支持 OpenID Connect 外部认证，含管理员配置面板、提供商管理、邮件验证和账户关联流程
- **WebAuthn Passkey** — 新增 Passkey 注册 / 登录 / 管理全流程，支持 conditional UI 自动探测和缓存
- **ZIP 归档只读预览** — 归档预览改为后台任务异步生成清单，支持 Range 直接扫描 ZIP 目录，无需下载完整文件
- **远程存储协议 v2** — 引入能力协商机制，探测阶段即拦截不兼容节点；细化 CORS / Range 合约
- **搜索与文件分类增强** — 文件新增分类与扩展名字段，全局搜索支持按类型过滤，侧边栏快捷分类入口
- **签名链路升级 HMAC-SHA256** — 直链与预览 token 改为 HMAC-SHA256，绑定 purpose 字符串，消除长度扩展攻击风险

### Added

- **OIDC 外部认证（SSO）**
  - 完整支持 OpenID Connect 单点登录流程
  - 管理后台新增外部登录提供商配置面板与状态管理
  - 邮件模板补齐外部认证邮箱验证、关联和异常通知
  - 外部认证服务与前端组件拆分为独立模块
- **WebAuthn Passkey**
  - 用户安全设置新增 Passkey 注册 / 登录 / 删除全流程
  - 登录页支持 conditional UI 自动探测可用凭证
  - 后端引入 `webauthn-rs`，新增 `passkeys` 表存储凭证元数据
- **归档预览**
  - 新增 ZIP 压缩包只读预览，前端按目录树浏览归档内容
  - 归档清单改为后台异步任务生成，避免阻塞预览请求
  - 支持 Range 读取直接扫描 ZIP 目录，跳过整文件下载
  - 新增 `archive_preview` 配置组与对应限流策略
- **远程存储协议 v2**
  - 引入 `RemoteStorageCapabilities` 能力协商机制
  - 节点探测阶段校验 `features` / `browser_cors` / `limits` 等约束
  - 不兼容的旧版本远程节点会在加入阶段被拒绝
- **搜索与文件分类**
  - 文件实体新增 `extension` / `compound_extension` / `file_category` 字段
  - 全局搜索 API 支持按文件类型分类和扩展名过滤
  - 前端侧边栏新增图片 / 视频 / 文档 / 音频等快捷分类入口
- **PDF 预览虚拟滚动**
  - 直接 URL 流式加载替换原有 Blob 预加载
  - 引入 `@tanstack/react-virtual` 虚拟滚动，大文档仅渲染视口内页面
- **WebDAV 系统文件拦截**
  - 新增 `webdav_block_system_files_enabled` 运行时配置
  - 支持按模式匹配拦截 `.DS_Store`、`Thumbs.db` 等系统垃圾文件
- **API 错误子码**
  - 引入类型安全的 `ApiSubcode` 枚举系统
  - OpenAPI schema 中 `subcode` 字段从动态字符串改为已知枚举集合
- **管理与运维**
  - 团队列表后端支持关键词搜索、分页与防抖查询
  - 后台任务调度器性能优化，审计日志改为批处理写入

### Changed

- **存储变更事件**
  - 软删除事件改为 `file.trashed` / `folder.trashed`，`*.deleted` 仅保留给硬删除
  - 新增 `file.purged` / `folder.purged` / `file.version_restored` / `file.version_deleted` 等精细事件
  - 事件携带 `affects_quota` 与 `storage_delta`，前端按字段刷新用户配额
- **分片上传**
  - 接口从 `web::Bytes` 改为 `web::Payload` 流式接收
  - 实时校验分片大小，超限立即返回 413，避免内存预分配
  - 优化上传路径资源占用，缩略图改为后台任务生成
- **审计日志**
  - `entity_type` 字段从动态字符串收紧为 `AuditEntityType` 强类型枚举
  - 数据库列扩展长度后改为 NOT NULL，历史空值分批回填
- **用户信息接口**
  - `/auth/me` 支持 `?fields=quota,profile,preferences,session` 按需查询
- **安全设置页面**
  - 重构为标签页布局，新增动画折叠组件
  - Passkey 列表加入本地缓存策略，减少接口往返
- **品牌配置**
  - 控制字符处理与验证逻辑更严格，避免异常字符污染品牌字段
- **归档解压事件**
  - 解压任务的存储变更事件从多次发布合并为单次发布
- **远程存储 CORS 合约**
  - presigned 下载 / 上传须满足 `Range` / `Content-Range` 等头部要求
  - 不满足的旧节点会在能力协商阶段被识别

### Fixed

- **归档预览**
  - 修复归档预览缓存边界条件、WebDAV 属性隔离和文件类型检测问题
  - 限制签名不再影响归档清单缓存有效性
- **WebDAV / 存储 / 任务调度**
  - 修复多处缓存校验竞态与后台任务调度逻辑缺陷
  - 修复任务错误处理与存储事件归并相关的边界问题
- **工作空间**
  - 修复多处搜索、认证及数据查询逻辑缺陷
- **前端输入体验**
  - 修复输入框编辑时光标跳到末尾的问题，优化焦点态行为

### Security

- **Token 签名升级 HMAC-SHA256**
  - 直链 token 新增 v2 格式 `v2.<base62-id>.<HMAC-SHA256>`
  - 预览链接与分享流签名由裸 SHA256 改为 HMAC-SHA256
  - 签名绑定 purpose 字符串，消除长度扩展攻击与跨用途重用风险
  - 预览链接限流逻辑同步重构
- **外部认证全链路加固**
  - 完善 OIDC 回调、邮箱验证、账户关联各环节的安全校验
  - 补充失败路径与异常通知

### Breaking Changes

- **远程存储协议最低版本提升至 v2** — 运行 v1 协议的旧远程节点将无法通过探测阶段的兼容性校验，必须同步升级远端实例
- **存储变更事件语义调整** — 软删除事件由 `file.deleted` / `folder.deleted` 改为 `file.trashed` / `folder.trashed`；监听 SSE / WebSocket 的第三方客户端需更新事件处理逻辑
- **API 子错误码 Schema 收紧** — OpenAPI 中 `subcode` 字段类型从 `string` 变为枚举集合，wire format 仍是字符串，但生成的 SDK 类型定义需同步更新

### Notes

- 本版本为 `0.2.0` 系列首个预发布版本（beta.1），仍处于功能扩展期，生产环境建议继续使用 `v0.1.0` 稳定版
- 升级前请备份数据库与数据目录，本版本包含 4 个新增 migration（passkeys、外部认证、文件类型字段、审计日志 entity_type）
- 远程从节点必须同步升级到支持协议 v2 的版本后才能继续工作
- 自定义客户端如果监听存储变更 SSE，需要把软删除监听从 `*.deleted` 切换到 `*.trashed`
- Docker 用户可使用 `v0.2.0-beta.1` 或 `edge` 镜像标签

---

**统计数据**：
- 491 files changed, 49,482 insertions(+), 5,069 deletions(-)
- 46 commits

---

## [v0.1.0] - 2026-05-15

### Release Highlights

**AsterDrive 第一个稳定版本！** 从 `v0.0.1-alpha.1` 到 `v0.1.0`，AsterDrive 完成了自托管云存储核心能力、远程存储、团队协作、WebDAV、分享、在线预览、后台任务和生产部署文档的第一轮产品化收口。

- **正式版稳定性收口** — 在 rc.2 基础上补齐服务优雅关闭、崩溃诊断、SSE 关闭语义和本地临时文件清理日志，降低生产环境排障成本
- **生产部署文档完善** — 文档站重构导航与首页，新增生产上线检查、S3 / MinIO / R2、远程从节点、团队权限、在线预览、术语表和 FAQ
- **多架构镜像发布优化** — Docker 镜像改为 amd64 / arm64 原生 runner 分架构构建，再发布 multi-arch manifest，并继续生成 SBOM 与 cosign 签名
- **文件浏览器操作体验微调** — 单文件选择时下载按钮直接下载原文件，多选或包含文件夹时才进入归档下载；工作空间切换器移动到侧边栏顶部
- **服务端可观测性增强** — 团队、策略、锁、WebDAV、删除、清理和版本回收等关键路径补充 tracing 日志，便于定位生产问题
- **E2E 与集成测试稳定性修复** — 优化任务卡片、批量操作、团队空间、WebDAV 密码字段和时间精度相关断言，减少测试选择器和数据库精度噪音

### Added

- **生产部署与使用文档**
  - 新增生产上线检查清单，覆盖反向代理、持久化目录、备份、邮件、任务、预览与升级前验证
  - 新增 S3 / MinIO / Cloudflare R2 存储配置文档
  - 新增远程从节点存储文档，说明主控-从节点部署、enrollment、反向代理和排障流程
  - 新增团队与权限、在线预览与 WOPI、术语表、FAQ 和文档贡献说明
- **运行时关闭与崩溃诊断**
  - HTTP 服务统一使用自定义 shutdown signal 处理，并设置 8 秒 graceful shutdown timeout
  - 存储变更 SSE 在服务关闭时主动结束已有连接，并拒绝关闭后的新连接
  - panic 诊断日志写入 `data/crash.log`，写入失败时会把完整诊断报告输出到 stderr
- **发布镜像能力**
  - Docker CI 增加 amd64 / arm64 分架构构建 job
  - 发布阶段生成 GHCR 与 Docker Hub multi-arch manifest
  - 稳定版本自动发布 `latest` / `stable` 标签，预发布版本继续发布 `edge`
- **观测日志**
  - 删除、永久清理、文件版本回收、锁生命周期、团队归档 / 恢复 / 强删、策略删除、WebDAV 账号删除等路径补充 tracing 事件
  - 本地存储临时文件清理失败不再静默忽略，会记录 warn 日志

### Changed

- **版本与发布定位**
  - 根 crate 版本从 `0.1.0-rc.2` 升级到 `0.1.0`
  - `0.1.0` 系列从 release candidate 切换为第一个稳定版
- **文档站结构**
  - VitePress 导航从扁平入口重组为“开始 / 使用 / 管理 / 配置 / 存储 / 部署 / 开发”
  - 首页改为面向部署、使用、运维和二次开发的入口页
  - 配置、部署和使用文档补充更多交叉链接与当前版本行为说明
- **文件浏览器操作**
  - 批量选择工具栏和右键菜单统一使用 `downloadAction`
  - 只选中单个文件时，“下载”直接下载原文件；多选或包含文件夹时继续使用归档下载任务
  - 工作空间切换器从 TopBar 移到侧边栏顶部，团队空间入口更稳定
- **Docker 运行环境**
  - 运行时镜像补充 `vips-poppler`，增强 PDF / 文档类预览处理依赖
  - `docker-compose.yml` 增加 `stop_grace_period: 45s`，配合服务端优雅关闭
- **前端实时事件**
  - EventSource 被服务端永久关闭时不再进入退避重连，避免关闭期间产生无意义重连

### Fixed

- **服务关闭时 SSE 连接处理**
  - 修复服务端关闭期间存储变更事件流可能继续挂起的问题
  - 修复关闭后新建 SSE 连接仍进入流式响应的问题；现在返回 `204 No Content`
- **崩溃日志可靠性**
  - 修复 crash log 目录不存在时无法写入诊断日志的问题
  - 修复 crash log 写锁竞争或权限失败时用户只能看到简短失败提示、拿不到完整报告的问题
- **本地存储临时文件清理**
  - 修复本地上传、去重提升和 copy fallback 路径中临时文件清理失败被静默吞掉的问题
  - 清理失败现在会记录具体路径与错误，便于追踪磁盘残留
- **测试稳定性**
  - 修复 WebDAV 账号测试中密码字段异步断言不稳定的问题
  - 修复后台任务健康检查测试中数据库时间精度差异导致的偶发失败
  - 优化 Playwright 测试定位方式，避免任务列表、团队空间和批量操作场景误匹配
- **生产构建流程**
  - 避免 Docker 多架构镜像在单 job QEMU 构建下耗时过长或不稳定，改为原生 runner 构建后合并 manifest

### Notes

- 本版本为 AsterDrive 第一个稳定版本，也是 `0.1.0` 系列正式发布版
- 从 `v0.1.0-rc.2` 升级到 `v0.1.0` 没有新增数据库 migration
- 升级前仍建议备份数据库与数据目录，生产部署建议按新增的 production checklist 逐项检查
- Docker 用户建议使用 `v0.1.0`、`stable` 或 `latest` 镜像标签；`edge` 继续保留给 alpha / beta / rc 预发布版本
- 自定义客户端如果依赖 `/api/v1/auth/events/storage` SSE 连接，需要注意服务关闭时连接可能正常结束，关闭后的新连接会返回 `204`

---

**统计数据**：
- 101 files changed, 3,621 insertions(+), 751 deletions(-)
- 12 commits

---

## [v0.1.0-rc.2] - 2026-05-13

### Release Highlights

- **文件浏览器批量操作重构** — 批量移动、复制、删除、归档下载和压缩入口统一到选择工具栏，操作反馈与刷新流程更一致
- **实时事件去重** — 本机触发的文件操作不再被 SSE 回声重复刷新，减少列表抖动和重复状态更新
- **顶部工作空间切换器** — 个人空间与团队空间可在顶部快速切换，并支持团队搜索与管理入口跳转
- **管理后台异步操作反馈增强** — 删除、解锁、清理等操作增加 pending 状态，避免重复点击和误操作
- **存储策略强制删除与兜底清理** — 管理员可强制删除被上传会话占用的策略，并自动清理相关上传会话与临时对象
- **公开配置缓存优化** — 公开品牌、预览应用和缩略图支持配置加入缓存与失效机制，减少重复计算和接口开销

### Added

- **工作空间切换器**
  - 顶部栏新增个人空间 / 团队空间切换入口
  - 支持团队搜索、当前空间标识和团队管理跳转
  - 团队加载逻辑上移到布局层，减少页面内重复处理
- **存储策略删除兜底任务**
  - 强制删除存储策略时会清理关联上传会话
  - 新增存储策略删除后的临时对象兜底清理任务
  - 补充预签名上传策略强删后的延迟清理测试
- **公开配置缓存**
  - 公开配置接口增加缓存与缓存失效机制
  - 品牌、预览应用和缩略图支持信息读取减少重复查询
- **测试覆盖**
  - 大幅补充本地 / S3 / 远程存储、任务调度、邮件、缓存、策略和上传集成测试
  - 补充文件浏览器批量操作、上下文菜单、工作空间切换器和管理后台 pending 状态单元测试

### Changed

- **文件浏览器批量操作**
  - 批量操作逻辑迁移到独立 hook，减少页面组件状态交织
  - 文件 / 文件夹上下文菜单与选择工具栏的批量行为更一致
  - 回收站批量操作、表格和网格视图的选择反馈同步调整
- **实时存储事件处理**
  - 新增前端 storage event echo 记录与去重逻辑
  - 删除、恢复等本机操作触发的 SSE 回声会被识别并跳过重复处理
- **管理后台列表体验**
  - 用户、策略、策略组、分享、锁和远程节点列表的异步操作增加 pending 状态
  - 删除、解锁、清理等操作期间按钮禁用并展示处理中反馈
- **模块拆分与可维护性**
  - 后端仓储、审计、锁、任务调度、上传完成、缩略图、本地 / S3 / 远程存储驱动拆分为子模块
  - 类型定义拆分到按领域组织的 `types/*` 模块
  - 前端管理后台查询参数类型迁移到生成 API 类型

### Fixed

- **SSE 重复刷新**
  - 修复本机操作后又收到同一事件导致列表重复刷新、状态抖动的问题
- **后台任务记录噪音**
  - 系统健康检查连续成功时复用最近记录，减少后台任务列表噪音和数据库增长
- **管理操作重复提交**
  - 删除、解锁等异步操作执行期间阻止重复点击，降低重复请求和误操作风险

### Notes

- 本版本为 `0.1.0` 系列第二个 release candidate
- 该版本包含大量内部模块拆分，主要影响可维护性和测试覆盖，不改变公开 API 的主要使用方式
- 自定义客户端如果依赖存储策略删除、上传会话或文件列表实时刷新行为，建议重点验证相关流程
- 升级前仍建议备份数据库，并按 rc.1 的迁移基线要求确认旧部署已完成 pre-rc.1 迁移链

---

**统计数据**：
- 244 files changed, 24,194 insertions(+), 15,147 deletions(-)
- 18 commits

---

## [v0.1.0-rc.1] - 2026-05-12

### Release Highlights

- **首个 RC 版本与迁移基线收口** — 版本升级到 `0.1.0-rc.1`，数据库 migration 重新压缩为 `m20260512_000001_baseline_schema`，已有部署需先完成 pre-rc.1 旧迁移链再升级
- **管理后台全列表排序** — 用户、团队、成员、策略、策略组、远程节点、分享、锁、后台任务和审计日志列表支持白名单字段排序，并通过 URL 参数保持排序状态
- **用户身份展示统一为 UserSummary** — 管理后台、团队、分享、锁、任务和审计相关响应从裸用户 ID / 用户名升级为嵌套用户摘要，前端统一展示头像、显示名和用户名
- **主题强调色改为 hex 色值** — 偏好设置的 `color_preset` 从固定枚举名切换为 `#rrggbb`，前端支持自定义颜色输入，并兼容旧预设名读取
- **管理后台表格体验统一** — 抽取 AdminTable 公共组件，统一列表留白、边界、排序表头、可访问性状态和交互反馈

### Added

- **管理后台排序参数**
  - 管理用户列表支持按 ID、用户名、邮箱、角色、状态、用量、配额、创建时间和更新时间排序
  - 管理团队列表支持按 ID、名称、用量、配额、创建时间、更新时间和归档时间排序
  - 团队成员列表支持按用户名、邮箱、角色、状态、创建时间和更新时间排序
  - 管理策略、策略组、远程节点、分享、锁、后台任务和审计日志列表均新增 `sort_by` / `sort_order` 查询参数
  - 后端使用白名单枚举映射排序字段，所有非 ID 排序追加 ID 作为稳定 tie-breaker
- **用户摘要响应模型**
  - 新增 `UserSummary`，包含用户 ID、用户名和 profile 信息
  - 管理概览最近任务、审计日志、团队列表 / 详情 / 成员、分享列表、WebDAV 锁列表和团队审计记录返回用户摘要
  - 前端新增 `UserIdentity` 公共组件，统一展示头像、显示名和 `@username`
- **自定义主题色**
  - `ColorPreset` 支持解析并返回规范化的 `#rrggbb` hex 颜色
  - 前端颜色选择器新增原生 color input，允许输入预设外的强调色
  - 旧的 `blue` / `green` / `purple` / `orange` 偏好值会被兼容读取并规范化
- **测试覆盖**
  - 新增管理后台各列表显式排序、非法排序参数拒绝和后台任务 ID tie-breaker 集成测试
  - 新增 migration rebase 完整 pre-rc.1 历史重写、不完整历史拒绝和 SQLite schema 基线对齐测试
  - 新增主题色 hex 接受、非法颜色拒绝和旧预设名规范化测试
  - 新增 AdminTable 单元测试，覆盖表格结构、样式和排序表头交互

### Changed

- **数据库 migration 基线**
  - 当前 migration 集合压缩为 `m20260512_000001_baseline_schema`
  - 旧的 `m20260502_000001_baseline_schema`、文件 / 文件夹 owner provenance 拆分迁移、后台任务 `failure_can_retry` 迁移纳入新的 rc.1 baseline
  - 已完整应用 pre-rc.1 迁移链的数据库会校验关键 schema sentinels 后，仅重写 `seaql_migrations` 元数据到新 baseline
  - 升级文档更新 pre-rc.1 rebase 策略和不完整旧库处理方式
- **管理后台 API 响应**
  - 审计日志从 `user_id` 改为 `user: UserSummary | null`
  - 分享列表从裸用户 ID 改为 `user: UserSummary | null`
  - WebDAV 锁列表从 `owner_id` 改为 `owner: UserSummary | null`
  - 后台任务事件从 `creator_user_id` 改为 `creator: UserSummary | null`
  - 团队创建者、成员用户、团队审计 actor / member 统一返回用户摘要
- **前端管理表格**
  - 管理后台表格迁移到统一 `AdminTable` / `AdminSortableTableHead` 组件
  - 排序状态写入 `sortBy` / `sortOrder` URL query，刷新或复制链接后可保持当前排序
  - 表头补充 `aria-sort`，当前排序列展示方向图标
- **依赖与生成类型**
  - Rust crate 版本升级到 `0.1.0-rc.1`
  - `aws-sdk-s3` 升级到 `1.132.0`，`utoipa` 升级到 `5.5.0`
  - 前端同步升级 i18next、react-arborist、tailwind-merge、Biome、Playwright、Vite、Vitest、MSW 等依赖
  - OpenAPI 生成类型同步更新排序参数、`UserSummary` 和 hex `ColorPreset` schema

### Fixed

- **用户更新请求兼容性**
  - 管理端更新用户时会剔除 `policy_group_id: null`，避免后端把“不修改策略组”误判为非法清空操作
- **迁移 rebase 安全校验**
  - rebase 校验补充 `owner_user_id`、`created_by_user_id`、`created_by_username` 和 `background_tasks.failure_can_retry` 等 pre-rc.1 schema sentinels
  - 混合新旧 baseline、空迁移记录但已有业务表、不完整 pre-rc.1 迁移链会被明确拒绝启动并给出升级提示
- **主题偏好兼容性**
  - 已保存旧颜色预设名的用户升级后不会丢失主题色，读取时会自动映射为对应 hex 色值

### Notes

- 本版本为 `0.1.0` 系列第一个 release candidate
- 升级前请备份数据库；已有部署必须先运行最后一个 pre-rc.1 构建并完成 `m20260502_000001_baseline_schema`、`m20260508_000001_split_file_folder_owner_provenance`、`m20260511_000001_add_background_task_failure_can_retry` 后，再升级到本版本
- 新部署会直接执行 `m20260512_000001_baseline_schema`；已有完整 pre-rc.1 历史的部署只重写 migration 元数据，不清空业务表
- 自定义客户端如果消费管理后台 / 团队 / 分享 / 锁 / 审计 API，需要将裸用户字段迁移到 `UserSummary` 嵌套对象
- 用户偏好里的 `color_preset` 现在以 `#rrggbb` 返回；旧预设名仍可读取，但会规范化输出为 hex
- 管理后台列表新增 `sort_by` / `sort_order`；未知排序字段会被请求参数校验拒绝

---

**统计数据**：
- 133 files changed, 6,236 insertions(+), 2,709 deletions(-)
- 6 commits

---

## [v0.1.0-beta.5] - 2026-05-12

### Release Highlights

- **HTTP Range 与视频流式预览** — 文件下载、直链、预览链接和公开分享下载支持单段 Range 请求，视频预览改为直链 / 临时 stream session 流式播放，降低大文件预览内存占用
- **分享视频流式播放会话** — 公开分享新增短期 stream session，同一播放会话多次 Range 拉取只计一次下载次数，并兼容密码分享和文件夹分享内文件
- **归档解压与构建安全限制** — ZIP 解压和归档构建新增大小、条目数、目录深度、路径长度、压缩比和耗时等多维度限制，强化 zip bomb 与异常归档防护
- **后台任务分通道调度** — 归档、缩略图和兜底任务按 lane 独立限流，失败任务记录可重试状态，任务认领流程减少重复认领和并发超额
- **上传与存储性能优化** — 上传初始化、目录上传、文件名冲突解析、本地去重、审计日志写入和临时文件落盘路径均减少重复查询、内存占用和系统调用
- **前端跨路由上传保活与 E2E 覆盖扩展** — 上传区域提升到工作区路由层，切换页面不丢上传状态；新增多模块 Playwright 覆盖

### Added

- **HTTP Range 支持**
  - 文件下载、直链下载、预览链接和公开分享下载支持 `Range` 请求
  - 响应返回 `206 Partial Content`、`Accept-Ranges` 和 `Content-Range`
  - 支持视频 / 音频拖动播放，当前仅支持单段 Range
- **分享视频流式播放会话**
  - 新增公开分享 stream session API
  - 单文件分享和文件夹分享内子文件均可生成短期播放会话
  - 同一播放会话的多次 Range 请求只计一次下载次数
  - 密码分享通过访问 cookie 校验播放权限
- **归档安全限制配置**
  - 新增 ZIP 解压源文件大小、展开后总大小、条目数、文件数、目录数限制
  - 新增路径深度、路径长度、压缩比、单任务耗时上限限制
  - 新增归档构建条目数、源文件总量和临时输出估算限制
  - 管理后台运行时设置补充归档限制与后台任务并发配置说明
- **后台任务失败可重试状态**
  - `background_tasks` 表新增 `failure_can_retry` 字段
  - 任务 API 的 `can_retry` 根据失败类型返回，安全 / 校验类失败不再允许手动重试
  - 历史失败任务保持兼容语义
- **E2E 与集成测试覆盖**
  - 新增管理审计、团队、搜索、设置、归档任务和 WebDAV 等 Playwright 覆盖
  - 新增 Range 下载、分享流式会话、上传初始化碰撞、目录上传、任务调度、归档安全限制等后端集成测试

### Changed

- **视频预览**
  - 前端视频预览从整段 Blob 拉取改为直接使用 HTTP / 公开分享 / stream session 链接
  - Artplayer 仅预加载 metadata，初始化失败时回退原生 `<video>`
  - 分享页视频预览在受控访问场景下自动创建临时流式播放会话
- **后台任务调度**
  - 归档任务、缩略图任务和兜底任务分通道并发控制
  - 任务认领在事务内复核 lane 容量，减少并发调度超额
  - 任务认领逻辑合并为单次批量事务
- **上传初始化与完成流程**
  - 上传初始化先插入 session 再准备外部资源 / 目录，`upload_id` 冲突时自动重试
  - S3 multipart 初始化失败时尝试 abort 远端 upload
  - 上传完成路径减少重复配额检查、策略解析、文件夹验证和 actor 信息查询
  - 目录上传批量预取父目录策略与候选文件名
- **审计日志写入**
  - 审计日志改为全局异步批量写入
  - 查询、统计和关闭流程会主动 flush 待写入审计记录
  - 高频上传和文件操作路径减少同步数据库写入压力
- **本地存储与缩略图生成**
  - 本地内容去重上传使用 no-clobber hard link / 临时复制提升原子性
  - 上传临时文件写入增加 `BufWriter` 缓冲
  - 缩略图生成优先读取本地路径，远端对象流式落临时文件后处理
  - 临时文件和目录清理抽出 RAII 守卫
- **回收站与存储事件**
  - 回收站清空文件夹改为批量 forest purge，批处理失败时保留单文件夹 fallback
  - 回收站列表数量使用服务端总数，文件夹和文件分页可继续独立加载
  - SSE 存储变更事件会同步刷新个人用量、团队列表和团队用量
  - 存储变更缓存失效做短窗口合并，减少无谓 folder path cache 清理
- **依赖与构建**
  - Rust profiling 构建配置重命名并调整
  - 前端脚本改为直接调用本地 `biome`
  - 升级 React / React DOM、Vite、Tailwind CSS、i18next、MSW 等前端依赖

### Fixed

- **上传 session 冲突判断**
  - 修复唯一冲突检测逻辑，只有确认 ID 已存在时才视为 `upload_id` 碰撞
  - 避免把其他唯一约束或数据库错误误判为可重试冲突
- **上传与配额正确性**
  - 修复预上传 / 完成阶段配额检查顺序，非去重预上传会在写入对象前 fast-fail
  - 修复数据库失败或配额失败时预上传对象未清理的风险
  - 修复上传完成时重复配额预检导致的额外查询
  - 修复 blob `ref_count` 自增前缺少溢出检查的问题
- **归档解压安全**
  - 拒绝加密条目、符号链接、特殊文件、重复路径、文件 / 目录冲突和异常压缩方法
  - 校验声明大小、压缩比和 zip bomb 风险
  - 修复解压导入失败后可能留下部分已创建目录 / 文件的问题，失败时清理新建根目录
- **分享下载计数**
  - 修复客户端中断或构建响应失败时分享下载次数可能虚增的问题
  - Range / stream session 路径统一下载次数记录语义
- **前端上传与回收站**
  - 修复切出文件浏览页后活跃上传任务可能随组件卸载丢失的问题
  - 修复回收站只加载第一页时数量显示过小的问题
  - 修复“还有更多文件夹但没有更多文件”时无法继续翻页的问题
- **其他正确性问题**
  - 修复文件名冲突解析中的 Unicode NFC / NFD 规范化边界
  - 修复 tracing 日志字段格式问题
  - 修复注册 / 首次初始化表单首字段占位符误导用户输入邮箱的问题
  - 修复存储变更事件到达后个人或团队用量信息可能滞后的问题

### Notes

- 本版本为 `0.1.0-beta` 系列第五个预发布版本
- 升级需要执行数据库 migration：`background_tasks` 表新增 `failure_can_retry` 可空布尔列
- 下载接口新增单段 `Range` 支持；多段 Range 暂不支持，会返回校验错误
- 任务 API 的 `can_retry` 语义收紧，新产生的失败会明确区分可重试 / 不可重试
- 新增公开分享 stream session API 与 OpenAPI schema，自定义客户端可接入该接口实现受控视频流式播放
- 系统配置新增后台任务分通道并发和归档限制配置；`background_task_max_concurrency` 作为 fallback lane 上限
- README 移除了“仍处于活跃开发、不可生产使用”的警告块

---

**统计数据**：
- 132 files changed, 10,303 insertions(+), 1,197 deletions(-)
- 29 commits

---

## [v0.1.0-beta.4] - 2026-05-08

### Release Highlights

- **多层缓存优化系统** — 引入应用层缓存抽象，支持内存和 Redis 双后端，Redis 故障时自动降级到本地缓存，分享服务查询性能大幅提升
- **文件/文件夹所有权模型重构** — 将 `user_id` 拆分为 `owner_user_id`、`created_by_user_id`、`created_by_username`，支持团队空间资源归属追溯
- **前端组件架构重构** — 管理后台大组件拆分为可维护的子组件目录结构，状态管理逻辑抽离为独立 hooks
- **多架构原生支持扩展** — Release CI 新增 Linux ARM64/ARMv7、macOS ARM64/x86_64、Windows ARM64 编译目标，Docker 镜像支持 linux/arm64 平台
- **公共配置自动重新验证** — 前端每 60 秒定时刷新 + 窗口聚焦/可见性变化触发，确保 branding 和预览应用配置实时生效

### Added

- **多层缓存系统**
  - 新增 `src/cache/` 模块，提供统一缓存抽象接口
  - 支持 moka 内存缓存和 Redis 双后端，自动故障检测与熔断降级
  - 缓存预留机制防止并发写入冲突（缩略图生成等场景）
  - 分享服务集成：分享 token 查找缓存（60s TTL）、活跃分享目标缓存
- **文件/文件夹所有权字段**
  - `files` 和 `folders` 表新增 `owner_user_id`、`created_by_user_id`、`created_by_username` 字段
  - 支持区分资源所有者（团队空间场景为 `NULL`）与实际创建者
- **公共配置自动重新验证机制**
  - `App.tsx` 每 60 秒自动重新验证公共配置
  - 窗口 `focus` 和 `visibilitychange` 事件触发即时刷新
  - 覆盖 branding、previewApp、thumbnailSupport 三个公共配置 store
- **Docker 多架构镜像支持**
  - 新增 `linux/arm64` 平台支持
  - 双层构建缓存策略（gha + registry）
  - 镜像 cosign 签名
- **Release CI 多架构编译大幅扩展**
  - 新增 Linux ARM64、ARMv7、macOS ARM64/x86_64、Windows ARM64 目标
  - checksums.txt cosign Sigstore 签名

### Changed

- **分享服务性能优化** — 引入多层缓存减少数据库查询，缓存按 scope 前缀批量失效
- **CI 构建策略优化** — Docker 构建缓存策略优化，Release 工作流架构矩阵扩展

### Refactored

- **数据库 Migration** — `user_id` 字段拆分迁移（三后端兼容：SQLite 表重建 / SQL 列变更）
- **前端组件架构** — 98 个组件拆分重组，典型如 `AdminTeamDetailDialog` → `admin-team-detail/` 目录结构
- **服务层代码结构** — 参数传递优化、函数拆分、减少重复代码

### Notes

- ⚠️ **数据库 Schema 破坏性变更**：`files` 和 `folders` 表移除 `user_id` 字段，升级必须执行 migration
- ⚠️ **API 响应格式变更**：`FileInfo` / `FolderInfo` 不再返回 `user_id`，改为 `owner_user_id` / `created_by_user_id` / `created_by_username`
- 该 migration 标记为不可逆，因创建者用户可能已被删除，无法安全回滚
- 自定义客户端依赖 `user_id` 字段需同步更新字段名

---

**统计数据**：
- 225 files changed, 19,206 insertions(+), 11,701 deletions(-)
- 8 commits

---

## [v0.1.0-beta.3] - 2026-05-06

### Release Highlights

- **系统健康监控** — 新增数据库、缓存与远程节点综合健康检查，管理后台首页展示 healthy / degraded / unhealthy 状态与问题组件
- **Redis 缓存降级与熔断** — Redis 操作增加超时保护、短时熔断和本地 reservation fallback，避免缓存故障拖慢主链路
- **审计覆盖扩展** — 大幅扩展用户、文件、文件夹、分享、批量操作、WebDAV、WOPI、后台任务和远程节点管理的审计日志
- **远程节点注册保护** — 远程节点 enrollment 完成前禁止连接测试、健康检测和网络同步，创建入口新增主站点 URL 前置校验
- **管理后台总览升级** — 首页新增系统健康横幅、最近后台任务、近期审计事件和趋势图，支持查看系统运行任务历史
- **回收站过期时间语义化** — 回收站列表 API / 前端展示从 `deleted_at` 改为 `expires_at`，直接显示清理时间
- **分享页视觉重构** — 公开分享页重做布局、加载骨架、所有者信息、密码页和文件列表视觉层级，移动端表现更稳

### Added

- **系统健康检查**
  - 新增 `health_service`，周期性检查数据库 ping、缓存后端健康状态和远程节点探测结果
  - 新增 `system-health-check` 系统运行任务，每 5 分钟记录一次健康检查结果
  - 后台任务结果支持携带 `system_health` 元数据，包含整体状态和组件明细
  - `/health/ready` 在主节点和 follower 节点都会先验证数据库可用性，再验证存储 / follower readiness
  - 远程节点健康检测并发限制为 4，并跳过未启用、未配置 URL 或 enrollment 未完成的节点
- **管理后台系统健康面板**
  - `GET /api/v1/admin/overview` 响应新增 `system_health` 和 `recent_background_tasks`
  - 后台首页展示系统健康横幅，异常时列出 degraded / unhealthy 组件并可跳转系统运行任务历史
  - 后台首页新增最近后台任务列表，展示状态、耗时、错误信息和完成时间
  - 趋势图接入 `recharts`，展示新用户、上传和分享创建趋势
- **Redis 缓存熔断**
  - Redis backend 增加 250ms 操作超时、500ms 连接超时和有限重连策略
  - Redis 操作失败或超时后打开 5 秒 fallback circuit，期间跳过 Redis 请求
  - `health_check()` 会报告 Redis fallback 状态，系统健康面板可直接暴露缓存降级
  - `set_bytes_if_absent` 使用本地 reservation fallback，Redis 不可用时仍能避免重复生成任务
- **审计日志覆盖**
  - 新增大量 `AuditAction` 枚举值，覆盖管理端用户/策略/配置/锁/任务/远程节点操作
  - 文件、文件夹、版本、属性、批量复制/移动/删除、归档下载、回收站恢复/永久删除等路径写入审计
  - WebDAV 文件写入、移动、删除、锁定/解锁和 WOPI 打开/编辑/重命名/UserInfo 更新写入审计
  - 用户登录、登出、注册、密码重置、邮箱变更、偏好设置、头像和 session 撤销写入审计
  - 前端新增 `lib/audit.ts`，管理端审计页面可本地格式化 action 和 entity type
- **远程节点 enrollment 状态**
  - 远程节点列表和详情响应新增 `enrollment_status`
  - enrollment 状态区分 `not_started`、`pending`、`redeemed`、`completed`、`expired`
  - 连接测试、健康检测和绑定同步只允许在 `completed` 后执行
  - 未完成 enrollment 时返回 `remote_node.enrollment_required` 子错误码
- **上传面板空状态**
  - 上传面板在有上传活动但任务列表为空时继续展示
  - 新增空任务文案，避免恢复 / 清空完成任务后的面板状态突兀

### Changed

- **版本号**
  - Rust crate 版本升级到 `0.1.0-beta.3`
- **运行时健康配置**
  - 原远程节点健康检测配置说明调整为系统健康检测，覆盖数据库、缓存和远程节点
  - `system_health_check_interval_secs` 语义从单一远程节点探测扩展为综合系统健康检查间隔
- **后台任务运行记录**
  - 周期性任务统一记录非 quiet 的 SystemRuntime 事件，包括清理任务、邮件派发、blob reconcile 和系统健康检查
  - SystemRuntime 任务带耗时、摘要、错误和可选健康检查详情，管理端可直接展示运行历史
- **回收站 API**
  - 回收站文件 / 文件夹列表项字段从 `deleted_at` 改为 `expires_at`
  - 文件 cursor 查询参数改为 `file_after_expires_at`，后端按保留期换算回内部 deleted cursor
  - 前端回收站表格、网格、分页 cursor 和文案统一展示"过期/清理时间"
- **分享页 UI**
  - 分享页面拆分所有者信息、元信息行、居中状态面板、加载骨架和文件夹内容区域
  - 文件夹分享支持更清晰的 breadcrumb、视图切换、下载操作和空状态
  - 密码输入、错误页、过期页和顶部栏视觉层级重新整理
  - 公开分享页的最大宽度、卡片边框、阴影和暗色模式表现统一
- **远程节点管理**
  - 创建远程节点前要求主站点 URL 已配置，否则前端直接提示并阻止创建流程
  - 远程节点更新后仅在 enrollment completed 时同步 follower 绑定配置
  - 远程节点健康检测会同步绑定配置并持久化 capability / last_error / last_checked_at
- **WOPI 服务接口整理**
  - 将 WOPI 写入、另存、重命名等服务入口参数收口为请求结构体，减少长参数列表并通过 clippy 检查
- **依赖升级**
  - `utoipa` 升级到 `5.5.0`
  - `react-router-dom` 升级到 `7.15.0`
  - `vite-plugin-pwa` 升级到 `1.3.0`
  - 若干 Rust transitive dependency 同步更新

### Fixed

- **Redis 故障拖慢请求**
  - 修复 Redis backend 操作无短超时 / 熔断时可能持续阻塞缓存调用的问题
  - 修复 Redis 不可用时缓存健康状态无法在管理后台明确体现的问题
- **远程节点未注册完成误触网络**
  - 修复远程节点 enrollment 未完成时仍可能执行连接测试、健康检测或绑定同步的问题
  - 修复创建远程节点流程在主站点 URL 缺失时继续进入 enrollment 的问题
- **审计缺口**
  - 修复大量关键写操作无审计留痕的问题，尤其是 WebDAV、WOPI、批量操作和管理员维护操作
  - 审计日志写入统一截断 IP / User-Agent，避免异常请求头污染审计记录
- **回收站展示语义**
  - 修复回收站界面把删除时间当成清理时间展示的问题
  - 修复回收站 cursor 以删除时间暴露给前端导致语义不清的问题

### Notes

- 本版本为 `0.1.0-beta` 系列第三个预发布版本，无数据库 schema 迁移
- 回收站列表响应字段 `deleted_at` 已改为 `expires_at`，依赖该接口的自定义前端或客户端需要同步字段名
- 回收站文件 cursor 查询参数从 `file_after_deleted_at` 改为 `file_after_expires_at`
- `system_health_check_interval_secs` 不再只表示远程节点健康检测间隔，而是系统健康检查间隔
- 健康检查会访问默认存储策略和已完成 enrollment 的远程节点；远程存储异常会在系统运行任务中记录为 unhealthy
- Redis fallback 熔断窗口为 5 秒，期间缓存请求会走本地降级逻辑并在健康检查中报告异常

---

**统计数据**：
- 118 files changed, 5,597 insertions(+), 752 deletions(-)
- 11 commits

## [v0.1.0-beta.2] - 2026-05-05

### Release Highlights

- **空文件上传** — 支持上传零字节文件，自动走 direct 模式，跳过实际存储操作
- **字节级上传进度** — 上传进度改为按文件大小加权计算，chunked 和 presigned 分块上传支持逐 chunk 实时回调
- **IME 输入法兼容** — 全面修复中文/日文等输入法组字过程中误触快捷键的问题，统一 IME 检测工具模块
- **永久删除级联清理分享** — 永久删除文件/文件夹时自动清理关联分享记录，消除孤儿分享
- **Panic 崩溃报告优化** — 用户看到简短友好提示，诊断详情写入 crash.log，不再泄露源码信息到 stderr
- **管理端分享分页** — 管理后台分享列表改为偏移分页，支持 URL 参数驱动
- **剪贴板兼容性** — 统一剪贴板复制工具，自动降级到 legacy API，提升浏览器兼容性

### Added

- **空文件上传**
  - 放宽 `total_size` 验证为 `min = 0`，空文件自动走 direct 模式
  - 新增负数大小校验，`total_size < 0` 返回 400 错误
  - 前端 multipart 上传区分"缺少 file 字段"和"空文件"两种情况
  - 新增集成测试：个人空间和团队空间空文件上传完整流程
- **字节级上传进度**
  - 新增 `totalBytes` 字段和 `calculateByteProgress` 加权进度计算
  - chunked 和 presigned 分块上传支持逐 chunk 实时进度回调
  - 任务恢复时正确计算已完成分片累计字节数
  - S3 presigned 上传进度上限从 90% 统一为 95%
- **IME 输入法兼容**
  - 新增 `lib/keyboard.ts` 工具模块：IME 组合状态检测、Safari 32ms 宽限期
  - 所有键盘快捷键和带输入框的组件新增 IME 检测
  - 涉及：全局快捷键、全选、搜索、代码编辑器、PDF 页码输入、新建文件夹、管理后台 Ctrl+S
  - 新增单元测试覆盖 IME 信号检测和浏览器兼容边界
- **剪贴板复制工具**
  - 新增 `lib/clipboard.ts`：优先 `navigator.clipboard.writeText`，自动降级到 `execCommand("copy")`
  - 分享链接、我的分享、WebDAV 凭据、远程节点等复制操作统一迁移
  - 新增单元测试覆盖四种场景
- **管理端分享分页**
  - `AdminSharesPage` 改为基于 URL 参数的偏移分页（offset + pageSize）
  - 页大小选项 10/20/50，删除最后一项时自动回退上一页
  - 新增测试覆盖分页加载、删除翻页回退、URL 参数联动
- **容器资源监控**
  - 新增 `scripts/monitor.sh`：容器内资源监控，支持 cgroup v1/v2，控制台表格和 CSV 输出
  - 新增 `scripts/test.sh`：运行时内存监控辅助脚本

### Changed

- **版本号**
  - Rust crate 版本升级到 `0.1.0-beta.2`
- **Panic 崩溃报告**
  - 用户 stderr 只显示简短友好提示，不再包含源码位置和堆栈
  - 完整诊断信息（版本、平台、backtrace）写入 crash.log
  - crash.log 打开失败时优雅降级而非再次 panic
- **i18n 文案**
  - 分享模块："分享链接已创建" → "链接已创建"、"创建分享链接" → "创建链接"
  - 任务模块："打包下载" → "下载为 ZIP"
  - WebDAV 和 WOPI 相关术语统一
  - 移除 `registration_closed_desc` key
- **管理员强制删除用户**
  - 分享删除提前到文件/文件夹删除之前，避免遗留孤儿记录
- **CI Rust 工具链**
  - 新增 `rust-toolchain.toml` 固定 stable channel
  - Clippy 扩展为 `--workspace --all-targets --all-features`

### Fixed

- **分享级联清理**
  - 修复永久删除文件/文件夹时关联分享记录未被清理的问题
  - 新增 `share_repo::delete_by_file_ids` / `delete_by_folder_ids` 批量删除方法
  - 覆盖垃圾桶清除、WebDAV 递归删除、管理员强制删除用户三条路径
- **IME 误触快捷键**
  - 修复中文/日文等输入法组字过程中按确认键同时触发快捷键操作的问题
- **剪贴板复制失败**
  - 修复 `navigator.clipboard.writeText` 在非 HTTPS 或页面未聚焦时静默失败的问题

### Notes

- 本版本为 `0.1.0-beta` 系列第二个预发布版本，无 API 层面的 breaking changes
- 不涉及数据库 schema 变更
- i18n 新增 `share_direct_link_action` key，移除 `registration_closed_desc` key；如有自定义翻译覆盖需同步更新
- crash.log 路径基于 `current_dir`，部署时注意运行目录
- S3 presigned 上传进度上限从 90% 调整为 95%，仅影响前端进度显示

---

**统计数据**：
- 65 files changed, 1,812 insertions(+), 177 deletions(-)
- 7 commits

## [v0.1.0-beta.1] - 2026-05-04

### Release Highlights

- **首个 Beta 预发布** — AsterDrive 从 alpha 阶段进入 beta 阶段，核心版本升级至 `0.1.0-beta.1`
- **服务端上传恢复能力** — 新增可恢复上传 session 列表接口，覆盖个人空间与团队空间，为刷新后恢复上传提供后端依据
- **上传面板体验升级** — 前端支持配置上传并发数与自动清除已完成任务，并重构上传任务展示与恢复流程
- **并发安全与数据一致性增强** — 上传完成、文件覆盖、文件夹移动/删除、锁清理和后台任务接管增加原子转换与复核
- **数据库批量化优化** — Blob 引用计数、版本清理、文件/文件夹批量操作减少串行查询，提升大批量操作效率
- **Presigned 上传收口** — 单文件直传完成后统一迁移到最终对象 key，并清理临时对象，降低临时对象泄漏风险
- **视觉系统打磨** — 全局色彩令牌、暗色模式、卡片/按钮/弹窗/上传面板视觉层级进一步统一

### Added

- **可恢复上传**
  - 新增个人空间可恢复上传 session 列表接口
  - 新增团队空间可恢复上传 session 列表接口
  - 响应包含上传模式、目标文件夹、进度、已完成分片、过期时间和恢复所需元数据
  - OpenAPI 与前端生成类型同步补齐恢复上传接口和 DTO
- **上传设置**
  - 前端新增上传并发数设置，支持 1-8 个并发任务
  - 新增自动移除已完成任务设置，并持久化到 localStorage
  - 上传恢复流程支持从服务端加载未完成 session，并与本地 pending file 状态衔接
- **测试覆盖**
  - 补充上传恢复、上传设置、PDF 预览、文件 store、远程存储与任务接管等测试场景

### Changed

- **版本阶段**
  - Rust crate 版本升级到 `0.1.0-beta.1`
  - 本版本为首个 beta 预发布版本，不是 stable release
- **上传完成流程**
  - 过期 session 不再进入组装阶段
  - Presigned 单文件上传完成后会从临时对象复制到最终 `files/{uuid}` key，并尝试清理临时对象
  - 上传进度响应与恢复响应补齐更多分片和 session 状态信息
- **并发一致性**
  - 文件覆盖、文件夹移动/删除、锁清理、后台任务认领等流程增加锁定、状态复核和原子条件更新
  - 后台任务只接管显式 lease 已过期的 processing 任务，避免误抢仍在运行的任务
- **批量操作性能**
  - Blob 引用计数支持批量 CASE 更新和批量查询
  - 文件删除、版本清理、移动和文件夹树处理减少重复查询与串行更新
- **前端体验**
  - 重构上传面板、上传任务项和恢复交互，减少状态混乱
  - 全局设计系统更新，提升明暗模式对比度、控件层级和整体视觉一致性
  - PDF 预览水平滚动和多种预览容器的布局表现更稳定
- **文档**
  - 全面同步用户文档、部署文档、API 文档和模块设计文档，补充错误说明、反代配置和上传恢复接口说明

### Fixed

- **上传可靠性**
  - 修复过期上传 session 可能继续进入完成流程的问题
  - 修复清理任务可能误处理 assembling 中上传 session 的问题
  - 修复直传大文件受默认请求超时影响的问题
- **文件与锁一致性**
  - 修复并发覆盖时 blob / 文件记录可能被旧状态污染的边界
  - 修复锁清理与并发重锁竞争导致状态不一致的边界
  - 修复文件夹移动/删除并发下树结构复核不足的问题
- **前端状态**
  - 修复移动、剪切粘贴等操作后文件列表 loading / error 状态残留的问题
  - 修复 PDF 放大后水平滚动范围不足的问题
  - 修复上传恢复与任务项状态展示的若干边界问题
- **安全与兼容**
  - 收紧头像路径和任务认领相关兼容逻辑
  - 修复 S3、远程存储、团队与任务测试覆盖到的若干一致性边界

### Notes

- 这是 AsterDrive 的第一个 beta 预发布版本，代表核心功能已从 alpha 探索阶段进入更稳定的验证阶段
- 本版本仍不承诺 stable 级别的 API、配置和数据迁移长期兼容；生产环境升级前仍建议备份数据库和存储目录
- 本次发布重点是上传恢复、并发一致性、批量性能和 UI 质感，为后续 stable 版本收口做准备
- 未发现明确的配置或 API breaking change
- 使用 presigned 上传的部署需要确认后端存储凭据具备对象 copy/delete 权限
- 旧的 processing 后台任务如果没有 `lease_expires_at`，不会再仅凭 heartbeat 或 started_at 被自动接管

---

**统计数据**：
- 164 files changed, 3,568 insertions(+), 1,017 deletions(-)
- 10 commits

## [v0.0.1-alpha.26] - 2026-05-03

### Release Highlights

- **迁移架构硬切换落地** — 将 23 个历史 migration 合并为 baseline 基线，简化新部署的升级路径与维护成本
- **分块上传持久化重构** — 重写本地分块上传的 session 持久化逻辑，增强幂等性与可靠性，修复 WebDAV 空文件写入失败
- **头像路径安全加固** — 修复头像存储路径校验漏洞，阻断路径穿越攻击
- **偏好设置健壮性提升** — 增强前端偏好设置的防御性，拒绝并清理无效值
- **CLI 与缓存模块提取** — 提取 db_shared 模块消除重复，ReservationSet 统一管理缓存预留逻辑

### Changed

- **数据库迁移**
  - 将历史 23 个 migration 文件合并为单一 baseline 基线，新部署无需逐步执行历史迁移
  - 引入 hard cutover 升级策略，支持从旧架构直接切换到新迁移系统
  - 清理 migration 模块依赖，统一 base64 版本
- **分块上传**
  - 重构本地分块上传的持久化逻辑，session 状态与 chunk 元数据更可靠
  - 增强幂等性处理，重复上传同一 chunk 不再导致状态混乱
- **CLI 重构**
  - 提取 `db_shared` 模块，消除数据库辅助函数的重复实现
- **缓存优化**
  - 提取 `ReservationSet` 结构，统一管理缓存预留逻辑

### Fixed

- **安全修复**
  - 修复头像存储路径校验漏洞，防止通过构造特殊文件名实现路径穿越攻击
- **WebDAV 写入**
  - 修复空文件写入失败的问题
- **偏好设置**
  - 增强前端偏好设置存储的健壮性，防御无效值写入

### Notes

- 本次升级迁移系统采用 hard cutover 策略，新环境将直接基于 baseline 创建表结构
- 现有生产环境升级时需确保当前数据库版本已达到历史最新状态（v0.0.1-alpha.25）

---

**统计数据**：
- 148 files changed, 6,461 insertions(+), 9,427 deletions(-)
- 9 commits

## [v0.0.1-alpha.25] - 2026-04-30

### Release Highlights

- **Managed ingress 架构落地** — 远程 follower 写入入口改为由 primary 托管的 ingress profile，支持 local / S3 落点与默认 profile 管理
- **多主控入口迁移准备完成** — master binding 引入 `storage_namespace` 隔离，支持多个 primary 绑定同一 follower 时避免对象 key 冲突
- **公开站点 URL 支持多来源** — `public_site_url` 从单一 origin 升级为来源列表，分享、预览、WebDAV 与 WOPI 链接可按当前请求来源匹配生成
- **远程存储下载能力增强** — remote 存储支持预签名下载、Range 读取与下载响应头透传
- **远程节点管理体验升级** — 管理后台新增接入状态展示、重复接入拦截与 managed ingress profile 管理区
- **上传审计日志补齐** — 文件上传完成时记录审计事件，并避免完成重试重复写日志
- **对象 key 与来源校验加固** — 统一 object key 规范化，修复路径逃逸、prefix 边界、CSRF same-site 与分享密码校验等安全边界问题

### Added

- **Managed ingress**
  - 新增 `managed_ingress_profiles` 表，用于 follower 侧维护由 primary 托管的写入落点配置
  - 新增 managed ingress profile 服务、仓储、实体与 Admin API，支持创建、更新、删除、查询和设置默认 profile
  - 支持 local 与 S3 managed ingress profile；显式拒绝 remote driver 作为 managed ingress 目标
  - local managed ingress 强制限制在 `server.follower.managed_ingress_local_root` 下，避免 primary 下发路径逃逸
  - follower 内部写入前会校验默认 profile 是否存在、是否已应用、是否存在错误，并返回明确的 precondition 错误
- **多主控入口隔离**
  - `master_bindings` 引入 `storage_namespace`，用于隔离不同 primary 的远程对象路径
  - managed ingress profile 的唯一约束从全局 `profile_key` 调整为 `master_binding_id + profile_key`
  - 新增多主控入口迁移，处理 master binding、managed ingress profile 和 namespace 兼容数据
- **远程节点管理**
  - 远程节点列表新增接入状态展示，覆盖 `not_started`、`pending`、`redeemed`、`completed`、`expired`
  - 已完成接入的远程节点不再允许再次生成 enrollment command
  - 远程节点详情新增 managed ingress profile 管理区，支持查看 ready / pending / error 状态、revision 与错误信息
  - 管理后台支持创建、编辑、删除 local / S3 ingress profile，并切换默认 profile
- **公开站点 URL 多来源**
  - `public_site_url` 配置类型升级为 `string_array`，支持配置多个可信 HTTP(S) origin
  - public branding API 新增 `site_urls`，前端启动阶段可读取全部公开来源
  - 新增请求来源匹配逻辑，分享、预览、WebDAV 与 WOPI URL 可根据当前请求 origin 选择公开来源
- **远程下载与内部对象接口**
  - remote 存储驱动实现预签名下载能力
  - 内部对象接口支持 `Range: bytes=...` 与 `offset` / `length` 查询参数
  - 远程预签名 GET 支持透传 `response-cache-control`、`response-content-disposition`、`response-content-type`
- **上传与审计**
  - 文件上传完成后新增 `FileUpload` 审计日志，覆盖个人空间与团队空间
  - 上传完成重试如果 session 已是 `Completed`，不会重复记录审计日志
- **前端体验**
  - 用户侧边栏支持拖拽和键盘调整宽度，并持久化到 localStorage
  - 文件类型图标逻辑优化，图片类扩展名统一显示图片图标，避免非代码文件误用 language icon

### Changed

- **远程节点 enrollment**
  - `node enroll` 不再要求或接受 ingress policy，follower 接入只负责建立 master binding
  - 删除 enrollment bootstrap 中的 namespace、ingress policy id 和 ingress policy name 返回信息
  - 实际远程写入落点改由 primary 侧 managed ingress profile 管理
- **远程写入目标解析**
  - follower 内部存储请求不再使用 master binding 上的 `ingress_policy_id`
  - 远程 PUT / compose / list / get / delete 统一通过 `storage_namespace + object_key` 计算 provider path
  - follower ready 检查会确认启用的 master binding 是否具备可用默认 managed ingress profile
- **配置系统**
  - 系统配置 API 与 CLI 从纯字符串值升级为 `SystemConfigValue`
  - CLI `config set` / `import` / `validate` 支持 `string_array` 类型的 JSON array 解析与校验
  - 敏感配置在 API 响应与审计日志中继续脱敏
- **公开 URL 生成**
  - 分享、预览、WebDAV、WOPI 相关 URL 不再固定使用单一 public origin
  - 如果当前请求来源匹配 `public_site_url` 列表，则生成对应来源的绝对 URL；否则回退到第一个配置来源
- **内部存储 CORS**
  - 远程预签名内部对象接口 CORS 从仅支持 PUT 扩展为 GET / PUT / OPTIONS
  - 预检允许 `content-type` 与 `range`
  - GET 响应暴露 `Cache-Control`、`Content-Disposition`、`Content-Length`、`Content-Range`、`Content-Type`、`ETag`
- **依赖与版本**
  - Rust crate 版本升级到 `0.0.1-alpha.25`
  - 前端依赖更新包括 `i18next`、`react-i18next`、`shadcn`、`@typescript/native-preview`、`jsdom`、`msw`

### Fixed

- **对象 key 与路径安全**
  - 新增统一 object key helper，规范化重复 slash、`.`、反斜杠并拒绝 `..` 路径逃逸
  - 禁止远程对象操作直接指向 storage namespace 根
  - prefix strip 改为只在完整路径段边界匹配，避免 `base` 错误匹配 `baseball/...`
  - 本地存储驱动增强相对路径清洗，拒绝父目录逃逸
- **CSRF 来源校验**
  - CSRF 来源校验支持多个 `public_site_url` origin
  - `Origin` / `Referer` 可精确匹配 request origin 或任一配置的 public origin
  - `Sec-Fetch-Site: same-site` 不再无条件放行；缺少可信 `Origin` / `Referer` 时会拒绝 cookie-authenticated action
- **分享访问限制**
  - 分享密码 cookie 校验改为加载有效分享记录，确保过期时间和下载次数限制在密码校验阶段也生效
- **Range 与下载**
  - follower 内部对象 GET 增加严格 Range 解析，拒绝多段 range、非法单位、非法边界、空 range 和越界 offset
  - 空对象不允许请求 range
  - S3 presigned download 透传响应 header override，修复远程 / S3 下载场景下文件名、content type、cache control 不一致的问题
- **远程节点接入**
  - 已完成接入的远程节点不允许再次生成 enrollment command
  - 新增 completed enrollment 查询与集成测试覆盖
- **任务与缩略图并发**
  - task drain 在没有新 claim 但仍有 processing 任务时不再提前退出
  - 缩略图读取遇到并发 worker 导致缓存对象瞬时变化时，按 cache miss 处理而不是暴露瞬时 500

### Breaking Changes

- **数据库迁移（必须执行）**
  - `m20260425_000001_create_managed_ingress_profiles`：新增 managed ingress profile 表
  - `m20260427_000001_drop_master_binding_ingress_policy_id`：移除 master binding 上的 ingress policy 绑定
  - `m20260429_000001_prepare_multi_primary_ingress`：迁移 master binding namespace，并调整 managed ingress profile 的作用域约束
- **远程节点升级风险**
  - 本版本重构 follower 写入入口与多主控 namespace 绑定模型，升级后旧远程节点的历史写入路径可能无法自动映射到新的 `storage_namespace + managed ingress profile`
  - 如果旧远程节点使用过旧版 ingress policy / namespace 绑定，升级后可能出现远程节点文件不可见或疑似丢失
  - 升级前必须备份数据库与远程节点存储目录；升级后请检查每个远程节点的 managed ingress profile、默认 profile 和文件访问状态
  - 如远程节点无法恢复到正确写入路径，可能需要删除并重新添加远程节点，再重新配置 managed ingress profile
- **`public_site_url` 配置格式**
  - `public_site_url` 从字符串变为 JSON 字符串数组
  - 旧格式：`https://drive.example.com`
  - 新格式：`["https://drive.example.com"]`
  - 不支持 wildcard origin；origin 必须是纯 HTTP(S) origin，不允许 path、query、fragment、username 或 password
  - public branding 响应字段从 `site_url` 改为 `site_urls`
- **follower 接入模型**
  - `node enroll` 删除 `--ingress-policy-id`
  - 删除 `ASTER_BOOTSTRAP_REMOTE_INGRESS_POLICY_ID`
  - `master_bindings.ingress_policy_id` 数据库列被删除
  - 迁移后需要在 primary 侧为 remote node 配置 managed ingress profile，follower 才能接受远程写入
- **namespace 字段迁移**
  - `master_bindings.namespace` 迁移为 `master_bindings.storage_namespace`
  - `managed_followers.namespace` 被移除
  - 存储隔离 namespace 不再由 primary 创建 remote node 时显式传入，而是在 follower master binding 上分配
- **managed ingress 本地根目录配置**
  - 新增配置项 `server.follower.managed_ingress_local_root`
  - 默认值为 `managed-ingress`
  - 旧配置键 `server.managed_ingress_local_root` 会被拒绝，需要迁移到 `server.follower.managed_ingress_local_root`

### Notes

- 远程节点用户请谨慎升级：本版本可能导致旧远程节点文件不可见或需要重新添加远程节点。升级前务必备份数据库和 follower 存储目录
- 多主控 ingress 迁移中，如果已有 `managed_ingress_profiles` 数据且 `master_bindings` 多于 1 条，迁移无法自动判断旧 profile 应绑定到哪个 master binding，会中止并要求人工处理
- managed ingress profile 的默认 profile 不能直接取消默认，也不能在仍有其他 profile 时直接删除；需要先切换默认 profile
- `public_site_url` 空字符串不再是合法 normalize 输入；应配置为空数组或至少一个 HTTP(S) origin
- 反向代理如果需要支持 remote presigned download，需要放行 `Range` 请求头，并正确转发下载响应头

---

**统计数据**：
- 167 files changed, 8,806 insertions(+), 1,625 deletions(-)
- 22 commits

## [v0.0.1-alpha.24] - 2026-04-24

### Release Highlights

- **统一媒体处理服务落地** — 新增可配置媒体处理链路，支持内置图片处理、`vips_cli`、`ffmpeg_cli` 与存储原生缩略图能力
- **缩略图能力大幅增强** — 缩略图升级至 v2，支持处理器元数据、旧缓存兼容、公开能力查询与前端智能降级
- **Docker 部署开箱支持媒体处理** — Docker 镜像内置 `vips-tools`、`ffmpeg`、`libheif`，并默认启用 CLI 媒体处理器
- **Docker follower 自动 enroll** — 从节点支持通过环境变量首次启动自动接入主控，减少远程节点部署的手工步骤
- **后台任务管理增强** — 管理后台支持任务类型/状态筛选，并新增按条件清理历史终态任务能力
- **存储错误分类体系落地** — 存储驱动错误细分为鉴权、权限、配置、限流、瞬时失败等类型，并映射到更明确的 API subcode 与前端文案
- **上传完成流程更可靠** — 上传完成阶段统一处理，可重试的存储瞬时错误不再直接把 session 标记为失败
- **beta 前兼容数据规范化** — 新增迁移清理旧缩略图、预览应用、远程上传策略与锁 owner 数据格式

### Added

- **媒体处理与缩略图**
  - 新增 `media_processing` 配置模块，统一管理处理器注册表、默认配置、扩展名匹配、命令规范化与公开缩略图能力导出
  - 新增 `media_processing_service`，统一承载头像处理、缩略图生成、CLI 输入准备、处理器解析与共享处理逻辑
  - 新增 `vips_cli` 与 `ffmpeg_cli` 媒体处理器，支持通过 libvips / ffmpeg 处理更多图片、视频与 HEIC 等输入格式
  - 新增公开接口 `/api/v1/public/thumbnail-support`，前端可在请求缩略图前获取服务端支持的扩展名能力
  - `file_blobs` 新增 `thumbnail_processor` 元数据字段，用于和 `thumbnail_version` 一起区分不同处理链路生成的缓存
  - 存储策略新增 `thumbnail_processor = "storage_native"` 与 `thumbnail_extensions`，支持按扩展名绑定存储原生缩略图能力
- **管理后台**
  - 新增媒体处理配置编辑器，支持编辑处理器启用状态、扩展名列表、CLI 命令，并触发 `vips` / `ffmpeg` 可用性探测
  - 系统设置页新增媒体处理配置入口与相关中英文文案
  - 后台任务页新增筛选工具栏、任务清理弹窗与独立任务表格组件
  - 后台任务 API 新增 `kind` / `status` 查询筛选，以及 `POST /admin/tasks/cleanup` 清理接口
  - 前端新增 `thumbnailSupportService` 与 `thumbnailSupportStore`，集中加载并缓存公开缩略图能力
- **Docker 与远程节点**
  - 新增 follower 环境变量自动 enroll 服务，支持首次启动自动写入 seed config、兑换 enrollment token 并绑定主控
  - 新增 `docs/deployment/docker-follower.md`，说明 Docker 从节点自动 enroll 部署流程
  - Docker 镜像新增 `vips-tools`、`ffmpeg`、`libheif`，并默认启用 CLI 媒体处理 bootstrap 配置
- **错误体系**
  - 新增 `StorageErrorKind` 分类体系，覆盖鉴权失败、权限拒绝、配置错误、对象不存在、限流、瞬时失败、前置条件失败、不支持操作等类型
  - API 错误响应新增结构化 `error` 信息，包含 `internal_code` 与 `subcode`
  - 前端 `ApiError` 支持解析 `subcode`，并新增上传、缩略图、头像、存储、远程节点等细粒度错误文案

### Changed

- **媒体处理行为**
  - 缩略图生成从内置 `image` 处理为主，升级为按处理器优先级解析的统一链路
  - 缩略图缓存路径和 ETag 纳入 `thumbnail_processor` 与 `thumbnail_version`，避免不同处理器或版本之间复用错误缓存
  - 头像上传处理迁移到统一媒体处理服务，支持内置图片处理与 `vips_cli` 处理路径
  - 前端缩略图组件改为先读取公开支持列表，仅对支持扩展名请求缩略图，减少无意义请求和错误 toast
  - 缩略图任务 payload、display name 与完成结果补充处理器信息，便于后台任务去重和排查
- **上传与存储**
  - 上传完成流程抽出 `run_upload_completion_stage`，统一处理 assembling、完成、错误恢复与失败标记
  - 上传 session 在可重试存储错误下会恢复到原状态，允许客户端再次完成；不可恢复错误仍会标记失败
  - S3 驱动升级错误分类，识别 `NoSuchKey`、`NoSuchUpload`、`SlowDown`、`Throttling`、`ServiceUnavailable` 等 provider 错误
  - 远程存储协议将远端 API 错误码和 HTTP 状态映射为本地 `StorageErrorKind`，跨节点错误更一致
  - AWS SDK S3 升级到 `1.131.0`，`reqwest` 升级到 `0.13`
- **后台任务与运行时**
  - 后台任务调度结果处理提取为独立函数，成功任务降低日志噪音，失败时记录 runtime 结果
  - 管理端任务列表改为服务端筛选，前端通过 URL search params 保存任务类型与状态过滤条件
  - 任务清理新增只删除终态任务的约束，并支持按完成时间、任务类型、终态状态组合筛选
  - follower 模式继续跳过 primary-only 后台任务，仅保留 follower-safe 基础任务
- **配置与预览应用**
  - `config_service` 拆分为 `actions`、`public`、`schema`、`system` 子模块
  - 预览应用内置 key 统一添加 `builtin.` 命名空间，例如 `builtin.image`、`builtin.video`、`builtin.pdf`
  - 预览应用配置移除旧版 `label_i18n_key` 字段，改用 `labels` 本地化标签
  - 管理后台移除本机存储策略里冗余的提示区块
  - 系统配置默认值初始化支持通过 bootstrap 环境变量启用媒体处理器
- **内部重构**
  - `file_service/deletion.rs` 拆分为 `soft_delete`、`purge`、`blob_cleanup` 子模块，并补充 blob 清理并发与重试保护
  - `user_service.rs` 拆分为 `admin`、`models`、`preferences`、`queries` 子模块
  - 媒体处理模块拆分为配置层与服务层，CLI 输入准备、处理器解析、头像/缩略图处理职责更清晰

### Fixed

- **上传可靠性**
  - 修复上传完成阶段遇到临时性存储错误后 session 直接失败的问题；限流/瞬时失败现在可重试
  - 改善直接 relay、chunk、assembly、临时对象缺失、大小不匹配等上传错误的 subcode 和前端提示
  - 修复 S3 multipart ETag 带引号时可能导致 complete 失败的风险
- **缩略图与媒体处理**
  - 修复不同缩略图处理器或版本之间可能复用旧缓存的问题
  - 修复旧版未带版本/处理器的缩略图缓存无法平滑迁移的问题，新增历史路径读取与元数据回填
  - 缩略图输出增加格式、尺寸、大小上限校验，防止 CLI 异常输出被当作有效图片
  - CLI 输入源准备支持本地路径、预签名 URL、流式临时文件等多种策略，提升远程存储下的处理可靠性
  - 前端缩略图加载失败后降级为文件图标，减少不支持格式导致的反复请求和错误干扰
- **存储与远程节点**
  - 存储驱动错误展示时剥离内部分类前缀，避免用户看到不友好的编码消息
  - 远程存储协议对远端状态码、远端业务错误和网络错误做分类，便于客户端判断鉴权、权限、配置或临时故障
  - Docker follower bootstrap 对已完成、过期、被替换 token 且本地已有绑定的场景做幂等跳过，避免重复启动失败
- **数据清理与一致性**
  - 文件永久删除逻辑增强：blob cleanup 先 claim，删除失败会恢复 claim，避免并发清理误删或留下不可恢复状态
  - blob 删除失败后会检查对象是否已不存在，若对象已消失则允许继续删除 DB 行，提升清理幂等性
  - 资源锁过期清理在清除 `is_locked` 缓存前检查是否已有替代锁，避免并发重锁时误清锁状态
  - 资源锁 `owner_info` 从旧 XML / 纯文本兼容形态迁移为结构化 JSON，提升反序列化稳定性
- **前端错误体验**
  - `useApiError` 支持 subcode 优先映射，使上传、缩略图、头像、存储、远程节点等错误显示更具体
  - HTTP 客户端解析响应中的 `error.subcode`，不再只能依赖顶层错误码
  - 新增大量中英文错误文案，覆盖存储鉴权、权限、配置、限流、瞬时失败、缩略图处理器不可用、头像处理失败等场景

### Breaking Changes

- **数据库迁移（必须执行）**
  - `m20260424_000001_normalize_thumbnail_metadata`：为 `file_blobs` 添加 `thumbnail_processor` 字段
  - `m20260424_000002_normalize_beta_compat_data`：清理 beta 前兼容数据，属于单向规范化迁移
- **预览应用配置**
  - 内置预览应用 key 统一改为 `builtin.*`；依赖旧 key 的外部配置需要确认迁移结果
  - 预览应用配置 schema 不再使用旧版 `label_i18n_key` 字段，应改用 `labels`
- **媒体处理配置**
  - 新增系统配置项 `media_processing_registry_json`
  - 如果启用 `vips_cli` / `ffmpeg_cli`，运行环境必须存在对应命令
  - Docker 镜像默认启用 CLI 媒体处理；非 Docker 部署如需同等能力，需要自行安装 `vips` / `ffmpeg` 并配置
- **存储策略配置**
  - 旧的 `remote_upload_strategy = "chunked"` 会被迁移为 `"presigned"`
  - `thumbnail_extensions` 仅在 `thumbnail_processor = "storage_native"` 时有效，否则配置校验会失败
- **API 错误结构**
  - API 错误响应新增 `error` 字段；旧客户端忽略该字段不受影响，新客户端可使用 `subcode` 做细粒度提示
  - 存储错误码从笼统 `StorageDriverError` 分化为更具体的存储错误类型

### Notes

- Docker 部署现在默认具备更完整的媒体处理能力；systemd / 裸机部署如果想启用同等能力，需要自行安装 `vips`、`ffmpeg` 与相关编解码依赖
- `m20260424_000002_normalize_beta_compat_data` 的 down migration 为空，升级前建议备份数据库
- 前端会依赖 `/api/v1/public/thumbnail-support` 判断是否请求缩略图，反向代理需要放行该公开接口

---

**统计数据**：
- 206 files changed, 16,525 insertions(+), 4,013 deletions(-)
- 28 commits

## [v0.0.1-alpha.23] - 2026-04-22

### Release Highlights

- **远程节点存储架构落地** — 新增主控-从节点模式、远程节点管理与 enrollment 接入流程，支持将存储能力扩展到独立节点
- **远程存储上传下载链路补齐** — remote 存储支持 `relay_stream` 与 `presigned` 两种下载策略，并补全 presigned 直传与浏览器 CORS 支持
- **远程中继流式分块上传** — 新增远程节点中继流式上传链路，降低大文件上传时对主控节点临时落盘的依赖
- **认证会话系统升级** — 引入 `auth_sessions` 表，支持 refresh token 轮换、设备级会话管理与撤销
- **时区偏好与时间显示统一** — 前端新增时区偏好设置，统一绝对时间显示格式，并在关键场景补充 UTC offset 信息
- **远程节点 CLI 与运维能力增强** — 新增 `aster_drive node enroll` 等命令，简化从节点接入与运维排障
- **文档体系继续补全** — 新增远程节点、自定义前端、直链下载分流、登录/会话与架构说明文档

### Added

- **远程节点与远程存储**
  - 新增远程节点管理 API、enrollment token / ack 流程，以及主控-从节点绑定能力
  - 新增 `remote` 存储驱动与内部存储协议，支持远程健康检查、文件传输与策略联动
  - 新增远程存储 `presigned` 直传、presigned 下载重定向与中继流式上传模式
  - 新增远程节点管理后台页面、节点对话框与接入流程界面
- **认证与会话管理**
  - 新增 `auth_sessions` 表及相关迁移，支持 refresh token 轮换与持久化会话管理
  - 安全设置页新增登录设备列表、注销当前会话/其他会话能力
- **用户偏好与前端体验**
  - 新增用户自定义偏好键值对
  - 新增 `display_time_zone` 偏好字段，用于控制绝对时间显示时区
  - 新增会话平台图标识别与展示
- **CLI 与文档**
  - 新增 `aster_drive node enroll` CLI 命令
  - 新增远程节点、归档任务、自定义前端、安装与部署相关文档

### Changed

- **上传与下载策略**
  - 统一上传策略解析逻辑，S3 与 remote 存储在初始化阶段按策略自动选择 direct / chunked / presigned 模式
  - 直链下载文档与存储策略说明更新，明确 `?download=1` 在命中 presigned 下载策略时的行为
- **认证体系**
  - refresh token 流程重构，认证状态改为围绕会话记录与轮换机制运作
- **前端时间展示**
  - 绝对时间显示统一走格式化工具与用户时区偏好
  - 垃圾桶、分享、设置等页面补充更明确的时区信息
- **命名与架构语义**
  - `remote_node` 重命名为 `managed_follower`
  - `AppState` 重命名为 `PrimaryAppState`
  - 相关运行时、服务层与路由命名同步调整，以突出主控-从节点语义
- **文档与依赖**
  - 补充架构与 API 文档，完善存储、认证、远程节点与部署说明
  - 前端与后端部分依赖升级，优化对话框动画与路由体验

### Fixed

- **远程存储上传兼容性**
  - 完善 remote presigned 直传模式下的浏览器 CORS 支持
- **远程节点可靠性**
  - 增加入站文件大小限制校验
  - 优化远程节点健康检查相关并发逻辑
- **认证安全性**
  - refresh token 复用检测后可撤销整组相关会话，降低 token 被重放后的持续有效窗口
- **时间显示一致性**
  - 统一前端绝对时间展示，减少跨时区场景下的误读

### Breaking Changes

- **数据库迁移（必须执行）**
  - `m20260420_000001_create_auth_sessions`：新增 `auth_sessions` 表，用于 refresh token 轮换与会话管理
  - `m20260420_000002_create_remote_nodes`：新增远程节点、绑定与 enrollment 相关表

### Notes

- `remote_node` → `managed_follower`、`AppState` → `PrimaryAppState` 主要属于内部命名重构，不影响对外 HTTP 路径
- 认证会话机制升级后，旧登录状态在升级后可能需要重新登录
- 时间展示现受用户时区偏好影响，界面显示可能与旧版本存在差异

---

**统计数据**：
- 427 files changed, 22,410 insertions(+), 3,511 deletions(-)
- 33 commits

## [v0.0.1-alpha.22] - 2026-04-19

### Release Highlights

- **WebDAV 自研协议层** — 移除 `dav-server` 依赖，自研协议分发层，支持流式读写消除临时文件开销，统一 Basic Auth 简化客户端兼容
- **后台任务系统升级** — 引入并发控制与租约（heartbeat）机制，缩略图生成迁移到任务系统统一调度，支持多实例安全协作
- **WOPI Microsoft 365 proof-key 验签** — 完整实现 RSA proof-key 双密钥校验机制，拒绝未来时间戳与重放攻击
- **存储驱动架构重构** — 通过 trait 扩展（`ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver`）分离驱动能力
- **运行时临时目录隔离** — 短命临时文件统一隔离至 `temp_dir/_runtime`，启动清理仅作用于该子目录
- **可信代理与限流加固** — 限流中间件新增 `trusted_proxies` CIDR 配置；`/auth` 拆分匿名/认证两个限流桶
- **测试基础设施大扩展** — 新增 `test_security_fixes` / `test_tasks` / `test_wopi` / `test_local_driver_security` / `test_health` 等测试文件

### Added

- **WebDAV 自研协议层与流式 I/O**
  - 移除 `dav-server` crate 依赖，新增 `webdav/dav.rs` / `webdav/mod.rs` 自研协议分发层（PROPFIND/PROPPATCH/MKCOL/COPY/MOVE/LOCK/UNLOCK 全实现）
  - 上传/下载改为完全流式，消除写入前的临时文件落盘开销
  - LOCK 请求对不存在的路径返回 404 而非 423，符合 RFC 4918
  - 移除 Bearer JWT 认证模式，统一使用 Basic Auth（兼容 Windows / macOS Finder / Cyberduck 等更多客户端）
- **后台任务并发控制与租约机制**
  - 新增 `background_task_heartbeat` 字段与租约接管机制（迁移 `m20260417_000001`），支持多实例任务系统
  - 新增 `task_service/runtime.rs`，引入并发上限、worker 池调度
  - 缩略图生成从 channel 队列迁移至 `task_service/thumbnail.rs` 后台任务系统统一管理
  - 缩略图元数据持久化到 `file_blob` 表（迁移 `m20260417_000002`），避免重复生成
- **WOPI proof-key 验签**
  - 新增 `wopi_service/proof.rs`，实现 RSA proof-key + old-proof-key 双密钥验签
  - `wopi_service/discovery` 拆分为 actions/apps/cache/parser/security/types/url 七个子模块
  - 拒绝未来时间戳，增加重放窗口校验
- **在线解压安全限制**
  - 新增 `archive_extract_max_staging_bytes` 系统配置（默认 2 GiB），限制单次解压临时磁盘占用
  - 解压前预校验源压缩包大小及解压后总大小之和
  - 按存储策略校验每个 entry 的文件大小权限
  - 使用声明大小校验实际写入字节数，防止 ZIP entry 大小篡改
  - 失败时自动清理 staging 临时目录
- **安全与文件名规范化**
  - 新增 `security_headers` 安全响应头中间件，注入 CSP / `X-Frame-Options` / `Referrer-Policy`
  - 文件名 Unicode NFC 规范化，拒绝 Windows 保留名（CON/PRN/AUX/NUL/COM*/LPT*）
  - 引入 `validator` crate，为 admin/teams/users/policies/batch/shares/properties/webdav/wopi 等所有 DTO 添加字段级校验，路由入口统一调用 `validate_request()`
  - 分享 cookie 签名从手写 SHA256 拼接改为 HMAC-SHA256，消除潜在侧信道
  - S3 presigned URL TTL 上限钳制（最大 1 小时），防止超长凭证泄露
- **可信代理与限流加固**
  - 限流中间件新增 `trusted_proxies` CIDR 列表，按白名单从 `X-Forwarded-For` 提取真实 IP
  - `/auth` 路由拆分为 `auth` 与 `api` 两个限流桶，避免匿名暴力请求耗尽已认证用户配额
  - 速率限制配置增加零值校验
- **下载与邮件可靠性**
  - 新增 `AbortAwareStream` + `on_abort` hook，客户端断连时回滚 `download_count`，消除虚增和提前触碰 `max_downloads`
  - `share_repo` 新增 `decrement_download_count_by` 批量回滚方法（防计数下溢）
  - 新增 `ShareDownloadRollbackQueue` 异步回滚队列与系统配置 `share_download_rollback_queue_capacity`
  - 邮件 `mark_sent` 在 SMTP 成功后增加退避重试（最多 5 次，总预算约 7.6s），压缩"DB 抖动→重复发信"窗口
- **流式上传支持**
  - 新增流式上传路径，突破 actix-web 默认 10MB payload 限制
- **MIT License 声明** — `Cargo.toml` 显式声明 `license = "MIT"`
- **文档**
  - 新增 `docs/deployment/troubleshooting.md` 故障排查（启动、上传下载、分享、WebDAV、Office/WOPI、后台任务、升级异常）
  - 新增 `docs/deployment/upgrade.md` 升级与版本迁移（Docker / systemd 流程，MySQL 大表注意事项，回滚步骤）
  - 新增 `docs/guide/errors.md` 错误码处理手册
  - 新增 `docs/guide/about.md` 项目定位与设计原则
  - 新增 `developer-docs/module-designs.md` 核心模块设计文档
- **测试**
  - 新增 `tests/test_security_fixes.rs`（287 行）覆盖 CSRF、HMAC、proxy IP、proof-key 等修复
  - 新增 `tests/test_tasks.rs`（979 行）覆盖任务调度、租约、并发控制、归档压缩/解压
  - 新增 `tests/test_wopi.rs`（345 行）覆盖 proof-key 验签、锁定、会话生命周期
  - 新增 `tests/test_local_driver_security.rs`、`tests/test_health.rs`、`tests/test_directory_upload.rs`、`tests/test_edit.rs`、`tests/test_batch.rs`、`tests/test_files.rs` 等
  - CI 集成测试支持 Postgres / MySQL 后端

### Changed

- **存储驱动架构**
  - 引入 trait 扩展机制：`StorageDriver` 拆分为基础 trait + `ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver` 三个能力 trait
  - 重构目录布局：`storage/local.rs` → `storage/drivers/local.rs`，`storage/s3.rs` → `storage/drivers/s3.rs`，新增 `storage/extensions.rs`
- **API 路由与 DTO 重组**
  - 新增 `api/dto` 模块统一管理所有请求/响应结构（admin/auth/batch/files/folders/properties/shares/teams/trash/validation/webdav/wopi）
  - 个人 / 团队空间路由合并：删除 `team_batch.rs` / `team_search.rs` / `team_shares.rs` / `team_space.rs` / `team_tasks.rs` / `team_trash.rs`，逻辑迁移至统一的 `batch` / `search` / `shares` / `folders` / `tasks` / `trash` 模块
  - `auth.rs` 拆分为 `auth/cookies` / `auth/profile` / `auth/public` / `auth/session`，每个端点独立绑定限流中间件和 `JwtAuth`
- **安全中间件重构**
  - CSRF 中间件按 constants / source / token / tests 拆分子模块
  - CORS 中间件按 constants / mod / tests 拆分；新增 `RuntimeCors` 支持动态策略与 WebDAV/WOPI 协议头
  - 提取 `request_auth` 模块统一 token 提取逻辑（cookie / bearer）
- **运行时临时目录隔离**
  - 新增 `runtime_temp_dir` / `runtime_temp_file_path` 函数
  - 启动时仅清理 `_runtime` 目录，保留 `tasks` 等后台任务产物
  - 避免误删共享临时目录（如 `/tmp`）中的其他内容
  - WebDAV、文件上传、WOPI 等模块统一切换至新临时路径
- **大模块拆分**
  - `download` 服务拆分为 `build` / `response` / `streaming` / `tests` / `types`
  - `upload_service/init` 拆分为 `context` / `s3` 子模块；`complete` 拆分出 `chunked` 子模块
  - `workspace_storage_core` 拆分为 `blob` / `file_record` / `finalize` / `path` / `policy` / `quota`
  - `workspace_storage_service/store` 拆分出 `from_temp` 子模块
  - `cli/doctor` 拆分为 `execute` / `storage_scan` 子模块
  - 前端 `useUploadAreaManager` 从 1210 行单 hook 拆分为 `uploadAreaManagerShared/View`、`UploadRunners`（simple/resumable）、`UploadTaskActions`、`useUploadAreaRestore`、`useUploadAreaUploads` 等独立模块
  - `TeamManageDialog`（1168 行）拆分为 `TeamManageShell` / `TeamManageSections` / `types`
  - `FileBrowserPage` 拆分出 `FileBrowserDialogs` / `useFileBrowserArchiveActions` / `useFileBrowserContextValue` / `useFileBrowserDragAndDrop` / `useFileBrowserPageState`
- **代码质量与防御性增强**
  - 启用 `clippy::cast_possible_truncation` / `cast_sign_loss` / `unwrap_used` lint，覆盖主 crate / migration / api-docs-macros
  - 全局以 `utils::numbers` 安全转换函数替换 `as` 数值转换
  - 多服务超参数函数引入参数结构体（`StoreFromTempParams` / `StoreFromTempHints` / `CreateFileWithBlobInput` / `FolderListParams` / `CopyNameTemplate` 等），消除 `clippy::too_many_arguments`
  - `get_ancestors_in_scope` 改用单次 SQL 递归查询替代逐层循环
  - 后台周期任务每轮迭代附加 `bg_task` span，正确跨 await 传播 trace 上下文
- **数据库**
  - 分页查询排序规则统一调整为创建时间倒序
  - SQLite 改用 `SqlxSqliteConnector` 替代 `Database::connect`，修复 Windows 反斜杠路径无法连接的问题
  - 改进 SQLite URL 检测逻辑（`starts_with` 替代 `contains`）
  - 新增 `db/transaction.rs` 统一 `begin/commit` 事务接口
- **i18n 命名空间统一**
  - `username` / `email` / `password` / `refresh` 等通用键迁移至 `core` 命名空间，删除 `admin` / `auth` 中的重复定义
  - `share_expired` / `share_not_found` 错误消息从 `share` 迁移至 `errors` 命名空间
  - `formatDate` 支持可选 i18n 参数，提供英文相对时间默认回退（just now / Xm ago / Xh ago / Xd ago）
- **前端**
  - 多处 `ConfirmDialog` 重构为 `useConfirmDialog` hook，消除冗余 open 状态
  - `useStorageChangeEvents` 新增指数退避重连（上限 30s，熔断阈值 8 次）及 `onopen` 重置计数
  - `uploadPersistence` 写入失败时优雅降级：quota 超限先裁半再重试，仍失败则清空 key 防崩溃
  - 新增 `FilePreviewBody` / `FilePreviewPanel` / `FilePreviewMethodChooser` / `AnimatedCollapsible`（支持 `prefers-reduced-motion`）

### Fixed

- **WebDAV LOCK 404** — 对不存在的路径返回 404 而非 423，符合 RFC 4918
- **SQLite Windows 路径** — 反斜杠路径无法连接的问题（改用 `SqlxSqliteConnector`），新增 Windows 风格路径集成测试
- **WOPI 时间戳验证** — 拒绝未来时间戳，防止重放攻击
- **存储策略失效顺序** — `policy delete` / `update` 改为先 `invalidate driver` 再 `reload snapshot`，消除静默错路由窗口
- **下载计数虚增** — 客户端中途断连时通过 `AbortAwareStream` 回滚 `download_count`，避免提前触发 `max_downloads`
- **邮件重复发送** — `mark_sent` 失败退避重试，压缩 DB 抖动导致的重复发信窗口
- **后台任务关闭延迟** — `shutdown` 改用 `join_all + timeout` 替代 50ms 轮询
- **限流配置零值** — 速率限制配置 `0` 时的退化行为修正
- **PDF 预览跨域** — 改用 Blob 对象而非 blob URL 传递给 react-pdf，避免缓存问题
- **CORS 配置冲突** — 前端校验禁止通配符来源与凭据同时启用
- **路径越界静默** — 路径解析逃出 `base_dir` 时打印 warn 日志，避免配置错误静默生效
- **`RUST_LOG` 静默覆盖** — 检测到环境变量时追加警告，提示 `config.toml` 的 level 已被覆盖
- **多处 `unwrap` 与不安全 `as` 转换** — `build.rs`、数据库迁移、进度条、重试、任务调度、WebDAV `DavPath::root()` / `StatusCode::MULTI_STATUS` 等
- **页面布局** — `SettingsPage` / `ShareViewPage` / `TasksPage` 等页面 flex 布局缺少 `flex-col` 的问题

### Breaking Changes

- **WebDAV 鉴权** — 移除 Bearer JWT 鉴权模式，WebDAV 客户端必须使用 Basic Auth（推荐使用 WebDAV 专用账号）
- **数据库迁移（必须执行）**
  - `m20260417_000001_add_background_task_heartbeat`：后台任务表新增 heartbeat 字段，支持多实例租约
  - `m20260417_000002_add_file_blob_thumbnail_metadata`：file_blob 表新增缩略图元数据列
- **存储驱动 trait 拆分** — 第三方实现的存储驱动需根据能力额外实现 `ListStorageDriver` / `PresignedStorageDriver` / `StreamUploadDriver` trait
- **临时目录布局** — 服务启动后短命临时文件位于 `temp_dir/_runtime`；自定义清理脚本如假设 `temp_dir` 直接被清空需相应调整
- **路由模块合并** — `team_batch` / `team_search` / `team_shares` / `team_space` / `team_tasks` / `team_trash` 等独立路由模块已删除并合入统一模块（对外 HTTP 路径不变，仅影响二次开发）

---

**统计数据**：
- 608 files changed, 41,139 insertions(+), 16,484 deletions(-)
- 33 commits

## [v0.0.1-alpha.21] - 2026-04-17

### Release Highlights

- **全文搜索加速（跨数据库）** — SQLite FTS5 + trigram、PostgreSQL pg_trgm GIN、MySQL ngram FULLTEXT 三种后端统一索引，查询自动降级，短查询走 LIKE
- **全局搜索对话框** — 顶栏搜索重构为 `/` / `Ctrl+K` 快捷键唤起的全局弹窗，支持防抖搜索、键盘导航、无限滚动和搜索结果直接预览跳转
- **在线压缩与解压任务** — 新增多步骤后台任务框架，支持批量压缩（ZIP）和单文件解压，个人空间与团队空间均可用
- **S3 presigned 直链下载** — 存储策略新增 S3 下载策略配置，`presigned` 模式下鉴权后 302 重定向至短时效 S3 URL，减轻服务端流量
- **服务模块大规模拆分** — `auth_service`/`file_service`/`folder_service`/`team_service` 等 12 个大型服务文件拆分为子模块，路由层同步拆分
- **测试基础设施优化** — PostgreSQL 模板数据库 + MySQL Schema 复制，测试并发速度提升；Argon2 测试参数降级加速

### Added

- **全文搜索加速 (FTS)**
  - SQLite FTS5 虚拟表 + trigram 索引 + 同步触发器，文件/文件夹/用户/团队搜索提速
  - PostgreSQL `pg_trgm` GIN 索引，MySQL `ngram` FULLTEXT 索引
  - 提取 `search_acceleration.rs` 公共工具统一生成建表/触发器/回滚 SQL
  - 抽象 `search_query.rs` 构建函数：`sqlite_fts_match_condition`、`mysql_boolean_mode_query` 等
  - 重构 `search_repo`/`team_repo`/`user_repo`：自动选择最优查询路径
  - `doctor` 命令新增 `sqlite_search_acceleration` 检查项
  - Dockerfile 基础镜像升至 Alpine 3.23
- **全局搜索对话框**
  - `GlobalSearchDialog` 组件：防抖搜索、键盘导航（↑↓/Enter/Esc）、无限滚动加载更多
  - 搜索结果按文件/文件夹分组展示，支持缩略图预览
  - TopBar 搜索入口重构，点击或按 `/` / `Ctrl+K` 唤起
  - `AppLayout` 注册全局快捷键，搜索结果可直接跳转到目标文件夹并打开预览
- **在线压缩与解压任务**
  - 新增 `steps_json` 字段（后台任务步骤进度）
  - `createArchiveCompressTask`：批量压缩个人/团队文件为 ZIP
  - `createArchiveExtractTask`：解压单文件（.zip）到目标文件夹
  - 任务步骤状态机：`Pending`/`Active`/`Succeeded`/`Failed`/`Canceled`
  - 任务详情面板默认折叠，展开后显示步骤流与时间线
- **S3 presigned 下载**
  - `S3DownloadStrategy` 枚举：`relay_stream`（默认，流式）/ `presigned`（重定向）
  - 下载时按策略分流：presigned 返回 302 至带签名 S3 URL，携带 `Content-Disposition` 等覆盖头
  - `StorageDriver::presigned_url` 新增 `PresignedDownloadOptions` 参数
  - 前端管理面板存储策略编辑页新增"S3 下载方式"选择
- **审计日志下沉服务层**
  - 批量操作/文件/文件夹/分享/上传服务新增 `*_with_audit` 包装函数
  - 审计日志调用从路由层移入服务层，消除路由层样板代码

### Changed

- **服务模块大规模拆分**
  - 12 个大型服务拆分为子模块：`auth_service`→password/registration/session/tokens，`file_service`→common/content/deletion/download/lock/thumbnail/transfer 等
  - `auth.rs` → `auth/mod.rs` + `auth/cookies.rs`，`files.rs` → `access/mutations/upload/versions`
  - 团队空间文件路由迁移至 `files/mod.rs` 统一管理
  - `repo` 层同步拆分：`file_repo`/`folder_repo` 按 common/blob/mutation/query/trash 拆分
- **配置来源与值类型强类型化**
  - `SystemConfigSource`/`SystemConfigValueType` 枚举替代字符串
  - `AuditAction`/`ThemeMode`/`ColorPreset`/`PrefViewMode`/`Language` 迁入 `types.rs`
  - 存储策略 options/allowed_types 从 JSON 字符串改为 `StoragePolicyOptions` 结构体
  - 任务 Payload/Result 改为标签枚举，通过 `kind` 区分压缩/解压类型
- **非去重 Blob 上传事务解耦**
  - 上传 I/O 移至数据库事务外执行，失败时自动清理孤立临时文件
  - 新增 `PreparedNonDedupBlobUpload` 枚举及 `prepare_non_dedup_blob_upload` 等函数
- **后台任务优雅关闭**
  - 引入 `CancellationToken` 替代粗暴 `abort`，关闭时最长 30s 宽限期
  - 周期任务添加随机 jitter（最大 30s），避免多实例同时触发清理竞争
  - 提取 `run_periodic_iteration` 统一 panic 捕获
- **文件夹树请求使用排序偏好**
  - 文件夹树请求同步携带 `sortBy`/`sortOrder`，排序变化时自动重置树缓存
- **E2E 测试模块化**
  - 删去 1391 行单文件，按功能域拆分为 `00-auth`/`admin`/`file-browser`/`shares`/`navigation`/`webdav` 等独立 spec
  - 提取 `support/` 公共工具：`auth`/`files`/`network`/`shares`/`test`
- **Release 构建优化级别调整**
  - Cargo.toml `opt-level` 从 `"s"`（优化体积）改为 `2`（优化性能）
- **Dockerfile 基础镜像升级**
  - Alpine 3.21 → 3.23
- **CI 工作流命名**
  - `rust.yml` 改为 `Rust CI`，`frontend.yml` 改为 `Frontend CI`

### Fixed

- **MySQL 时间戳 2038 年溢出** — 全部 `timestamp_with_time_zone` 替换为 `utc_date_time_column`，MySQL 下使用 `DATETIME(6)`；历史迁移文件同步更新
- **上传取消竞态** — 取消时引入宽限期等待在途 chunk 排空后再清理；`mark_upload_session_completed` 在 assembly 期间被取消的竞态检测
- **MySQL 全文搜索最小字符数** — 从 2 提升至 3，修复 `ngram` 索引下的空结果问题
- **测试容器孤立数据库泄漏** — 按 PID 记录容器生命周期数据库，下次启动时自动清理已退出进程遗留的测试库

### Breaking Changes

- **MySQL 数据库迁移（必须执行）** — `m20260415_000004_fix_mysql_utc_datetime_columns` 将所有 `TIMESTAMP` 列改为 `DATETIME(6)`，已在使用的 MySQL实例需运行迁移
- **测试基础设施变更** — `ASTER_TEST_DATABASE_BACKEND=postgres/mysql` 时测试容器管理方式有变，详见 `developer-docs/testing.md`

---

**统计数据**：
- 347 files changed, 36,054 insertions(+), 21,310 deletions(-)
- 21 commits

## [v0.0.1-alpha.20] - 2026-04-15

### Release Highlights

- **全链路 CSRF 防护** — 实现 Double Submit Cookie 模式的 CSRF 双重提交令牌防护，所有 Cookie 认证的写操作需携带 `X-CSRF-Token` 请求头，前端 axios 拦截器自动注入，后端同时校验 Origin/Referer/Sec-Fetch-Site 来源可信性
- **`doctor --deep` 深度一致性检查** — 新增 `integrity_service` 支持存储计数漂移检测、Blob 引用计数校验、存储对象清单比对（发现无主/缺失/孤儿对象）、目录树结构校验（循环引用/丢失父节点），支持 `--fix` 自动修复
- **文件信息侧边栏与预览全屏** — 桌面端文件信息面板从弹窗改造为持久化侧边栏，支持滑入/滑出动画，新增快捷操作区和概览/状态分区；文件预览对话框新增全屏/还原窗口切换
- **安全加固全面升级** — SVG/HTML 内联沙箱 CSP 策略、Docker 非 root 运行、Sigstore cosign 签名、依赖安全审计 CI、密码最小长度提升至 8 位、修复高并发下载栈溢出
- **大规模代码重构** — 文件浏览器状态管理 7-slice 拆分、管理设置页组件化、WOPI 服务模块化、数据库迁移工具模块化、团队详情组件拆分、`parking_lot` 替换标准库锁


### Added

- **CSRF 双重提交令牌防护**
  - 后端新增 `csrf.rs` 中间件：登录/刷新时生成 32 字节随机令牌写入 `aster_csrf` Cookie，非安全请求校验 `X-CSRF-Token` 请求头
  - 同时校验 `Origin`/`Referer`/`Sec-Fetch-Site` 请求头的来源可信性
  - 前端 axios 拦截器自动从 Cookie 读取并注入 CSRF 令牌，分块上传 (XHR) 同步附加
- **`doctor --deep` 深度一致性审计**
  - 新增 `integrity_service`：存储计数漂移、Blob 引用计数、存储对象清单比对、目录树结构校验
  - 存储驱动新增 `scan_paths` visitor 接口（本地按目录遍历，S3 按分页流式消费）
  - CLI 支持 `--deep`、`--scope`、`--policy-id`、`--fix` 参数，keyset 分批（每批 1000）避免全表加载
- **SVG 内联沙箱与预览双模式**
  - HTML/SVG/XHTML 文件改为内联响应 + `Content-Security-Policy: sandbox` + `X-Content-Type-Options: nosniff`，允许预览同时阻止脚本执行
  - 前端 SVG 文件新增图片/代码双模式预览切换
- **文件信息侧边栏**
  - 桌面端 `FileInfoDialog` 改造为持久化侧边栏（220ms 滑入/滑出动画），移动端保留弹窗
  - 新增快捷操作区：预览、下载、分享、重命名、版本历史、锁定（乐观更新）
  - 信息面板拆分为概览/状态两个分区，引入 `DetailList`、`Section`、`ActionGrid` 子组件
- **文件预览全屏切换**
  - 预览对话框新增全屏/还原窗口切换按钮
- **版本号自动重排**
  - 删除历史版本后自动将后续版本号减 1，保持显示编号连续
- **对话框预加载**
  - 新增 `lazyWithPreload` 工具，封装 `requestIdleCallback` 空闲时预加载弹窗模块
  - 新增 `adminPolicyGroupLookup` 模块，策略组数据全局缓存与去重请求
- **移动端响应式优化**
  - 面包屑导航：小屏超过两级时折叠中间项为省略号下拉菜单，根目录使用 House 图标
  - 工具栏、排序菜单、视图切换按钮适配小屏尺寸
  - 汉堡菜单 List/X 图标切换动画，侧边栏遮罩层透明度过渡
- **安全基础设施**
  - Docker 容器改为 UID/GID 10001 非 root 用户运行
  - CI 新增 Sigstore cosign 签名（Docker 镜像 + Release checksums.txt）
  - CI 新增每周依赖安全审计（`cargo audit` + `bun pm audit`）
  - 密码最小长度从 6 位提升至 8 位，新增 `existingPasswordSchema` 保证已有短密码用户可登录
- **E2E 测试套件**
  - Playwright E2E 覆盖：管理员用户增删查、存储策略 CRUD、文件批量操作、分块上传断点续传、WebDAV PROPFIND/MKCOL/PUT/GET/DELETE、移动端布局
- **k6 性能基准**
  - 10+ 个性能基准脚本覆盖：登录、令牌刷新、文件夹列表、搜索、下载、直传/分块上传、批量移动、WebDAV 读写、长稳混合负载、分阶段并发爬坡 (mixed-ramp)
  - 下载/上传/WebDAV 脚本新增字节计数器，支持从 summary 直接推算吞吐量
- **文档**
  - 反向代理文档重写：Caddy/Nginx/Traefik 三套完整配置示例，HTTPS 从"建议"改为"必须"
  - 新增备份与恢复文档，覆盖 SQLite/PostgreSQL/MySQL + 本地/S3 场景
  - 新增性能基准文档和社区行为准则 (`CODE_OF_CONDUCT.md`)


### Changed

- **文件浏览器状态管理重构**
  - `fileStore` 拆分为 7 个 slice：`navigationSlice`、`searchSlice`、`selectionSlice`、`clipboardSlice`、`crudSlice`、`preferencesSlice`、`requestSlice`
  - 引入 `FileBrowserContext`/`FileBrowserProvider` 消除 `FileGrid`/`FileTable` 的 props 透传
  - HTTP 请求层添加 `AbortSignal` 支持，导航/搜索/排序操作防止竞态
- **文件浏览器与团队详情组件拆分**
  - `FileBrowserPage` 拆分为 `FileBrowserToolbar`、`FileBrowserWorkspace` 等独立组件
  - `AdminTeamDetailDialog` 拆分为 `AdminTeamDetailShell`、`AdminTeamDetailSections` 等子组件，支持页面与对话框双布局
  - 提取 `useUploadAreaManager` hook 将上传区域逻辑从 `UploadArea` 组件中解耦
  - 新增 `useMediaQuery` hook 封装媒体查询响应式逻辑
- **管理设置页拆分**
  - `AdminSettingsPage` 从 3220+ 行单文件拆分为 `CategoryContent`、`SaveBar`、`Dialogs` 等子组件和 3 个自定义 Hook
  - `AdminPolicyGroupsPage` 拆分为 `PolicyGroupsTable`、`PolicyGroupDialog`、`PolicyGroupMigrationDialog`
- **WOPI 服务模块化与 `parking_lot` 引入**
  - `wopi_service.rs` 拆分为 `locks`/`operations`/`session`/`targets`/`types`/`discovery`/`tests` 子模块
  - 全局引入 `parking_lot` 替换标准库 `Mutex`/`RwLock`，消除 lock-poison 样板代码
- **数据库迁移工具模块化**
  - `database_migration.rs` 拆分为 `apply`/`checkpoint`/`helpers`/`schema`/`verify` 子模块
- **WebDAV 接口简化**
  - `AppState` 实现 `Clone`，`AsterDavFs`/`AsterDavFile` 改为持有 `AppState` 替代多字段展开，消除大量冗余参数传递
- **SQLite 行锁简化**
  - 移除 file_repo/folder_repo/team_repo 中针对 SQLite 的伪行锁 UPDATE，依赖单连接池序列化并发
- **预览应用配置持久化缓存**
  - `previewAppStore` 新增 localStorage 缓存与会话级单次重验证，跨刷新即时水合
  - `FilePreviewDialog` 合并双 Dialog 为单一 Dialog
- **全局错误映射统一**
  - 新增 `map_aster_err_with` 方法，提取 `display_error` 工具函数
  - 全局统一为 `map_aster_err_with(|| ...)` 和 `map_aster_err_ctx("ctx", f)` 模式
- **旧版根目录布局兼容代码移除**
  - 删除 `reject_legacy_root_layout` 及 `LEGACY_*` 常量等 alpha.17 引入的临时兼容路径
- **后端路由重构**
  - `team_scope` 辅助函数上移至 `routes/mod.rs`，消除各团队路由模块中的重复定义
- **对话框挂载策略**
  - 所有对话框添加 `keepMounted`，避免切换 tab 时表单输入值丢失
- **Redis 缓存错误处理**
  - `set_ex`/`del`/前缀扫描失败时输出 `warn` 日志替代静默丢弃
- **CI 独立化**
  - 前端 CI 从 `rust.yml` 抽离为 `frontend.yml`，仅在 `frontend-panel/**` 变更时触发
  - Rust CI 新增 `cargo fmt --check` 格式检查
  - 新增代码覆盖率上报 Codecov


### Fixed

- **高并发下载栈溢出** — `RequestId` 中间件将跨 `.await` 的 `span.enter()` 改为 `.instrument(span)`，避免 actix worker 上请求 span 错误嵌套导致的 stack overflow（[`3ce13e2`](https://github.com/AsterCommunity/AsterDrive/commit/3ce13e2)，Co-authored-by: AptS-1738）
- **危险 MIME 类型内联漏洞** — HTML/SVG/XHTML 文件通过直链和预览链接可被同源内联执行，改为 CSP sandbox 策略
- **密码重置 token 误用** — 密码重置 token 被用于联系方式验证端点时错误地 `unreachable!`，改为返回 `Invalid` 重定向
- **指数退避整数溢出** — `db/retry.rs` 中延迟计算使用 `checked_shl` 与 `saturating_mul` 防止溢出
- **移动端侧边栏未撑满全高** — `inset-y-16` 拆分为 `top-16 bottom-0`
- **侧边栏展开/收起无动画** — 改用 `translate-x` 过渡动画替代 display 切换
- **对话框切换 tab 时输入值丢失** — `<Wrapper>` JSX 改为函数调用防止 React 重新挂载
- **RenameDialog 外部 name 变化未同步** — 补充 `useEffect` 同步 `currentName` prop
- **面包屑长文件名撑破布局** — 修复溢出截断样式
- **SVG 图片预览尺寸失控** — `BlobMediaPreview` 对 SVG 单独处理布局宽度
- **`public_site_url` 使用 http 未警告** — `doctor` 检查时对 `http://` 返回 warn 状态


### Breaking Changes

- **CSRF 令牌强制校验**：所有通过 Cookie 认证的写操作必须携带 `X-CSRF-Token` 请求头，自定义 API 客户端需从 `aster_csrf` Cookie 读取令牌并注入
- **密码最小长度从 6 改为 8**：新注册和修改密码必须满足 8 位，已有 6-7 位密码用户仍可登录
- **Docker 容器以非 root 运行**：挂载卷需对 UID/GID 10001 可读写，需调整 `chown` 或使用 `user:` 指令覆盖
- **旧版根目录布局兼容代码移除**：alpha.17 之前的 `config.toml`/`asterdrive.db` 放在根目录的布局不再有迁移提示


---

**统计数据**：
- 327 files changed, 32,763 insertions(+), 15,727 deletions(-)
- 29 commits


## [v0.0.1-alpha.19] - 2026-04-14

### Release Highlights

- **跨数据库后端迁移工具** — 新增 `aster-drive database-migrate` 子命令，支持在 SQLite、PostgreSQL、MySQL 之间做离线全量数据迁移。表依赖感知的复制顺序、断点续传、数据完整性验证、进度条展示
- **离线健康检查** — 新增 `aster-drive doctor` 子命令，类似 `brew doctor`，一键检查数据库连接、迁移状态、运行时配置、邮件配置、存储策略完整性，支持 `--strict` 模式
- **WOPI 协议补全** — 新增 GET_LOCK、RENAME_FILE、PUT_USER_INFO、UnlockAndRelock、PutRelativeFile 五个 WOPI 操作，大幅提升 Office 在线编辑兼容性
- **文件/文件夹同名唯一索引** — 在数据库层面添加条件唯一索引，彻底解决软删除场景下的同名竞态条件和数据完整性问题
- **CLI 模块重构与 human 输出** — CLI 拆分为模块目录结构，新增 human-readable 终端输出格式，支持彩色输出和自动格式检测


### Added

- **跨数据库迁移工具 (`database-migrate`)**
  - 三种运行模式：`apply`（执行）、`dry-run`（计划）、`verify-only`（验证）
  - 22 张表按外键依赖顺序复制，断点续传支持中断恢复
  - 迁移完成后自动验证：行数匹配、唯一约束、外键约束
  - 跨后端类型映射（Bool/Int32/Int64/Float64/String/Bytes/TimestampWithTimeZone）
  - PostgreSQL/MySQL 序列自动重置
  - 可配置批量大小（`ASTER_CLI_COPY_BATCH_SIZE`，默认 200）
- **离线健康检查 (`doctor`)**
  - 检查项：数据库连接与后端类型、迁移状态、运行时配置快照、Public Site URL 格式、SMTP 配置完整性、预览应用注册表、存储策略与策略组
  - `--strict` 模式将 warning 视为失败
- **WOPI 协议扩展**
  - GET_LOCK：查询当前文件锁值
  - RENAME_FILE：WOPI 重命名（自动保留扩展名、清理非法字符、截断超长名称、冲突自动分配）
  - PUT_USER_INFO：保存/读取 WOPI 用户偏好（存储到 `user_profiles.wopi_user_info`）
  - UnlockAndRelock：原子换锁操作
  - PutRelativeFile：创建/覆写相邻文件（Suggested 模式自动去重命名 + Relative 模式精确指定）
  - CheckFileInfo 新增 `SupportsGetLock`/`SupportsRename`/`UserCanRename`/`SupportsUserInfo`/`FileNameMaxLength` 字段
- **数据库唯一索引**
  - `idx_files_unique_live_name`：文件名在活跃状态下的唯一约束（区分个人/团队空间）
  - `idx_folders_unique_live_name`：文件夹名在活跃状态下的唯一约束
  - `idx_contact_verification_tokens_single_active`：同一用户/渠道/用途只允许一个未消费验证令牌
  - `user_profiles.wopi_user_info` 列（VARCHAR(1024)）
- **CLI human 输出格式**
  - 终端自动检测：终端显示 human 格式，管道输出 JSON
  - 彩色输出：支持 `CLICOLOR_FORCE` / `NO_COLOR` 环境变量
  - 敏感值掩码、多行值摘要、来源徽章（`[system]`/`[custom]`）
  - 进度条展示（database-migrate）
- **运维 CLI 文档** — 新增 `docs/deployment/ops-cli.md`，覆盖 doctor/config/database-migrate 完整使用指南；README 和全站文档交叉引用


### Changed

- **CLI 模块结构重构**
  - 从 `cli.rs` 单文件拆分为 `cli/config.rs`、`cli/doctor.rs`、`cli/database_migration.rs`、`cli/shared.rs` 模块目录
  - 提取公共工具到 `cli/shared.rs`：OutputFormat、CliTerminalPalette、Success/ErrorEnvelope
- **`/auth/check` 接口简化**
  - 移除 `CheckReq` 请求体（原含 `identifier` 字段），接口仅返回实例认证状态
  - `operation_id` 从 `check_identifier` 改为 `check_auth_state`
  - 前端 `authService.check()` 和 `LoginPage` 同步更新
- **后台任务管理**
  - 新增 `BackgroundTasks` 结构体收集所有 JoinHandle
  - panic 捕获从子任务 spawn 改为 `AssertUnwindSafe + catch_unwind`
  - 关闭顺序改为：先 abort 后台任务 → 再关闭数据库连接
- **config_repo upsert 优化**
  - `upsert_with_actor` 改为 INSERT ON CONFLICT DO NOTHING + TryInsertResult 检查
  - 消除 SELECT-then-INSERT 的竞态条件
- **文件复制重试逻辑**
  - 文件/文件夹复制从 check-then-create 改为 try-create-and-retry（最多 32 次）
  - 彻底消除复制操作中的 TOCTOU 竞态条件
- **WOPI 错误响应**
  - 不再将 403 映射为 401，改用标准 actix_web 错误响应
- **存储配额计算**
  - 文件覆写时配额增量改为新内容全量（而非差值）


### Fixed

- **文件/文件夹同名冲突** — 软删除后无法创建同名文件、回收站恢复冲突、批量操作后名称释放等问题，通过数据库唯一索引彻底解决
- **验证令牌重复发送** — 同一用户/渠道/用途重复请求验证邮件时不再发送新邮件，唯一索引保证只有一个活跃令牌
- **用户注册/邮箱变更唯一约束** — 区分用户名和邮箱冲突，返回更精确的错误信息
- **SQLite URL 缺少写模式** — 不带查询参数的 SQLite URL 自动补齐 `?mode=rwc`


### Breaking Changes

- **`/auth/check` 接口变更**：移除请求体，`operation_id` 从 `check_identifier` 改为 `check_auth_state`，依赖此接口的客户端需移除 `identifier` 参数
- **CLI 输出格式默认行为**：`config` 子命令在终端中默认输出 human 格式而非 JSON，依赖 JSON 输出的脚本需显式指定 `--output-format json`
- **WOPI CheckFileInfo 响应变更**：`UserCanNotWriteRelative` 从 `true` 改为 `false`，新增多个能力声明字段
- **存储配额计算变更**：文件覆写时配额增量改为新内容全量，接近配额上限的用户可能受影响
- **数据库 Schema**：4 个新迁移（唯一索引 + wopi_user_info 列），需运行数据库迁移。唯一索引迁移会自动清理已有的重复数据


---

**统计数据**：
- 71 files changed, 10,354 insertions(+), 1,030 deletions(-)
- 9 commits


## [v0.0.1-alpha.18] - 2026-04-13

> **⚠️ 升级必读**：本版本将配置文件和数据库文件迁移至 `data/` 目录。升级前需手动迁移：
> ```bash
> mkdir -p data
> mv config.toml data/
> mv asterdrive.db data/        # SQLite 用户
> ```
> 未迁移的旧实例将拒绝启动并提示操作步骤。

### Release Highlights

- **运维 CLI** — 新增 `aster-drive cli` 子命令系统，支持离线查看、修改、导入/导出运行时配置，脱离 Web 管理后台即可完成运维操作
- **配置文件迁移至 data/ 目录** — `config.toml` 和 SQLite 数据库文件统一迁移到 `data/` 目录，规范化数据布局。旧布局自动检测并提示迁移
- **预览应用配置 v2** — 预览应用配置从规则匹配模式重构为扩展名直接绑定模式，简化配置逻辑。新增 WOPI Discovery 自动导入功能，可一键从 Collabora/OnlyOffice 生成预览应用配置
- **服务层 DTO 重构** — 所有 API 响应从直接暴露数据库实体模型改为返回专用 DTO，增强 API 契约稳定性与安全性
- **多项安全与性能改进** — 批量操作权限校验统一化、回收站清理游标分批处理、团队成员数据库侧分页、Redis 日志凭据脱敏


### Added

- **运维 CLI**
  - 新增 `cli config` 子命令：`list`/`get`/`set`/`delete`/`validate`/`export`/`import`
  - 支持环境变量传参：`ASTER_CLI_DATABASE_URL`、`ASTER_CLI_CONFIG_KEY` 等
  - 输出格式：JSON / Pretty JSON，标准 envelope 结构
  - 无用户身份写入：配置写入支持 CLI 场景（`upsert_with_actor`）
- **WOPI Discovery 自动导入**
  - `execute_config_action` 新增 `build_wopi_discovery_preview_config` 动作
  - 解析 WOPI Discovery XML 自动生成 WOPI 预览应用配置
  - 智能去重：基于 discovery_url 识别已导入应用，保留用户手动禁用状态
  - 前端新增 Discovery URL 输入弹窗
- **管理控制台趋势图增强**
  - 概览页趋势图从单线扩展为 4 线（总事件、上传量、分享创建、新用户），自定义 tooltip 展示
- **全链路 debug 埋点**
  - 认证、文件/文件夹操作、搜索、上传等核心路径新增 `tracing::debug` 日志
- **API 文档**
  - 新增 WOPI API、批量打包下载、后台任务 API 文档
  - 配置文档重写（五层配置结构）、用户指南和部署文档更新


### Changed

- **预览应用配置 v2**
  - 配置版本升至 v2：移除 `rules` 字段，扩展名列表直接声明在 app 上
  - 合并 `builtin.formatted_json` 和 `builtin.formatted_xml` 为 `builtin.formatted`
  - 前端编辑器改为弹窗模式，新增"新增应用"选择弹窗（Embed/URL 模板/WOPI Discovery）
- **配置文件路径迁移**
  - `config.toml` 迁移至 `data/config.toml`，SQLite 默认路径改为 `data/asterdrive.db`
  - 旧布局自动检测，服务拒绝启动并提示迁移步骤
- **服务层 DTO 重构**
  - 新增 `workspace_models`（FileInfo/FolderInfo/FileVersion）及各服务 DTO
  - 新增 `workspace_scope_service` 集中管理作用域校验
  - 所有服务层公开函数返回类型从实体模型替换为 DTO
- **批量操作权限校验**
  - `load_normalized_selection_in_scope` 统一接管 delete/move/copy 权限校验
  - 新增 `find_by_ids_in_scope` 系列 repo 方法，防止跨作用域越权
- **回收站清理**
  - `purge_all` 改为游标分批处理（每批 100 条），降低大数据量场景内存压力
- **团队成员列表**
  - 从内存全量加载改为数据库侧过滤/排序/分页
- **上传路径解析**
  - 拆分为 `parse_relative_upload_path`（校验）+ `ensure_upload_parent_path`（创建），解耦校验与创建逻辑
- **遗留存储策略清理**
  - 删除 `user_storage_policies` 表和 `user_profiles.avatar_policy_id` 字段
  - 清理 `policy_repo` 中废弃的用户策略 CRUD 方法
- **后台任务类型精简**
  - 移除 `BackgroundTaskKind::ArchiveDownload`（已改为 stream ticket 直接流式下载）


### Fixed

- **分享密码状态误判** — 更新分享时不传 password 字段会错误清除已有密码，现在保持原密码状态
- **团队归档删除原子性** — 引入事务锁保证并发安全，清理失败时容忍目标缺失
- **Redis 日志凭据泄露** — 连接日志自动剥离 URL 中的用户名/密码


### Breaking Changes

- **配置文件路径**：`config.toml` 和 SQLite 数据库文件需手动迁移至 `data/` 目录，旧布局启动将报错并提示迁移步骤
- **预览应用配置 v2**：配置格式从 v1 升至 v2（移除 `rules`，扩展名直接声明在 app 上），自定义预览应用配置需重新设置
- **数据库 Schema**：删除 `user_storage_policies` 表和 `avatar_policy_id` 字段，需运行数据库迁移
- **ArchiveDownload 任务类型移除**：`BackgroundTaskKind::ArchiveDownload` 已删除，打包下载改为 stream ticket 直接流式下载


---

**统计数据**：
- 143 files changed, 7,850 insertions(+), 5,115 deletions(-)
- 7 commits


## [v0.0.1-alpha.17] - 2026-04-12

### Release Highlights

- **WOPI 协议支持** — 完整实现 WOPI (Web Application Open Platform Interface) 协议，可与 Collabora Online、OnlyOffice 等 WOPI 兼容办公套件集成，实现文档在线编辑。包含 CheckFileInfo、GetFile/PutFile、完整锁机制、Discovery 缓存、Access Token 管理
- **预览应用系统重构** — 将硬编码的文件预览逻辑重构为基于规则引擎的可配置"预览应用"系统。支持三种 Provider（Builtin/UrlTemplate/Wopi），管理后台提供可视化配置编辑器，内置 12 个默认预览应用
- **后台任务系统与打包下载** — 新增通用后台任务框架（状态机、自动重试、指数退避、过期清理），并新增基于 stream ticket 的多文件/文件夹 ZIP 流式下载
- **缩略图系统优化** — 引入缩略图版本控制（v2）、源文件大小限制、视口懒加载、并发 worker 优化，降低内存峰值并提升加载体验
- **运行与调度配置** — 新增 operations 配置分类，邮件发送间隔、任务调度间隔、维护清理周期等均可在管理后台热改。设置页新增时间/大小单位选择器


### Added

- **WOPI 协议**
  - 新增 `wopi_service`：CheckFileInfo、GetFile/PutFile、完整锁机制（lock/unlock/refresh）、Discovery XML 缓存
  - WOPI 端点路由：`/api/v1/wopi/files/{id}` 及 `/contents` 子路由
  - `wopi_sessions` 数据表：Access Token 存储（SHA-256 哈希）、过期清理
  - 运行时配置：`wopi_access_token_ttl_secs`、`wopi_lock_ttl_secs`、`wopi_discovery_cache_ttl_secs`
  - 前端 `WopiPreview` 组件：通过隐藏 form POST 提交 token 到 WOPI action_url，支持 iframe/new_tab 模式
  - CORS 中间件新增 WOPI 相关请求/响应头
  - 完整集成测试覆盖（1400+ 行）
- **预览应用系统**
  - 新增 `preview_app_service`：三种 Provider 类型、规则引擎按 extensions/mime_types/categories 匹配文件到预览应用
  - `PublicPreviewAppsConfig` 存储于 `system_config` 表，含 12 个内置应用（image, video, audio, pdf, markdown, table, formatted_json, formatted_xml, code, try_text, office_google, office_microsoft）
  - `UrlTemplatePreview` / `EmbeddedWebAppPreview` 通用预览组件
  - 管理后台 `PreviewAppsConfigEditor` 可视化编辑器（2700+ 行），支持应用增删改、规则编辑、校验
  - 14 个 SVG 预览应用图标
  - `/api/v1/public/preview-apps` 公开端点
- **后台任务框架**
  - 新增 `task_service`：任务调度（批量认领）、状态机（pending→processing→succeeded/failed/retry）、自动重试（指数退避）、过期清理
  - `background_tasks` 数据表：含 kind, status, progress, payload_json, attempt_count 等字段
  - 任务 API：`GET /api/v1/tasks`（分页列表）、`GET /api/v1/tasks/{id}`（详情）、`POST /api/v1/tasks/{id}/retry`（手动重试）
  - 团队空间任务 API（同结构）
- **打包下载**
  - `stream_ticket_service`：一次性下载凭证（5 分钟有效），支持 moka 缓存
  - `POST /api/v1/batch/archive-download` + `GET /api/v1/batch/archive-download/{token}` 端点
  - 团队空间打包下载路由
  - 文件右键菜单/批量操作栏新增"打包下载"选项
- **运行与调度配置**
  - `operations` 配置分类：`mail_outbox_dispatch_interval_secs`、`background_task_dispatch_interval_secs`、`maintenance_cleanup_interval_secs`、`blob_reconcile_interval_secs`、`team_member_list_max_limit`、`task_list_max_limit`、`avatar_max_upload_size_bytes`、`thumbnail_max_source_bytes`
  - 设置页新增时间单位选择器（秒/分钟/小时/天/周）和大小单位选择器（字节/KB/MB/GB/TB），自动检测最合适单位
  - 新增 `auth_register_activation_enabled` 配置项（注册后是否需要邮箱激活）
  - 设置分类细化：`user` 拆分为 `user.registration_and_login` + `user.avatar`，新增 `general.preview` 子分类


### Changed

- **缩略图系统**
  - 存储路径引入版本号：`_thumb/v2/{hash...}.webp`，旧路径缩略图自动清理
  - ETag 格式改为 `thumb-v2-{blob_hash}`，分享页缓存策略改为 `must-revalidate`
  - 最大并发 worker 数从 `min(cpu, 4)` 降为 `min(cpu, 2)`
  - worker 接收 `runtime_config` 参数以读取动态配置
  - 前端缩略图支持视口懒加载（`IntersectionObserver`）和加载状态指示
- **后台定时任务调度**
  - `spawn_periodic()` 间隔从固定 Duration 改为从运行时配置动态读取的闭包
  - 所有定时任务（upload/trash/lock/audit cleanup 等）统一使用 `maintenance_cleanup_interval` 配置
- **文件预览架构**
  - `OpenWithMode` 从受限枚举改为开放 string 类型，支持服务端定义任意打开方式
  - `formatted` 预览模式拆分为 `formatted_json` 和 `formatted_xml`
  - 删除 `OfficeOnlinePreview`、`OpenWithChooser`、`PreviewModeSwitch` 等旧组件
- **CORS 中间件**
  - 允许头列表从硬编码字符串改为 `ALLOWED_HEADERS` 常量数组动态拼接


### Fixed

- **管理设置页面** — 桌面端导航栏改为 sticky 定位，解决长页面滚动时导航不跟随的问题
- **品牌资源预览** — favicon 和深色 wordmark 预览框背景统一为白色，确保不同主题下效果一致


### Breaking Changes

- **数据库 Schema**：新增 `background_tasks` 和 `wopi_sessions` 表，需运行数据库迁移
- **缩略图路径**：存储路径从 `_thumb/{hash...}` 变为 `_thumb/v2/{hash...}`，升级后旧缩略图访问时自动清理重新生成
- **缩略图 ETag**：格式加入 `thumb-v2-` 前缀，客户端缓存的旧 ETag 将失效
- **预览应用配置**：`frontend_preview_apps_json` 格式已完全重构（新增 version, provider, config 等字段），自定义配置需重新设置
- **设置分类键**：`user` 分类拆分为子分类，`general` 新增 `general.preview`，可能影响依赖分类名的自动化脚本


---

**统计数据**：
- 191 files changed, 19,997 insertions(+), 2,048 deletions(-)
- 7 commits


## [v0.0.1-alpha.16] - 2026-04-09

### Release Highlights

- **邮件系统** — 引入 lettre/SMTP 邮件服务，新增 outbox 异步投递队列与 5 种可自定义 HTML 邮件模板（注册激活、邮箱变更、密码重置等），管理后台支持在线编辑模板
- **完整认证流程** — 新增邮箱验证激活、邮箱变更确认、密码重置三大流程，所有敏感操作均有邮件通知。新增注册开关配置，支持关闭公开注册
- **Office 在线预览** — 支持 Microsoft Office Online 和 Google Docs 两种 provider，可在线预览 Word/Excel/PowerPoint/ODF 文档。新增预览链接服务，生成限时限次的预览令牌
- **文件变更实时推送 (SSE)** — 后端通过 Server-Sent Events 广播文件/文件夹变更事件，前端自动刷新当前目录，用户可在设置中开关实时同步
- **站点品牌配置** — 支持自定义站点标题、描述、Favicon、亮/暗色 Logo (Wordmark)，登录前页面即可展示自定义品牌


### Added

- **邮件基础设施**
  - 新增 `mail_service.rs`：基于 lettre 的 SMTP 邮件发送，支持 TLS/STARTTLS
  - 新增 `mail_outbox` 数据表：异步邮件投递队列，支持失败重试
  - 后台任务定期处理邮件重试（`spawn_background_tasks` 新增邮件处理任务）
  - 新增 `MemoryMailSender` 用于测试环境
- **邮件模板系统**
  - 5 种内置 HTML 模板：注册激活、邮箱变更确认/通知、密码重置/通知
  - 模板变量替换：`{{username}}`、`{{verification_url}}`、`{{reset_url}}` 等
  - 管理后台新增邮件模板编辑页面，支持展开/折叠分组编辑
- **邮箱验证流程**
  - 注册后发送激活邮件，未激活账号登录返回 `PendingActivation` 错误码
  - 前端登录页新增待激活提示面板 + 重发激活邮件功能
  - 邮箱变更需确认：发送变更确认邮件到新邮箱，通知邮件到旧邮箱
- **密码重置**
  - `POST /auth/request_password_reset` + `POST /auth/confirm_password_reset`
  - 复用 `contact_verification_token` 基础设施，新增 `PasswordReset` 验证用途
  - 重置成功后自动轮换 `session_version`，所有现有会话强制失效
  - 发送重置链接邮件及重置成功通知邮件，记录审计日志
- **注册开关**
  - 新增 `auth_allow_user_registration` 运行时配置项（默认 `true`）
  - 关闭后 `/auth/register` 返回 403，`/auth/setup` 初始化流程不受影响
  - 前端登录页根据配置隐藏注册入口
- **Office 在线预览**
  - 新增 `OfficeOnlinePreview` 组件，支持 Microsoft Office Online / Google Docs
  - 超时检测、localhost/HTTP 链接错误提示及重试
  - 文件类型识别增强：doc/docx/xls/xlsx/ppt/pptx/odt/ods/odp 文件归入 document/spreadsheet/presentation 分类
- **预览链接服务** (`preview_link_service`)
  - 为个人/团队文件及分享文件生成带使用次数限制的预览令牌
  - `GET /pv/{token}/{filename}` 路由提供 inline 下载
  - 令牌有效期 5 分钟，最大使用次数 5 次
- **文件变更实时推送 (SSE)**
  - `storage_change_service`：通过 broadcast channel 广播文件/文件夹变更事件
  - `GET /auth/events/storage` SSE 端点，含心跳保活（30s）与消息积压降级
  - 前端 `useStorageChangeEvents` hook：订阅实时变更并自动刷新当前目录
  - 用户偏好 `storage_event_stream_enabled` 字段，可在设置中开关
- **站点品牌配置**
  - 新增 `branding_title`、`branding_description`、`branding_favicon_url` 配置项
  - 新增 `branding_wordmark_dark_url`、`branding_wordmark_light_url` Logo 配置
  - 前端启动时通过 `/api/v1/public/branding` 拉取品牌配置
  - 后端渲染 `index.html` 时注入品牌占位符，登录前即展示自定义品牌
- **前端增强**
  - `usePageTitle` hook：所有页面动态标题，格式 `页面名 · 应用名`
  - `AdminSiteUrlMismatchPrompt` 独立组件：站点 URL 不匹配检测与更新
  - CORS 新增 `cors_enabled` 独立开关配置


### Changed

- **认证流程重构**
  - `/auth/check` 不再接受 `identifier` 参数，改为返回公开认证状态（注册开关、初始化状态等）
  - 前端登录页改为页面初始化时一次性拉取认证状态，移除输入框防抖检查逻辑
  - 统一响应时间下限防止用户枚举攻击
- **头像存储迁移**
  - 从对象存储策略迁移到本地文件系统，新增 `avatar_dir` 配置项
  - 删除时递归清理空目录
  - 兼容旧 `avatar_policy_id` 记录，平滑迁移
- **管理后台设置页**
  - 默认路由从 `/admin/settings/auth` 改为 `/admin/settings/general`
  - 新增邮件模板编辑分区
- **CI 改进**
  - 替换 `actions/cache` 为 `Swatinem/rust-cache@v2`，简化配置


### Fixed

- **代码编辑器**
  - 默认关闭自动换行 (`wordWrap: off`)


### Breaking Changes

- **认证 API**: `/auth/check` 移除 `identifier` 参数，改为返回全局认证状态。前端需适配新的登录初始化逻辑
- **注册激活**: 邮件验证成为注册必需步骤（需配置 SMTP），未激活账号无法登录
- **密码重置**: 重置成功后自动轮换 `session_version`，所有现有会话强制失效
- **头像存储**: 新上传头像存到本地文件系统 (`avatar_dir`)，不再使用对象存储策略
- **管理后台**: 设置页默认路由从 `/admin/settings/auth` 改为 `/admin/settings/general`
- **CORS**: 新增 `cors_enabled` 独立开关，需显式启用


---

**统计数据**：
- 243 files changed, 19,542 insertions(+), 1,920 deletions(-)
- 15 commits


## [v0.0.1-alpha.15] - 2026-04-07

### Release Highlights

- **文件直链分享** — 新增 Direct Link 分享模式，生成不经过分享页面的直接下载链接。支持强制下载参数，独立速率限制。前端分享弹窗可一键切换分享页/直链两种模式
- **运行时认证策略** — 将 Cookie 安全策略、Token TTL 等认证配置从静态 config.toml 迁移至数据库运行时配置，管理员可在后台实时调整，无需重启服务
- **管理设置页面重构** — 系统配置按分类标签页导航（认证/网络/存储/WebDAV/审计/通用/自定义），支持批量保存、敏感值掩码、默认值展示与一键恢复、i18n 标签
- **头像裁剪** — 新增圆形裁剪器，支持缩放和位置调整，输出 1024×1024 WebP 格式
- **移动端响应式优化** — 对话框与设置页面全面适配移动端布局，标签页增加切换动画方向检测


### Added

- **文件直链服务**
  - 新增 `direct_link_service.rs`：生成带签名的直链下载 token
  - API 端点：`GET /api/v1/files/{id}/direct-link`、`GET /api/v1/team-space/files/{id}/direct-link`
  - 公开下载端点：`GET /d/{token}/{filename}`，支持 `?download=1` 强制下载
  - 独立速率限制配置
- **运行时认证配置**
  - 新增 `auth_runtime.rs`：从数据库读取 `auth_cookie_secure`、`auth_access_token_ttl_secs`、`auth_refresh_token_ttl_secs`
  - 静态配置新增 `bootstrap_insecure_cookies` 引导选项（仅首次初始化生效）
  - Cookie 路径隔离：Access Token → `/`，Refresh Token → `/api/v1/auth/refresh`
- **头像裁剪**
  - 新增 `AvatarCropDialog` 组件 + `avatarCrop.ts` 工具
  - 基于 `react-image-crop`，圆形裁剪框 + 实时预览
- **前端分享增强**
  - 分享弹窗新增双模式切换：分享页 (Share page) / 直链 (Direct link)
  - 直链模式不支持密码和过期时间，支持生成强制下载链接
  - 文件右键菜单支持直接选择分享模式
- **系统配置 i18n**
  - 配置定义新增 `label_i18n_key` / `description_i18n_key` 字段
  - 配置项支持分类：auth / network / storage / webdav / audit / general
  - 敏感值标记 (`is_sensitive`) 和需重启标记 (`requires_restart`)
  - 中英文翻译覆盖所有系统配置项
- **UI 组件增强**
  - Select 新增 `width` 变体（compact / page-size / fit / full）
  - Tabs `line` 变体支持全宽样式 + 动画方向检测
  - 审计日志页面支持 URL 参数同步、每页条目数选择、筛选激活指示器


### Changed

- **认证服务重构**
  - `issue_tokens_for_user` 改为从运行时配置获取 Token TTL 和 Cookie 策略
  - 分享验证 Cookie 增加安全标志和路径隔离（`/api/v1/s/{token}`）
- **管理设置页面**
  - 重构为分类标签页导航（桌面端侧边栏，移动端下拉）
  - 新增批量保存机制（草稿值管理）
  - 敏感值显示掩码（`********`），支持默认值展示与一键恢复
- **对话框响应式布局**
  - `AdminTeamDetailDialog` / `TeamManageDialog` / `UserDetailDialog` 全面适配移动端
  - 两栏布局重构为 flex + overflow-hidden，移动端自适应单列
  - 新增滚动位置记忆和标签切换动画方向检测
- **Select 组件**
  - 移除硬编码高度，改用变体系统
  - 管理页面统一使用 `width` prop


### Fixed

- **Cookie 安全策略**
  - 修复纯 HTTP 环境首次部署无法登录的问题（`bootstrap_insecure_cookies` 引导配置）
- **审计日志页面**
  - 修复筛选和分页状态无法保存或通过 URL 分享的问题
- **移动端布局**
  - 修复管理对话框在移动端滚动行为混乱的问题
  - 修复用户详情对话框底部按钮被遮挡的问题


### Breaking Changes

- **配置文件**: `[auth]` 段移除 `access_token_ttl_secs`、`refresh_token_ttl_secs`、`cookie_secure`，改为运行时配置。新增 `bootstrap_insecure_cookies`（仅首次初始化生效）
- **Cookie 行为**: Refresh Token Cookie 路径从 `/` 限制为 `/api/v1/auth/refresh`，分享验证 Cookie 路径限制为 `/api/v1/s/{token}`
- **前端路由**: 管理设置页面新增子路由 `/admin/settings/:section`


---

**统计数据**：
- 99 files changed, 6,749 insertions(+), 1,629 deletions(-)
- 7 commits


## [v0.0.1-alpha.14] - 2026-04-05

### Release Highlights

- **团队工作空间** — 新增完整团队生命周期管理，支持创建团队、成员邀请、角色分配（Owner/Member）、多空间文件隔离。分享链接新增团队范围支持，团队协作更顺畅
- **上传性能优化** — 移除 proxy_tempfile 中间策略，新增 relay_stream 无暂存直传快速路径；本地存储上传跳过全局临时目录，小文件上传延迟降低
- **自定义 CORS 中间件** — 替换 actix-cors 为运行时可配置的自定义实现，支持动态调整跨域策略，管理后台可实时生效
- **Admin 路由重构** — 将臃肿的 admin.rs 拆分为 8 个独立子模块（users/policies/teams/shares/config/locks/audit_logs/overview），代码可维护性提升
- **缩略图错误精细化** — 区分 202（生成中）、400（不支持类型）、500（生成失败）状态码，前端可做出更精确的用户反馈


### Added

- **团队功能**
  - 新增 `teams` / `team_members` / `team_spaces` 数据库表，支持软删除
  - 完整 Team API：创建、更新、删除、成员管理、空间列表
  - 团队空间文件管理：独立于用户空间的团队文件存储
  - 分享支持团队范围（`team_id` 字段），团队成员可访问团队分享
  - 前端 `TeamManagePage` / `TeamsSettingsView` / `TeamManageDialog` 完整界面
  - 支持团队维度批量操作、搜索、回收站、分享管理
  - 审计日志覆盖团队相关操作
- **团队文件存储服务** (`workspace_storage_service`)
  - 独立的空间配额计算与权限校验
  - 支持团队内文件夹/文件的完整生命周期管理
  - 团队文件版本历史支持
- **上传优化**
  - `relay_stream` 无暂存直传模式（替代原 relay 模式）
  - 本地存储快速路径：小文件直接写入目标路径，跳过全局临时目录
- **自定义 CORS 中间件**
  - `CorsConfig` 运行时配置支持
  - 基于 `http` crate 的手动 CORS 头处理
  - 管理后台配置变更实时生效
- **缩略图 API 细化**
  - `ThumbnailStatus` 枚举：Generating/Unsupported/Error
  - HTTP 202 + `Retry-After` 头表示生成中
  - HTTP 400 明确标识不支持的 MIME 类型


### Changed

- **Admin 路由重构**
  - 拆分 `admin.rs` 为 8 个子模块：users/policies/teams/shares/config/locks/audit_logs/overview
  - 共享工具函数抽离至 `admin/common.rs`
- **上传策略**
  - 移除 `S3UploadStrategy::ProxyTempfile` 变体
  - `relay_stream` 成为新的 relay 模式实现
- **文件仓库**
  - `find_or_create_blob` 重试策略改为指数退避（减少高并发冲突）
- **分享服务**
  - 重构分享权限校验，支持团队范围校验
  - 分享列表查询优化，支持团队过滤
- **缩略图错误处理**
  - 生成失败返回 500（原为 404）
  - 不支持的类型返回 400（带有明确错误信息）


### Fixed

- **安全性**
  - 优化 API 错误信息，避免泄露敏感内部细节（如数据库结构、内部路径）
- **S3 驱动**
  - 修复负数 content_length 处理边界情况
- **应用关闭**
  - 重构优雅关闭逻辑，确保缩略图 worker 和后台任务正确收尾


### Breaking Changes

- **API**: `POST /api/v1/uploads` 移除 `proxy_tempfile` 策略选项（已自动迁移至 `relay_stream`）
- **API**: 缩略图端点状态码语义变更：
  - 202: 缩略图正在生成中（原行为返回 404）
  - 400: 不支持的文件类型（新增）
  - 500: 生成失败（原行为返回 404）
- **内部**: `S3UploadStrategy` 枚举移除 `ProxyTempfile` 变体


---

**统计数据**：
- 180 files changed, 33,028 insertions(+), 6,842 deletions(-)
- 12 commits


## [v0.0.1-alpha.13] - 2026-04-02

### Release Highlights

- **存储策略组** — 新增策略组子系统，替代原来的用户-策略一对一分配。策略组支持多策略规则（按优先级+文件大小区间匹配），用户绑定策略组后上传自动路由到最合适的存储策略
- **Access Token 自动续期** — 前端新增基于 `expires_at` 的自动续期机制，提前 2 分钟触发 refresh，登录/改密码响应返回 `expires_in`，会话生命周期全程可追踪
- **代码预览轻量化** — 移除 Monaco Editor 依赖（~350 行），替换为基于 Prism 的轻量代码编辑器，按需加载 40+ 语言，构建产物体积大幅缩减
- **OpenAPI 可选编译** — utoipa 全系列依赖改为 optional feature，release 构建默认不编译 OpenAPI 支持，二进制体积更小
- **管理后台策略组页面** — 完整的策略组 CRUD 页面，含规则编辑、用户迁移确认、系统默认策略组自动种子化
- **前端基础设施增强** — 新增分页/查询参数工具函数、分享对话框共享逻辑提取、useApiList 竞态保护


### Added

- **存储策略组**
  - `storage_policy_groups` + `storage_policy_group_items` 数据库表（migration）
  - `users` 表新增 `policy_group_id` 列（FK + SET NULL 级联）
  - 6 个 Admin API 路由：CRUD + 用户迁移（`/admin/policy-groups/*`）
  - `PolicySnapshot` 扩展：缓存策略组/条目/用户绑定，新增 `resolve_policy_in_group`、`resolve_user_policy_for_size` 等方法
  - 启动时 `ensure_policy_groups_seeded`：系统默认策略自动包装为默认策略组，旧 `user_storage_policies` 记录自动迁移
  - 上传时按文件大小在策略组中匹配最合适的策略
  - 审计日志新增 4 种 action：`AdminCreatePolicyGroup`、`AdminUpdatePolicyGroup`、`AdminDeletePolicyGroup`、`AdminMigratePolicyGroupUsers`
  - 前端 `AdminPolicyGroupsPage` 完整策略组管理页面（1439 行）
  - `UserDetailDialog` 重构：存储策略分配改为单策略组选择
  - 中英文 i18n 各增加约 40 条策略组翻译
- **Access Token 自动续期**
  - 后端 auth 响应体返回 `expires_in` 和 `access_token_expires_at`
  - `authStore` 新增 `expiresAt` 状态、sessionStorage 持久化、`refreshToken()` 去重复用
  - `startAutoRefresh()` / `stopAutoRefresh()`：基于 setTimeout 提前 2 分钟自动续期
  - HTTP 拦截器 refresh 队列从数组改为 `refreshPromise` 复用
- **Prism 代码编辑器**
  - 新增 `CodePreviewEditor` 替代 MonacoCodeEditor，基于 prism-react-renderer
  - 按需动态加载 40+ 种语言的 Prism 组件
  - 新增 `prismClassNames` 模块解决 Scoped CSS className 冲突
  - 新增 `toml` 和 `groovy` 语言映射
- **前端基础设施**
  - `lib/pagination.ts`：通用 offset 分页参数解析与构建
  - `lib/queryParams.ts`：通用 query string 构建工具
  - `components/files/shareDialogShared.ts`：分享对话框共享逻辑（过期计算、下载次数归一化）
  - `api-docs-macros` workspace crate：自定义 proc-macro，debug+openapi feature 下展开为 `#[utoipa::path]`
- **测试覆盖**
  - 新增 `AdminPolicyGroupsPage.test.tsx`（873 行）
  - 新增 `policyGroupDialogShared.test.ts`、`storagePolicyDialogShared.test.ts`、`shareDialogShared.test.ts`
  - 新增 `prismClassNames.test.ts`、`file-capabilities.test.ts`
  - 新增 `useApiList.test.tsx`、`pagination.test.ts`、`queryParams.test.ts`
  - 新增 `authStore.edge.test.ts`


### Changed

- **OpenAPI 可选编译**
  - `utoipa` / `utoipa-swagger-ui` 改为 `optional = true`，新增 `openapi` feature
  - 全项目 `#[derive(ToSchema)]` / `#[derive(IntoParams)]` 改为 `#[cfg_attr]` 条件编译
  - `#[utoipa::path]` 替换为 `#[api_docs_macros::path]`
  - `openapi` 模块整体条件编译
- **管理后台页面重构**
  - `AdminUsersPage` 大幅重构，使用 `useApiList` hook + URL search params 管理
  - `AdminPoliciesPage` 使用新分页工具函数
  - `AdminAuditPage` 从手动 `useCallback + useEffect` 改为 `useApiList` hook
  - `adminService.ts` 全面使用 `withQuery()` 构建 query string，参数改用生成的请求类型
- **上传策略解析改为基于文件大小路由**
  - `upload_service` 调用新的 `resolve_policy_for_size` 替代原 `resolve_policy`
- **用户创建流程简化**
  - `create_user_with_role` 不再创建 `user_storage_policies` 行，改为设置 `policy_group_id`
- **`useApiList` hook 增强**
  - 新增 `requestIdRef` 竞态保护，快速切换 filter/offset 时丢弃过期响应
  - 新增 `setTotal` 返回值
- **移除 relay 上传模式**
  - 删除 `relay_field_to_s3`、`create_relay_cleanup_handle` 等函数（约 170 行）


### Fixed

- 修复 `StoragePolicyDialog` 策略摘要卡片在大屏下粘性定位失效问题（添加 `self-start`）


### Breaking Changes

- **API**: 移除 4 个旧的 user-storage-policy 路由（`/admin/users/{user_id}/policies/*`），替代方案为 `/admin/policy-groups/*` + `PATCH /admin/users/{id}` 的 `policy_group_id`
- **API**: `POST /auth/login`、`POST /auth/refresh`、`PUT /auth/password` 响应体从 `{ data: null }` 变为 `{ data: { expires_in } }`
- **API**: `GET /auth/me` 响应新增 `access_token_expires_at` 和 `policy_group_id` 字段
- **API**: 所有用户信息响应体新增 `policy_group_id` 字段
- **行为**: `user_storage_policies` 标记为 deprecated，新代码应使用策略组体系
- **前端**: 移除 `monaco-editor` 依赖，替换为 `prismjs` + `prism-react-renderer`


---

**统计数据**：
- 137 files changed, 10,275 insertions(+), 3,305 deletions(-)
- 4 commits


## [v0.0.1-alpha.12] - 2026-03-31

### Release Highlights

- **会话吊销机制** — 用户表新增 `session_version` 字段，JWT 嵌入版本号，管理员可一键吊销用户全部会话，改密码自动失效旧令牌
- **内存运行时配置与策略快照** — 系统配置和存储策略缓存至 `RwLock<HashMap>`，热路径零 DB 查询，写入时即时同步
- **批量 SQL 操作** — 删除/移动/复制重构为批量 SQL，单事务校验+执行，逐项错误上报，N 项操作 DB 往返从 ~6N 降至 ~10
- **管理员权限中间件** — 提取 `RequireAdmin` 独立中间件，admin 路由嵌套 `JwtAuth → RequireAdmin`，移除 handler 内联角色检查
- **本地存储可选内容去重** — 新增 `content_dedup` 策略选项，关闭时跳过 SHA256 计算，使用独立 blob 短令牌键
- **数据库索引优化** — 新增目录列表与回收站分页复合索引，消除全表扫描


### Added

- **会话吊销**
  - `users` 表新增 `session_version` 列（migration）
  - `AuthSnapshot` 结构体携带 `status`、`role`、`session_version`
  - 新增 `POST /api/v1/admin/users/{id}/sessions/revoke` — 管理员吊销用户全部会话
  - 改密码/管理员重置密码自动递增 `session_version`，当前会话返回新 token 保持在线
  - JWT Claims 嵌入 `session_version`，认证中间件校验一致性
  - WebDAV Bearer 认证升级为 `authenticate_access_token`，拒绝 refresh token
  - 新增审计动作：`AdminRevokeUserSessions`、`UserLogout`
  - 前端用户详情对话框新增"吊销全部会话"按钮
- **内存运行时配置**
  - `RuntimeConfig` 结构体：`reload`、`apply`、`remove` + 类型化 getter（`get_bool`、`get_i64`、`get_u64` 等）
  - `PolicySnapshot` 结构体：`reload`、`get_policy`、`resolve_default_policy_id`、`set_user_default_policy`
  - 启动时预加载全部配置和策略到内存
  - 所有服务（audit、auth、config、file、thumbnail、upload、trash、version、webdav）改为从快照读取
- **本地存储内容去重选项**
  - `StoragePolicyOptions` 新增 `content_dedup` 字段
  - 关闭时：跳过 SHA256，使用 `new_short_token()` 生成独立 blob 键
  - 开启时：写入临时文件后计算 SHA256，复用相同内容 blob
  - `local_content_dedup_enabled()` / `create_nondedup_blob()` 公共函数
- **管理后台关于页面**
  - 新增 `AdminAboutPage`：展示版本号、发布渠道（alpha/beta/rc/stable）、许可证（MIT）、外部链接
  - `AsterDriveWordmark` 主题感知 SVG 组件（dark/light 自动切换）
  - `index.html` 注入 `asterdrive-version` meta 标签，构建时写入版本号
  - 中英文 i18n 完整支持
- **数据库索引**
  - `idx_folders_user_deleted_parent_name` / `idx_files_user_deleted_folder_name` — 目录列表查询
  - `idx_folders_user_deleted_at_id` / `idx_files_user_deleted_at_id` — 回收站分页查询
- **测试覆盖**
  - `test_batch.rs` — 批量操作测试（472 行）
  - `test_db_indexes.rs` — 索引有效性验证（`EXPLAIN QUERY PLAN`）
  - `test_webdav_path_resolver.rs` — WebDAV 路径解析测试（518 行）
  - `test_services.rs` — 树可见性、空叶子、回收站路径等（332 行）


### Changed

- **上传完成逻辑重构**
  - 提取 `create_new_file_from_blob`、`finalize_upload_session_blob`、`finalize_upload_session_file` 公共原语
  - 提取 `complete_s3_multipart_upload_session` 统一 multipart 完成逻辑
  - 提取 `ensure_uploaded_s3_object_size`、`transition_upload_session_to_assembling` 辅助函数
  - 删除旧的 `finalize_upload_session` 和 `clear_relay_cleanup_handle` 实现
- **批量操作重构为批量 SQL**
  - 新增 `find_by_folders`、`find_all_in_folders`、`find_children_in_parents`、`find_all_children_in_parents` 批量查询方法
  - `batch_delete`：单事务校验+递归子树收集+批量软删除
  - `batch_move`：批量冲突/循环检测+批量更新，逐项错误上报
  - `batch_copy`：预分配唯一文件名，支持重复 ID 重命名
- **文件夹树遍历改为迭代式**
  - BFS 迭代替换递归异步逐条查询
  - `build_trash_path_cache` 批量预加载回收站父目录路径
  - WebDAV 路径解析改用递归 CTE 查询
- **管理员路由中间件化**
  - admin 路由改为嵌套 scope：`JwtAuth` → `RequireAdmin`
  - 移除 handler 中 `claims: web::ReqData<Claims>` 参数和 `require_admin()` 辅助函数
- **搜索多数据库兼容**
  - `name_search_condition` 根据数据库后端选择查询策略
  - PostgreSQL 使用 `ilike`，MySQL 使用 `MATCH AGAINST BOOLEAN MODE`
  - 新增 `escape_like_query` 防止通配符注入
- **管理后台 UI 重构**
  - 存储策略对话框拆分为概览/连接/存储详情/上传规则四个分区，编辑模式右侧新增策略摘要卡片
  - 策略表格行改为整行可点击，移除独立编辑按钮
  - 用户表格行改为整行可点击
  - 创建向导新增步骤过渡动画
  - 驱动类型徽章颜色区分（S3=蓝、本地=绿）
  - 内置系统策略禁止删除，带 tooltip 提示
- **认证服务调整**
  - `refresh_token` 改为 async 函数
  - `logout` 从 Authorization header 提取 token 记录审计日志
  - 改密码返回新 access/refresh token（保持会话连续性）


### Fixed

- 修复 MySQL migration 中 `allowed_types` 和 `options` 列不兼容 `DEFAULT` 值语法的问题
- 修复 raw SQL `Expr::cust_with_values` 替换为类型安全的 SeaORM 表达式（ref_count、storage_used、view_count）
- 修复最大文件大小为 0 时显示 "0 bytes" 而非"无限制"的问题
- 修复密码输入框浏览器自动填充问题（添加 `autoComplete="new-password"`）
- 修复访问密钥输入框浏览器自动填充问题（添加 `autoComplete="off"`）


### Breaking Changes

- **API**: `PUT /api/v1/auth/password` 现在返回新的 access/refresh token（Cookie），保持当前会话连续性
- **JWT**: 新 token 包含 `session_version` 字段；旧 token（无此字段）通过 `#[serde(default)]` 兼容
- **行为**: S3 上传统一使用 `files/{upload_id}` 路径格式
- **行为**: 本地存储默认 `content_dedup: false`，每次上传创建独立 blob（与之前隐式去重行为不同）
- **内部**: 所有服务必须从快照读取配置/策略，禁止直接调用 `policy_repo`/`config_repo`


---

**统计数据**：
- 113 files changed, 7,785 insertions(+), 1,815 deletions(-)
- 13 commits


## [v0.0.1-alpha.11] - 2026-03-30

### Release Highlights

- **管理后台总览面板** — 新增系统概览仪表板，展示用户统计、文件存储、每日活动趋势图表及最近审计事件
- **流式中继上传策略** — 新增 S3 流式直传中继模式，无需本地临时文件即可直接转发到 S3 Multipart
- **密码管理增强** — 支持用户自助修改密码，管理员可直接重置用户密码
- **分享管理升级** — 支持编辑已有分享设置（密码/过期时间/下载次数），新增批量删除分享功能
- **存储策略向导重构** — 分步创建向导优化体验，新增 S3/R2 端点自动归一化与验证
- **搜索 API 正式启用** — 完整文件/文件夹搜索能力，支持多维度过滤与分页
- **API 响应类型安全化** — 全面替换内联 JSON，使用强类型响应结构  


### Added

- **管理后台总览面板**
  - 新增 `GET /api/v1/admin/overview` 端点，支持 `days`/`timezone`/`event_limit` 参数
  - 用户统计：总数、活跃、禁用数量
  - 文件统计：总文件数、存储字节数、blob 数量
  - 每日活动报表：登录、上传、分享、删除趋势
  - 前端 `AdminOverviewPage` 集成 Recharts 图表展示
- **流式中继上传策略**
  - 新增 `S3UploadStrategy` 枚举：`ProxyTempfile` / `RelayStream` / `Presigned`
  - 新增 `upload_session_parts` 表持久化记录 part 与 ETag
  - `RelayStream` 模式直接流式转发至 S3，无需本地缓冲
  - 上传进度查询支持 relay multipart 模式
- **密码管理**
  - 新增 `PUT /api/v1/auth/password` — 用户自助密码修改（需验证当前密码）
  - 新增 `PUT /api/v1/admin/users/{id}/password` — 管理员重置密码
  - 前端 `SecuritySettingsView` 安全设置页
  - 审计动作：`UserChangePassword`、`AdminResetUserPassword`
- **分享管理增强**
  - 新增 `PATCH /api/v1/shares/{id}` — 编辑分享设置
  - 新增 `POST /api/v1/shares/batch-delete` — 批量删除分享（最多 1000 个）
  - 分享密码语义：`null` = 保留，`""` = 移除，`"value"` = 替换
  - 前端 `EditShareDialog` 编辑对话框
- **S3/R2 端点归一化**
  - 自动从 R2 端点路径提取 bucket 名称
  - 拒绝不安全的 `.r2.dev` 公网 URL
  - 校验端点与 bucket 字段一致性
  - 强制要求 `http://` 或 `https://` 协议头
- **搜索 API**
  - `GET /api/v1/search` 正式启用，支持文件名模糊搜索
  - 过滤条件：类型、MIME、大小、日期、目录范围
  - 分页返回 `FileSearchItem` / `FolderSearchItem`
- **分享页面增强**
  - 分享页面显示所有者头像和展示名称
  - 单文件分享新增缩略图展示
  - 文件图标与颜色优化
- **数据库维护索引**
  - `upload_sessions_status_expires_at` — 清理查询优化
  - `files_blob_id` / `file_versions_blob_id` — 引用计数优化
  - `file_blobs_storage_path` — 孤儿 blob 检测
- **后台维护服务**
  - `maintenance_service` 定时任务：过期上传清理（每小时）、blob 对账（每 6 小时）
  - 原子 `claim_blob_cleanup` 机制防止并发竞争
- **数据库查询指标**
  - `db_queries_total` 计数器（按后端/类型/状态）
  - `db_query_duration_seconds` 延迟直方图  


### Changed

- **存储策略对话框重构**
  - 分步创建向导：选择类型 → 配置连接 → 确认规则
  - 编辑模式保留单页布局
  - 内置系统策略禁止删除
  - S3 参数变更检测与强制保存确认
- **API 响应强类型化**
  - 替换内联 `serde_json::json!()` 为结构化响应类型
  - 审计详情结构化：`AdminCreateUserDetails`、`BatchDeleteDetails` 等
  - 前端类型按模块分组重组织
- **PATCH 语义修复**
  - 引入 `NullablePatch<T>` 三态类型：`Absent` / `Null` / `Value`
  - `PATCH /files/{id}` 支持 `folder_id: null` 移动到根目录
  - `PATCH /folders/{id}` 支持 `parent_id: null` 移动到根目录
- **分享过期状态码**
  - `ShareExpired` 错误 HTTP 状态码从 410 改为 404
  - 错误响应新增 `Cache-Control: no-store` 防止 CDN 缓存
- **数字类型转换工具化**
  - 新增 `utils::numbers` 模块：`bytes_to_usize`、`i32_to_usize`、`calc_total_chunks`
  - 消除跨层裸 `as` 强转，统一 checked conversion  


### Fixed

- 修复 relay multipart 进度查询未读取数据库 parts 表的问题
- 修复 blob 清理并发竞争条件
- 修复分享下载链接缓存控制头缺失  


### Breaking Changes

- **API**: `ShareExpired` 错误 HTTP 状态码从 410 改为 404
- **API**: `presigned_upload` 布尔配置已迁移为 `s3_upload_strategy` 枚举（自动兼容）
- **API**: `PATCH` 端点现在正确处理 `null` 语义（显式清空 vs 忽略字段）
- **Frontend**: 存储策略配置项结构变更，自定义前端需适配新策略向导  


---

**统计数据**：

- 179 files changed, 13,838 insertions(+), 1,756 deletions(-)
- 14 commits


## [v0.0.1-alpha.10] - 2026-03-29

### Release Highlights

- 新增**用户个人资料系统**：支持自定义展示名称、头像上传、Gravatar 及来源切换，并支持自定义 Gravatar 镜像地址
- 文件列表引入**虚拟滚动**，网格视图和表格视图均使用 `@tanstack/react-virtual`，大数据量下渲染性能显著提升
- 新增**视频预览增强**：集成 Artplayer 播放器，支持动态宽高比计算与自定义视频浏览器
- 代码编辑器从 `@monaco-editor/react` 迁移至原生 `monaco-editor`，按需懒加载语言支持，构建产物体积大幅优化
- 设置页拆分为**个人资料**与**界面偏好**两个独立路由分区，导航更清晰
- 错误页面重构：区分生产/开发环境，生产环境隐藏调试信息
- 图标库从 `@devicon/react` 迁移至 `react-devicons`，统一使用 original 变体
- 新增路由过渡动画（View Transitions API），页面切换体验更流畅
- 禁止删除内置系统存储策略，新增 S3 参数变更检测与强制保存确认

### Added

- **用户个人资料系统**
  - 新增 `user_profiles` 数据库表及两次 migration
  - `profile_service` 完整实现：展示名称编辑（最大 64 字符）、头像上传（自动裁剪为正方形 + WebP 编码，512px/1024px 两档）、Gravatar 及来源切换
  - 新增 API 端点：`PATCH /auth/profile`、`POST /auth/profile/avatar/upload`、`PUT /auth/profile/avatar/source`、`GET /auth/profile/avatar/{size}`
  - 前端 `UserAvatarImage` 组件，支持 sm/md/lg/xl 四种尺寸
  - 新增 `ProfileSettingsView` 个人资料设置页：展示名称编辑、头像管理、只读用户名/邮箱展示
  - 新增 `gravatar_base_url` 运行时配置，支持自定义 Gravatar 镜像（如 Cravatar）
- **文件列表虚拟滚动**
  - `FileGrid` 和 `FileTable` 引入 `@tanstack/react-virtual` 虚拟滚动
  - 网格视图响应式列数（2-6 列），overscan 优化滚动流畅度
- **视频预览增强**
  - 新增 `VideoPreview` 组件，基于 Artplayer 播放器，支持动态宽高比计算
  - 新增 `CustomVideoBrowserPreview`，支持外部视频源的自定义浏览器
  - 视频浏览器配置模块 `video-browser-config.ts`
- **界面设置页**
  - 新增 `InterfaceSettingsView`：主题模式、色板、语言、视图模式统一管理
- **路由过渡动画**
  - 导航链接集成 View Transitions API，页面切换更流畅
- **运行时配置模块**
  - 新增 `frontend-panel/src/config/runtime.ts`，统一管理环境变量与开发模式标识
- **策略保护与变更检测**
  - 内置系统存储策略（ID=1）禁止删除
  - Admin 策略编辑新增 S3 参数变更检测与强制保存确认对话框

### Changed

- **Monaco 编辑器迁移**
  - 从 `@monaco-editor/react` 迁移至原生 `monaco-editor`
  - 新增 `monaco-environment.ts` 按需懒加载语言支持
  - `MonacoCodeEditor` 替代旧的编辑器组件
- **设置页路由重构**
  - 设置页拆分为 `/settings/profile` 和 `/settings/interface` 两个路由分区
  - 原 `ThemeSwitcher` / `LanguageSwitcher` 独立组件移入设置页内
- **错误页面重构**
  - 全面重写 `ErrorPage`，卡片式布局 + 状态码徽章 + 恢复建议
  - 生产环境隐藏堆栈跟踪等调试信息
- **动画性能优化**
  - 文件卡片/表格过渡动画从 300ms 缩短至 150ms，移除 scale 变换
  - Tooltip 动画时长调整为 100ms
- **图标库迁移**
  - 从 `@devicon/react` 迁移至 `react-devicons`
  - 语言图标统一使用 original 变体
- **Vite 构建拆分优化**
  - `manualChunks` 策略增强：vendor-react / vendor-router / vendor-i18n / vendor-react-icons / vendor-devicons 等
  - Base UI 拆分为 vendor-ui-forms / vendor-ui-overlays / vendor-ui-controls
  - 预览专属 chunks：preview-data / preview-xml
  - PWA workbox 排除未使用的 Monaco worker 文件
- **分享页面体验优化**
  - 新增所有者信息展示（名称/邮箱）与拖拽预览支持
  - 文件分享卡片新增预览按钮
- **文件预览统一加载状态**
  - 新增 `PreviewLoadingState` 组件，统一各预览器的加载态展示
  - 文件预览对话框优化高度自适应与视频尺寸计算
- **HeaderControls 增强**
  - 顶栏控件集成用户头像与展示名称

### Fixed

- 修复存储策略零值字段处理及用户列表头像显示问题
- 修复策略连接测试逻辑
- 修复网络错误后无法重新发起身份校验请求的问题
- 修复 Vue 图标显示及配额单元格样式问题

### Breaking Changes

- **API**：`GET /api/v1/auth/me` 响应体新增 `profile` 字段，含 `display_name`、`avatar`（source / url_512 / url_1024 / version）
- **API**：Admin 用户相关端点响应体新增用户资料信息
- **Frontend**：设置页路由从 `/settings` 拆分为 `/settings/profile` 和 `/settings/interface`
- **Frontend**：`ThemeSwitcher` / `LanguageSwitcher` 独立组件已移除，功能整合至 `InterfaceSettingsView`

---

**统计数据**：
- 147 files changed, 7,340 insertions(+), 1,484 deletions(-)
- 21 commits

## [v0.0.1-alpha.9] - 2026-03-28

### Release Highlights

- 新增**服务端用户偏好持久化**（主题、色板、视图模式、排序、语言），支持多设备自动同步
- 新增**"我的分享"页面**，支持分享状态追踪（active / expired / exhausted / deleted）与分页管理
- 文件和文件夹列表新增**分享与锁定状态标识**，一眼区分资源状态
- 集成 **devicon 语言图标**，代码预览与文件类型图标全面升级
- **拖放交互增强**：文件夹树支持跨组件拖拽、防止文件夹拖入自身或后代目录
- **i18n 命名空间拆分**：common → core / errors / validation / offline + 按需加载 share / settings / webdav
- **大规模前后端测试覆盖补充**，新增 4000+ 行单元测试 + 集成测试

### Added

- **服务端用户偏好持久化**
  - 新增 `PATCH /api/v1/auth/preferences` 端点
  - 支持主题模式、色板、视图模式、排序、语言等偏好
  - 前端 debounce 同步，多设备登录自动同步
  - 数据库 migration: users.config JSON 字段
- **"我的分享"页面**
  - 新增 `/my-shares` 路由，支持分享列表浏览与管理
  - 后端 `ShareStatus` 枚举（active / expired / exhausted / deleted）
  - `MyShareInfo` DTO 含资源名称、状态、剩余下载次数等
- **文件/文件夹状态标识**
  - 列表和网格视图新增分享状态与锁定状态图标
  - `FileItemStatusIndicators` 组件
- **devicon 语言图标集成**
  - 新增 `language-icon.tsx` 组件，基于 devicon 图标库
  - 代码预览文件类型图标升级
  - 新增 CMap 提取脚本，PDF 中文显示支持
- **拖放增强**
  - 文件夹树支持拖拽到文件浏览器
  - 防止文件夹拖入自身或后代目录
  - 拖放逻辑提取到 `lib/dragDrop.ts` 公共模块
- **代码预览 minimap**
  - TextCodePreview 启用 minimap 功能
- **分享查找索引**
  - migration 新增 share 表查询索引，优化 token 和 resource 查询性能

### Changed

- **审计动作类型安全**
  - 审计日志从字符串字面量重构为 `AuditAction` 枚举
- **路由层逻辑下沉**
  - auth、share_public、files、folders、batch 等路由层业务逻辑下沉至 service 层
- **i18n 命名空间拆分**
  - `common` 拆分为 `core`、`errors`、`validation`、`offline`
  - 新增 `settings`、`share`、`webdav` 独立命名空间
  - 初始加载与延迟加载分层优化
- **错误日志分级**
  - 5xx → `tracing::error`，4xx → `tracing::warn`
  - 静默忽略的错误统一替换为 warn 日志
- **前端公共模块提取**
  - `ToolbarBar` 通用工具栏组件
  - `AdminTableList` 通用管理后台列表组件
  - 多个 hooks / utils 去重
- **admin 用户更新优化**
  - 合并为单次批量修改（role + status + quota）
  - 补充审计日志
- **分享页面布局重构**
  - 提取 `ShareTopBar`、`ToolbarBar` 通用组件

### Fixed

- 修复分享下载链接使用相对路径导致下载失败的问题
- 修复复制操作中 null 目标路径未正确解析为根目录的问题
- 修复 fire-and-forget 操作中静默忽略的错误（改为 warn 日志）
- 修复前端非空断言导致的潜在运行时错误
- 修复布局滚动区域样式问题
- 消除多处无障碍访问问题

### Breaking Changes

- **API**：`GET /api/v1/shares` 响应体从 `share::Model` 改为 `MyShareInfo` 分页对象，包含 `status` 枚举、`resource_name`、`remaining_downloads` 等新字段
- **API**：`GET /api/v1/auth/me` 响应体从 `UserInfo` 改为 `MeResponse`，新增 `preferences` 字段
- **API**：新增 `PATCH /api/v1/auth/preferences` 端点
- **Frontend**：i18n 命名空间 `common` 已拆分为 `core` / `errors` / `validation` / `offline`，自定义前端需同步更新翻译引用

---

**统计数据**：
- 291 files changed, 28,047 insertions(+), 2,216 deletions(-)
- 24 commits

## [v0.0.1-alpha.8] - 2026-03-27

### Release Highlights

- 管理后台新增**管理员创建用户**能力，适合自托管场景下集中管理账号
- 多个管理接口与用户侧列表统一为 **offset 分页结构**，大数据量场景下体验更稳、前后端类型更一致
- 文件拖拽体验升级：新增**自定义拖拽预览**，文件夹树支持**拖拽悬停自动展开**
- PWA 启动体验优化：新增**离线启动降级页**，并在登录后预热常用路由资源
- 分享访问边界与 WebDAV 账号管理补强，公开访问、路径展示与权限校验更可靠

### Added

- **管理员创建用户**
  - 后端新增 `POST /api/v1/admin/users`
  - 管理后台支持直接创建用户，无需依赖用户自行注册
- **管理后台用户详情面板**
  - 用户详情查看与编辑体验升级
  - 角色、状态、配额等信息改为统一保存交互
- **拖拽体验增强**
  - 文件卡片与列表行新增自定义拖拽预览
  - 文件夹树支持拖拽悬停自动展开，移动到深层目录更顺手
- **PWA 启动增强**
  - 新增离线启动降级页面
  - 登录后预热常用路由资源，改善安装态和弱网场景体验
- **统一分页基础结构**
  - 新增通用 `LimitOffsetQuery` / `OffsetPage<T>` 分页结构
  - 管理接口与部分用户接口统一接入 offset 分页

### Changed

- **管理后台列表统一分页**
  - 用户、策略、分享、配置、锁、审计日志、用户策略列表统一切换到 offset 分页返回
- **用户侧部分列表统一分页**
  - `/api/v1/shares` 与 `/api/v1/webdav-accounts` 改为分页对象返回
- **管理后台布局重构**
  - 顶栏、页面容器、说明文案与控件尺寸做了一轮统一整理
- **WebDAV 账号路径构建优化**
  - 通过批量路径构建减少重复查询，路径展示更稳定
- **依赖与构建配置更新**
  - 升级部分前后端依赖
  - 新增性能构建 profile，并适配新版 `sha2` Digest API

### Fixed

- 修复分享公开访问中的多个边界问题，包括过期分享、越界访问、已删除子文件 / 子目录访问等情况
- 修复重复活跃分享创建未被正确拦截的问题
- 修复 WebDAV 账号 root folder 校验与禁用账号测试相关边界问题
- 修复 PWA 离线启动时无缓存用户场景下的启动流程问题
- 补强审计日志、分享、WebDAV 相关测试覆盖与权限边界验证

### Breaking Changes

- **API**：多个列表接口的响应结构已从数组调整为分页对象：
  - `/api/v1/shares`
  - `/api/v1/webdav-accounts`
  - 多个 `/api/v1/admin/*` 列表接口
- 依赖旧数组响应格式的自定义前端、脚本或第三方客户端需要同步适配

---

**统计数据**：
- 87 files changed, 6,021 insertions(+), 1,783 deletions(-)
- 15 commits

## [v0.0.1-alpha.7] - 2026-03-26

### Release Highlights

- 文件列表新增多字段排序，并升级为基于 cursor 的分页，深目录和大文件夹浏览更顺手
- 前端接入 PWA，支持更新提示与离线登录态保持，弱网/断网场景体验更稳
- 文件夹树状态管理重构，引入按需加载与祖先路径恢复，目录导航性能明显改善
- 新增文件/文件夹详情信息对话框，快速查看大小、类型、时间、锁状态和子项数量
- 回收站批量恢复与批量清理链路重构，减少事务和 DB 往返，删除与清空操作更高效
- 上传面板引入虚拟滚动，预览错误态与重试入口统一，大量任务和异常场景下前端更稳定

### Added

- **文件列表排序与分页能力增强**
  - 文件列表支持按 `name` / `size` / `created_at` / `updated_at` / `type` 排序
  - 前端新增排序菜单，支持升序 / 降序切换
  - 文件列表分页升级为 cursor 模式，支持 `file_after_value` + `file_after_id`
- **PWA 支持**
  - 前端接入 `vite-plugin-pwa`
  - 支持 manifest、service worker 注册与新版本更新提示
- **离线登录态保持**
  - `authStore` 缓存用户信息，网络异常时保留现有登录态
- **文件/文件夹详情信息对话框**
  - 文件支持查看大小、MIME、创建/修改时间、锁状态、blob id
  - 文件夹支持查看创建/修改时间、锁状态、策略 id 与子项数量
- **文件夹祖先路径接口**
  - 新增 `/folders/{id}/ancestors`，用于恢复深层目录导航路径

### Changed

- **文件夹树状态管理重构**
  - 前端文件夹树改为按需加载，减少一次性加载整棵树的压力
  - 深层目录进入时可正确恢复祖先路径与树展开状态
- **回收站批量链路重构**
  - 批量恢复、批量清理与递归清理逻辑统一走批处理路径
  - 减少事务次数与数据库往返
- **上传面板性能优化**
  - 引入虚拟滚动，优化大量上传任务场景下的渲染性能
- **前端资源加载优化**
  - i18n 改为按需加载
  - Vite 构建拆分优化，配合 PWA 缓存策略改进加载体验

### Fixed

- 排序切换后文件列表状态不同步的问题，切换排序时会正确重置列表并重新加载
- 文件预览错误态不一致的问题，统一错误展示与重试入口
- 分享内容列表与主文件列表能力不一致的问题，补齐排序与 cursor 分页链路
- 缩略图生成重复入队与高负载下体验不稳定的问题，增加去重与重试优化
- 回收站批量恢复 / 清理过程中的部分边界问题，避免重复处理和漏处理

### Breaking Changes

- **API**：文件列表查询不再使用 `file_offset`，改为 cursor 分页参数 `file_after_value` + `file_after_id`
- **API**：文件列表相关接口新增 `sort_by` 与 `sort_order` 查询参数，旧调用方需要同步适配

---

**统计数据**：
- 91 files changed, 4,209 insertions(+), 1,477 deletions(-)
- 18 commits

## [v0.0.1-alpha.6] - 2026-03-25

### Release Highlights

- 文件列表、回收站、分享页面全面支持分页 + 前端无限滚动，告别一次加载全量数据
- 缩略图改为后台异步生成，接口返回 202 让前端轮询重试，解决大量文件上传后的内存峰值问题
- 回收站永久删除批量优化，N 个文件由 ~12N 次 DB 查询降至 ~10 次
- 新增剪贴板操作（Ctrl+C/X/V）与 F2 重命名快捷键
- 新增四档限流中间件（auth/public/api/write）、空文件创建接口、用户状态缓存

### Added

- **分页系统**
  - 后端新增 `FolderListQuery` 分页参数（`folder_limit/offset`、`file_limit/offset`），默认 folder_limit=200, file_limit=100
  - 文件夹列表、回收站列表、分享内容列表三个接口全面支持分页
  - 响应体新增 `folders_total` / `files_total` 字段
  - 前端 `fileStore` 新增 `loadMoreFiles` + IntersectionObserver 无限滚动
  - TrashPage、ShareViewPage 同步接入分页及无限滚动
  - 文件夹树与目标文件夹选择弹窗传入 `file_limit: 0` 仅加载文件夹
- **缩略图异步后台生成**
  - `thumbnail_service::get_or_enqueue()` — 缩略图不存在时入队后台生成，返回 202 + `Retry-After: 2`
  - `AppState.thumbnail_tx` 独立 tokio worker 顺序消费队列，HashSet 去重防止同一 blob 重复处理
  - WebDAV fs/file/handler 全链路透传 thumbnail channel
  - 前端 `useBlobUrl` 收到 202 自动按 `Retry-After` 间隔重试（最多 5 次）
- **限流中间件**
  - `RateLimitConfig` 四档限流（auth/public/api/write），默认关闭，支持按需启用
  - `AsterIpKeyExtractor` — 429 响应返回统一 JSON 格式并携带 `Retry-After` 头
  - 各路由通过 `Condition` 按 tier 挂载 Governor 限流中间件
- **空文件创建接口**
  - `POST /api/v1/files/new` 创建 0 字节空文件，支持 blob 去重与文件名冲突自动重命名
  - 前端 `CreateFileDialog` 组件，支持文件浏览器内直接创建空文件
- **剪贴板操作与重命名快捷键**
  - `fileStore` 新增 `clipboardCopy` / `clipboardCut` / `clipboardPaste` / `clearClipboard`
  - `useKeyboardShortcuts` 新增 Ctrl+C/X/V 剪贴板快捷键与 F2 重命名快捷键
  - FileGrid / FileTable 新增 `onRename` 回调
- **回收站批量操作 Repo 函数**
  - `file_repo::delete_many` / `delete_blobs` / `decrement_blob_ref_counts`
  - `folder_repo::delete_many` / `find_all_children` / `find_all_files_in_folder`
  - `property_repo::delete_all_for_entities`、`version_repo::delete_all_by_file_ids`

### Changed

- **回收站批量清理重构**
  - `file_service::batch_purge` — 单次事务处理所有 DB 操作，事务后并行物理清理
  - `webdav_service::recursive_purge_folder` 改为先递归收集再批量清理
  - `trash_service::purge_all` 优先批量处理顶层文件夹，再批量清理顶层散文件
- **用户状态缓存**
  - auth 中间件引入用户状态缓存（TTL=30s），减少每次请求查 DB
  - admin 禁用用户时主动失效缓存
- **前端组件**
  - `ScrollArea` 改为 `forwardRef`，ref 指向 Viewport 元素支持 IntersectionObserver
  - 前端空文件创建改为调用新接口，移除 multipart FormData 逻辑
- **代码格式化**
  - 统一 rustfmt 格式化全项目代码，拆分过长链式调用与函数参数

### Fixed

- 移除 `purge` 中对 `is_locked` 的检查，回收站内文件不应受锁限制
- 回收站列表改为 SQL 级顶层删除项过滤分页，移除内存 HashSet 过滤逻辑
- `recursive_purge_folder` 改用 `find_all_children`（不过滤 deleted_at），修复漏掉已软删除子目录的问题

---

**统计数据**：
- 72 files changed, 2,844 insertions(+), 318 deletions(-)
- 6 commits

## [v0.0.1-alpha.5] - 2026-03-25

### Release Highlights

- S3 上传流程大幅简化：去掉 SHA256 回读和 copy_object，直接以 `files/{uuid}` 作为最终存储路径，降低延迟和流量消耗
- 上传幂等重试：upload_session 记录 file_id，重复 complete 直接返回已有文件，新增 Assembling 中间状态（HTTP 202）防止前端轮询卡死
- 日志轮转：支持按天自动轮转 + 保留历史文件数量配置（`enable_rotation` / `max_backups`）
- 前端设置页和 WebDAV 账号页用 SettingsScaffold 组件重构，统一卡片式布局
- 前端类型统一从生成的 API schema 导出，消除手写重复定义
- 文件流式响应性能优化，减少内存占用

### Added

- **上传幂等重试**
  - upload_sessions 表新增 `file_id` 列（migration），完成后记录关联文件 ID
  - 重复 complete 请求：session 已完成 → 直接返回已有文件；正在处理 → 返回 HTTP 202（ErrorCode 3011）
  - assembly 失败自动标记 session 为 Failed，防止前端无限重试
  - `generate_upload_id()` 碰撞检测，最多重试 5 次
- **日志轮转**
  - `LoggingConfig` 新增 `enable_rotation`（默认 true）和 `max_backups`（默认 5）
  - 基于 tracing_appender rolling 按天轮转，自动清理超出数量的历史日志
  - 轮转失败自动 fallback 到 stdout 并输出警告
- **前端 SettingsScaffold 组件**
  - `SettingsPageIntro` / `SettingsSection` / `SettingsRow` / `SettingsIcon` 复用组件
  - 统一卡片式布局，支持 action slot 和自定义内容区

### Changed

- **S3 上传流程简化**
  - presigned / multipart 上传不再回读 S3 对象做 SHA256，改用 `s3-{upload_id}` 占位 hash
  - 不再 copy_object 到内容寻址路径，直接以 `files/{upload_id}` 为最终 key
  - 去除 S3 临时对象删除步骤（不再有临时→正式的两步操作）
- **前端页面重构**
  - SettingsPage 用 SettingsScaffold 重写，代码量大幅减少
  - WebdavAccountsPage 重构精简，统一布局风格
  - 前端类型统一从 `api.generated.ts` 导出，`types/api.ts` 仅做 re-export
  - searchService / fileService / uploadService 改用生成的类型定义
- **macOS 临时目录清理**
  - `cleanup_temp_dir` 增加重试机制（最多 3 次 + 50ms 间隔），处理 Spotlight 造成的 ENOTEMPTY
- **文件流式响应**
  - `file_service` 优化流式响应性能，减少内存占用

### Fixed

- 修正 PDF 预览头部信息区域缩进格式
- 修复目录上传工具函数的边界处理

---

**统计数据**：
- 24 files changed, 1,045 insertions(+), 950 deletions(-)
- 5 commits

## [v0.0.1-alpha.4] - 2026-03-25

### Release Highlights

- 支持 S3 分片直传（presigned_multipart）及断点续传，提升大文件上传性能和稳定性
- 重构回收站页面及功能，新增批量操作与拖拽删除功能
- 文件预览新增内嵌 PDF 预览，支持分页、缩放、旋转及下载
- 重构 WebDAV 账号管理页面，升级 UI 并完善国际化文案
- 优化文件夹树缓存与交互，提高初始加载和操作响应速度
- 设置页面改为响应式卡片布局，增强国际化支持
- 大幅重构用户文档站点组织，迁移 API 与架构文档至 developer-docs
- 多项安全加固，包括 Cookie Secure 标志、上传权限校验及并发更新防护
- 性能优化和 bug 修复，包括上传流程、文件树交互及前端状态管理  

### Added

- presigned_multipart 上传模式批量预取签名、上传和状态持久化
- 拖拽、快捷键、批量选择至回收站功能
- react-pdf集成，内置 PDF 预览窗口和工具栏
- 目录上传支持，前端拖拽/选择目录解析及后端相对路径递归创建
- 审计日志清理及多项后台任务panic-safe封装
- upload panel 进度条及分组显示  

### Changed

- 文档站重构，聚焦用户视角，优化导航和结构
- 文件浏览器视图初始加载性能优化
- 重写上传相关 hooks，移除冗余代码与无用接口
- 将 iframe sandbox 限制提升安全性，限制脚本执行

### Fixed

- 修复 token 刷新失败后前端清理登录状态问题
- 修正文件大小信息多处不一致与版本回归错误
- 修复重名文件自动后缀问题
- 修复上传状态互相覆盖与可能的并发冲突
- 修正回收站路径过滤及回收站详情与同步问题  

### Breaking Changes

- API /api/v1/auth/login 请求字段由 username 调整为 identifier


## [v0.0.1-alpha.3] - 2026-03-24

### Release Highlights

**预览、上传与认证体验全面升级！** 从文件预览、登录流程到上传任务面板，这一版把前后端体验一起往前拽了一大截。

- **认证流程重构** — 支持用户名 / 邮箱统一登录，并新增首次初始化管理员引导
- **统一文件预览系统** — 支持 Markdown、JSON、XML、CSV/TSV、媒体与代码预览
- **分享能力增强** — 公开文件可直接预览，文件夹分享支持下载其中的文件
- **上传体验升级** — 新增上传任务面板、并发上传、分片重试与状态追踪
- **版本恢复重构** — 回退时裁剪后续历史版本，并完善 blob 清理与回归测试
- **前端体验优化** — 登录页、文件浏览器、TopBar、提示通知与国际化整体打磨

### Added

- **认证与初始化流程**
  - 新增 `/api/v1/auth/check`，根据输入自动判断登录 / 注册 / 首次初始化路径
  - 新增 `/api/v1/auth/setup`，支持系统首次启动时创建管理员账号
  - 登录支持邮箱或用户名作为统一标识符
- **新文件预览体系**
  - 统一 `FilePreviewDialog` 作为预览入口
  - 新增 Markdown、JSON、XML、CSV/TSV、文本代码等多种预览器
  - 支持 Open With 模式切换、能力判断与未保存修改离开确认
- **分享增强**
  - 公开分享文件页支持直接预览
  - 文件夹分享新增子文件公开下载能力
  - 分享元信息补充 `mime_type` 与 `size`
- **上传任务面板**
  - 新增 `UploadPanel` / `UploadTaskItem`
  - direct / chunked / presigned 三种上传模式统一进任务列表
  - 支持并发上传、分片重试、状态跟踪与完成后保留任务
- **文件尺寸冗余字段**
  - `files` 表新增 `size` 字段
  - migration 回填历史数据，为列表展示和接口返回提供稳定大小信息
- **骨架屏与品牌资源优化**
  - 新增文件网格 / 表格 / 树等骨架组件
  - 重构 logo SVG 结构并优化登录页、TopBar 的品牌展示

### Changed

- **登录页**
  - 重构为双栏品牌布局 + 多步骤认证交互
  - 支持自动检查账号状态、动态切换登录 / 注册 / 初始化模式
  - 优化表单校验、过渡动画与退出动画
- **文件浏览器**
  - 批量移动 / 复制改为目标目录选择对话框
  - 批量操作结果改为更友好的详细提示
  - 版本历史弹窗改为受控模式，并补全恢复 / 删除确认交互
- **通知与国际化**
  - Toast 改为右下角出现，支持右滑关闭
  - 批量操作、错误提示、版本历史等文案统一接入中英文翻译
- **版本恢复语义**
  - 恢复到某个版本时，删除该版本及之后的历史版本
  - 恢复逻辑改为事务化处理，并在提交后做 blob 引用清理
- **后台周期任务**
  - 上传清理、回收站清理、锁清理、审计日志清理统一纳入 `runtime/tasks.rs`
  - 周期任务增加 panic-safe 包装，避免单个任务异常打死整个循环
- **错误处理**
  - 引入 `MapAsterErr`，统一错误上下文映射，减少重复样板

### Fixed

- 修复公开分享页被登录态检查误伤并跳转到 `/login` 的问题
- 修复 token 刷新失败后的前端会话状态清理逻辑
- 修复版本恢复后历史列表与 blob 清理不一致的问题
- 修复文件大小信息在多个链路中的不一致问题
- 修复上传任务列表状态互相覆盖、不可滚动、完成即消失等体验问题
- 修复文件树拖拽到根目录时缺少操作反馈的问题

### Breaking Changes

- **API**: `/api/v1/auth/login` 请求字段由 `username` 调整为 `identifier`

---

**统计数据**：
- 139 files changed, 7,915 insertions(+), 1,786 deletions(-)
- 11 commits

## [v0.0.1-alpha.2] - 2026-03-23

### Release Highlights

**前端完整重写！** 从 PoC 级别升级到现代 UI 架构，新增国际化、主题系统、响应式布局。

- **i18n 国际化** — react-i18next，中英双语，5 个命名空间，即时切换
- **主题系统** — Light / Dark / System 三种模式 + 4 套色板（Blue / Green / Purple / Orange），CSS 变量 oklch
- **响应式布局** — 可折叠侧栏、全局顶栏、移动端 overlay
- **网格 / 列表视图** — 双视图切换，记住偏好，缩略图卡片 + 可排序表格
- **多选 + 批量操作** — 勾选框选择，底部浮动操作栏，批量删除 / 移动 / 复制
- **递归文件夹树** — 懒加载展开，替代原来的平铺列表

### Added

- **i18n 系统**
  - react-i18next + i18next-browser-languagedetector
  - 5 个命名空间：common / files / auth / admin / search
  - 中英双语完整翻译（125+ 键值对）
  - 自动检测浏览器语言，localStorage 持久化
- **主题系统**
  - `themeStore` — Light / Dark / System 模式，matchMedia 监听系统偏好
  - 4 套色板预设（blue / green / purple / orange），每套含 light + dark 变体
  - CSS 变量 oklch 色彩空间，`[data-theme]` 属性切换
  - 所有偏好存 localStorage
- **公共组件库** `components/common/`
  - ThemeSwitcher — Sun / Moon / Monitor 下拉切换
  - ColorPresetPicker — 色板圆点选择器
  - LanguageSwitcher — 中英语言下拉
  - EmptyState — 图标 + 标题 + 描述 + 操作按钮
  - LoadingSpinner — 居中旋转加载
  - ConfirmDialog — AlertDialog 封装，destructive 变体
  - ViewToggle — 网格 / 列表图标切换
  - BatchActionBar — 底部浮动栏（选择数 + 删除 / 移动 / 复制）
- **新布局组件**
  - Sidebar — 桌面可折叠（240px / 56px），移动端 overlay + 遮罩
  - TopBar — 全局顶栏：汉堡菜单 + 面包屑 + 主题 / 语言 / 用户下拉
- **文件浏览器组件**
  - FileGrid — 响应式网格（2-6 列），缩略图卡片
  - FileTable — 列表表格，可排序列头，全选勾选框
  - FileCard — 网格卡片，hover 显示勾选框
  - FileThumbnail — 提取复用，sm / lg 两种尺寸
  - FileContextMenu — 右键菜单（下载 / 分享 / 复制 / 重命名 / 锁 / 版本 / 删除）
  - CreateFolderDialog — 从 FileBrowserPage 提取
  - RenameDialog — 文件 / 文件夹重命名，自动选中文件名（不含扩展名）
- **设置页** `/settings`
  - 主题模式 + 色板选择
  - 语言切换
  - 文件浏览器默认视图模式
- **键盘快捷键**
  - Ctrl/Cmd + A — 全选
  - Escape — 取消选择
  - / 或 Ctrl/Cmd + K — 聚焦搜索
- **工具函数** `lib/format.ts`
  - `formatBytes` / `formatDate` / `formatDateAbsolute`
  - 替代 5 处重复实现

### Changed

- **AppLayout** — 重写为 TopBar + 可折叠 Sidebar + main content 三段式
- **FolderTree** — 从平铺列表重写为递归懒加载树（展开 / 折叠 / 子文件夹加载）
- **fileStore** — 完全重写，新增 viewMode / sortBy / sortOrder / selectedFileIds / selectedFolderIds
- **FileBrowserPage** — 从 267 行单体重写为 ~80 行编排器
- **PageHeader** — 简化为薄层组件，面包屑移至 TopBar
- **AdminLayout** — 加 i18n 翻译 + ThemeSwitcher / LanguageSwitcher
- **所有 11 个页面** — 全部加入 i18n 翻译，hardcoded 英文字符串归零
- **所有破坏性操作** — 统一使用 ConfirmDialog 确认
- **所有原生 `<select>`** — 统一替换为 shadcn Select 组件
- **暗色模式兼容** — Badge / 状态色全部加 `dark:` 变体

### Removed

- `FileList.tsx` — 被 FileGrid + FileTable 替代
- FileBrowserPage 中的 batch PoC 面板（手动输入 ID）— 被 BatchActionBar 替代
- 5 处重复的 `formatBytes` / `formatDate` 内联函数

### Dependencies

- 新增 `react-i18next` 16.6
- 新增 `i18next` 25.10
- 新增 `i18next-browser-languagedetector` 8.2

---

**统计数据**：
- 79 files changed, 3,632 insertions(+), 1,506 deletions(-)
- 1 commit

## [v0.0.1-alpha.1] - 2026-03-23

### Release Highlights

**AsterDrive 第一个公开版本！** 自托管云存储系统，Rust 单二进制分发，MIT 许可证。

- **完整文件管理** — 上传（直传/分片/S3 presigned）、下载、复制、移动、在线编辑、版本历史、缩略图
- **WebDAV 协议** — RFC 4918 Class 1 + LOCK，独立账号系统，数据库持久化锁，DeltaV 版本查询
- **存储策略系统** — Local + S3 双驱动，用户级/文件夹级策略覆盖，sha256 去重 + ref_count
- **分享链接** — 密码保护、过期时间、下载次数限制、缩略图支持
- **搜索 + 批量操作 + 审计日志** — 完整的后端 API，Admin 审计可追溯

### Added

- **文件管理**
  - multipart 流式上传（64KB 块 sha256，blob 去重 + ref_count）
  - 分片上传（init → chunk → complete，幂等性保证）
  - S3 presigned 直传（策略级开关，临时路径 → copy_object → 删 temp）
  - 流式下载（Content-Length，不全量缓冲）
  - 文件复制（blob 引用计数，不复制实际数据）
  - 移动 / 重命名（同名冲突检测）
  - 在线编辑（PUT /content，ETag 乐观锁 + 悲观锁检查）
  - 文件版本历史（自动保存旧版本，支持回滚）
  - 图片缩略图（WebP，按需生成，长期缓存）
- **文件夹管理**
  - 创建 / 删除 / 复制 / 移动 / 重命名
  - 递归操作（软删除、硬删除、复制均支持深层嵌套）
  - 循环检测（移动时防止 A → B → A）
- **存储系统**
  - 存储策略体系（系统默认 + 用户级 + 文件夹级覆盖）
  - Local 驱动 + S3 驱动（aws-sdk-s3）
  - 存储配额管理（用户级，管理员可调）
  - Driver Registry 热加载（策略更新后自动清理缓存）
- **认证授权**
  - JWT 双 Token（Access + Refresh），HttpOnly Cookie 存储
  - argon2 密码哈希
  - 自动 401 → refresh token 重试
  - 角色系统（admin / user），第一个注册用户自动成为管理员
- **WebDAV**
  - RFC 4918 Class 1 + LOCK 完整实现
  - Basic Auth（独立 webdav_accounts 表）+ Bearer JWT
  - DbLockSystem 数据库持久化锁（重启不丢锁，后台每小时清理过期锁）
  - root_folder_id 访问限制
  - 大文件临时文件流式处理
  - macOS 兼容（过滤 `._*` / `.DS_Store`）
  - RFC 3253 DeltaV 版本历史查询
- **分享链接**
  - 唯一 token + 密码保护（argon2）+ 过期时间 + 下载次数限制
  - 公开路由 `/s/{token}`（查看 / 验证密码 / 下载 / 文件夹浏览 / 缩略图）
  - Cookie 签名验证（SHA256，1 小时有效）
- **回收站**
  - 软删除（deleted_at 列，所有列表查询自动过滤）
  - 恢复（原文件夹已删除时自动恢复到根目录）
  - 永久删除（blob cleanup + 缩略图 + 属性 + 配额）
  - 后台自动清理（可配置保留天数，默认 7 天）
- **搜索 API**
  - GET `/api/v1/search` — 文件名 LIKE 模糊搜索 + 元数据过滤（MIME / 大小 / 日期）
  - 跨数据库兼容（LOWER() + LIKE）
  - 支持 file / folder / all 类型过滤，folder_id 限定范围，分页
- **批量操作**
  - POST `/api/v1/batch/{delete,move,copy}` — file_ids + folder_ids 混合类型
  - 每项独立执行，返回 succeeded / failed / errors 汇总
  - 100 项上限
- **审计日志**
  - audit_logs 表（action + entity + details + IP / UA）
  - Fire-and-forget 写入（不阻塞业务操作）
  - 运行时配置开关（audit_log_enabled / audit_log_retention_days）
  - Admin 查询 API（过滤 + 分页）
  - 后台自动清理过期日志
  - 覆盖：文件 / 文件夹 / 登录注册 / 分享 / 批量操作 / 配置变更
- **自定义属性**
  - entity_properties 表（entity_type + entity_id + namespace + name + value）
  - WebDAV PROPPATCH 兼容
  - REST API: GET / PUT / DELETE
- **配置系统**
  - 静态配置: `config.toml`（环境变量 ASTER__ 覆盖），首次启动自动生成
  - 运行时配置: system_config 表（Admin API 热改）
  - 配置定义单一数据源（definitions.rs），启动时 ensure_defaults
  - Schema API + 类型校验 + 前端分组渲染
- **缓存**
  - CacheBackend trait（NoopCache / MemoryCache / RedisCache）
  - CacheExt 泛型扩展（自动 serde 序列化）
  - Policy + Share 查询缓存
- **监控**
  - Prometheus 指标（`metrics` feature 门控）+ sysinfo 系统指标
  - Health / Ready 端点
- **管理后台**
  - 用户管理（角色、状态、配额、强制删除）
  - 存储策略管理（CRUD、连接测试、用户级分配）
  - 分享管理（全局列表、管理员删除）
  - WebDAV 锁管理（列表、强制释放、过期清理）
  - 系统配置管理（分类、schema、类型校验）
  - 审计日志查询
- **前端 PoC**
  - React 19 + Vite 8 + Tailwind CSS 4 + shadcn/ui + zustand
  - 文件浏览器（列表视图 + 面包屑导航 + 缩略图 + 预览 + 拖拽上传）
  - 管理后台（用户 / 策略 / 分享 / 锁 / 配置 / 审计日志）
  - 搜索页、批量操作面板
  - rust-embed 编译进单二进制
- **测试**
  - 30+ 集成测试覆盖全部核心功能
  - OpenAPI spec 自动生成（utoipa + swagger-ui）
- **API 文档**
  - utoipa 注解全部端点
  - Swagger UI（debug 构建）
  - OpenAPI JSON 自动导出

### Dependencies

- **Web**: actix-web 4.13, actix-governor 0.10
- **ORM**: sea-orm 2.0.0-rc.37（SQLite / MySQL / PostgreSQL）
- **Auth**: jsonwebtoken 10, argon2 0.5
- **Storage**: aws-sdk-s3 1.127
- **Cache**: moka 0.12, redis 1.1
- **WebDAV**: dav-server 0.11
- **API Docs**: utoipa 5.4, utoipa-swagger-ui 9.0
- **Image**: image crate（jpeg/png/gif/webp/bmp/tiff）
- **Frontend**: React 19, Vite 8, Tailwind CSS 4, shadcn/ui, zustand 5, uppy 5

---

**统计数据**：
- 287 files changed, 48,597 insertions(+)
- 66 commits
- Rust Edition 2024, MSRV 1.91.1

[Unreleased]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-beta.1...HEAD
[v0.4.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.4.0-beta.1...v0.4.0-beta.2
[v0.4.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.2...v0.4.0-beta.1
[v0.3.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.1...v0.3.2
[v0.3.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0...v0.3.1
[v0.3.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-rc.2...v0.3.0
[v0.3.0-rc.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-rc.1...v0.3.0-rc.2
[v0.3.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-beta.2...v0.3.0-rc.1
[v0.3.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-beta.1...v0.3.0-beta.2
[v0.3.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.5...v0.3.0-beta.1
[v0.3.0-alpha.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.4...v0.3.0-alpha.5
[v0.3.0-alpha.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.3...v0.3.0-alpha.4
[v0.3.0-alpha.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.2...v0.3.0-alpha.3
[v0.3.0-alpha.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.3.0-alpha.1...v0.3.0-alpha.2
[v0.3.0-alpha.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.7...v0.3.0-alpha.1
[v0.2.7]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.6...v0.2.7
[v0.2.6]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.5...v0.2.6
[v0.2.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.4...v0.2.5
[v0.2.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.3...v0.2.4
[v0.2.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.2...v0.2.3
[v0.2.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.1...v0.2.2
[v0.2.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-hotfix.1...v0.2.1
[v0.2.0-hotfix.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0...v0.2.0-hotfix.1
[v0.2.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-rc.1...v0.2.0
[v0.2.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.3...v0.2.0-rc.1
[v0.2.0-beta.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.2...v0.2.0-beta.3
[v0.2.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.2.0-beta.1...v0.2.0-beta.2
[v0.2.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0...v0.2.0-beta.1
[v0.1.0]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-rc.2...v0.1.0
[v0.1.0-rc.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-rc.1...v0.1.0-rc.2
[v0.1.0-rc.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.5...v0.1.0-rc.1
[v0.1.0-beta.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.4...v0.1.0-beta.5
[v0.1.0-beta.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.3...v0.1.0-beta.4
[v0.1.0-beta.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.2...v0.1.0-beta.3
[v0.1.0-beta.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.1.0-beta.1...v0.1.0-beta.2
[v0.1.0-beta.1]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.26...v0.1.0-beta.1
[v0.0.1-alpha.26]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.25...v0.0.1-alpha.26
[v0.0.1-alpha.25]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.24...v0.0.1-alpha.25
[v0.0.1-alpha.24]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.23...v0.0.1-alpha.24
[v0.0.1-alpha.23]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.22...v0.0.1-alpha.23
[v0.0.1-alpha.22]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.21...v0.0.1-alpha.22
[v0.0.1-alpha.21]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.20...v0.0.1-alpha.21
[v0.0.1-alpha.20]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.19...v0.0.1-alpha.20
[v0.0.1-alpha.19]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.18...v0.0.1-alpha.19
[v0.0.1-alpha.18]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.17...v0.0.1-alpha.18
[v0.0.1-alpha.17]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.16...v0.0.1-alpha.17
[v0.0.1-alpha.16]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.15...v0.0.1-alpha.16
[v0.0.1-alpha.15]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.14...v0.0.1-alpha.15
[v0.0.1-alpha.14]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.13...v0.0.1-alpha.14
[v0.0.1-alpha.13]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.12...v0.0.1-alpha.13
[v0.0.1-alpha.12]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.11...v0.0.1-alpha.12
[v0.0.1-alpha.11]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.10...v0.0.1-alpha.11
[v0.0.1-alpha.10]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.9...v0.0.1-alpha.10
[v0.0.1-alpha.9]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.8...v0.0.1-alpha.9
[v0.0.1-alpha.8]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.7...v0.0.1-alpha.8
[v0.0.1-alpha.7]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.6...v0.0.1-alpha.7
[v0.0.1-alpha.6]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.5...v0.0.1-alpha.6
[v0.0.1-alpha.5]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.4...v0.0.1-alpha.5
[v0.0.1-alpha.4]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.3...v0.0.1-alpha.4
[v0.0.1-alpha.3]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.2...v0.0.1-alpha.3
[v0.0.1-alpha.2]: https://github.com/AsterCommunity/AsterDrive/compare/v0.0.1-alpha.1...v0.0.1-alpha.2
[v0.0.1-alpha.1]: https://github.com/AsterCommunity/AsterDrive/releases/tag/v0.0.1-alpha.1
