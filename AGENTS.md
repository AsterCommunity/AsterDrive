# AsterDrive

AsterDrive 是面向小团队的 Rust 自托管文件基础设施项目。它关注文件存储控制、可靠大文件上传、个人/团队工作空间、分享、回收站、版本历史、WebDAV/WOPI、远端存储节点和运维可观测性。

当前代码来自通用 Rust + React 服务模板长期演进，但已经是 AsterDrive 自己的产品。修改时要围绕云盘/文件基础设施语义组织代码，不要把其他项目的领域概念带进来。

## 工作前必须先看

- 先读现有代码模式，再动手。看不清模式就停下问 1547，别凭感觉硬写。
- 修改前优先从现有入口追链路：`src/api/routes/*` -> `src/services/*` -> `src/db/repository/*` / `src/storage/*` / `src/webdav/*`。
- 前端改动先看 `frontend-panel/AGENTS.md`，那里有更细的 TypeScript、i18n、UI/UX、Base UI 组件坑位约束。
- 这个仓库可能有大量未提交改动。不要回滚用户改动；只改任务相关文件，遇到同文件交叉改动先读清楚再动手。
- code review comments 进来时，先分辨真问题还是误报；真实问题分批修，修完每批都编译/测试。别被机器人牵着鼻子走。

## 项目结构

```text
src/                         Rust 后端
src/api/                     primary/follower 路由、DTO、OpenAPI、中间件、响应封装
src/api/routes/              REST API、公开分享、内部存储、远端隧道路由
src/cache/                   cache trait 以及 memory/noop/Redis 实现
src/cli/                     doctor、config、database-migrate、node enroll 等运维 CLI
src/config/                  静态配置、运行时配置定义、配置规范化、模板
src/db/                      数据库连接、reader/writer 句柄、repository
src/entities/                SeaORM Entity
src/runtime/                 AppState、primary/follower 启动、关闭、周期任务
src/services/                Auth、file、folder、upload、share、team、policy、task、audit、WebDAV/WOPI 等业务层
src/storage/                 存储驱动、连接器、远端协议、multipart/stream 能力抽象
src/types/                   共享枚举、DTO 辅助类型和 DB wrapper 类型
src/utils/                   crypto、ID、path、number、email、RAII 等工具
src/webdav/                  WebDAV/DeltaV 协议接入、文件系统、锁、属性和传输
migration/                   SeaORM migration crate
api-docs-macros/             OpenAPI 辅助宏
frontend-panel/              React + Vite 前端，构建产物嵌入后端
developer-docs/              开发说明和架构文档
docs/                        用户/部署文档站
tests/                       集成测试、迁移测试、OpenAPI 导出测试
```

## 技术栈

- 后端: Rust 2024, actix-web 4, SeaORM 2.0-rc, tokio, jsonwebtoken, argon2
- 数据库: SQLite 默认，兼容 MySQL/PostgreSQL
- 缓存: memory/noop/Redis 后端
- 存储: local filesystem、S3-compatible object storage、Azure Blob、OneDrive、remote AsterDrive follower node
- 协议: REST API、WebDAV/DeltaV、WOPI、remote internal storage protocol
- 前端: React 19, Vite, TypeScript native-preview/tsgo, Tailwind CSS 4, shadcn/ui(Base UI), Biome, Vitest, Playwright
- OpenAPI: `utoipa` + `api-docs-macros` + `openapi-typescript`
- 嵌入: `rust-embed` 将 `frontend-panel/dist/` 编译进二进制

## 开发命令

```bash
# 后端
cargo run
cargo check
cargo test
cargo test --lib <test_filter>
cargo test --test <test_name> <test_filter>
cargo test --features openapi --test generate_openapi
cargo test --features metrics

# 指定集成测试数据库后端
ASTER_TEST_DATABASE_BACKEND=sqlite cargo test --test test_database_backends
ASTER_TEST_DATABASE_BACKEND=postgres cargo test --test test_database_backends
ASTER_TEST_DATABASE_BACKEND=mysql cargo test --test test_database_backends

# 前端
cd frontend-panel
bun install
bun run dev
bun run build
bun run check
bun run test
bun run test:e2e
```

跑单元测试时优先缩小范围，避免没必要地编译全包。批量修复后再跑 `cargo check` 和相关测试。改动 OpenAPI schema 后先跑 `cargo test --features openapi --test generate_openapi`，再到 `frontend-panel/` 跑 `bun run generate-api`。

## 当前核心能力

- 本地认证: setup/register/login/refresh/logout/me/sessions、用户偏好、头像、SSE 事件
- 外部认证/MFA: provider 配置、外部登录流程、WebAuthn/Passkey 基础能力
- 文件工作流: folder/file CRUD、上传、下载、移动、复制、删除、恢复、永久删除、版本历史
- 工作空间: personal workspace 和 team workspace 共用核心链路，通过 scope 切换作用域
- 分享: 公开分享页、密码、过期、下载次数、直链、预览直链
- 上传: direct、chunked resumable、presigned、multipart、remote relay/presigned 等策略
- 存储策略: local、S3-compatible、Azure Blob、OneDrive、remote follower node、policy group 路由
- WebDAV/WOPI: 独立 WebDAV 账号、锁、DeltaV 子集、Office 预览/编辑启动会话
- 远端节点: primary/follower 模式、internal storage API、direct/reverse tunnel/auto 传输
- 运维: runtime config、audit logs、background tasks、health、metrics、doctor、migration CLI

## 产品域边界

新增功能要围绕 AsterDrive 的文件基础设施组织，命名直接表达业务含义：

- 文件与目录: `file`, `folder`, `file_blob`, `file_version`, `workspace`, `trash`
- 工作空间与权限: `personal`, `team`, `member`, `role`, `quota`, `scope`
- 分享与公开访问: `share`, `public_link`, `download_token`, `preview`
- 上传链路: `upload_session`, `chunk`, `multipart`, `presigned`, `complete`, `cancel`
- 存储与路由: `storage_policy`, `policy_group`, `connector`, `driver`, `object_key`, `blob`
- 远端节点: `remote_node`, `follower`, `internal_storage`, `reverse_tunnel`, `managed_ingress`
- 协议能力: `webdav`, `wopi`, `lock`, `delta_v`
- 运维能力: `task`, `audit`, `runtime_config`, `doctor`, `health`

不要引入和当前产品无关的领域词来伪装功能。需要处理外部对象存储时，从 AsterDrive 的存储策略、连接器、驱动和上传策略建模，不要另起一套平行抽象。

## API 约定

项目 REST API 使用统一响应体：

```json
{ "code": "success", "msg": "", "data": { } }
```

失败使用稳定字符串错误码，定义在 `src/api/api_error_code.rs` 的 `AsterErrorCode`。内部错误类型是 `src/errors.rs` 的 `AsterError`，通过 `ResponseError` 统一转 HTTP 响应和日志。

新增项目 API 应继续使用这套 envelope 和错误码体系。例外包括：

- 文件下载、缩略图、预览、公开直链等直接返回流或二进制响应的接口
- SSE，例如 storage events
- Prometheus metrics text exposition
- WebDAV/DeltaV 协议响应
- WOPI 协议端点需要满足 WOPI host 的字段、状态码和错误行为
- follower internal storage protocol 和 reverse tunnel 内部传输需要满足内部协议签名/预签名约定

协议端点不能为了项目内部 envelope 破坏客户端兼容性。如果协议错误格式与 `AsterError` 不一致，单独建协议错误映射层，不要污染全局错误系统。

## 后端代码约定

- 路由模块放在 `src/api/routes/`，按现有 primary/follower 注册方式接入 `src/api/primary.rs`、`src/api/follower.rs` 或对应 `routes/mod.rs`。
- DTO 放在 `src/api/dto/`，领域共享类型放在 `src/types/`，不要在 handler 里散落匿名 JSON 拼装。
- 业务逻辑放 `src/services/`，数据库访问放 `src/db/repository/`，对象内容能力放 `src/storage/`，handler 只做认证、参数提取、调用 service、返回响应。
- WebDAV 不走普通 REST 路由；WebDAV 相关协议行为放在 `src/webdav/`，只在需要复用业务语义时调用 service/repo。
- 新表必须有 SeaORM entity 和 migration，测试覆盖 SQLite；涉及数据库差异时同时考虑 MySQL/PostgreSQL。
- 配置项统一定义在 `src/config/definitions.rs`，由运行时配置初始化逻辑写入默认值。不要在业务代码里写散落默认值。
- 运行时共享状态走现有 `AppState`/runtime startup 初始化路径，不要引入全局可变单例。
- 需要后台异步处理的用户可见任务，优先复用 `task_service` 的 task record/dispatch/retry/presentation 结构。
- fire-and-forget 操作用 `if let Err(error) = ... { tracing::warn!(...) }`，不要静默 `let _ =`。
- 数据库事务失败不用手写多余 `rollback()`；SeaORM transaction drop 会自动回滚。
- 不要把下载、上传完成、配额扣减、版本写入、审计写入拆成互相看不见的散逻辑；跨表一致性要在 service/repo 边界清楚表达。

## 数据库和类型约定

- 运行态通过 `DbHandles` 保存 writer 和 reader。写入、事务、读后写、配额权威判断、登录签发 session、refresh token rotation、上传 init/chunk/complete/cancel 继续走 writer。
- `reader_db()` 只能用于列表、详情、搜索、上传进度、recoverable sessions、presign 查询阶段、auth snapshot cache miss、public runtime snapshot、admin overview 统计这类允许短暂滞后的纯读路径。
- 不要把通用校验 helper 偷偷改成 reader，除非确认所有调用方都是纯读；更推荐在 service 入口显式选择 reader/writer。
- 枚举字段优先使用 `DeriveActiveEnum` 或明确的强类型 wrapper，禁止魔法字符串在 service/repo 间传来传去。
- 数据库列不要为了省事直接上 JSON。除非确实需要数据库侧 JSON 查询、索引或约束，否则结构化内容用 `TEXT` 存储，并在代码层用强类型 DTO + serde 校验。
- 禁止跨层裸写 `as i32` / `as usize` / `as i64` 做静默截断；使用 `src/utils/numbers.rs` 的 checked conversion helper。
- 多数据库 SQL 要保守：
  - 不用 SQLite-only 标量 `MAX(a, b)`，改用 `CASE WHEN ...`
  - 多表 join 下 `COUNT`/`GROUP BY` 必须显式限定列来源
  - 原子计数优先封装在 repo 函数里
- UUID、token、share password、session secret、object key、credential id 等敏感或易混字段要用专门类型或清晰命名，避免 `String` 到处裸传导致用错。

## 存储和上传约定

- 存储能力优先通过 `src/storage/traits/` 和 connector/driver registry 表达，不要在业务层直接分支到具体 SDK。
- 新增存储后端时，要同时考虑 policy descriptor、连接测试、credential 管理、上传/下载策略、admin 前端配置、OpenAPI 类型和文档最小更新。
- 上传路径必须尊重策略协商结果：direct、chunked、presigned、multipart、remote relay/presigned 不要互相绕过。
- 上传完成要保持元数据、blob/object、version、quota、audit、task/progress 的一致性；失败路径要能清理 session 或留下可恢复状态。
- object key、upload id、multipart part、remote node request id 等不要写进普通日志的高噪声字段；需要排查时用受控 tracing 字段并避免泄露凭据。
- 本地 blob 去重、引用计数、孤儿清理属于文件安全链路，改动时必须补测试。
- 公开下载/预览/缩略图要设置合理 cache header，但不能让私有、过期、撤销或未授权资源被缓存泄露。

## 安全约定

- access token、refresh token、WebDAV 密码、MFA secret、外部 OAuth credential、对象存储 secret、remote node secret 只能存哈希或加密后的必要形式；日志、审计、错误消息不得泄露明文。
- 登录、注册、分享访问、公开下载、上传 init/complete、WebDAV、WOPI、internal storage 都要接入对应鉴权、限流、安全头、CORS/CSRF 策略；协议兼容需要豁免时必须写清楚理由并加测试。
- 用户名、邮箱、URL、MIME、文件名、路径片段、文件大小、分片大小、分享密码、WebDAV 路径都要在 DTO/service 边界校验。
- 路径处理必须防 traversal、双重编码、Unicode/大小写边界和平台差异；优先复用现有 path 工具。
- 公开分享和 WOPI 回调尤其要检查权限快照、过期时间、锁状态、版本状态和下载次数扣减。
- 管理接口必须检查 admin 权限，不要只靠前端隐藏入口。

## 前端约定

`frontend-panel/` 是 AsterDrive 的产品前端，具体规则以 `frontend-panel/AGENTS.md` 为准。根目录只列必须遵守的总约束：

- 使用 Vite + React + TypeScript native-preview/tsgo + Tailwind CSS 4 + shadcn/ui(Base UI) + Biome。
- `erasableSyntaxOnly` 思路：禁止 TS enum，用 `as const` 对象；类型导入使用 `import type`。
- 后端 schema 类型从生成 SDK 和 `@/types/api.ts` 导入，禁止手写重复接口类型。
- `src/services/api.generated.ts` 是生成文件，不要手动修改。
- API 调用统一通过 `src/services/http.ts` 和现有 service 层，不要在组件里裸写 axios/fetch。
- 图标优先用 `src/components/common/Icon` 或项目已有封装，不要手写 SVG。
- 新页面和新组件优先复用 `src/components/common/`、`src/components/ui/`、`src/hooks/`、`src/lib/` 的公共模块。
- 新增翻译按 namespace 分层放到 `src/i18n/locales/{zh,en}/`，不要把文案硬编码在组件里。
- 管理界面要信息密度适中、可扫描、可重复操作；不要做营销落地页当首屏。
- ScrollArea / `overflow-auto` 要保持从视口根到滚动容器的 flex 链完整：中间层通常需要 `flex flex-col min-h-0 flex-1`。
- Base UI Select 要给 `items={[{ label, value }]}`，不要让 trigger 回显 raw value。
- `DropdownMenuItem` 的 SVG hover 变色有坑；需要图标跟随 hover 变色时用原生 `<button>` + CSS `:hover`。

## 测试要求

- 新增后端行为至少补对应单元测试或集成测试。鉴权、安全、上传、分享、WebDAV/WOPI、数据库迁移、存储驱动必须有测试。
- 修改 migration、entity、repo 或 SQL 时，至少跑 SQLite 相关测试；跨数据库逻辑要考虑 `ASTER_TEST_DATABASE_BACKEND=postgres|mysql`。
- 修改上传完成、配额、版本、删除/恢复、引用计数、后台清理时，要覆盖成功路径和失败/回滚边界。
- 修改存储策略或 connector 时，补连接测试、descriptor/payload 测试和前端表单测试。
- 修改 OpenAPI schema 后，跑 OpenAPI 导出并重新生成前端 SDK。
- 修改前端 service 层、关键 UI 流程或 i18n 时，跑 `bun run test`；涉及页面流程时补/跑 Playwright。
- code review fixes 要按批次验证：修一批真实问题，跑能覆盖该批的最小编译/测试，再继续。

## 文档和命名

- 文档可以更新，但不要主动写长篇使用说明，除非任务明确要求。
- README/docs 里若有过期描述，修改相关功能时顺手纠正直接相关部分，不做无关大清洗。
- 命名要面向 AsterDrive 领域：`workspace`, `file`, `folder`, `share`, `trash`, `storage_policy`, `policy_group`, `connector`, `driver`, `upload_session`, `blob`, `webdav`, `wopi`, `remote_node`, `task`, `audit`。
- 不要为了“通用化”把清楚的业务名改成含糊的 manager/helper/data/object。
- 保留 MIT 许可证约束。可参考其他项目的产品概念，但不得复制不兼容许可证代码。

## 参考资料

- 架构概览: `developer-docs/zh-CN/architecture.md`
- 模块设计: `developer-docs/zh-CN/module-designs.md`
- 前端约束: `frontend-panel/AGENTS.md`
- 用户文档: `docs/index.md`
- 配置示例: `config.example.toml`
- OpenAPI 导出: `frontend-panel/generated/openapi.json`（以当前生成流程为准）
