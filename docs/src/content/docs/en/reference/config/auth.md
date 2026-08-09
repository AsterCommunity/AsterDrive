---
description: "AsterDrive `config.toml` `[auth]` static authentication fields: the six signing and encryption secrets, the Argon2 concurrency limit, bootstrap_insecure_cookies, and the list of runtime auth settings maintained in the admin console."
title: "Login and Sessions"
---

:::tip[This page now holds only static fields and the settings list]
- This page: `[auth]` in `config.toml` — **static authentication startup settings** (signing secrets, the Argon2 concurrency limit, first plain-HTTP bootstrap), plus the list of runtime auth settings
- The first admin and the login-page state machine — [First Start and the First Admin](/en/start/first-admin/)
- Registration switch, MFA policy, Passkey switch, external auth onboarding — [Registration, Login and SSO](/en/admin/auth-sso/)
- Users binding MFA / Passkeys / external identities themselves — [Account and Security](/en/using/account-security/)
:::

## `[auth]` in `config.toml`

```toml
[auth]
jwt_secret = "<random secret generated on first startup>"
share_cookie_secret = "<random secret generated on first startup>"
direct_link_secret = "<random secret generated on first startup>"
mfa_secret_key = "<random secret generated on first startup>"
storage_credential_secret_key = "<random secret generated on first startup>"
webdav_auth_cache_secret = "<random secret generated on first startup>"
password_hash_max_concurrency = 2
bootstrap_insecure_cookies = false
```

### `jwt_secret`

When the configuration is generated for the first time, the service writes a random secret. You can think of it as the "site-wide login signing secret".

:::caution[Keep it stable in production]
Once changed:
- All current login sessions become invalid
- Everyone must log in again
:::

### `share_cookie_secret`

This is the HMAC secret for public-share password verification cookies. Changing it invalidates already verified share-password cookies, so users must enter the share password again.

### `direct_link_secret`

This is the HMAC secret for public direct links, preview links, and share streaming sessions. Changing it invalidates existing direct links and short-lived preview / streaming session tokens, so links must be regenerated.

### `mfa_secret_key`

This is the server-side encryption key for MFA/TOTP secrets. When the configuration is generated for the first time, the service automatically writes a random value.

:::caution[Preserve it during backup and migration]
If users have already enabled MFA, do not casually replace it while migrating, restoring, or rebuilding `config.toml`.

Once changed, existing authenticator secrets can no longer be decrypted, and users with MFA enabled cannot complete two-step verification with their old authenticator. An administrator can only reset that user's MFA from `Admin -> Users -> User Details -> Security Actions`, then ask the user to bind an authenticator again and save new recovery codes.
:::

### `storage_credential_secret_key`

This is the server-side encryption master key for storage connector credentials. The service writes a random value when configuration is first generated; a derived key encrypts static secrets, authorization-application secrets, and OAuth tokens with AES-256-GCM. API responses and audit logs do not echo plaintext credentials.

:::tip[Credentials covered by this key]
It protects connector-owned, versioned payloads in `storage_policy_connector_credentials.ciphertext`: static secrets for S3, Alibaba Cloud OSS, Azure Blob, Tencent COS, and SFTP, plus Microsoft Graph Client Secrets, access tokens, and refresh tokens for OneDrive.

The Local and Remote Node connectors declare `credential_mode = none` and create no payload in this connector-credential table. Internal-protocol credentials between nodes belong to the separate remote-node management boundary.
:::

:::caution[Preserve it during backup and migration]
As long as any storage policy has saved connector credentials, do not replace this key while migrating, restoring, or rebuilding `config.toml`.

Once changed or lost, saved static secrets and OAuth credentials can no longer be decrypted. Static-credential backends need their secrets entered again; old OneDrive refresh tokens cannot be recovered, so an administrator must re-run authorization for each affected policy from `Admin -> Storage Policies -> target OneDrive policy -> Authorize`.

Back up the entire `[auth]` section together with this key before upgrading or moving hosts.
:::

### `webdav_auth_cache_secret`

This is the dedicated HMAC secret for WebDAV authentication cache keys. It prevents a leaked Redis key list from exposing a directly testable SHA-256 target for valid passwords. Every Primary sharing the same Redis cache must use the same value.

Changing it makes all existing WebDAV authentication cache entries miss and repopulate after successful authentication; old keys remain only for their existing 60-second TTL. It is independent from the JWT, share-cookie, direct-link, MFA, and storage-credential secrets.

### `password_hash_max_concurrency`

This is the maximum number of Argon2 password hashing or verification tasks that each AsterDrive process runs at the same time. The default is `2`. Argon2 runs on the blocking thread pool instead of an Actix async worker; authentication work above the limit waits asynchronously.

The current default password policy uses about `64 MiB` of working memory per task, so the default limit permits about `128 MiB` of concurrent Argon2 working memory per process. In a multi-Primary deployment, each process enforces its own limit. Small-memory instances can reduce it to `1`; before increasing it, account for instance memory, concurrent logins, and WebDAV/MFA authentication traffic. The value must be greater than `0`, and changing it requires a restart.

The service continues to verify password hashes created with the previous lower work factor. After a successful user login, WebDAV credential check, or share-password verification, it progressively upgrades the stored hash without changing the plaintext password.

### `bootstrap_insecure_cookies`

- **First plain-HTTP trial run** - temporarily set it to `true`
- **Production HTTPS deployment** - keep it `false`

It **only affects the default value written when `auth_cookie_secure` is initialized for the first time**. If this runtime system setting already exists in the database, changing this value will not rewrite the old value. For the overall first-run flow, see [First Start and the First Admin](/en/start/first-admin/).

## Common Examples

### Local or Intranet HTTP Trial Run

```toml
[auth]
bootstrap_insecure_cookies = true
```

### Production HTTPS Deployment

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

Environment variable overrides:

```bash
ASTER__AUTH__JWT_SECRET="replace-with-your-own-secret"
ASTER__AUTH__SHARE_COOKIE_SECRET="replace-with-share-cookie-secret"
ASTER__AUTH__DIRECT_LINK_SECRET="replace-with-direct-link-secret"
ASTER__AUTH__MFA_SECRET_KEY="replace-with-another-stable-secret"
ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY="replace-with-storage-credential-secret"
ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET="replace-with-webdav-auth-cache-secret"
ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES=false
```

## The Settings You Actually Change Day to Day Are in the Admin Console

The following settings are not in `config.toml`; they are all maintained in the admin console:

- `auth_cookie_secure` - Whether cookies are sent only over HTTPS
- `auth_access_token_ttl_secs` - Access token TTL
- `auth_refresh_token_ttl_secs` - Refresh token TTL
- `auth_register_activation_ttl_secs` - Registration activation link TTL
- `auth_contact_change_ttl_secs` - Email address change link TTL
- `auth_password_reset_ttl_secs` - Password reset link TTL
- `auth_contact_verification_resend_cooldown_secs` - Verification email resend cooldown
- `auth_password_reset_request_cooldown_secs` - Password reset request cooldown
- `auth_email_code_login_enabled` - Whether email-code MFA is enabled
- `auth_email_code_login_allow_totp_fallback` - Whether users with TOTP enabled may use email codes as a fallback
- `auth_email_code_login_ttl_secs` - Email login code TTL
- `auth_email_code_login_resend_cooldown_secs` - Email login code resend cooldown
- `auth_passkey_login_enabled` - Whether users may sign in with registered Passkeys
- `auth_allow_user_registration` - Public registration switch
- `auth_register_activation_enabled` - Whether newly registered users must complete email activation first
- `auth_local_email_allowlist` - Email addresses or exact domains allowed for local registration and local email changes
- `auth_local_email_blocklist` - Email addresses or exact domains blocked for local registration and local email changes
- External authentication email verification, login email code, and related mail templates - maintained in the `Mail Delivery` group

See [runtime system settings](/en/reference/config/runtime/) for field details, and [Registration, Login and SSO](/en/admin/auth-sso/) for the scenarios these switches are used in.
