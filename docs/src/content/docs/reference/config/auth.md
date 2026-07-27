---
description: "AsterDrive `config.toml` 的 `[auth]` 静态认证字段：六个签名与加密密钥、Argon2 并发上限、bootstrap_insecure_cookies，以及后台运行时认证设置清单。"
title: "登录与会话"
---

:::tip[这一页只剩静态字段与设置清单]
- 本页：`config.toml` 里的 `[auth]` —— **启动时的静态认证配置**（签名密钥、Argon2 并发上限、首次纯 HTTP 引导），以及后台运行时认证设置清单
- 第一个管理员和登录页状态机 —— [首次启动与第一个管理员](/start/first-admin/)
- 注册开关、MFA 策略、Passkey 开关、外部认证接入 —— [注册、登录与 SSO](/admin/auth-sso/)
- 用户自己绑定 MFA / Passkey / 外部身份 —— [账号与安全](/using/account-security/)
:::

## `config.toml` 里的 `[auth]`

```toml
[auth]
jwt_secret = "<首次生成的一串随机密钥>"
share_cookie_secret = "<首次生成的一串随机密钥>"
direct_link_secret = "<首次生成的一串随机密钥>"
mfa_secret_key = "<首次生成的一串随机密钥>"
storage_credential_secret_key = "<首次生成的一串随机密钥>"
webdav_auth_cache_secret = "<首次生成的一串随机密钥>"
password_hash_max_concurrency = 2
bootstrap_insecure_cookies = false
```

### `jwt_secret`

首次自动生成配置时，服务会写入一段随机密钥。可以理解成"全站登录签名密钥"。

:::caution[正式环境固定它，避免来回改动]
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

:::caution[备份和迁移时必须保留]
如果你已经有用户启用了 MFA，不要在迁移、恢复或重建 `config.toml` 时随手换掉它。

一旦修改，已有认证器密钥无法解密，启用了 MFA 的用户会无法通过原来的认证器完成二次验证。管理员只能到 `管理 -> 用户 -> 用户详情 -> 安全操作` 里重置对应用户的 MFA，让用户重新绑定认证器并保存新的恢复码。
:::

### `storage_credential_secret_key`

这是 OneDrive 存储策略的 Microsoft Graph 凭据（Client Secret、access token、refresh token）的服务端加密主密钥。首次生成配置时，服务会自动写入一段随机值；派生出的密钥用 AES-256-GCM 把凭据加密后落库，API 与审计只暴露 `client_secret_configured` 这类布尔状态。

:::tip[这把密钥目前只覆盖 OneDrive]
它保护的是 `storage_connector_application_configs.client_secret_ciphertext` 和 `storage_policy_credentials` 表里的 access / refresh token 密文。

S3、Azure Blob、腾讯云 COS 的 `access_key` / `secret_key`，以及远程节点（follower）凭据，**目前是明文落库**，不依赖这把密钥——换掉它不会影响这些驱动。
:::

:::caution[备份和迁移时必须保留]
只要有一条 OneDrive 策略完成过 Microsoft Graph 授权，就不要在迁移、恢复或重建 `config.toml` 时换掉它。

一旦修改或丢失，已加密落库的 Client Secret 和 OAuth token 都无法解密，所有 OneDrive 策略会进入需要重新授权状态。旧 refresh token 无法恢复，管理员只能逐条回到 `管理 -> 存储策略 -> 目标 OneDrive 策略 -> 授权` 重新走一遍授权流程。

升级或换机前，把整个 `[auth]` 段连同这把密钥一起备份。
:::

### `webdav_auth_cache_secret`

这是 WebDAV 认证缓存 key 的专用 HMAC 密钥，用来避免 Redis key 列表把有效密码暴露成可直接离线枚举的 SHA-256 目标。所有共享同一 Redis cache 的 Primary 必须使用相同值。

修改后，已有 WebDAV 认证缓存会全部 miss 并在认证成功后重新写入；旧 key 最多保留原有的 60 秒 TTL。它与 JWT、分享 Cookie、直链、MFA 和存储凭据密钥相互独立。

### `password_hash_max_concurrency`

这是每个 AsterDrive 进程同时执行的 Argon2 密码哈希或验证任务上限，默认是 `2`。Argon2 使用 blocking 线程池执行，不会占住 Actix 异步 worker；超过上限的认证任务会异步等待。

当前默认密码策略每个任务使用约 `64 MiB` 工作内存，因此默认上限对应每个进程最多约 `128 MiB` 的并发 Argon2 工作内存。多 Primary 部署时，这个限制按进程分别计算。小内存实例可以降到 `1`；提高之前要同时核对实例内存、登录并发和 WebDAV/MFA 认证流量。值必须大于 `0`，修改后需要重启。

服务会继续验证旧版较低参数的密码 hash，并在用户成功登录、WebDAV 凭据成功验证或分享密码成功验证后渐进升级；这个过程不会修改明文密码。

### `bootstrap_insecure_cookies`

- **纯 HTTP 首次试跑** —— 临时设 `true`
- **正式 HTTPS 部署** —— 保持 `false`

它**只影响第一次初始化** `auth_cookie_secure` 时写入的默认值。如果数据库里已经有这个运行时设置，再改这里不会回写旧值。首次试跑的整体流程见 [首次启动与第一个管理员](/start/first-admin/)。

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
webdav_auth_cache_secret = "replace-with-webdav-auth-cache-secret"
bootstrap_insecure_cookies = false
```

环境变量覆盖：

```bash
ASTER__AUTH__JWT_SECRET="replace-with-your-own-secret"
ASTER__AUTH__SHARE_COOKIE_SECRET="replace-with-share-cookie-secret"
ASTER__AUTH__DIRECT_LINK_SECRET="replace-with-direct-link-secret"
ASTER__AUTH__MFA_SECRET_KEY="replace-with-another-stable-secret"
ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY="replace-with-storage-credential-secret"
ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET="replace-with-webdav-auth-cache-secret"
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

具体说明见 [系统设置](/reference/config/runtime/)；这些开关的使用场景见 [注册、登录与 SSO](/admin/auth-sso/)。
