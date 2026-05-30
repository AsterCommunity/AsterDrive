---
description: AsterDrive 外部认证配置指南，覆盖 OpenID Connect、通用 OAuth2、Logto、GitHub、账号绑定策略和故障排查。
---

# 外部认证

外部认证用于把外部身份提供商的登录结果映射到 AsterDrive 本地用户。管理员入口是：

```text
管理 -> 外部认证
```

当前支持两类 provider：

| 类型 | 适合场景 | 行为 |
| --- | --- | --- |
| `oidc` / OpenID Connect | 标准 OIDC 身份提供商 | 通过 issuer discovery 获取 endpoint，使用授权码流程、PKCE、nonce，并校验 ID Token |
| `generic_oauth2` / 通用 OAuth2 | 只有 OAuth2 授权码 + UserInfo 接口，或需要手动填 endpoint 的提供商 | 手动配置 authorization / token / userinfo endpoint，使用授权码流程和 PKCE，通过 UserInfo 响应提取用户身份 |

如果提供商支持标准 OIDC，优先用 `oidc`。只有在 provider 没有完整 OIDC discovery，或者你明确要手动接 OAuth2 userinfo 时，再用 `generic_oauth2`。

## 基础配置

创建 provider 前先确认：

1. `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 已经填成真实外部访问地址
2. 反向代理已经处理 HTTPS、Host、真实客户端 IP 和大请求体
3. 身份提供商里已经创建应用，并准备好 Client ID
4. 如果是 confidential client，准备好 Client Secret

保存 provider 后，页面会显示 AsterDrive 生成的重定向 URI。把它登记到身份提供商侧。回调路径形如：

```text
https://drive.example.com/api/v1/auth/external-auth/{kind}/{provider}/callback
```

其中 `{kind}` 是 `oidc` 或 `generic_oauth2`，`{provider}` 是服务端生成的 provider key。

## 通用字段

| 字段 | 说明 |
| --- | --- |
| 显示名称 | 登录页按钮上显示的名称 |
| 图标 URL | 可填站内路径，例如 `/static/external-auth/oauth_logo.svg`，也可填 HTTPS 图片 URL |
| Issuer URL | OIDC 必填；Generic OAuth2 可选。Generic OAuth2 填了以后会作为身份命名空间 |
| Authorization URL | Generic OAuth2 必填；OIDC 从 discovery 获取 |
| Token URL | Generic OAuth2 必填；OIDC 从 discovery 获取 |
| UserInfo URL | Generic OAuth2 必填；OIDC 当前主要使用 ID Token claims |
| Client ID / Client Secret | 身份提供商应用凭据；Secret 读取时会脱敏 |
| 授权范围 | 留空时使用 provider 类型默认值 |
| 允许邮箱域名 | 限制自动绑定 / 自动创建时可接受的邮箱域名 |
| Claim 映射 | 自定义 subject、用户名、显示名、邮箱、邮箱验证状态等字段 |

默认 scope 是：

```text
openid email profile
```

Generic OAuth2 留空时会使用 `openid email profile`。OIDC 留空时同样使用 `openid email profile`，并且服务端会保证 `openid` 存在；如果你手动把 OIDC scope 改成 `email profile`，保存时也会自动补回 `openid`。

## 账号绑定策略

AsterDrive 不把邮箱当成唯一身份来源。外部身份会优先按 `identity_namespace + subject` 匹配已有绑定。

| 策略 | 默认值 | 说明 |
| --- | --- | --- |
| 要求已验证邮箱 | 开 | provider 必须返回可用邮箱且 `email_verified=true`，否则进入补验或失败路径 |
| 按已验证邮箱自动绑定 | 关 | 只有 provider 返回 `email_verified=true`，并且本地存在唯一同邮箱用户时，才会自动绑定 |
| 自动创建本地用户 | 关 | 未绑定外部身份可以创建本地普通用户；仍受公开注册开关、邮箱域名和邮箱验证策略约束 |

保守建议：

- 对 OIDC / Logto 这类可信 provider，可以开启“要求已验证邮箱”
- 对不会可靠返回 `email_verified=true` 的 provider，不要开启“按已验证邮箱自动绑定”
- 接 GitHub 这类 OAuth2 provider 时，先关闭“要求已验证邮箱”和“按已验证邮箱自动绑定”，除非后续专门接了 verified email API

如果外部身份不能直接登录，用户会走登录并绑定已有账号，或通过邮箱补验继续。邮箱补验依赖 `管理 -> 系统设置 -> 邮件投递` 的外部登录邮箱验证邮件模版。

## Claim 映射

Generic OAuth2 会从 UserInfo JSON 里提取字段。默认映射是：

| AsterDrive 字段 | 默认 claim | 说明 |
| --- | --- | --- |
| Subject | `sub`，缺失时回退到 `id` | 必须存在；用于识别外部身份 |
| Email | `email` | 必须是合法邮箱格式才会接受 |
| Email verified | `email_verified` | 缺失时按 `false` 处理 |
| Display name | `name` | 用作本地显示名快照 |
| Username | `preferred_username` | 用作自动创建用户时的用户名候选 |

自定义 claim 支持三种写法：

- 顶层 key：`email`
- 点路径：`user.profile.email`
- JSON Pointer：`/user/profile/email`

布尔 claim 支持 JSON boolean，也支持字符串 `"true"` / `"false"`。

## Logto 示例

如果用 Logto 走 Generic OAuth2，常见配置如下：

```text
Provider kind: Generic OAuth2
Authorization URL: http://localhost:3001/oidc/auth
Token URL: http://localhost:3001/oidc/token
UserInfo URL: http://localhost:3001/oidc/me
Scopes: openid email profile
Subject claim: sub
Email claim: email
Email verified claim: email_verified
Display name claim: name
Username claim: preferred_username
```

正式环境把 `http://localhost:3001` 换成 Logto 对外地址，并使用 HTTPS。Logto 的 UserInfo 如果返回 `403 insufficient_scope` 且提示 access token 缺少 openid scope，说明登录发起时没有带 `openid`，把 scope 改为 `openid email profile` 后重新发起登录。

## GitHub 临时接入

GitHub 当前可以用 Generic OAuth2 暂时接入，但它不是完整 OIDC 登录 provider。推荐先这样配：

```text
Provider kind: Generic OAuth2
Authorization URL: https://github.com/login/oauth/authorize
Token URL: https://github.com/login/oauth/access_token
UserInfo URL: https://api.github.com/user
Scopes: read:user user:email
Subject claim: id
Username claim: login
Display name claim: name
Email claim: email
Email verified claim: 留空
Require email verified: false
Auto link verified email: false
```

注意两点：

- `https://api.github.com/user` 的 `email` 可能为空，取决于用户公开邮箱设置
- 当前通用 OAuth2 driver 不会额外调用 GitHub `/user/emails`，所以不能可靠判断 primary email 是否 verified

GitHub 专用 provider 或 verified email 支持跟踪 issue：<https://github.com/AptS-1547/AsterDrive/issues/235>。

## Token 请求方式

Generic OAuth2 当前只发起一次 token exchange，避免重放一次性 authorization code。

- 配了 Client Secret：使用 `client_secret_post`，也就是把 `client_id` 和 `client_secret` 放在 token endpoint 的 form body 里
- 没配 Client Secret：按 public client 发送 `client_id`
- 不会自动 fallback 到 `client_secret_basic`

如果某个 provider 只接受 `client_secret_basic`，当前需要等后续显式 client auth method 配置支持，不要靠重试同一个 authorization code。

## 常见问题

### 回调地址不匹配

先检查 `公开站点地址` 是否是用户实际访问的外部 URL，再把 `管理 -> 外部认证` 页面显示的重定向 URI 复制到身份提供商。改公开站点地址后，provider 侧登记的 redirect URI 也要同步更新。

### `OAuth2 userinfo request failed (403 Forbidden; error=insufficient_scope)`

通常是 scope 不够。Logto / OIDC 风格 userinfo 通常需要 `openid`，Generic OAuth2 默认已经是 `openid email profile`；如果是旧 provider 或手动改过 scope，重新保存 scope 并重新登录。

### `OAuth2 token exchange failed`

检查 Client ID、Client Secret、Token URL、redirect URI 是否完全匹配。Generic OAuth2 有 secret 时使用 `client_secret_post`，如果 provider 只允许 `client_secret_basic`，当前版本还不能直接接。

### 缺少 subject

Generic OAuth2 默认读 `sub`，缺失时读 `id`。如果 provider 放在别的字段，改 Subject claim，例如 `user.id` 或 `/user/id`。

### 邮箱无法自动绑定

自动绑定要求 provider 返回可用邮箱和 `email_verified=true`，并且本地只有一个同邮箱用户。GitHub 这类 provider 不可靠返回 verified email 时，先不要开启自动绑定。

### 保存测试通过，但真实登录失败

测试按钮只检查 provider endpoint 配置和 discovery / endpoint 可达性，不会替你完成真实授权码登录。上线前必须用真实账号跑一次登录、自动创建、自动绑定、MFA 和邮箱补验路径。
