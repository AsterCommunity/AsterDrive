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
| Driver 注册表 | `src/external_auth/registry.rs` | 注册 `oidc`、`generic_oauth2`、`github`、`qq`、`google` 和 `microsoft` |
| OIDC driver | `src/external_auth/providers/oidc.rs` | discovery、PKCE、nonce、ID Token 校验 |
| Generic OAuth2 driver | `src/external_auth/providers/oauth2.rs` | 手动 endpoint、PKCE、token exchange、UserInfo claim 映射 |
| GitHub driver | `src/external_auth/providers/github.rs` | 复用 OAuth2 driver，固定 GitHub endpoint，并从 `/user/emails` 读取已验证主邮箱 |
| QQ driver | `src/external_auth/providers/qq.rs` | QQ 互联 OAuth2 专用流程：GET token、获取 openid、再调用 get_user_info |
| Google driver | `src/external_auth/providers/google.rs` | 复用 OIDC driver，固定 Google Accounts issuer、默认 scope 和 claim 语义 |
| Microsoft driver | `src/external_auth/providers/microsoft.rs` | 复用 OIDC driver，支持 Microsoft tenant / issuer 规范化和多租户 issuer 校验 |

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
| `qq` | `oauth2` | `get_user_info` | QQ 固定 authorization / token / openid / get_user_info URL |
| `google` | `oidc` | `openid profile email` | Google Accounts 固定 issuer / discovery |
| `microsoft` | `oidc` | `openid profile email` | Microsoft tenant 派生 issuer / discovery |

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

## Google driver

`google` 是专用 provider kind，wire value 固定为 `google`。它采用和 GitHub provider 类似的“专用封装 + 通用 driver”模式，但底层复用的是 OIDC driver，而不是 OAuth2 driver。

固定行为：

- protocol 是 `oidc`
- issuer 默认固定为 `https://accounts.google.com`
- discovery 固定来自 `https://accounts.google.com/.well-known/openid-configuration`
- 默认 scope 是 `openid profile email`
- subject 固定使用 ID Token `sub`
- display name 固定使用 ID Token `name`
- email 固定使用 ID Token `email`
- email verified 固定使用 ID Token `email_verified`
- avatar URL claim 预设为 ID Token `picture`
- authorization / token / userinfo 手动 endpoint 不支持

Google 专用 provider 仍允许测试传入 loopback issuer，以便集成测试使用本地 mock OIDC server；生产 UI 不展示 issuer 输入。外部身份绑定必须依赖稳定的 `sub`，不要把 email 作为主键。Google API / Google Drive 授权属于后续资源访问能力，不应混进登录 provider 默认 scope。

前端后台对 Google 做了特异性 UI：

- 创建 / 编辑时展示固定 issuer 和 discovery，不展示可编辑 issuer / endpoint 字段
- 规则面板展示固定 claim，不展示可编辑 claim mapping
- 默认图标使用 `/static/external-auth/google-logo.svg`
- 登录入口、后台列表和 `settings/security` 外部身份列表都会优先显示后台配置的 icon，失败后回退到 provider kind 默认 icon

## Microsoft driver

`microsoft` 是专用 provider kind，wire value 固定为 `microsoft`。它按 OIDC 登录实现，底层复用通用 OIDC driver 的授权发起、discovery、PKCE、nonce 和 ID Token 校验，但为 Microsoft identity platform 补充 tenant / issuer 语义。

固定行为：

- protocol 是 `oidc`
- 默认 tenant 是 `common`
- tenant 可填具体 tenant ID、`common`、`organizations` 或 `consumers`
- issuer 规范化为 `https://login.microsoftonline.com/{tenant}/v2.0`
- discovery 固定为 `https://login.microsoftonline.com/{tenant}/v2.0/.well-known/openid-configuration`，不要用 URL join 误退到 v1 metadata
- 默认 scope 是 `openid profile email`
- subject 固定使用 ID Token `sub`
- display name 固定使用 ID Token `name`
- email 固定使用 ID Token `email`
- 不声明 `email_verified`，也不把 `email` 当 GitHub 式已验证主邮箱
- authorization / token / userinfo 手动 endpoint 不支持
- `require_email_verified` 默认关闭

Microsoft 多租户入口的 v2 discovery issuer 可能包含 tenant 模板，而真实 ID Token issuer 会落到具体 tenant。Microsoft driver 的 callback exchange 会复用 `openidconnect` 做签名、audience、nonce、过期时间等校验，同时对 issuer 做 Microsoft 专用校验：具体 tenant 要求 issuer 精确匹配；`common` 接受组织和个人账号租户；`organizations` 不接受 Microsoft Account 固定租户；`consumers` 只接受 Microsoft Account 固定租户 `9188040d-6c67-4c5b-b112-36a304b66dad`。

Microsoft App Registration 侧应添加 Web redirect URI，因为 AsterDrive 后端执行 authorization code token exchange。不能把 callback URL 配到 Public client/native 平台后再发送 Client Secret，否则会触发 `AADSTS90023`。Client Secret 必须保存 Azure / Entra 创建 secret 时显示的 `Value`，不是 `Secret ID`；`AADSTS7000215` 通常就是填错了这个值。

Microsoft 的邮箱 claim 可能缺失，且不应默认视为已验证邮箱。无法直接解析本地账号时，登录服务继续走现有邮箱补验 / 密码绑定流程。

前端后台对 Microsoft 做了特异性 UI：

- 创建 / 编辑时展示 tenant 输入和派生 issuer / discovery，不展示可编辑 issuer / endpoint 字段
- 规则面板展示固定 claim，不展示可编辑 claim mapping
- 默认图标使用 `/static/external-auth/microsoft-logo.svg`
- 登录入口、后台列表和 `settings/security` 外部身份列表都会优先显示后台配置的 icon，失败后回退到 provider kind 默认 icon

## QQ driver

`qq` 是 QQ 互联 OAuth2 专用 provider kind，wire value 固定为 `qq`。它不能复用 Generic OAuth2 的“POST token -> Bearer UserInfo -> 直接读 subject/email”模型，因为 QQ 需要先拿 access token，再请求 `/oauth2.0/me` 获取 `openid`，最后用 `access_token`、`oauth_consumer_key` 和 `openid` 请求 `get_user_info`。

固定行为：

- protocol 是 `oauth2`
- authorization URL 固定为 `https://graph.qq.com/oauth2.0/authorize`
- token URL 固定为 `https://graph.qq.com/oauth2.0/token`
- openid URL 固定为 `https://graph.qq.com/oauth2.0/me`
- userinfo URL 固定为 `https://graph.qq.com/user/get_user_info`
- 默认 scope 是 `get_user_info`
- token exchange 按 QQ 文档使用 GET，并显式带 `fmt=json`
- openid 请求显式带 `fmt=json`，避免 QQ 默认 JSONP
- subject 固定使用 `openid`
- identity namespace 使用 `qq:{client_id}`，避免不同 QQ App ID 的 openid 混用
- display name 使用 `get_user_info.nickname`
- 不返回 email，也不声明 `email_verified`
- `require_email_verified` 默认关闭

QQ 回调缺邮箱时走现有缺邮箱分支：先尝试已有外部身份绑定；未绑定时进入邮箱补验 / 密码绑定流程。QQ provider 不应该触发 verified-email auto-link。

前端后台对 QQ 做了特异性 UI：

- 创建 / 编辑时展示固定授权、Token、OpenID 和 get_user_info endpoint，不展示可编辑 endpoint 字段
- 规则面板展示固定 claim，不展示可编辑 claim mapping
- 默认图标使用 `/static/external-auth/qq-logo.svg`
- 登录入口、后台列表和 `settings/security` 外部身份列表都会优先显示后台配置的 icon，失败后回退到 provider kind 默认 icon

## Provider 应用申请入口

面向部署者的配置文档需要说明每类 provider 在哪里申请应用 / Client ID：

- OIDC / Generic OAuth2：对应 IdP 管理后台的 Applications / Clients 页面，例如 Logto、Authentik、Keycloak、Zitadel。
- Logto 示例：Logto Cloud Console <https://cloud.logto.io/>；自托管为 `https://<logto-host>/console`。
- GitHub：个人账号 <https://github.com/settings/developers>；组织为 `https://github.com/organizations/{org}/settings/applications`。
- QQ：QQ 互联管理中心 <https://connect.qq.com/manage.html>，创建网站应用后登记 AsterDrive callback URL。
- Google：Google Cloud Console Credentials <https://console.cloud.google.com/apis/credentials>，创建 OAuth client ID -> 选择 Web 应用 -> 在 Authorized redirect URIs 中添加 AsterDrive callback URL；创建后复制 client secret 的 `Value`。
- Microsoft：Microsoft Entra admin center <https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade> 或 Azure portal <https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade>，App registrations 创建应用 -> Authentication -> Add a platform -> Web -> 在 Redirect URI 填入 AsterDrive callback URL；在 Certificates & secrets 创建 client secret 后复制 `Value`。

所有入口都应使用 AsterDrive 生成的 `/api/v1/auth/external-auth/{kind}/{provider}/callback` 作为 redirect URI。

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
- `cargo test --test test_oidc`
- `cargo test --lib oauth2`
- `cargo test --lib external_auth::providers::github`
- `cargo test --lib external_auth::providers::google`
- `cargo test --lib external_auth::providers::microsoft`
- `cargo clippy --lib --tests -- -D warnings`

相关 mock 在 `tests/external_auth/oauth2/mock.rs`。前端 provider kind、默认 scope、表单和陈旧请求保护相关测试在：

- `frontend-panel/src/pages/admin/AdminExternalAuthPage.test.tsx`
- `frontend-panel/src/components/admin/admin-external-auth-page/*.test.tsx`

改 driver 行为时至少跑后端 OAuth2 / OIDC 相关测试；改管理端 UI 时跑上述前端测试和 `bun run check`。
GitHub 相关边界要覆盖 `/user/emails` 成功、无已验证主邮箱、`/user.email` 不能绕过、非法邮箱、emails API 失败、`require_email_verified` 缺失邮箱拒绝。
Google 相关边界要覆盖 descriptor 默认值、固定 discovery / issuer、`sub` 作为稳定身份、邮箱变化不新建身份、`email_verified=false`、缺失、非布尔值不能绕过验证。
Microsoft 相关边界要覆盖 descriptor 默认值、tenant / issuer 规范化、多租户 issuer 校验、具体 tenant issuer 精确匹配、邮箱缺失进入本地邮箱补验流程，以及默认不要求已验证邮箱。

## 已知限制

- Generic OAuth2 当前没有显式 client auth method 配置，只支持 public client 和 `client_secret_post`。
- Generic OAuth2 不校验 ID Token，因为它只消费 access token + UserInfo。
- `groups_claim` 和 `avatar_url_claim` 已进入 provider 配置模型，但当前登录解析只落地身份、邮箱、显示名和用户名快照。
