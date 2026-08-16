---
description: "AsterDrive first start and the first admin: how the login page adapts to system state, why the first account becomes admin directly, HTTP/HTTPS cookie bootstrap behavior, and what to do after creating the admin."
title: "First Start and the First Admin"
---

:::tip[What this page covers]
What happens when you open the site for the first time after deployment, where the first administrator comes from, how HTTP/HTTPS first login selects the cookie policy, and what to do after creating the admin.
:::

## The Login Page Adapts to System State

The login page is not a fixed "login" or "register" page — it follows current state:

- **No users exist yet** — the initialization flow runs, creating the first administrator directly
- **Users exist, and the entered account exists** — sign in
- **Users exist, the entered account is new, and public registration is allowed** — create a regular account
- **The administrator enabled external auth providers** — the login page shows the matching external sign-in entries
- **The current browser supports Passkeys** — the login page shows the Passkey entry
- **The account requires MFA** — after password or external identity passes, a second factor is still required

Things to note:

- The first account becomes administrator directly, without email activation
- Regular accounts registered publicly afterwards must click the activation mail before signing in (activation mail depends on [Mail Delivery](/en/admin/mail/))
- After the administrator disables public registration, the login page only offers sign-in and password reset

## After Creating the First Admin

Once the first administrator is created, the system enters the `needs_storage` state: there is no default storage policy yet, so uploads are not possible. Next steps:

1. Create the first storage policy under `Admin -> Storage Policies` and set it as default; concepts and order in [Storage Policies and Policy Groups](/en/admin/storage-policies/)
2. Set `Admin -> System Settings -> Site -> Public Site URL` to the real HTTP(S) origin
3. If you plan to open registration, password reset, or external auth, set up [Mail Delivery](/en/admin/mail/) first
4. Walk through the [First-Start Checklist](/en/ops/first-check/) before going live

## Cookie Policy for the First Login

Fresh installations allow the administrator to be created and logged in directly over HTTP without an extra environment variable:

```toml
[auth]
bootstrap_insecure_cookies = true
```

- **Administrator created from an HTTP origin** — keep `auth_cookie_secure = false`, so the following automatic login can carry cookies
- **Administrator created from an HTTPS origin** — automatically promote `auth_cookie_secure` to `true` before the following login
- **Require Secure cookies from process startup** — explicitly set `bootstrap_insecure_cookies = false` before the database is initialized

This static option **only affects the first database write** of `auth_cookie_secure`. Once the runtime setting exists, changing `config.toml` does not rewrite it. If the instance was bootstrapped over HTTP and later moves to HTTPS, enable `Authentication Cookie Sent Only Over HTTPS` under `Admin -> System Settings -> Authentication and Cookies`.

## Related Pages

- [Choose a Deployment](/en/start/choose-deployment/) — start here if you have not deployed yet
- [Storage Policies and Policy Groups](/en/admin/storage-policies/) — the main thread after `needs_storage`
- [Registration, Login and SSO](/en/admin/auth-sso/) — registration switch, MFA policy, and external auth
- [Login and Sessions](/en/reference/config/auth/) — static auth secrets in `config.toml`
