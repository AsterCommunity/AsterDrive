# Login and Sessions

::: tip This page explains two layers
- `[auth]` in `config.toml` - **only handles static startup bootstrap**: signing secrets and the first plain-HTTP bootstrap
- `Admin -> System Settings` - **daily rules**: public registration, cookies, token TTLs, activation/reset links, and cooldowns

In normal operation, almost everything you actually change is in the admin console. The static part on this page is usually touched only once during initial deployment or migration to another machine.
:::

## `[auth]` in `config.toml`

```toml
[auth]
jwt_secret = "<random secret generated on first startup>"
share_cookie_secret = "<random secret generated on first startup>"
direct_link_secret = "<random secret generated on first startup>"
mfa_secret_key = "<random secret generated on first startup>"
storage_credential_secret_key = "<random secret generated on first startup>"
bootstrap_insecure_cookies = false
```

### `jwt_secret`

When the configuration is generated for the first time, the service writes a random secret. You can think of it as the "site-wide login signing secret".

::: warning Keep it stable in production
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

::: warning Preserve it during backup and migration
If users have already enabled MFA, do not casually replace it while migrating, restoring, or rebuilding `config.toml`.

Once changed, existing authenticator secrets can no longer be decrypted, and users with MFA enabled cannot complete two-step verification with their old authenticator. An administrator can only reset that user's MFA from `Admin -> Users -> User Details -> Security Actions`, then ask the user to bind an authenticator again and save new recovery codes.
:::

### `storage_credential_secret_key`

This is the server-side encryption master key for the Microsoft Graph credentials (Client Secret, access token, refresh token) used by OneDrive storage policies. When the configuration is generated for the first time, the service automatically writes a random value; a key derived from it encrypts the credentials at rest with AES-256-GCM, and API responses and audit logs expose only boolean state such as `client_secret_configured`.

::: tip This key currently only covers OneDrive
It protects `storage_connector_application_configs.client_secret_ciphertext` and the access / refresh token ciphertext in the `storage_policy_credentials` table.

The `access_key` / `secret_key` for S3, Azure Blob, and Tencent COS, as well as remote node (follower) credentials, **are currently stored in plaintext** and do not depend on this key — rotating it does not affect those drivers.
:::

::: warning Preserve it during backup and migration
As long as any OneDrive policy has completed Microsoft Graph authorization, do not casually replace it while migrating, restoring, or rebuilding `config.toml`.

Once changed or lost, the encrypted Client Secret and OAuth tokens can no longer be decrypted, and every OneDrive policy enters a requires-reauthorization state. Old refresh tokens cannot be recovered; an administrator must re-run the authorization flow for each policy from `Admin -> Storage Policies -> target OneDrive policy -> Authorize`.

Back up the entire `[auth]` section together with this key before upgrading or moving hosts.
:::

### `bootstrap_insecure_cookies`

- **First plain-HTTP trial run** - temporarily set it to `true`
- **Production HTTPS deployment** - keep it `false`

It **only affects the default value written when `auth_cookie_secure` is initialized for the first time**. If this runtime system setting already exists in the database, changing this value will not rewrite the old value.

## The Login Page Chooses the Flow by State

The login page is not a fixed "login" or "register" page. It follows the current state:

- **There are no users in the system yet** - enter initialization and create the first administrator directly
- **Users already exist, and the entered account exists** - log in
- **Users already exist, the entered account is new, and the administrator allows public registration** - create a normal account
- **The administrator enabled an external authentication provider** - the login page shows the corresponding external login entry
- **The current browser supports Passkey** - the login page shows the Passkey login entry, and accounts with registered Passkeys can log in directly with device unlock or a security key
- **The account needs MFA** - after password or external identity succeeds, the user must complete a second verification step. This may be an authenticator code, a recovery code, or an email code enabled by the administrator.

Important details:

- The first account becomes an administrator directly and does not go through email activation
- Later normal accounts created through public registration must click the activation email before logging in
- After the administrator disables public registration, the login page only keeps login and password recovery

## MFA Multi-Factor Authentication

Users enable MFA themselves here:

```text
Settings -> Security -> Multi-Factor Authentication
```

The factor users can bind themselves is a TOTP authenticator app. Common apps include 1Password, Bitwarden, Google Authenticator, and Microsoft Authenticator.

The enablement flow is roughly:

1. Open `Settings -> Security -> Multi-Factor Authentication`
2. Click set up authenticator
3. Scan the QR code with an authenticator app, or enter the secret manually
4. Enter the 6-digit code generated by the authenticator to finish binding
5. Download or copy recovery codes, and save them in a password manager or another secure location

Recovery codes are shown in plaintext only once when generated, and each recovery code can be used only once. If the authenticator is lost, a recovery code can complete MFA verification on the login page. After logging in, regenerate recovery codes or bind an authenticator again as soon as possible.

After MFA is enabled, these login methods enter second-step verification:

- Local password login
- External authentication login

Passkey login does not enter the MFA challenge described here. It relies on device unlock or a security key for user verification, and is a separate login path from "password/external identity + TOTP".

The MFA login verification flow is valid for `5` minutes by default and allows at most `5` attempts. If verification expires or attempts are exhausted, return to the login page and start again.

### Email Code MFA

Administrators can enable email-code MFA in the admin console:

```text
Admin -> System Settings -> Authentication and Cookies -> Require Email Code MFA
```

After it is enabled, users with a verified email address can complete the MFA step with an 8-digit email code after password or external identity login. This feature depends on working mail delivery: SMTP host and sender address must be set, and SMTP username and password must either both be filled or both be empty.

Default rules:

- Email codes are valid for `10` minutes by default, but never longer than the remaining lifetime of the current MFA login flow
- The same user cannot resend a code within `60` seconds by default
- With only `Require Email Code MFA` enabled, users who do not have TOTP enabled and have a verified email can use email codes
- If `Allow TOTP Email Fallback` is also enabled, users who already have an authenticator can also use email codes as an additional login verification method

::: warning Be careful with email fallback
Email codes depend on the security of the user's mailbox. For stricter deployments, email-code MFA is usually used only for verified-email users without an authenticator. Whether TOTP users may fall back to email should follow your site's security policy.
:::

If a user loses both the authenticator and recovery codes, an administrator can reset it here:

```text
Admin -> Users -> User Details -> Security Actions -> Reset MFA
```

Resetting clears that user's authenticator, recovery codes, and unfinished MFA login flows, and invalidates the user's existing sessions. The user must set up MFA again after the next login.

## Passkey Login

Passkey is a login method managed by each user. The entry point is:

```text
Settings -> Security -> Passkey
```

Users can:

- Add new Passkeys
- Rename a Passkey, such as `MacBook`, `iPhone`, or a specific security key
- View creation time and last-used time
- Delete Passkeys that are no longer used

When adding a Passkey, the browser opens the system WebAuthn / Passkey verification window. For production deployments, first configure `Admin -> System Settings -> Site Configuration -> Public Site URL` correctly and use HTTPS. Local `localhost` / `127.0.0.1` debugging is the exception. Browsers usually expose the full Passkey capability only in secure contexts.

Passkeys do not replace local passwords. Users can continue logging in with passwords. After deleting a Passkey, only that device or security key can no longer log in to the current account directly.

Administrators can also temporarily disable Passkey sign-in in the admin console:

```text
Admin -> System Settings -> User Management -> Registration & Login -> Allow Passkey Sign-In
```

After it is disabled, users cannot complete login with registered Passkeys, but existing Passkeys are not deleted. After the setting is enabled again, previously registered Passkeys can be used again.

## External Authentication / SSO

Administrators can connect external identity providers here:

```text
Admin -> External Authentication
```

AsterDrive supports OpenID Connect, Generic OAuth2, and dedicated providers for GitHub, QQ, Google, and Microsoft. After creating a provider, the login page shows the corresponding external login entry. The administrator must register the generated redirect URI on the identity provider side. See [External Authentication](/en/config/external-auth) for the full setup guide.

The relationship between external identities and local users is determined by provider rules:

- Previously bound external identities log in to the corresponding local user directly
- After "auto-bind by verified email" is enabled, if the identity provider returns `email_verified=true` and exactly one local user has the same email address, the system can bind automatically
- After "auto-create local users" is enabled, unbound identities can automatically create normal users
- Without auto-bind or auto-create, the user must first log in to an existing account to complete binding, or continue through the email verification flow

Users can view and unlink external identities they have already bound here:

```text
Settings -> Security -> External Identities
```

If the administrator enabled auto-bind, an external login that matches the same rules may still bind back to the local account after the user unlinks it.

## Where Is the Public Registration Switch?

```text
Admin -> System Settings -> User Management -> Allow Public User Registration
```

After it is disabled:

- External users can no longer create new accounts from the login page
- The first-administrator initialization flow still exists
- Users manually created by administrators in the admin console still work

### Local Account Email Allowlist / Blocklist

If the site only allows company email addresses, or needs to block disposable email domains, configure these settings:

```text
Admin -> System Settings -> User Management -> Registration & Login -> Local Account Email Allowlist
Admin -> System Settings -> User Management -> Registration & Login -> Local Account Email Blocklist
```

These settings apply only to **local accounts**:

- Email addresses entered during public registration
- Local email address changes under `Settings -> Security`

They do not restrict external identities returned by third-party SSO. Email-domain rules for external authentication are still configured in each provider under `Admin -> External Authentication`.

Entries can be full email addresses or exact domains:

```text
alice@example.com
example.com
@example.com
```

`example.com` and `@example.com` are equivalent. They match `user@example.com`, but they do not automatically match `user@sub.example.com`. Internationalized domains must be entered in punycode form.

Rule order:

- The blocklist overrides the allowlist
- An empty allowlist means no allowlist restriction
- An empty blocklist means no additional blocked email addresses
- If both lists are empty, any valid email address can be used for local registration and local email changes

## Which Features Depend on Mail Configuration?

These features do not work without mail:

- Activation email after public registration
- Password recovery on the login page
- Email address change confirmation email in `Settings -> Security`
- Email verification flow when external authentication cannot directly match a local account
- Email-code MFA

::: warning Configure mail before enabling registration
If you do it in the wrong order, new user accounts may already be created but cannot receive activation emails, so they remain stuck at "waiting for activation".

Before enabling these capabilities, check together:
1. `Admin -> System Settings -> Mail Delivery`
2. `Admin -> System Settings -> Site Configuration -> Public Site URL`
3. If you are connecting external authentication, also check whether the redirect URI in `Admin -> External Authentication` has been registered on the identity provider side
:::

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
bootstrap_insecure_cookies = false
```

Environment variable overrides:

```bash
ASTER__AUTH__JWT_SECRET="replace-with-your-own-secret"
ASTER__AUTH__SHARE_COOKIE_SECRET="replace-with-share-cookie-secret"
ASTER__AUTH__DIRECT_LINK_SECRET="replace-with-direct-link-secret"
ASTER__AUTH__MFA_SECRET_KEY="replace-with-another-stable-secret"
ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY="replace-with-storage-credential-secret"
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

See [runtime system settings](/en/config/runtime) for details.
