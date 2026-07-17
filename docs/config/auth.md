# 登录与会话

::: tip 这一篇分两层讲
- `config.toml` 里的 `[auth]` —— **只负责启动时的静态引导**（签名密钥、首次纯 HTTP 引导）
- `管理 -> 系统设置` —— **日常规则**（公开注册、Cookie、Token 有效期、激活 / 重置链接、各种冷却时间）

平时真正常改的几乎都在后台，本页静态部分只在初次部署或换机时碰一次。
:::

## `config.toml` 里的 `[auth]`

```toml
[auth]
jwt_secret = "<首次生成的一串随机密钥>"
share_cookie_secret = "<首次生成的一串随机密钥>"
direct_link_secret = "<首次生成的一串随机密钥>"
mfa_secret_key = "<首次生成的一串随机密钥>"
storage_credential_secret_key = "<首次生成的一串随机密钥>"
bootstrap_insecure_cookies = false
```

### `jwt_secret`

首次自动生成配置时，服务会写入一段随机密钥。可以理解成"全站登录签名密钥"。

::: warning 正式环境固定它，避免来回改动
一旦修改：
- 当前所有登录会话失效
- 所有人都要重新登录
:::

### `share_cookie_secret`

这是公开分享密码验证 Cookie 的 HMAC 密钥。修改后，已通过密码验证的分享访问 Cookie 会失效，用户需要重新输入分享密码。

### `direct_link_secret`

这是公共直链、预览链接和分享流式播放会话的 HMAC 密钥。修改后，已生成的直链和短期预览 / 流式会话 token 会失效，需要重新生成。

### `mfa_secret_key`

这是 MFA/TOTP 密钥的服务端加密密钥。首次生成配置时，服务会自动写入一段随机值。

::: warning 备份和迁移时必须保留
如果你已经有用户启用了 MFA，不要在迁移、恢复或重建 `config.toml` 时随手换掉它。

一旦修改，已有认证器密钥无法解密，启用了 MFA 的用户会无法通过原来的认证器完成二次验证。管理员只能到 `管理 -> 用户 -> 用户详情 -> 安全操作` 里重置对应用户的 MFA，让用户重新绑定认证器并保存新的恢复码。
:::

### `storage_credential_secret_key`

这是 OneDrive 存储策略的 Microsoft Graph 凭据（Client Secret、access token、refresh token）的服务端加密主密钥。首次生成配置时，服务会自动写入一段随机值；派生出的密钥用 AES-256-GCM 把凭据加密后落库，API 与审计只暴露 `client_secret_configured` 这类布尔状态。

::: tip 这把密钥目前只覆盖 OneDrive
它保护的是 `storage_connector_application_configs.client_secret_ciphertext` 和 `storage_policy_credentials` 表里的 access / refresh token 密文。

S3、Azure Blob、腾讯云 COS 的 `access_key` / `secret_key`，以及远程节点（follower）凭据，**目前是明文落库**，不依赖这把密钥——换掉它不会影响这些驱动。
:::

::: warning 备份和迁移时必须保留
只要有一条 OneDrive 策略完成过 Microsoft Graph 授权，就不要在迁移、恢复或重建 `config.toml` 时换掉它。

一旦修改或丢失，已加密落库的 Client Secret 和 OAuth token 都无法解密，所有 OneDrive 策略会进入需要重新授权状态。旧 refresh token 无法恢复，管理员只能逐条回到 `管理 -> 存储策略 -> 目标 OneDrive 策略 -> 授权` 重新走一遍授权流程。

升级或换机前，把整个 `[auth]` 段连同这把密钥一起备份。
:::

### `bootstrap_insecure_cookies`

- **纯 HTTP 首次试跑** —— 临时设 `true`
- **正式 HTTPS 部署** —— 保持 `false`

它**只影响第一次初始化** `auth_cookie_secure` 时写入的默认值。如果数据库里已经有这个运行时设置，再改这里不会回写旧值。

## 登录页是按状态自动判断的

登录页不是固定的"登录"或"注册"页面，而是按当前状态走：

- **系统里还没有任何用户** —— 进入初始化流程，直接创建第一个管理员
- **系统里已有用户，输入的是现有账号** —— 登录
- **系统里已有用户，输入的是新账号，且管理员允许公开注册** —— 创建普通账号
- **管理员启用了外部认证提供商** —— 登录页会出现对应的外部登录入口
- **当前浏览器支持 Passkey** —— 登录页会显示 Passkey 登录入口，已登记 Passkey 的账号可以直接用设备解锁或安全密钥登录
- **账号需要 MFA** —— 密码或外部身份通过后，还需要完成二次验证；可能是认证器验证码、恢复码，或管理员开启的邮箱验证码

需要注意：

- 第一个账号直接成为管理员，不走邮箱激活
- 后续公开注册的普通账号，要先点激活邮件才能登录
- 管理员关闭公开注册后，登录页只剩登录和找回密码

## MFA 多因素认证

MFA 由用户自己在这里启用：

```text
设置 -> 安全 -> 多因素认证
```

当前用户自己能绑定的是 TOTP 认证器应用。常见应用包括 1Password、Bitwarden、Google Authenticator、Microsoft Authenticator 等。

启用流程大致是：

1. 打开 `设置 -> 安全 -> 多因素认证`
2. 点击设置认证器
3. 用认证器应用扫描二维码，或手动输入密钥
4. 输入认证器生成的 6 位验证码完成绑定
5. 下载或复制恢复码，并保存到密码管理器或其他安全位置

恢复码只在生成时明文显示一次，每个恢复码只能使用一次。丢失认证器时，可以在登录页用恢复码完成 MFA 验证；登录后应尽快重新生成恢复码，或重新绑定认证器。

启用 MFA 后，下面这些登录方式都会进入二次验证：

- 本地密码登录
- 外部认证登录

Passkey 登录不会进入这里描述的 MFA 挑战。它本身依赖设备解锁或安全密钥完成用户验证，和“密码/外部身份 + TOTP”是两条不同登录路径。

MFA 登录验证流程默认 `5` 分钟内有效，最多允许 `5` 次尝试。验证过期或尝试次数用完后，返回登录页重新开始即可。

### 邮箱验证码 MFA

管理员可以在后台开启邮箱验证码 MFA：

```text
管理 -> 系统设置 -> 认证与 Cookie -> 要求邮箱验证码 MFA
```

开启后，已验证邮箱的用户在密码或外部身份通过后，可以通过 8 位邮箱验证码完成二次验证。这个功能依赖完整可用的邮件投递配置；SMTP 主机和发件人地址不能为空，SMTP 用户名和密码必须同时填写或同时留空。

默认规则：

- 邮箱验证码默认 `10` 分钟有效，但不会超过当前 MFA 登录流程剩余时间
- 同一用户默认 `60` 秒内不能重复发送
- 只开启 `要求邮箱验证码 MFA` 时，未启用 TOTP 且邮箱已验证的用户会走邮箱验证码
- 如果还开启 `允许 TOTP 使用邮箱兜底`，已经启用认证器的用户也可以把邮箱验证码作为额外登录验证方式

::: warning 谨慎开启邮箱兜底
邮箱验证码依赖邮箱账号安全。安全要求高的部署，通常只把它用于没有认证器的已验证邮箱用户；是否允许 TOTP 用户用邮箱兜底，要按你的安全策略决定。
:::

如果用户丢失认证器和恢复码，管理员可以在这里重置：

```text
管理 -> 用户 -> 用户详情 -> 安全操作 -> 重置 MFA
```

重置会清除该用户的认证器、恢复码和未完成的 MFA 登录流程，并让该用户现有会话失效。用户下次登录后需要重新设置 MFA。

## Passkey 登录

Passkey 是每个用户自己管理的登录方式，入口在：

```text
设置 -> 安全 -> Passkey
```

用户可以在这里：

- 添加新的 Passkey
- 给 Passkey 改名，例如 `MacBook`、`iPhone` 或某把安全密钥
- 查看创建时间和最近使用时间
- 删除不再使用的 Passkey

添加时浏览器会打开系统自己的 WebAuthn / Passkey 验证窗口。正式部署要先填对 `管理 -> 系统设置 -> 站点配置 -> 公开站点地址`，并使用 HTTPS；本地 `localhost` / `127.0.0.1` 调试例外。浏览器通常只在安全上下文里开放完整 Passkey 能力。

Passkey 不替代本地密码。用户仍然可以继续使用密码登录；删除某个 Passkey 后，只是那台设备或那把安全密钥不能再直接登录当前账号。

管理员也可以在后台临时关闭 Passkey 登录入口：

```text
管理 -> 系统设置 -> 用户管理 -> 注册与登录 -> 允许使用 Passkey 登录
```

关闭后，用户不能再用已登记的 Passkey 完成登录，但已有 Passkey 不会被删除。重新开启后，原来登记过的 Passkey 仍然可以继续使用。

## 外部认证 / SSO

管理员可以在这里接入外部身份提供商：

```text
管理 -> 外部认证
```

当前支持 OpenID Connect、通用 OAuth2，以及 GitHub、QQ、Google、Microsoft 专用 provider。创建提供商后，登录页会展示对应的外部登录入口；管理员需要把页面生成的重定向 URI 登记到身份提供商侧。完整配置细节见 [外部认证](/config/external-auth)。

外部身份和本地用户的关系由提供商规则决定：

- 已绑定过的外部身份会直接登录对应本地用户
- 开启“按已验证邮箱自动绑定”后，身份提供商返回 `email_verified=true` 且本地存在唯一匹配邮箱时，系统可以自动绑定
- 开启“自动创建本地用户”后，未绑定身份可以自动创建普通用户
- 没开启自动绑定或自动创建时，用户需要先登录现有账号完成绑定，或按邮箱验证流程继续

用户已经绑定的外部身份在这里查看和解绑：

```text
设置 -> 安全 -> 外部身份
```

如果管理员开启了自动绑定，用户解绑后，后续满足相同规则的外部登录仍可能重新绑定到本地账号。

## 公开注册开关在哪

```text
管理 -> 系统设置 -> 用户管理 -> 允许公开注册新用户
```

关闭后：

- 外部用户不能再从登录页创建新账号
- 第一个管理员初始化流程仍然存在
- 管理员在后台手动创建的用户仍然可以使用

### 本地账号邮箱白名单 / 黑名单

如果站点只允许公司邮箱注册，或者要阻止一次性邮箱，可以在这里配置：

```text
管理 -> 系统设置 -> 用户管理 -> 注册与登录 -> 本地账号邮箱白名单
管理 -> 系统设置 -> 用户管理 -> 注册与登录 -> 本地账号邮箱黑名单
```

这两项只作用于**本地账号**：

- 公开注册时填写的邮箱
- 用户在 `设置 -> 安全` 里改绑的本地邮箱

它们不会限制第三方 SSO 返回的外部身份。外部认证的邮箱域名限制仍然在 `管理 -> 外部认证` 的 provider 规则里配置。

名单项可以写完整邮箱，也可以写精确域名：

```text
alice@example.com
example.com
@example.com
```

`example.com` 和 `@example.com` 等价，只匹配 `user@example.com`，不会自动匹配 `user@sub.example.com`。国际化域名需要写 punycode 形式。

规则顺序：

- 黑名单优先于白名单
- 白名单为空时，表示不启用白名单限制
- 黑名单为空时，表示不额外阻止邮箱
- 两个名单都为空时，所有合法邮箱都可以用于本地注册和本地邮箱改绑

## 哪些功能依赖邮件配置

下面这些功能没邮件就用不了：

- 公开注册后的激活邮件
- 登录页的找回密码
- `设置 -> 安全` 里的邮箱改绑确认邮件
- 外部认证无法直接匹配本地账号时的邮箱验证流程
- 邮箱验证码 MFA

::: warning 先配通邮件，再开放注册
顺序反了的话，新用户账号已经创建出来，却收不到激活邮件，只会卡在"等待激活"。

准备开放这些能力前，先一起检查：
1. `管理 -> 系统设置 -> 邮件投递`
2. `管理 -> 系统设置 -> 站点配置 -> 公开站点地址`
3. 如果要接外部认证，再检查 `管理 -> 外部认证` 里的重定向 URI 是否已经登记到身份提供商侧
:::

## 常见写法

### 本地或内网 HTTP 试跑

```toml
[auth]
bootstrap_insecure_cookies = true
```

### 正式 HTTPS 部署

```toml
[auth]
jwt_secret = "replace-with-your-own-secret"
share_cookie_secret = "replace-with-share-cookie-secret"
direct_link_secret = "replace-with-direct-link-secret"
mfa_secret_key = "replace-with-another-stable-secret"
storage_credential_secret_key = "replace-with-storage-credential-secret"
bootstrap_insecure_cookies = false
```

环境变量覆盖：

```bash
ASTER__AUTH__JWT_SECRET="replace-with-your-own-secret"
ASTER__AUTH__SHARE_COOKIE_SECRET="replace-with-share-cookie-secret"
ASTER__AUTH__DIRECT_LINK_SECRET="replace-with-direct-link-secret"
ASTER__AUTH__MFA_SECRET_KEY="replace-with-another-stable-secret"
ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY="replace-with-storage-credential-secret"
ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=false
```

## 日常真正常改的是后台这些

下面这些不在 `config.toml` 里，全在后台维护：

- `auth_cookie_secure` —— Cookie 是否仅 HTTPS 发送
- `auth_access_token_ttl_secs` —— 访问令牌有效期
- `auth_refresh_token_ttl_secs` —— 刷新令牌有效期
- `auth_register_activation_ttl_secs` —— 注册激活链接有效期
- `auth_contact_change_ttl_secs` —— 邮箱改绑链接有效期
- `auth_password_reset_ttl_secs` —— 密码重置链接有效期
- `auth_contact_verification_resend_cooldown_secs` —— 验证邮件重发冷却
- `auth_password_reset_request_cooldown_secs` —— 密码重置请求冷却
- `auth_email_code_login_enabled` —— 是否启用邮箱验证码 MFA
- `auth_email_code_login_allow_totp_fallback` —— 是否允许已启用 TOTP 的用户用邮箱验证码兜底
- `auth_email_code_login_ttl_secs` —— 邮箱登录验证码有效期
- `auth_email_code_login_resend_cooldown_secs` —— 邮箱登录验证码重发冷却
- `auth_passkey_login_enabled` —— 是否允许用户用已登记的 Passkey 登录
- `auth_allow_user_registration` —— 公开注册开关
- `auth_register_activation_enabled` —— 新注册用户是否必须先完成邮箱激活
- `auth_local_email_allowlist` —— 本地注册和本地邮箱改绑允许使用的邮箱或精确域名
- `auth_local_email_blocklist` —— 本地注册和本地邮箱改绑禁止使用的邮箱或精确域名
- 外部认证邮箱验证、登录邮箱验证码等邮件模版 —— 在 `邮件投递` 分组里维护

具体说明见 [系统设置](/config/runtime)。
