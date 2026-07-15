# Authentication

All paths below are relative to `/api/v1`.

## Endpoints

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/auth/check` | Return public authentication state: initialized or not, public registration allowed or not |
| `POST` | `/auth/setup` | Initialize the system and create the first admin |
| `POST` | `/auth/register` | Register an ordinary user after system initialization |
| `POST` | `/auth/register/resend` | Resend registration activation email |
| `GET` | `/auth/contact-verification/confirm` | Consume email verification token and redirect frontend |
| `POST` | `/auth/password/reset/request` | Request password reset email |
| `POST` | `/auth/password/reset/confirm` | Complete password reset with token |
| `POST` | `/auth/login` | Log in and write auth cookies |
| `POST` | `/auth/mfa/challenge/verify` | Complete MFA challenge and write auth cookies |
| `POST` | `/auth/mfa/challenge/email-code/send` | Send email code for the active MFA login flow |
| `POST` | `/auth/passkeys/login/start` | Start WebAuthn passkey login challenge |
| `POST` | `/auth/passkeys/login/finish` | Finish passkey login and write auth cookies |
| `GET` | `/auth/external-auth/providers` | List anonymous-visible external auth providers |
| `POST` | `/auth/external-auth/{kind}/{provider}/start` | Start external-auth login |
| `GET` | `/auth/external-auth/{kind}/{provider}/callback` | External-auth callback |
| `POST` | `/auth/external-auth/email-verification/start` | Send email verification for external-auth fallback |
| `GET` | `/auth/external-auth/email-verification/confirm` | Finish external-auth email verification and redirect |
| `POST` | `/auth/external-auth/password-link` | Link external identity to an existing account using local password |
| `POST` | `/auth/refresh` | Rotate access / refresh token using refresh cookie |
| `POST` | `/auth/logout` | Clear auth cookies |
| `GET` | `/auth/me` | Read current user info |
| `GET` | `/auth/sessions` | List current user's active login sessions |
| `DELETE` | `/auth/sessions/others` | Revoke all sessions except the current refresh session |
| `DELETE` | `/auth/sessions/{id}` | Revoke one login session |
| `PUT` | `/auth/password` | Change current user's password |
| `GET` | `/auth/mfa` | Read current user's MFA state |
| `POST` | `/auth/mfa/totp/setup/start` | Start TOTP MFA setup |
| `POST` | `/auth/mfa/totp/setup/finish` | Verify TOTP and enable MFA |
| `DELETE` | `/auth/mfa/factors/{id}` | Delete current user's MFA factor |
| `POST` | `/auth/mfa/recovery-codes/regenerate` | Regenerate MFA recovery codes |
| `GET` | `/auth/passkeys` | List current user's registered passkeys |
| `POST` | `/auth/passkeys/register/start` | Start passkey registration challenge |
| `POST` | `/auth/passkeys/register/finish` | Finish passkey registration |
| `PATCH` | `/auth/passkeys/{id}` | Rename passkey |
| `DELETE` | `/auth/passkeys/{id}` | Delete passkey |
| `GET` | `/auth/external-auth/links` | List current user's linked external identities |
| `DELETE` | `/auth/external-auth/links/{id}` | Unlink external identity |
| `POST` | `/auth/email/change` | Request email change |
| `POST` | `/auth/email/change/resend` | Resend email-change confirmation |
| `PATCH` | `/auth/preferences` | Update current user's preferences |
| `PATCH` | `/auth/profile` | Update current user's profile |
| `POST` | `/auth/profile/avatar/upload` | Upload avatar image |
| `PUT` | `/auth/profile/avatar/source` | Switch avatar source |
| `GET` | `/auth/events/storage` | Subscribe to storage-change events for visible workspaces |
| `GET` | `/auth/profile/avatar/{size}` | Read current user's uploaded avatar |

## Initialization and registration

- `POST /auth/check` returns `has_users` and `allow_user_registration`; it only tells the frontend which high-level state to show and does not expose whether a specific account exists
- `POST /auth/setup` is the only endpoint that can create the first admin and is available only before any user exists
- `POST /auth/register` requires completed setup and never grants the admin role; after initialization it is available when `auth_allow_user_registration = true`, and the default quota comes from `default_storage_quota`
- `POST /auth/register/resend` resends activation mail for accounts that have not completed activation

Resend request:

```json
{
  "identifier": "admin@example.com"
}
```

Public resend and password-recovery flows apply minimum response-time padding to avoid directly exposing account existence.

Local-account email policy is controlled by two runtime config keys:

- `auth_local_email_allowlist`: allowed exact normalized email addresses or exact ASCII domains; an empty list means no allowlist restriction
- `auth_local_email_blocklist`: blocked exact normalized email addresses or exact ASCII domains; blocklist wins over allowlist

The policy applies to local registration and local email changes only. Internationalized domains must be stored in punycode form. These keys are not a CORS allowlist and do not configure external-auth provider domain restrictions.

`/auth/setup` and `/auth/register` use the same body:

```json
{
  "username": "admin",
  "email": "admin@example.com",
  "password": "password"
}
```

Before setup, `/auth/register` returns `400` with `validation.system_not_initialized`, regardless of the public-registration setting. After setup, disabling public registration makes `/auth/register` return `403`, while `/auth/setup` always reports that the system is already initialized.

## Login state

`POST /auth/login` body:

```json
{
  "identifier": "admin",
  "password": "password"
}
```

Successful login writes two HttpOnly cookies:

- `aster_access`
- `aster_refresh`

`aster_refresh` has Cookie Path `/api/v1/auth`, so it is sent only to `/api/v1/auth/*`.

Related endpoints:

- `POST /auth/refresh`: atomically consumes the old refresh token and issues a new access / refresh pair. Reusing an old refresh token is treated as token replay and revokes all sessions for that user.
- `POST /auth/logout`: clears both auth cookies and revokes the current refresh token
- `GET /auth/me`: supports both cookies and `Authorization: Bearer <jwt>`
- `GET /auth/sessions`: lists active login devices / sessions; current session is marked when a refresh cookie is present
- `DELETE /auth/sessions/others`: requires the request to identify the current refresh session
- `DELETE /auth/sessions/{id}`: revokes one session and clears cookies when the current session is removed

Disabled users cannot log in.

`GET /auth/me` supports a `fields` query such as `GET /auth/me?fields=profile,preferences,quota,session`. Supported groups are `profile`, `preferences`, `quota`, and `session`. Missing or empty fields returns the full model; unknown fields return `400`.

If `must_change_password` is set for the user, successful login completion returns `password_change_required` instead of normal app access:

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

This can happen after password login, MFA challenge verification, passkey login finish, or external-auth login finish. The response writes auth cookies, but the access token is scoped to the forced password-change flow. Until `PUT /auth/password` succeeds, only these authenticated routes are allowed:

- `GET /auth/me`
- `PUT /auth/password`
- `POST /auth/logout`

Other authenticated routes return `403` with `auth.password_change_required`. `POST /auth/refresh` is also rejected while the user still has `must_change_password` or the token carries password-change scope. `PUT /auth/password` still validates `current_password`, clears `must_change_password` after success, rotates sessions, and writes a normal auth cookie pair.

## MFA login flow

If the user has TOTP enabled, or email-code MFA is available for the verified email user, `POST /auth/login` returns `mfa_required` instead of writing cookies:

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

`methods` is the actual method list available for this login flow:

- `totp`: user has an enabled TOTP factor
- `recovery_code`: TOTP is enabled and unused recovery codes remain
- `email_code`: email is verified, `auth_email_code_login_enabled = true`, SMTP is available, and TOTP fallback policy allows it if TOTP is already enabled

Then the frontend calls:

```json
{
  "flow_token": "mfa_xxx",
  "method": "totp",
  "code": "123456"
}
```

Supported methods are `totp`, `recovery_code`, and `email_code`.

For `email_code`, call `POST /auth/mfa/challenge/email-code/send` first:

```json
{
  "flow_token": "mfa_xxx"
}
```

Success:

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

Email-code TTL is bounded by both `auth_email_code_login_ttl_secs` and the remaining MFA flow lifetime. Resend cooldown comes from `auth_email_code_login_resend_cooldown_secs`.

MFA flow expires after 5 minutes by default and allows at most 5 wrong attempts. Expired, consumed, or exhausted flows require a fresh first-factor login.

## Passkey login and management

Passkey uses a two-step WebAuthn flow. Challenge responses and credential request bodies keep the browser-native WebAuthn JSON shape.

Login:

- `POST /auth/passkeys/login/start`: body may include `{ "identifier": "alice", "conditional": false }`; `identifier` may be omitted for conditional UI / discoverable credentials
- `POST /auth/passkeys/login/finish`: body is `{ "flow_id": "...", "credential": { ... } }`; success writes the same cookies as password login

Registration and management require login:

- `GET /auth/passkeys`
- `POST /auth/passkeys/register/start`: body may include `{ "name": "MacBook Touch ID" }`
- `POST /auth/passkeys/register/finish`: body is `{ "flow_id": "...", "credential": { ... }, "name": "MacBook Touch ID" }`
- `PATCH /auth/passkeys/{id}` with `{ "name": "New name" }`
- `DELETE /auth/passkeys/{id}`

Passkeys are stored in the `passkeys` table. Credentials are stored as strongly typed wrapped JSON. The server requires discoverable credentials.

`auth_passkey_login_enabled = false` disables anonymous passkey sign-in and hides the login entry point from current frontend bootstrap config, but it does not delete registered credentials. Logged-in users can still manage their saved passkeys.

## MFA management

MFA self-management requires login. Persistent factors currently support TOTP. Login challenges can use TOTP, one-time recovery code, or email code. Email code is not a persistent factor; it exists only inside the login flow and is stored in `mfa_email_codes`.

`GET /auth/mfa` returns:

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

TOTP setup:

- `POST /auth/mfa/totp/setup/start`: returns `flow_token`, `expires_in`, Base32 `secret`, and `otpauth_uri`
- `POST /auth/mfa/totp/setup/finish`: body is `{ "flow_token": "...", "code": "123456", "name": "Phone" }`

On success, the server returns the factor and recovery codes:

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

## External authentication

Supported provider kinds are `oidc`, `generic_oauth2`, `github`, `qq`, `google`, and `microsoft`.

Anonymous provider list:

- `GET /auth/external-auth/providers`

Login flow:

- `POST /auth/external-auth/{kind}/{provider}/start`
- `GET /auth/external-auth/{kind}/{provider}/callback`

Fallback / binding flow:

- `POST /auth/external-auth/email-verification/start`
- `GET /auth/external-auth/email-verification/confirm`
- `POST /auth/external-auth/password-link`

User self-management:

- `GET /auth/external-auth/links`
- `DELETE /auth/external-auth/links/{id}`

The `oidc` driver uses discovery, PKCE, nonce, and ID Token validation. The `generic_oauth2` driver uses manually configured endpoints, PKCE, token exchange, and UserInfo claim mapping. Dedicated `github`, `qq`, `google`, and `microsoft` providers use backend-fixed endpoints and claim semantics; callers should not send manual OAuth endpoint fields for those provider kinds. Microsoft tenant selection lives under `options.microsoft.tenant`. See [External Authentication Module](../external-auth.md).

## Profile, preferences, avatars, and events

- `PATCH /auth/preferences` updates structured user preferences
- `PATCH /auth/profile` updates display profile fields
- `POST /auth/profile/avatar/upload` stores an uploaded avatar image
- `PUT /auth/profile/avatar/source` switches between uploaded / gravatar / none where supported
- `GET /auth/profile/avatar/{size}` returns the uploaded avatar binary
- `GET /auth/events/storage` is an SSE stream for storage-change events visible to the current user

Uploaded avatars are raw binary responses and do not use the JSON wrapper.
