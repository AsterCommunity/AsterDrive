---
description: AsterDrive identity and access feature map covering login sessions, MFA, Passkey, external authentication, WebDAV accounts, and public access boundaries.
---

# Identity and Access

Identity and access answers three questions: who is accessing, how they prove identity, and where the access boundary ends.

## Capability Boundaries

| Capability | Notes | Related docs |
| --- | --- | --- |
| Local account login | Username/email login, password validation, session cookies, access tokens | [Login and Sessions](/en/config/auth) |
| First administrator setup | Create the first admin when a new instance has no users | [Quick Start](/en/guide/getting-started), [First-Start Checklist](/en/deployment/runtime-behavior) |
| MFA | TOTP, recovery codes, email-code MFA, login-flow expiration and attempt limits | [Login and Sessions](/en/config/auth) |
| Passkey | User-managed Passkey registration and login | [Login and Sessions](/en/config/auth) |
| External authentication | OIDC, generic OAuth2, external identity binding, email verification, automatic user creation | [External Authentication](/en/config/external-auth) |
| WebDAV accounts | Dedicated WebDAV credentials, separate from the web login password | [WebDAV](/en/config/webdav), [User Manual](/en/guide/user-guide) |
| Public access | Share links, public preview, direct links, short-lived streaming tickets | [Sharing and Public Access](/en/guide/sharing), [Preview and Processing](./preview-processing) |

## Backend Modules

| Module | Owns |
| --- | --- |
| `auth_service` | Registration, passwords, sessions, email verification, login flow |
| `mfa_service` | MFA login flow, TOTP, recovery codes, email codes |
| `passkey_service` | Passkey registration, authentication, credential management |
| `external_auth_service` | Providers, login flow, identity resolution, account binding |
| `webdav::account` | WebDAV accounts and scoped access |
| `api/request_auth.rs`, `api/middleware` | Request authentication, admin permissions, auth context |

## Configuration Entry Points

| Entry point | Purpose |
| --- | --- |
| `Admin -> System Settings -> Authentication and Cookies` | Cookie, security token, and email-code MFA runtime rules |
| `Admin -> External Authentication` | Manage OIDC / OAuth2 providers |
| `Settings -> Security` | User MFA, Passkey, external identities, WebDAV credentials |
| `config.toml [auth]` | Login signing secret, MFA encryption key, first HTTP bootstrap behavior |

## Troubleshooting Direction

- Repeated logout after login: check public site URL, HTTPS Cookie settings, and reverse proxy Host handling.
- External auth callback failure: check redirect URI, provider settings, and public site URL.
- WebDAV login works but sees the wrong scope: check the WebDAV account root and workspace permissions.
- Public share opens but preview or download fails: continue with [Files and Workspaces](./files-workspaces) and [Preview and Processing](./preview-processing).
