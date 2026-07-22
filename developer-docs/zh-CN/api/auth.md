# 认证 API

以下路径都相对于 `/api/v1`。

## 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/auth/check` | 返回公开认证状态（系统是否已初始化、是否允许公开注册） |
| `POST` | `/auth/setup` | 初始化系统并创建首个管理员 |
| `POST` | `/auth/register` | 系统初始化后注册普通用户 |
| `POST` | `/auth/register/resend` | 重发注册激活邮件 |
| `GET` | `/auth/invitations/{token}` | 校验用户邀请并读取邮箱、过期时间 |
| `POST` | `/auth/invitations/{token}/accept` | 接受邀请并创建普通用户 |
| `GET` | `/auth/contact-verification/confirm` | 消费邮箱验证 token 并重定向前端 |
| `POST` | `/auth/password/reset/request` | 请求密码重置邮件 |
| `POST` | `/auth/password/reset/confirm` | 使用 token 完成密码重置 |
| `POST` | `/auth/login` | 登录并写入认证 Cookie |
| `POST` | `/auth/mfa/challenge/verify` | 完成 MFA 二次验证并写入认证 Cookie |
| `POST` | `/auth/mfa/challenge/email-code/send` | 为当前 MFA 登录 flow 发送邮箱验证码 |
| `POST` | `/auth/passkeys/login/start` | 发起 WebAuthn Passkey 登录挑战 |
| `POST` | `/auth/passkeys/login/finish` | 完成 Passkey 登录并写入认证 Cookie |
| `GET` | `/auth/external-auth/providers` | 列出匿名态可用的外部认证提供商 |
| `POST` | `/auth/external-auth/{kind}/{provider}/start` | 发起外部认证登录 |
| `GET` | `/auth/external-auth/{kind}/{provider}/callback` | 外部认证回调入口 |
| `POST` | `/auth/external-auth/email-verification/start` | 为外部认证补验邮箱发送邮件 |
| `GET` | `/auth/external-auth/email-verification/confirm` | 完成外部认证邮箱补验并重定向前端 |
| `POST` | `/auth/external-auth/password-link` | 用本地密码把外部身份绑定到已有账号 |
| `POST` | `/auth/refresh` | 使用 refresh Cookie 轮换 access/refresh token |
| `POST` | `/auth/logout` | 清除认证 Cookie |
| `GET` | `/auth/me` | 读取当前登录用户信息 |
| `GET` | `/auth/sessions` | 列出当前用户的活跃登录会话 |
| `DELETE` | `/auth/sessions/others` | 吊销除当前 refresh session 外的其他会话 |
| `DELETE` | `/auth/sessions/{id}` | 吊销指定登录会话 |
| `PUT` | `/auth/password` | 修改当前用户密码 |
| `GET` | `/auth/mfa` | 读取当前用户 MFA 状态 |
| `POST` | `/auth/mfa/totp/setup/start` | 发起 TOTP MFA 设置流程 |
| `POST` | `/auth/mfa/totp/setup/finish` | 校验 TOTP 并启用 MFA |
| `DELETE` | `/auth/mfa/factors/{id}` | 删除当前用户 MFA 因子 |
| `POST` | `/auth/mfa/recovery-codes/regenerate` | 重新生成 MFA 恢复码 |
| `GET` | `/auth/passkeys` | 列出当前用户已注册的 Passkey |
| `POST` | `/auth/passkeys/register/start` | 发起 Passkey 注册挑战 |
| `POST` | `/auth/passkeys/register/finish` | 完成 Passkey 注册 |
| `PATCH` | `/auth/passkeys/{id}` | 重命名 Passkey |
| `DELETE` | `/auth/passkeys/{id}` | 删除 Passkey |
| `GET` | `/auth/external-auth/links` | 列出当前用户绑定的外部认证身份 |
| `DELETE` | `/auth/external-auth/links/{id}` | 解绑外部认证身份 |
| `POST` | `/auth/email/change` | 请求变更当前登录用户邮箱 |
| `POST` | `/auth/email/change/resend` | 重发邮箱变更确认邮件 |
| `PATCH` | `/auth/preferences` | 更新当前用户偏好设置 |
| `PATCH` | `/auth/profile` | 更新当前用户资料 |
| `POST` | `/auth/profile/avatar/upload` | 上传头像图片 |
| `PUT` | `/auth/profile/avatar/source` | 切换头像来源 |
| `GET` | `/auth/events/storage` | 订阅当前用户可见工作空间的存储变更事件 |
| `GET` | `/auth/profile/avatar/{size}` | 读取当前用户已上传头像 |

## 初始化与注册

- `POST /auth/check`：返回 `has_users` 和 `allow_user_registration`，只用于判断实例处于初始化、登录还是“关闭公开注册”的大状态，不会公开暴露账号是否存在
  这条接口当前不需要请求体。
- `POST /auth/setup`：创建首个管理员的唯一入口，仅在系统还没有任何用户时可用
- `POST /auth/register`：必须在系统完成初始化后使用，并且只会创建普通用户；初始化完成且 `auth_allow_user_registration = true` 时可用，新用户默认配额来自 `default_storage_quota`
- `POST /auth/register/resend`：对“尚未完成激活”的账号重发确认邮件，请求体如下：

```json
{
  "identifier": "admin@example.com"
}
```

公开请求的重发与找回流程都会做最短响应时间填充，尽量避免把账号存在性直接暴露给外部。

本地账号邮箱策略由两个运行时配置控制：

- `auth_local_email_allowlist`：允许注册 / 绑定的精确规范化邮箱地址或精确 ASCII 域名；空数组表示不启用 allowlist 限制
- `auth_local_email_blocklist`：禁止注册 / 绑定的精确规范化邮箱地址或精确 ASCII 域名；blocklist 优先于 allowlist

这两个策略只作用于本地注册和本地账号邮箱变更。国际化域名必须按 punycode 写入。它们不是 CORS 白名单，也不是外部认证 provider 的域名限制。

系统尚未初始化时，`/auth/register` 固定返回 `400` 和 `validation.system_not_initialized`，不会代替 `/auth/setup` 创建管理员。

系统初始化完成后，如果运营方关闭了 `auth_allow_user_registration`：

- `/auth/register` 会返回 `403`
- `/auth/setup` 会继续返回“系统已经初始化”

`/auth/setup` 和 `/auth/register` 的请求体相同：

```json
{
  "username": "admin",
  "email": "admin@example.com",
  "password": "password"
}
```

### 邀请注册

邀请接口是公开 auth 端点，不依赖现有登录态：

- `GET /auth/invitations/{token}` 校验 token，成功返回邀请中的 `email` 和 `expires_at`
- `POST /auth/invitations/{token}/accept` 接收下面的账号凭据，成功返回 `201` 并创建普通用户

```json
{
  "username": "new-user",
  "password": "password"
}
```

邮箱来自 invitation 记录，客户端不提交、也不能替换。邀请是一次性状态机：`pending` 可以被接受或撤销，过期后为 `expired`，成功接受后为 `accepted`。同一 token 不能重复创建用户。

稳定错误码：

- `auth.invitation_invalid`
- `auth.invitation_expired`
- `auth.invitation_revoked`
- `auth.invitation_accepted`

前端邀请页应把这两条接口当成匿名流程，不要在 `401` 后启动普通登录态的 refresh-token 轮换。

## 登录态

`POST /auth/login` 使用下面的请求体：

```json
{
  "identifier": "admin",
  "password": "password"
}
```

成功后会写入两个 HttpOnly Cookie：

- `aster_access`
- `aster_refresh`

其中 `aster_refresh` 的 Cookie Path 是 `/api/v1/auth`，会随 `/api/v1/auth/*` 下的请求一起发送。

相关接口：

- `POST /auth/refresh`：读取 refresh Cookie，原子消费旧 refresh token，签发新的 access/refresh token；旧 refresh token 再次使用会被视为复用攻击并撤销该用户全部会话
- `POST /auth/logout`：清除两个认证 Cookie，并吊销当前 refresh token
- `GET /auth/me`：既支持 Cookie，也支持 `Authorization: Bearer <jwt>`
- `GET /auth/sessions`：列出当前用户的活跃登录设备 / 会话；如果请求带当前 refresh Cookie，会标记当前会话
- `DELETE /auth/sessions/others`：要求当前请求能定位到 refresh session，只吊销其他会话
- `DELETE /auth/sessions/{id}`：吊销指定会话；如果删的是当前会话，响应会同时清理认证 Cookie

如果用户状态是 `disabled`，登录会直接失败。

`GET /auth/me` 支持用 `fields` query 返回局部资料，例如 `GET /auth/me?fields=profile,preferences,quota,session`。支持的字段组是 `profile`、`preferences`、`quota`、`session`；不传或传空值时返回完整模型，传未知字段会返回 `400`。

如果用户的 `must_change_password` 标记为 `true`，成功完成登录后不会直接获得普通应用访问权限，而是返回：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "password_change_required",
    "expires_in": 900
  }
}
```

密码登录、MFA 验证完成、Passkey 登录完成和外部认证登录完成都可能返回这个状态。响应仍会写入认证 Cookie，但 access token 只允许走强制改密流程。在 `PUT /auth/password` 成功之前，仅允许访问：

- `GET /auth/me`
- `PUT /auth/password`
- `POST /auth/logout`

其他已认证接口会返回 `403` 和 `auth.password_change_required`。用户仍处于 `must_change_password` 状态或 token 带改密 scope 时，`POST /auth/refresh` 也会被拒绝。`PUT /auth/password` 仍然会校验 `current_password`，成功后清除 `must_change_password`、轮换会话，并写入正常认证 Cookie。

如果用户启用了 TOTP，或者邮箱验证码 MFA 策略对该已验证邮箱用户可用，`POST /auth/login` 不会立即写入认证 Cookie，而是返回：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "mfa_required",
    "flow_token": "mfa_xxx",
    "expires_in": 300,
    "methods": ["totp", "recovery_code", "email_code"]
  }
}
```

`methods` 是当前登录 flow 实际可用的验证方式，不是固定列表：

- `totp`：用户已启用 TOTP factor
- `recovery_code`：用户已启用 TOTP，且还有未使用的恢复码
- `email_code`：用户邮箱已验证，`auth_email_code_login_enabled = true`，且 SMTP 发信配置可用；如果用户已经启用 TOTP，还要求 `auth_email_code_login_allow_totp_fallback = true`

随后前端调用 `POST /auth/mfa/challenge/verify`：

```json
{
  "flow_token": "mfa_xxx",
  "method": "totp",
  "code": "123456"
}
```

`method` 当前支持：

- `totp`
- `recovery_code`
- `email_code`

使用 `email_code` 前必须先调用 `POST /auth/mfa/challenge/email-code/send`：

```json
{
  "flow_token": "mfa_xxx"
}
```

成功后服务端会给用户已验证邮箱发送 8 位数字验证码，并返回：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "expires_in": 300,
    "resend_after": 60
  }
}
```

邮箱验证码的最长有效期来自 `auth_email_code_login_ttl_secs`，但实际不会超过当前 MFA flow 剩余时间；重发冷却来自 `auth_email_code_login_resend_cooldown_secs`。如果还没请求验证码就用 `method = "email_code"` 验证，会返回 `auth.mfa_email_code_required` 子码；验证码过期会返回 `auth.mfa_email_code_expired` 子码。

验证成功后响应形状和普通登录成功一样：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "authenticated",
    "expires_in": 900
  }
}
```

同时会写入 `aster_access`、`aster_refresh` 和 CSRF Cookie。MFA 登录 flow 默认 5 分钟过期，最多允许 5 次错误尝试；过期、被消费或超过尝试次数后都必须重新完成第一因子登录。

## Passkey 登录与管理

Passkey 使用 WebAuthn 两段式流程。所有 challenge 响应和 credential 请求体都保持 WebAuthn 原始 JSON 结构，由浏览器的 `navigator.credentials.*` 直接消费或回传。

登录流程：

- `POST /auth/passkeys/login/start`：请求体可传 `{ "identifier": "alice", "conditional": false }`；`identifier` 可省略，用于 conditional UI / discoverable credential 场景
- `POST /auth/passkeys/login/finish`：请求体是 `{ "flow_id": "...", "credential": { ... } }`；成功后和密码登录一样写入 `aster_access`、`aster_refresh` 和 CSRF Cookie

注册和管理流程需要已登录：

- `GET /auth/passkeys`：返回当前用户的 Passkey 列表
- `POST /auth/passkeys/register/start`：请求体可传 `{ "name": "MacBook Touch ID" }`
- `POST /auth/passkeys/register/finish`：请求体是 `{ "flow_id": "...", "credential": { ... }, "name": "MacBook Touch ID" }`
- `PATCH /auth/passkeys/{id}`：请求体是 `{ "name": "New name" }`
- `DELETE /auth/passkeys/{id}`：删除当前用户自己的 Passkey

当前 Passkey 记录保存在 `passkeys` 表，credential 以强类型包装后的 JSON 存储。服务端要求可发现凭证；不支持的 credential 会返回带 `passkey.*` 子码的校验错误。

`auth_passkey_login_enabled = false` 会关闭匿名 Passkey 登录，并让当前前端启动配置隐藏 Passkey 登录入口，但不会删除已经注册的凭证。已登录用户仍可管理自己保存的 Passkey。

## MFA 管理

MFA 自助管理接口都需要已登录。当前持久化因子只支持 TOTP；登录挑战允许用 TOTP、一次性恢复码或邮箱验证码完成。邮箱验证码不是持久化 factor，只在登录 flow 中按需发送并记录到 `mfa_email_codes` 表。

`GET /auth/mfa` 返回当前用户 MFA 状态：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "enabled": true,
    "factors": [
      {
        "id": 7,
        "method": "totp",
        "name": "Authenticator app",
        "enabled_at": "2026-05-24T12:00:00Z",
        "last_used_at": null
      }
    ],
    "recovery_codes_remaining": 10
  }
}
```

启用 TOTP 是两段式流程：

- `POST /auth/mfa/totp/setup/start`：返回 `flow_token`、`expires_in`、Base32 `secret` 和 `otpauth_uri`
- `POST /auth/mfa/totp/setup/finish`：请求体是 `{ "flow_token": "...", "code": "123456", "name": "Phone" }`

完成设置成功后返回新 factor 和一组恢复码：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "factor": {
      "id": 7,
      "method": "totp",
      "name": "Phone",
      "enabled_at": "2026-05-24T12:00:00Z",
      "last_used_at": null
    },
    "recovery_codes": ["ABCD-EFGH-IJKL"]
  }
}
```

删除因子和重新生成恢复码都属于敏感操作，必须带一个当前可用的 MFA code：

```json
{
  "code": "123456"
}
```

`code` 可以是 TOTP，也可以是未使用过的恢复码。删除最后一个 TOTP factor 会同时清理该用户的恢复码、待处理 MFA 登录 flow、待处理邮箱验证码和待处理 TOTP setup flow；重新生成恢复码会替换旧恢复码，旧码立即失效。

## 外部认证

外部认证当前支持的 provider kind 是 `oidc`、`generic_oauth2`、`github`、`qq`、`google` 和 `microsoft`，管理端通过 `/admin/external-auth/*` 配置。匿名登录页先调用 `GET /auth/external-auth/providers` 读取启用中的 provider：

```json
{
  "code": "success",
  "msg": "",
  "data": [
    {
      "key": "corp",
      "kind": "oidc",
      "display_name": "Corp SSO",
      "icon_url": "/static/external-auth/corp.svg"
    }
  ]
}
```

登录流程：

- `POST /auth/external-auth/{kind}/{provider}/start`：请求体可传 `{ "return_path": "/files" }`，返回 `authorization_url`
- 浏览器跳到 `authorization_url` 后，外部 provider 回调 `GET /auth/external-auth/{kind}/{provider}/callback`
- 如果账号未启用 MFA，回调成功时服务端写入认证 Cookie，并 `302` 到 `return_path`
- 如果账号需要 MFA（已启用 TOTP，或邮箱验证码 MFA 策略对该账号可用），回调会先创建 MFA 登录 flow，并重定向到登录页携带 MFA challenge 信息；前端继续调用 `POST /auth/mfa/challenge/verify` 完成二次验证

`oidc` driver 使用 discovery、PKCE、nonce 和 ID Token 校验；`generic_oauth2` driver 使用手动 endpoint、PKCE、token exchange 和 UserInfo claim 映射。`github`、`qq`、`google` 和 `microsoft` 是专用 provider kind，端点和默认 claim 语义由后端固定，不应在请求里传手动 OAuth endpoint；Microsoft 租户配置放在 `options.microsoft.tenant`。更完整的模块说明见 [外部认证模块](../design/external-auth.md)。

如果 provider 返回的身份缺少可用邮箱，而当前策略需要邮箱确认，回调会重定向到登录页并带上 `external_auth=email_required` 和 `flow`。随后前端使用：

- `POST /auth/external-auth/email-verification/start`：请求体 `{ "flow_token": "...", "email": "alice@example.com" }`
- `GET /auth/external-auth/email-verification/confirm?token=...`：消费邮件里的 token，成功后写 Cookie 并重定向

如果外部身份需要绑定已有本地账号，可以调用：

```json
{
  "flow_token": "...",
  "identifier": "alice",
  "password": "local-password"
}
```

对应接口是 `POST /auth/external-auth/password-link`，成功后同样写入认证 Cookie。

登录后用户可管理自己的外部身份绑定：

- `GET /auth/external-auth/links`
- `DELETE /auth/external-auth/links/{id}`

外部认证临时 login flow 和 email verification flow 会由 primary 后台的 `external-auth-flow-cleanup` 周期任务清理。

## Cookie 写操作的 CSRF 来源判断

使用 Cookie 鉴权执行不安全方法时，服务端同时检查双提交 CSRF token 和请求来源：

- `same-origin` 请求允许继续做 CSRF token 校验
- `same-site` 请求必须带可信 `Origin` 或 `Referer`
- 可信来源必须精确匹配当前请求来源，或命中 `public_site_url` 列表中的某个 HTTP(S) origin
- `cross-site`、非法 `Sec-Fetch-Site`、不可信 `Origin` / `Referer`、以及缺少可信来源的 `same-site` 请求都会被拒绝

这里的 `public_site_url` 是运行时配置里的公开站点来源列表，不是 CORS 白名单。它的作用是声明哪些同站公开入口属于本实例，避免把浏览器层面的 `same-site` 直接等同于可信。

## 当前用户资料、密码与偏好

- `PUT /auth/password`：修改当前用户密码，请求体如下：

```json
{
  "current_password": "old-password",
  "new_password": "new-password"
}
```

这个接口会校验当前密码；新密码仍然走和注册相同的长度校验。

- `GET /auth/me`：返回的 `preferences` 除了内置界面偏好外，还可能包含 `preferences.custom`，用于自定义前端读写自己的用户级 KV 配置
- `PATCH /auth/preferences`：只会合并请求体里非 `null` 的内置字段，并返回完整的最新偏好对象；当前偏好里也包含 `storage_event_stream_enabled`
  还支持两个和自定义前端有关的字段：
  - `custom`：把任意 JSON 值按 key 合并写入当前用户的自定义偏好
  - `remove_custom_keys`：显式删除一组自定义偏好 key
  自定义 key 不能和内置字段重名（例如 `theme_mode`、`language`）
- `PATCH /auth/profile`：当前只支持修改 `display_name`

`PATCH /auth/preferences` 的一个自定义 KV 示例：

```json
{
  "language": "zh",
  "custom": {
    "my-frontend.sidebar": { "collapsed": true },
    "my-frontend.accent": "sunset"
  },
  "remove_custom_keys": ["my-frontend.legacy"]
}
```

## 联系方式验证与密码重置

- `GET /auth/contact-verification/confirm?token=...`：这是浏览器入口，不返回 JSON，而是消费 token 后 `302` 重定向到前端页面。注册激活和邮箱变更都复用这条确认路径
- `POST /auth/email/change`：请求体是 `{ "new_email": "new@example.com" }`，会为当前登录用户写入待确认邮箱并发送确认邮件
- `POST /auth/email/change/resend`：对当前登录用户尚未完成的邮箱变更请求重发确认邮件
- `POST /auth/password/reset/request`：请求体是 `{ "email": "alice@example.com" }`，如果地址有效会发密码重置邮件；对外仍返回“请求已接受”的统一成功响应
- `POST /auth/password/reset/confirm`：请求体如下：

```json
{
  "token": "reset-token",
  "new_password": "new-password"
}
```

密码重置成功后，不需要当前登录态；接口会直接校验 token、写入新密码并记审计日志。

## 头像

头像相关接口都需要登录：

- `POST /auth/profile/avatar/upload`：`multipart/form-data` 上传图片，后端会生成 WebP 头像资源
- `PUT /auth/profile/avatar/source`：只能在 `none` 和 `gravatar` 之间切换；`upload` 来源必须通过上传接口设置
- `GET /auth/profile/avatar/{size}`：只读取“已上传头像”的 WebP 资源，当前支持 `512` 和 `1024`

也就是说：

- 如果你要把头像来源切到上传图，应该调用 `/auth/profile/avatar/upload`
- 如果当前来源是 `gravatar` 或 `none`，应优先使用 `/auth/me` 或资料更新接口返回的 `profile.avatar.url_*`

公开分享页和管理员接口会复用同一套头像资源，但读取路径不同。

## 实时存储事件

`GET /auth/events/storage` 是登录后可用的 SSE 接口，返回 `text/event-stream`，不是普通 JSON：

- 只会推送当前用户可见的个人空间和团队空间事件
- 空闲时每 15 秒发一次 `: keep-alive`
- 如果订阅端落后太多，服务端会发一个 `sync.required` 事件，提示前端整页重新同步
- 前端当前会用 `EventSource(..., { withCredentials: true })` 走 Cookie 鉴权
- 用户可通过偏好 `storage_event_stream_enabled = false` 关闭这条事件流

## 限流

`/auth` 不再按单个接口分别硬编码限流，但当前实现有两档：

- 登录、注册、重置密码、Passkey 登录、外部认证、MFA challenge 校验 / 邮箱验证码发送、refresh / logout 等未登录或登录流程接口走 `[rate_limit].auth`
- `/auth/me`、会话管理、密码修改、MFA 自助管理、Passkey 管理、资料 / 偏好 / 头像 / SSE 等已登录账号接口走 `[rate_limit].api`

`[rate_limit].auth` 默认配置：

- `seconds_per_request = 2`
- `burst_size = 5`

如果全局 `rate_limit.enabled = false`，则不会启用这层限流。
