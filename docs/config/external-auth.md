---
description: AsterDrive 外部认证配置指南，覆盖 OpenID Connect、通用 OAuth2、Logto、GitHub、QQ、Google、Microsoft、账号绑定策略和故障排查。
---

# 外部认证

外部认证用于把外部身份提供商的登录结果映射到 AsterDrive 本地用户。管理员入口是：

```text
管理 -> 外部认证
```

当前支持六类 provider：

| 类型 | 适合场景 | 行为 |
| --- | --- | --- |
| `oidc` / OpenID Connect | 标准 OIDC 身份提供商 | 通过 issuer discovery 获取 endpoint，使用授权码流程、PKCE、nonce，并校验 ID Token |
| `generic_oauth2` / 通用 OAuth2 | 只有 OAuth2 授权码 + UserInfo 接口，或需要手动填 endpoint 的提供商 | 手动配置 authorization / token / userinfo endpoint，使用授权码流程和 PKCE，通过 UserInfo 响应提取用户身份 |
| `github` / GitHub | GitHub OAuth App 登录 | 使用固定 GitHub OAuth endpoint，读取 `/user` 身份字段，并额外调用 `/user/emails` 只接受已验证主邮箱 |
| `qq` / QQ | QQ 互联网站登录 | 使用固定 QQ OAuth2 endpoint，先读取 `openid`，再调用 `get_user_info` 获取昵称资料，并按 `openid` 绑定外部身份 |
| `google` / Google | Google OIDC 登录 | 使用固定 Google Accounts issuer / discovery，校验 ID Token，并按 `sub` 绑定外部身份 |
| `microsoft` / Microsoft | Microsoft Entra ID / Microsoft Account OIDC 登录 | 根据 tenant 生成 Microsoft identity platform issuer / discovery，校验 ID Token，并按 `sub` 绑定外部身份 |

如果提供商支持标准 OIDC，优先用 `oidc`。GitHub、QQ、Google 和 Microsoft 用专用 provider。只有在 provider 没有完整 OIDC discovery，或者你明确要手动接 OAuth2 userinfo 时，再用 `generic_oauth2`。

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

其中 `{kind}` 是 `oidc`、`generic_oauth2`、`github`、`qq`、`google` 或 `microsoft`，`{provider}` 是服务端生成的 provider key。

## 应用申请入口

创建 AsterDrive provider 前，需要先在对应身份提供商创建应用 / OAuth client，并把 AsterDrive 回调地址登记为 redirect URI。

| Provider | 去哪里创建应用 | 需要登记的回调 |
| --- | --- | --- |
| OIDC / Generic OAuth2 | 你的身份提供商管理后台，例如 Logto、Authentik、Keycloak、Zitadel 等的 Applications / Clients 页面 | 保存 AsterDrive provider 后显示的 callback URL；如果 IdP 要求预填，可先按 `/api/v1/auth/external-auth/{kind}/{provider}/callback` 规则生成 |
| GitHub | GitHub 用户或组织的 `Settings -> Developer settings -> OAuth Apps -> New OAuth App` | OAuth App 的 Authorization callback URL |
| QQ | QQ 互联管理中心 <https://connect.qq.com/manage.html> 创建网站应用 | 应用回调地址 / 回调域名；使用 AsterDrive provider 保存后显示的 callback URL |
| Google | Google Cloud Console 的 `APIs & Services -> Credentials -> Create Credentials -> OAuth client ID` | OAuth client 的 Authorized redirect URIs |
| Microsoft | Microsoft Entra admin center / Azure portal 的 `App registrations -> New registration` | Authentication 页面添加 `Web` redirect URI；Supported account types 要和 AsterDrive 的 tenant 选择一致 |

Microsoft Supported account types 和 AsterDrive tenant 的常用对应关系：

- AsterDrive tenant `consumers`：选择 “Personal Microsoft accounts only”
- AsterDrive tenant `organizations`：选择 “Accounts in any organizational directory”
- AsterDrive tenant 为具体 tenant ID：选择该目录内账号，或选择多租户组织账号后只在 AsterDrive 限定该 tenant ID
- AsterDrive tenant `common`：选择 “Accounts in any organizational directory and personal Microsoft accounts”

如果身份提供商要求应用类型，登录 provider 通常选择 Web application / Confidential client；只在你明确知道 provider 支持 public client 且不需要 secret 时才留空 Client Secret。

## 通用字段

| 字段 | 说明 |
| --- | --- |
| 显示名称 | 登录页按钮上显示的名称 |
| 图标 URL | 可填站内路径，例如 `/static/external-auth/oauth-logo.svg`，也可填 HTTPS 图片 URL；登录页、后台列表和用户安全设置页会优先使用后台配置的图标，加载失败时回退到 provider 类型默认图标 |
| Issuer URL | OIDC 必填；Generic OAuth2 可选。Generic OAuth2 填了以后会作为身份命名空间；GitHub / QQ 使用固定身份命名空间；Google 固定为 `https://accounts.google.com`；Microsoft 由 tenant 自动生成 |
| Authorization URL | Generic OAuth2 必填；OIDC 从 discovery 获取；GitHub / QQ 固定为对应 OAuth endpoint；Google / Microsoft 从固定 discovery 获取 |
| Token URL | Generic OAuth2 必填；OIDC 从 discovery 获取；GitHub / QQ 固定为对应 OAuth endpoint；Google / Microsoft 从固定 discovery 获取 |
| UserInfo URL | Generic OAuth2 必填；OIDC 当前主要使用 ID Token claims；GitHub 固定读取 `https://api.github.com/user`；QQ 固定读取 `https://graph.qq.com/user/get_user_info`；Google / Microsoft 当前主要使用 ID Token claims |
| Client ID / Client Secret | 身份提供商应用凭据；Secret 读取时会脱敏 |
| 授权范围 | 留空时使用 provider 类型默认值 |
| 允许邮箱域名 | 限制自动绑定 / 自动创建时可接受的邮箱域名 |
| Claim 映射 | 自定义 subject、用户名、显示名、邮箱、邮箱验证状态等字段 |

默认 scope 是：

```text
openid email profile
```

Generic OAuth2 留空时会使用 `openid email profile`。OIDC 留空时同样使用 `openid email profile`，并且服务端会保证 `openid` 存在；如果你手动把 OIDC scope 改成 `email profile`，保存时也会自动补回 `openid`。

Google 留空时使用：

```text
openid profile email
```

GitHub 留空时使用：

```text
read:user user:email
```

QQ 留空时使用：

```text
get_user_info
```

Microsoft 留空时使用：

```text
openid profile email
```

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
- GitHub 专用 provider 会读取 `/user/emails` 并只接受 `primary=true` 且 `verified=true` 的邮箱；如果开启“要求已验证邮箱”但拿不到已验证主邮箱，登录会直接失败，不会用本地邮箱补验绕过 GitHub 的验证语义
- QQ 专用 provider 不返回邮箱，也没有 `email_verified` 语义；默认关闭“要求已验证邮箱”，首次登录通常需要邮箱补验或绑定已有本地账号
- Google 专用 provider 使用 ID Token 的 `email_verified`；开启“要求已验证邮箱”时，`email_verified=false`、缺失或非布尔值都会拒绝直接登录
- Microsoft 专用 provider 不把 `email` 当作已验证主邮箱，也不读取 `email_verified`；默认关闭“要求已验证邮箱”，邮箱缺失时走现有邮箱补验 / 账号绑定流程

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

## 提供商示例

<details>
<summary>Logto / 通用 OAuth2</summary>

如果用 Logto 走 Generic OAuth2，常见配置如下：

申请入口：

- Logto Cloud Console: <https://cloud.logto.io/>
- 自托管 Logto Console: `https://<logto-host>/console`
- Logto 应用文档: <https://docs.logto.io/end-user-flows/sign-in-experience/applications>

在 Logto Console 里创建 Traditional Web 应用，把 AsterDrive 生成的 callback URL 填到 Redirect URI。其他 OIDC / OAuth2 provider 也在各自的 Applications / Clients 页面创建应用，并登记同一个 callback URL。

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

</details>

<details>
<summary>GitHub</summary>

GitHub 使用专用 `github` provider，不再需要把它当 Generic OAuth2 手动配置端点：

申请入口：

- 个人账号 OAuth Apps: <https://github.com/settings/developers>
- 组织 OAuth Apps: `https://github.com/organizations/{org}/settings/applications`
- GitHub 官方创建说明: <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app>

创建 OAuth App 时，把 AsterDrive 生成的 callback URL 填到 Authorization callback URL。

```text
Provider kind: GitHub
Scopes: read:user user:email
Subject claim: 固定读取 /user.id
Username claim: 固定读取 /user.login
Display name claim: 固定读取 /user.name
Email: 固定从 /user/emails 筛选 primary=true 且 verified=true 的邮箱
```

注意：

- 后台表单不需要填写 GitHub authorization / token / userinfo endpoint
- GitHub `/user.email` 不可信，AsterDrive 不用它判断登录邮箱
- 如果 GitHub `/user/emails` 没有返回已验证主邮箱，`email_verified=false`
- 如果开启“要求已验证邮箱”，没有已验证主邮箱会直接拒绝登录

</details>

<details>
<summary>QQ 互联</summary>

QQ 使用专用 `qq` provider，不需要把它当 Generic OAuth2 手动配置端点。QQ 互联网站登录不是 OIDC：没有 discovery、ID Token、JWKS 或 nonce；AsterDrive 会按 QQ OAuth2 流程先换取 access token，再获取 `openid`，最后读取用户资料。

在 QQ 互联管理中心 <https://connect.qq.com/manage.html> 创建网站应用，拿到 App ID 和 App Key。AsterDrive 保存 provider 后会显示 callback URL，把它登记到 QQ 互联应用的回调地址 / 回调域名配置里。OAuth2 细节可参考 QQ 互联文档。

```text
Provider kind: QQ
Client ID: QQ App ID
Client Secret: QQ App Key
Authorization URL: 固定为 https://graph.qq.com/oauth2.0/authorize
Token URL: 固定为 https://graph.qq.com/oauth2.0/token
OpenID URL: 固定为 https://graph.qq.com/oauth2.0/me
UserInfo URL: 固定为 https://graph.qq.com/user/get_user_info
Scopes: get_user_info
Subject claim: 固定读取 openid
Display name claim: 固定读取 get_user_info.nickname
Email: 不返回
```

注意：

- 后台表单不需要填写 QQ authorization / token / openid / userinfo endpoint
- `openid` 是 QQ App ID 维度的身份，AsterDrive 使用 `qq:{client_id}` 作为身份命名空间，避免不同 QQ 应用之间混绑
- QQ 不返回邮箱，也不会声明邮箱已验证；首次登录通常会进入邮箱补验或本地账号绑定流程，不要期待它按已验证邮箱自动绑定

</details>

<details>
<summary>Google</summary>

Google 使用专用 `google` provider，不需要把它当普通 OIDC 手动配置 issuer：

在 Google Cloud Console Credentials <https://console.cloud.google.com/apis/credentials> 创建 OAuth client ID，应用类型选择 Web application，把 AsterDrive 生成的 callback URL 填到 Authorized redirect URIs。首次使用前通常还需要配置 OAuth consent screen。

```text
Provider kind: Google
Issuer URL: 固定为 https://accounts.google.com
Discovery: 固定为 https://accounts.google.com/.well-known/openid-configuration
Scopes: openid profile email
Subject claim: 固定读取 ID Token sub
Display name claim: 固定读取 ID Token name
Email claim: 固定读取 ID Token email
Email verified claim: 固定读取 ID Token email_verified
Avatar URL claim: 固定读取 ID Token picture
```

注意：

- 后台表单不需要填写 Google issuer / discovery / authorization / token / userinfo endpoint
- 外部身份必须用 `sub` 绑定，不要用 email 做唯一标识
- 如果后续要接 Google Drive，请单独创建资源访问用 OAuth client / 授权流程，不要把 `drive.*` scopes 加到登录 provider

</details>

<details>
<summary>Microsoft Entra ID / Microsoft Account</summary>

Microsoft 使用专用 `microsoft` provider。它按 OIDC 接入，不要为了登录改成 Generic OAuth2；OAuth2 access token 只在后续需要访问 Microsoft Graph 等资源时才需要。

在 Microsoft Entra admin center <https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade> 或 Azure portal <https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade> 创建应用。推荐流程：

1. 先在 AsterDrive 的 Microsoft provider 表单里选择 Tenant。

   - `consumers`：只允许个人 Microsoft Account 登录
   - `organizations`：允许组织 / Entra ID 账号登录
   - `common`：同时允许个人 Microsoft Account 和组织账号登录
   - 具体 tenant ID：只允许对应目录的组织账号登录

2. 在 Microsoft Entra admin center 或 Azure portal 打开 `App registrations -> New registration`。

3. 在 Supported account types 里选择和 AsterDrive Tenant 匹配的账号类型。

   - AsterDrive Tenant 是 `consumers` 时，选择 personal Microsoft accounts only
   - AsterDrive Tenant 是 `organizations` 或具体 tenant ID 时，选择对应的 organizational directory accounts
   - AsterDrive Tenant 是 `common` 时，选择同时支持 organizational directory accounts 和 personal Microsoft accounts 的类型

4. 创建应用后，复制 `Application (client) ID`，填到 AsterDrive 的 Client ID。

5. 打开应用的 `Authentication` 页面，添加平台 `Web`，把 AsterDrive 显示的 callback URL 填到 Redirect URI。

   AsterDrive 的 Microsoft 登录由后端用 authorization code 换 token，属于 Web / confidential client 流程。不要把 callback URL 登记到 `Public client/native (mobile and desktop)`；如果已经加在那里，删除或不要使用那一项。

6. 打开 `Certificates & secrets -> Client secrets`，创建一个 client secret。

   AsterDrive 里要填写创建后显示的 `Value`，不要填写 `Secret ID`。`Value` 只显示一次，如果没有保存只能新建一个 secret。

7. 回到 AsterDrive 保存 provider。保存后可以点测试按钮确认 discovery / JWKS 可访问，再从登录页用真实 Microsoft 账号完整登录一次。

Microsoft provider 常见配置：

```text
Provider kind: Microsoft
Tenant: tenant ID / common / organizations / consumers
Issuer URL: 自动生成，例如 https://login.microsoftonline.com/{tenant}/v2.0
Discovery: 自动使用 https://login.microsoftonline.com/{tenant}/v2.0/.well-known/openid-configuration
Scopes: openid profile email
Subject claim: 固定读取 ID Token sub
Display name claim: 固定读取 ID Token name
Email claim: 固定读取 ID Token email
Email verified claim: 不使用
```

Tenant 可填具体 tenant ID，也可填 `organizations`、`consumers` 或 `common`。多租户入口返回的 ID Token issuer 可能是具体 tenant，AsterDrive 会按 Microsoft issuer 规则校验，不要求它逐字等于 `common` / `organizations` / `consumers` issuer。Microsoft 的 `email` claim 不保证一定存在，也不要默认套用 GitHub 的 verified primary email 语义；邮箱缺失时按现有邮箱补验 / 绑定流程处理。Microsoft Graph 权限属于后续资源访问授权，不应混入登录 provider 的默认 scopes。

::: warning 常见坑 / 提醒
- `AADSTS9002346`：Supported account types 和 AsterDrive tenant 不匹配。按上面的 tenant 对应关系调整。
- `AADSTS7000215`：通常是把 Azure 的 `Secret ID` 填成了 Client Secret。请新建 client secret，并把创建时显示的 `Value` 填入 AsterDrive。
- `AADSTS90023`：Redirect URI 被登记在 `Public client/native (mobile and desktop)` 平台。把该 Redirect URI 移到 `Authentication -> Web`，保留 AsterDrive 的 Client Secret，然后从登录页重新发起 Microsoft 登录，不要刷新旧 callback URL。
:::

</details>

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

Microsoft 报 `AADSTS7000215` 时，优先检查 Client Secret 是否填的是 `Certificates & secrets -> Client secrets` 里的 `Value`，不是 `Secret ID`。如果 `Value` 已经看不到，只能新建 secret。

Microsoft 报 `AADSTS90023` 时，说明 Redirect URI 被登记在 `Public client/native (mobile and desktop)` 平台。把该 Redirect URI 移到 `Authentication -> Web` 平台，保留 AsterDrive 的 Client Secret，然后从登录页重新发起 Microsoft 登录，不要复用或刷新旧 callback URL。

### 缺少 subject

Generic OAuth2 默认读 `sub`，缺失时读 `id`。如果 provider 放在别的字段，改 Subject claim，例如 `user.id` 或 `/user/id`。

### 邮箱无法自动绑定

自动绑定要求 provider 返回可用邮箱和 `email_verified=true`，并且本地只有一个同邮箱用户。GitHub 专用 provider 只在 `/user/emails` 返回已验证主邮箱时才会认为邮箱已验证；Google 专用 provider 使用 ID Token `email_verified`；Microsoft 专用 provider 不声明邮箱已验证，必要时让用户走邮箱补验或密码绑定。

### 保存测试通过，但真实登录失败

测试按钮只检查 provider endpoint 配置和 discovery / endpoint 可达性，不会替你完成真实授权码登录。上线前必须用真实账号跑一次登录、自动创建、自动绑定、MFA 和邮箱补验路径。
