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
| Driver registry | `src/external_auth/registry.rs` | Registers `oidc`, `generic_oauth2`, `github`, and `google` |
| OIDC driver | `src/external_auth/providers/oidc.rs` | Discovery, PKCE, nonce, and ID token validation |
| Generic OAuth2 driver | `src/external_auth/providers/oauth2.rs` | Manual endpoints, PKCE, token exchange, and UserInfo claim mapping |
| GitHub driver | `src/external_auth/providers/github.rs` | Reuses the OAuth2 driver, fixes GitHub endpoints, and fetches the verified primary email from `/user/emails` |
| Google driver | `src/external_auth/providers/google.rs` | Reuses the OIDC driver, fixes Google Accounts issuer, default scopes, and claim semantics |

## Supported provider kinds

Current supported provider kinds are:

- `oidc`
- `generic_oauth2`
- `github`
- `google`

All provider kinds are configured by admins and shown on the login page only after being enabled.

| kind | protocol | default scopes | endpoint source |
| --- | --- | --- | --- |
| `oidc` | `oidc` | `openid email profile` | `issuer_url` discovery |
| `generic_oauth2` | `oauth2` | `openid email profile` | Admin-configured authorization / token / userinfo URLs |
| `github` | `oauth2` | `read:user user:email` | Fixed GitHub authorization / token / user / user-emails URLs |
| `google` | `oidc` | `openid profile email` | Fixed Google Accounts issuer / discovery |

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

Useful commands:

- `cargo test --test test_oauth2`
- `cargo test --test test_oidc`
- `cargo test --lib external_auth::providers::github`
- `cargo test --lib external_auth::providers::google`
- `cargo clippy --lib --tests -- -D warnings`

Frontend provider-kind, form, summary, and stale-request tests live under:

- `frontend-panel/src/pages/admin/AdminExternalAuthPage.test.tsx`
- `frontend-panel/src/components/admin/admin-external-auth-page/*.test.tsx`

## Known limitations

- Generic OAuth2 currently has no explicit client-auth-method setting; it supports public clients and `client_secret_post`.
- Generic OAuth2 does not validate ID tokens because it consumes only access token + UserInfo.
- Microsoft can currently be configured manually through OIDC. Manual Microsoft configuration should prefer a concrete tenant issuer because the current OIDC driver strictly requires the ID token issuer to equal the provider `issuer_url`. The dedicated Microsoft preset is tracked in <https://github.com/AptS-1547/AsterDrive/issues/263>.
- `groups_claim` and `avatar_url_claim` exist in the provider configuration model, but the login resolver currently persists identity, email, display name, and username snapshots only.
