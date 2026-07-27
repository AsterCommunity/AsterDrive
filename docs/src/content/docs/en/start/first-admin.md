---
description: "AsterDrive first start and the first admin: how the login page adapts to system state, why the first account becomes admin directly, the bootstrap_insecure_cookies switch for plain-HTTP trials, and what to do after creating the admin."
title: "First Start and the First Admin"
---

:::tip[What this page covers]
What happens when you open the site for the first time after deployment, where the first administrator comes from, which switch a plain-HTTP trial needs, and what to do after creating the admin.
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

## Plain-HTTP First Trial

This switch in `config.toml` only affects the default value written for `auth_cookie_secure` during first initialization:

```toml
[auth]
bootstrap_insecure_cookies = true
```

- **Plain-HTTP first trial** — set `true` temporarily
- **Production HTTPS deployment** — keep `false`

If the runtime setting already exists in the database, changing this later does not rewrite the old value; after moving to HTTPS for real, adjust it under `Admin -> System Settings -> Auth and Cookies`.

## Related Pages

- [Choose a Deployment](/en/start/choose-deployment/) — start here if you have not deployed yet
- [Storage Policies and Policy Groups](/en/admin/storage-policies/) — the main thread after `needs_storage`
- [Registration, Login and SSO](/en/admin/auth-sso/) — registration switch, MFA policy, and external auth
- [Login and Sessions](/en/reference/config/auth/) — static auth secrets in `config.toml`
