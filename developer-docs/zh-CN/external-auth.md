# 外部认证模块

外部认证把外部身份提供商的授权结果映射到 AsterDrive 本地用户。它不是独立账号系统，而是登录第一因子的一种来源；回调成功后仍会进入本地用户状态、MFA、注册开关、邮箱策略和审计流程。

## 代码结构

| 层 | 主要文件 | 职责 |
| --- | --- | --- |
| 路由 | `src/api/routes/auth/external_auth.rs` | 匿名 provider 列表、登录发起、回调、邮箱补验、密码绑定、用户解绑 |
| 管理路由 | `src/api/routes/admin/external_auth.rs` | provider kind、provider CRUD、草稿测试、已保存 provider 测试 |
| 服务聚合 | `src/services/external_auth_service/mod.rs` | DTO、常量和服务导出 |
| Provider 管理 | `src/services/external_auth_service/providers.rs` | provider 创建、更新、列表、测试、driver descriptor 映射 |
| 登录流程 | `src/services/external_auth_service/login.rs` | state flow、回调消费、driver 调用、邮箱补验分支 |
| 账号解析 | `src/services/external_auth_service/resolution.rs` | 既有身份匹配、已验证邮箱自动绑定、自动创建本地用户 |
| 邮箱补验 | `src/services/external_auth_service/verification.rs` | 临时 flow、邮件发送、确认后继续登录 |
| 密码绑定 | `src/services/external_auth_service/password_link.rs` | 用户输入本地密码后绑定外部身份 |
| Driver trait | `src/external_auth/driver.rs` | provider driver 统一接口和 descriptor |
| Driver 注册表 | `src/external_auth/registry.rs` | 注册 `oidc`、`generic_oauth2` 和 `github` |
| OIDC driver | `src/external_auth/providers/oidc.rs` | discovery、PKCE、nonce、ID Token 校验 |
| Generic OAuth2 driver | `src/external_auth/providers/oauth2.rs` | 手动 endpoint、PKCE、token exchange、UserInfo claim 映射 |
| GitHub driver | `src/external_auth/providers/github.rs` | 复用 OAuth2 driver，固定 GitHub endpoint，并从 `/user/emails` 读取已验证主邮箱 |

持久化表来自 `migration/src/m20260517_000001_add_external_auth.rs`：

- `external_auth_providers`
- `external_auth_identities`
- `external_auth_login_flows`
- `external_auth_email_verification_flows`

临时登录 flow 的 TTL 是 300 秒；邮箱补验 flow 的 TTL 是 1800 秒。过期清理由 primary 后台任务 `external-auth-flow-cleanup` 执行。

## Provider descriptor

每个 driver 通过 `ExternalAuthProviderDescriptor` 暴露能力，管理端 `GET /admin/external-auth/provider-kinds` 直接返回这些信息。前端据此决定字段是否必填、是否显示手动 endpoint、默认 scope 和 claim 区域。

当前内置 kind：

| kind | protocol | 默认 scope | endpoint 来源 |
| --- | --- | --- | --- |
| `oidc` | `oidc` | `openid email profile` | `issuer_url` discovery |
| `generic_oauth2` | `oauth2` | `openid email profile` | 管理员手动填写 authorization / token / userinfo URL |
| `github` | `oauth2` | `read:user user:email` | GitHub 固定 authorization / token / user / user emails URL |

新增 provider kind 时，不要在前端写死能力。应先实现 driver descriptor，再让前端消费 `/admin/external-auth/provider-kinds`。

## 登录流程

1. 登录页调用 `GET /auth/external-auth/providers`，只拿启用 provider 的公开摘要。
2. 前端调用 `POST /auth/external-auth/{kind}/{provider}/start`，可传 `return_path`。
3. 服务端规范化 provider key，加载 provider，计算 callback redirect URI。
4. Driver 生成授权 URL、state、PKCE verifier；OIDC 还会生成 nonce。
5. 服务端把 state hash、nonce、PKCE verifier、redirect URI 和 return path 写入 `external_auth_login_flows`。
6. 用户在身份提供商授权后回调 `/auth/external-auth/{kind}/{provider}/callback`。
7. 服务端按 state hash 原子消费 flow，校验 kind / provider 是否匹配，再调用 driver exchange。
8. Driver 返回 `ExternalAuthProfile`，服务层按 `identity_namespace + subject` 解析本地用户。
9. 找到或创建本地用户后，走 `mfa_service::complete_primary_login_or_start_mfa()`。
10. 不需要 MFA 时写 Cookie 并重定向；需要 MFA 时重定向到登录页继续 challenge。

回调错误不会直接输出 JSON，而是重定向回登录页，并记录 `external auth callback failed` warn 日志。

## OIDC driver

`oidc` 使用 `openidconnect` crate：

- `issuer_url` 必填
- authorization endpoint、token endpoint、JWKS 从 discovery 获取
- 授权码流程使用 PKCE S256
- 生成并校验 nonce
- token response 必须包含 ID Token
- 使用 ID Token verifier 校验 claims
- `identity_namespace` 来自 ID Token issuer，必须等于 provider `issuer_url`
- subject 来自 ID Token subject
- email、email_verified、name、preferred_username 从 ID Token claims 读取

OIDC scope 保存时会自动保证 `openid` 存在。driver 发起授权请求时会跳过手动添加 `openid`，因为 `openidconnect` 的 authentication flow 会处理这个基础 scope。

## Generic OAuth2 driver

`generic_oauth2` 面向只有 OAuth2 authorization-code + UserInfo 的 provider：

- `authorization_url`、`token_url`、`userinfo_url` 必填
- `issuer_url` 可选；存在时作为 `identity_namespace`
- 未配置 `issuer_url` 时，`identity_namespace` 使用 authorization URL 的 origin
- 授权码流程使用 PKCE S256
- 回调后先换 access token，再用 Bearer token 请求 UserInfo JSON
- 不做 discovery、JWKS、ID Token 或 nonce 校验

UserInfo claim 默认：

| 字段 | 默认 claim | 备注 |
| --- | --- | --- |
| `subject` | `sub`，缺失时回退 `id` | 必填 |
| `email` | `email` | 存在时必须通过本地邮箱格式校验 |
| `email_verified` | `email_verified` | 缺失时为 `false` |
| `display_name` | `name` | 会清理控制字符并截断 |
| `preferred_username` | `preferred_username` | 会清理控制字符并截断 |

自定义 claim 支持顶层 key、点路径和 JSON Pointer，例如 `email`、`user.email`、`/user/email`。

## GitHub driver

`github` 是专用 provider kind，wire value 固定为 `github`，不要使用 Rust enum 派生出来的 `git_hub`。它采用 storage driver 中 S3-compatible / Tencent COS 类似的模式：复用通用 OAuth2 driver 的授权发起、token exchange、UserInfo 读取和 claim 映射，再覆盖 GitHub 固定配置和邮箱语义。

固定行为：

- protocol 是 `oauth2`
- authorization URL 固定为 `https://github.com/login/oauth/authorize`
- token URL 固定为 `https://github.com/login/oauth/access_token`
- userinfo URL 固定为 `https://api.github.com/user`
- user emails URL 从 userinfo URL 派生为 `/user/emails`
- 默认 scope 是 `read:user user:email`
- subject 从 `/user.id` 读取
- username 从 `/user.login` 读取
- display name 从 `/user.name` 读取
- 不信任 `/user.email`
- 只接受 `/user/emails` 中 `primary=true` 且 `verified=true` 的邮箱

如果 GitHub 没有返回已验证主邮箱，driver 返回的 `email=None`、`email_verified=false`。登录服务层有一个 GitHub 专用边界：当 provider 开启 `require_email_verified` 且没有已验证主邮箱时，直接返回 forbidden，不进入本地邮箱补验流程，避免把 GitHub 邮箱验证语义降级成本地补验。

前端后台对 GitHub 做了特异性 UI：

- 创建 / 编辑时展示固定端点说明，不展示可编辑 endpoint 字段
- 规则面板展示固定 claim，不展示可编辑 claim mapping
- 默认图标使用 `/static/external-auth/github-logo.svg`
- 登录入口、后台列表和 `settings/security` 外部身份列表都会优先显示后台配置的 icon，失败后回退到 provider kind 默认 icon

## Token exchange 约束

Generic OAuth2 的 token exchange 只能请求一次。authorization code 是一次性凭据，不能先试 `client_secret_basic` 失败后再用同一个 code 试 `client_secret_post`。

当前行为：

- 有 `client_secret`：只使用 `client_secret_post`
- 无 `client_secret`：只按 public client 发送 `client_id`
- 失败时返回这一次请求的错误，不做 fallback retry

如果要支持 `client_secret_basic`，应新增 provider 级显式配置，例如 `client_auth_method`，并在创建 / 更新 / 前端表单 / OpenAPI / 测试中一起落地。不要恢复“探测式重试”。

## URL、scope 和 secret 规范化

规范化在 `src/services/external_auth_service/normalize.rs`：

- provider key 只允许小写字母、数字、短横线，长度 2-64
- provider endpoint 必须是 HTTPS，localhost / loopback HTTP 例外
- URL 不允许 fragment
- icon URL 可以是根相对路径或 HTTPS URL，localhost HTTP 例外
- Client Secret 创建时空字符串视为未配置；更新时 `***REDACTED***` 表示保留旧值
- scope 去重、去空项，单个 scope 最长 128 字节且不能有控制字符
- OIDC scope 会自动补 `openid`

Generic OAuth2 默认 scope 也是 `openid email profile`，但不会在更新时额外强制补 `openid`；它使用 driver descriptor 的默认值处理空 scope。

## 账号解析策略

账号解析在 `resolution.rs`。顺序是：

1. 按 `identity_namespace + subject` 查找已绑定外部身份。
2. 如果 provider 要求已验证邮箱，必须有 email 且 `email_verified=true`。
3. 若启用按已验证邮箱自动绑定，并且 provider 返回 verified email，查找本地同邮箱用户并创建外部身份绑定。
4. 若启用自动创建用户，检查公开注册开关、邮箱、邮箱域名和邮箱验证策略，再创建普通用户和外部身份绑定。
5. 如果无法直接解析，创建邮箱补验 flow 或要求用户输入本地账号密码绑定。

自动创建用户时会生成随机内部密码，用户仍可后续通过本地密码重置 / 改密等流程管理账号。

注意 GitHub 的 `require_email_verified` 缺失邮箱拒绝逻辑位于 `login.rs`，不是通用 `resolution.rs` 策略。新增类似 provider 时要明确它的“外部邮箱验证”是否允许被本地邮箱补验替代。

## API 文档入口

- 管理端 provider API：`./api/admin.md#外部认证提供商`
- 登录端外部认证 API：`./api/auth.md#外部认证`
- 面向部署者的配置说明：`../../docs/config/external-auth.md`

## 测试

重点测试：

- `cargo test --test test_oauth2`
- `cargo test --lib oauth2`
- `cargo test --lib external_auth::providers::github`
- `cargo clippy --lib --tests -- -D warnings`

相关 mock 在 `tests/external_auth/oauth2/mock.rs`。前端 provider kind、默认 scope、表单和陈旧请求保护相关测试在：

- `frontend-panel/src/pages/admin/AdminExternalAuthPage.test.tsx`
- `frontend-panel/src/components/admin/admin-external-auth-page/*.test.tsx`

改 driver 行为时至少跑后端 OAuth2 / OIDC 相关测试；改管理端 UI 时跑上述前端测试和 `bun run check`。
GitHub 相关边界要覆盖 `/user/emails` 成功、无已验证主邮箱、`/user.email` 不能绕过、非法邮箱、emails API 失败、`require_email_verified` 缺失邮箱拒绝。

## 已知限制

- Generic OAuth2 当前没有显式 client auth method 配置，只支持 public client 和 `client_secret_post`。
- Generic OAuth2 不校验 ID Token，因为它只消费 access token + UserInfo。
- Microsoft / Google 当前可通过通用 OIDC 手动配置；Microsoft 手动配置应优先使用具体 tenant issuer，因为当前 OIDC driver 会严格要求 ID Token issuer 等于 provider `issuer_url`。Microsoft / Google 专用预设分别见 <https://github.com/AptS-1547/AsterDrive/issues/263> 和 <https://github.com/AptS-1547/AsterDrive/issues/265>。
- `groups_claim` 和 `avatar_url_claim` 已进入 provider 配置模型，但当前登录解析只落地身份、邮箱、显示名和用户名快照。
