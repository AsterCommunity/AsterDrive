# External Authentication Module

This document explains the current external-authentication implementation in the repository, not a future plan.

## What this module does

External authentication lets users sign in through external identity providers such as OpenID Connect or Generic OAuth2. The module also covers account binding, email verification fallback, auto-provisioning, and admin-side provider management.

## Code locations

| Area | Path | Notes |
| --- | --- | --- |
| Route | `src/api/routes/auth/external_auth.rs` | Anonymous provider list, login start, callback, email verification fallback, password linking, user unbinding |
| Admin route | `src/api/routes/admin/external_auth.rs` | Provider kind list, provider CRUD, draft testing, saved provider testing |
| Service | `src/services/external_auth_service/` | Provider config, login flow, identity binding, and account provisioning |
| Entity / repo | `src/entities/external_auth_*`, `src/db/repository/external_auth_*` | Persistent provider and identity storage |
| Driver trait | `src/external_auth/driver.rs` | Shared driver interface and descriptors |
| Driver registry | `src/external_auth/registry.rs` | Registers `oidc`, `generic_oauth2`, `github`, `qq`, `google`, and `microsoft` |
| OIDC driver | `src/external_auth/providers/oidc.rs` | Discovery, PKCE, nonce, and ID token validation |
| Generic OAuth2 driver | `src/external_auth/providers/oauth2.rs` | Manual endpoints, PKCE, token exchange, and UserInfo claim mapping |
| GitHub driver | `src/external_auth/providers/github.rs` | Reuses the OAuth2 driver, fixes GitHub endpoints, and fetches the verified primary email from `/user/emails` |
| QQ driver | `src/external_auth/providers/qq.rs` | Dedicated QQ Connect OAuth2 flow: GET token, fetch openid, then call get_user_info |
| Google driver | `src/external_auth/providers/google.rs` | Reuses the OIDC driver, fixes Google Accounts issuer, default scopes, and claim semantics |
| Microsoft driver | `src/external_auth/providers/microsoft.rs` | Reuses the OIDC driver, normalizes Microsoft tenant / issuer input, and validates multi-tenant token issuers |

## Supported provider kinds

Current supported provider kinds are:

- `oidc`
- `generic_oauth2`
- `github`
- `qq`
- `google`
- `microsoft`

All provider kinds are configured by admins and shown on the login page only after being enabled.

| kind | protocol | default scopes | endpoint source |
| --- | --- | --- | --- |
| `oidc` | `oidc` | `openid email profile` | `issuer_url` discovery |
| `generic_oauth2` | `oauth2` | `openid email profile` | Admin-configured authorization / token / userinfo URLs |
| `github` | `oauth2` | `read:user user:email` | Fixed GitHub authorization / token / user / user-emails URLs |
| `qq` | `oauth2` | `get_user_info` | Fixed QQ authorization / token / openid / get_user_info URLs |
| `google` | `oidc` | `openid profile email` | Fixed Google Accounts issuer / discovery |
| `microsoft` | `oidc` | `openid profile email` | Microsoft tenant-derived issuer / discovery |

## High-level flow

1. An admin creates a provider in `Admin -> External Auth`.
2. The login page reads the enabled public summary and shows the corresponding entry.
3. The user starts the external login flow.
4. The provider redirects back to the callback endpoint.
5. The service resolves the returned identity.
6. Depending on provider settings and account state, the user is either:
   - signed in directly
   - linked to an existing local account
   - asked to complete email verification
   - asked to bind the external identity with a local password

## Important provider behaviors

### OIDC

- Uses discovery
- Uses PKCE and nonce validation
- Verifies the ID token

### Generic OAuth2

- Uses manually configured authorization, token, and userinfo endpoints
- Uses PKCE and token exchange
- Maps claims from the UserInfo response

Default UserInfo claim mapping:

| Field | Default claim | Notes |
| --- | --- | --- |
| `subject` | `sub`, falling back to `id` | Required |
| `email` | `email` | Must pass local email validation when present |
| `email_verified` | `email_verified` | Missing means `false` |
| `display_name` | `name` | Sanitized and truncated |
| `preferred_username` | `preferred_username` | Sanitized and truncated |

Custom claims support top-level keys, dotted paths, and JSON Pointers, such as `email`, `user.email`, or `/user/email`.

### GitHub

`github` is a dedicated provider kind. Its wire value is `github`; do not let Rust enum casing leak as `git_hub`.

It follows the same pattern as storage drivers such as S3-compatible / Tencent COS: reuse generic OAuth2 capabilities, then wrap them with provider-specific defaults and semantics.

Fixed behavior:

- protocol is `oauth2`
- authorization URL is `https://github.com/login/oauth/authorize`
- token URL is `https://github.com/login/oauth/access_token`
- userinfo URL is `https://api.github.com/user`
- user emails URL is derived from the userinfo URL as `/user/emails`
- default scopes are `read:user user:email`
- subject is read from `/user.id`
- username is read from `/user.login`
- display name is read from `/user.name`
- `/user.email` is not trusted
- email is accepted only from `/user/emails` where `primary=true` and `verified=true`

If GitHub does not return a verified primary email, the driver returns `email=None` and `email_verified=false`. The login service has a GitHub-specific boundary: when the provider enables `require_email_verified` and no verified primary email is returned, login is rejected with forbidden instead of falling back to local email verification.

The admin UI also has GitHub-specific behavior:

- create / edit panels show fixed endpoint guidance instead of editable endpoint fields
- rules panels show fixed claims instead of editable claim mapping
- the default icon is `/static/external-auth/github-logo.svg`
- the login entry, admin provider list, and `settings/security` linked-identity list prefer the configured icon and fall back to the provider-kind default icon

### Google

`google` is a dedicated provider kind. Its wire value is `google`.

It follows the same dedicated-wrapper pattern as GitHub, but reuses the generic OIDC driver instead of the OAuth2 driver.

Fixed behavior:

- protocol is `oidc`
- issuer defaults to `https://accounts.google.com`
- discovery is fixed to `https://accounts.google.com/.well-known/openid-configuration`
- default scopes are `openid profile email`
- subject is fixed to the ID token `sub`
- display name is fixed to the ID token `name`
- email is fixed to the ID token `email`
- email verification is fixed to the ID token `email_verified`
- avatar URL claim is preset to the ID token `picture`
- manual authorization / token / userinfo endpoints are not supported

The Google provider still allows tests to pass a loopback issuer so integration tests can use the local mock OIDC server; the production admin UI does not expose the issuer input. External identities must be linked by the stable `sub`, not by email. Google API / Google Drive authorization is a later resource-access capability and should not be mixed into the login provider's default scopes.

The admin UI also has Google-specific behavior:

- create / edit panels show fixed issuer and discovery guidance instead of editable issuer / endpoint fields
- rules panels show fixed claims instead of editable claim mapping
- the default icon is `/static/external-auth/google-logo.svg`
- the login entry, admin provider list, and `settings/security` linked-identity list prefer the configured icon and fall back to the provider-kind default icon

### Microsoft

`microsoft` is a dedicated provider kind. Its wire value is `microsoft`.

It signs users in through OIDC and reuses the generic OIDC driver for authorization start, discovery, PKCE, nonce, and ID token validation. The dedicated layer adds Microsoft identity platform tenant / issuer semantics.

Fixed behavior:

- protocol is `oidc`
- default tenant is `common`
- tenant may be a concrete tenant ID, `common`, `organizations`, or `consumers`
- issuer is normalized to `https://login.microsoftonline.com/{tenant}/v2.0`
- discovery is fixed to `https://login.microsoftonline.com/{tenant}/v2.0/.well-known/openid-configuration`; do not use URL join in a way that falls back to v1 metadata
- default scopes are `openid profile email`
- subject is fixed to the ID token `sub`
- display name is fixed to the ID token `name`
- email is fixed to the ID token `email`
- it does not declare `email_verified` and does not treat `email` as a GitHub-style verified primary email
- manual authorization / token / userinfo endpoints are not supported
- `require_email_verified` defaults to false

Microsoft v2 multi-tenant discovery may expose a template-like issuer, while real ID tokens come from concrete tenant issuers. The Microsoft callback exchange still uses `openidconnect` for signature, audience, nonce, and expiration validation, then applies Microsoft-specific issuer validation: concrete tenant issuers must match exactly; `common` accepts organization and personal-account tenants; `organizations` rejects the Microsoft Account tenant; `consumers` only accepts the Microsoft Account tenant `9188040d-6c67-4c5b-b112-36a304b66dad`.

Microsoft App Registration should add a Web redirect URI because AsterDrive performs the authorization-code token exchange on the backend. Do not register the callback URL under a public/native client platform and then send a Client Secret, otherwise Microsoft returns `AADSTS90023`. The Client Secret must be the generated `Value` shown when creating a secret in Azure / Entra, not the `Secret ID`; `AADSTS7000215` usually means this value was copied incorrectly.

Microsoft may omit the email claim, and the email claim should not be treated as verified by default. When the identity cannot be resolved directly, the login service continues through the existing email-verification / password-binding flow.

The admin UI also has Microsoft-specific behavior:

- create / edit panels show tenant input plus derived issuer / discovery guidance instead of editable issuer / endpoint fields
- rules panels show fixed claims instead of editable claim mapping
- the default icon is `/static/external-auth/microsoft-logo.svg`
- the login entry, admin provider list, and `settings/security` linked-identity list prefer the configured icon and fall back to the provider-kind default icon

### QQ

`qq` is the dedicated QQ Connect OAuth2 provider kind. Its wire value is `qq`. It cannot reuse Generic OAuth2's "POST token -> Bearer UserInfo -> read subject/email directly" model because QQ first requires an access token, then `/oauth2.0/me` for `openid`, and then `get_user_info` with `access_token`, `oauth_consumer_key`, and `openid`.

Fixed behavior:

- protocol is `oauth2`
- authorization URL is fixed to `https://graph.qq.com/oauth2.0/authorize`
- token URL is fixed to `https://graph.qq.com/oauth2.0/token`
- openid URL is fixed to `https://graph.qq.com/oauth2.0/me`
- userinfo URL is fixed to `https://graph.qq.com/user/get_user_info`
- default scope is `get_user_info`
- token exchange uses GET per QQ documentation and explicitly sends `fmt=json`
- openid requests explicitly send `fmt=json` to avoid QQ's default JSONP response
- subject is fixed to `openid`
- identity namespace is `qq:{client_id}` to avoid mixing openid values from different QQ App IDs
- display name is read from `get_user_info.nickname`
- email is not returned and `email_verified` is not declared
- `require_email_verified` defaults to false

When QQ callbacks do not include email, the existing missing-email path applies: existing external identity bindings may still sign in, while unbound identities enter email verification / password binding. QQ should not trigger verified-email auto-linking.

The admin UI also has QQ-specific behavior:

- create / edit panels show fixed authorization, token, OpenID, and get_user_info endpoints instead of editable endpoint fields
- rules panels show fixed claims instead of editable claim mapping
- the default icon is `/static/external-auth/qq-logo.svg`
- the login entry, admin provider list, and `settings/security` linked-identity list prefer the configured icon and fall back to the provider-kind default icon

## Provider App Registration Entry Points

Deployment-facing documentation should tell admins where to create each application / Client ID:

- OIDC / Generic OAuth2: the IdP admin console Applications / Clients page, for example Logto, Authentik, Keycloak, or Zitadel.
- Logto example: Logto Cloud Console <https://cloud.logto.io/>; self-hosted deployments use `https://<logto-host>/console`.
- GitHub: personal account <https://github.com/settings/developers>; organizations use `https://github.com/organizations/{org}/settings/applications`.
- QQ: QQ Connect management center <https://connect.qq.com/manage.html>, create a website application, then register the AsterDrive callback URL.
- Google: Google Cloud Console Credentials <https://console.cloud.google.com/apis/credentials>; create an OAuth client ID, choose Web application, open the OAuth client ID, and add the AsterDrive callback under Authorized redirect URIs.
- Microsoft: Microsoft Entra admin center <https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade> or Azure portal <https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade>; open the app, go to Authentication, choose Add a platform -> Web, paste the AsterDrive callback into Redirect URIs, then create a client secret under Certificates & secrets and copy its generated `Value`.

All entries should use the AsterDrive-generated `/api/v1/auth/external-auth/{kind}/{provider}/callback` as the redirect URI.

## Account provisioning and binding

The service supports several account-resolution paths:

- If the external identity already has a local binding, sign in directly
- If verified email auto-linking is enabled and the provider returns a verified email, find the local user with the same email and create a binding
- If auto-provisioning is enabled, check the public registration switch, email, email domain, and email verification policy, then create a normal user and bind the identity
- If the identity cannot be resolved directly, create an email verification flow or ask the user to bind through their local password

When auto-provisioning a user, the system creates a random internal password. The user can still later manage the account through normal local password reset / change flows.

The GitHub `require_email_verified` missing-email rejection lives in `login.rs`, not in the generic `resolution.rs` path. If another provider has non-substitutable external email-verification semantics, add and test that boundary explicitly.

## API entry points

- Admin provider API: [`./api/admin.md#external-authentication-providers`](./api/admin.md)
- Login-side external-auth API: [`./api/auth.md#external-authentication`](./api/auth.md)
- Deployment-facing configuration guide: [`../../docs/config/external-auth.md`](../../docs/config/external-auth.md)

## Testing

Key tests cover:

- provider CRUD
- callback and identity resolution
- verified-email linking
- auto-provisioning policy checks
- email verification fallback
- password binding
- unlinking external identities
- GitHub verified-primary-email handling and `/user.email` bypass prevention
- Google fixed descriptor defaults, `sub` stability, email changes not creating new identities, and `email_verified=false` / missing / non-boolean rejection
- Microsoft fixed descriptor defaults, tenant / issuer normalization, multi-tenant issuer validation, concrete-tenant issuer exact matching, missing-email local verification flow, and default no verified-email requirement

Useful commands:

- `cargo test --test test_oauth2`
- `cargo test --test test_oidc`
- `cargo test --lib external_auth::providers::github`
- `cargo test --lib external_auth::providers::google`
- `cargo test --lib external_auth::providers::microsoft`
- `cargo clippy --lib --tests -- -D warnings`

Frontend provider-kind, form, summary, and stale-request tests live under:

- `frontend-panel/src/pages/admin/AdminExternalAuthPage.test.tsx`
- `frontend-panel/src/components/admin/admin-external-auth-page/*.test.tsx`

## Known limitations

- Generic OAuth2 currently has no explicit client-auth-method setting; it supports public clients and `client_secret_post`.
- Generic OAuth2 does not validate ID tokens because it consumes only access token + UserInfo.
- `groups_claim` and `avatar_url_claim` exist in the provider configuration model, but the login resolver currently persists identity, email, display name, and username snapshots only.
