---
description: "AsterDrive registration, login and SSO administration: public registration switch, email allow/blocklists, email-code MFA policy, Passkey switch, external auth onboarding order, account binding policies, and pre-launch validation."
title: "Registration, Login and SSO"
---

:::tip[What this page covers]
The login system from the administrator's perspective: who may register, MFA and Passkey policy, and how to connect external identity providers.
For how users bind MFA / Passkeys / external identities themselves, see [Account and Security](/en/using/account-security/); for the static secret fields in `config.toml`, see [Login and Sessions](/en/reference/config/auth/).
:::

## Public Registration Switch

```text
Admin -> System Settings -> User Management -> Allow Public Registration
```

After turning it off:

- External visitors can no longer create accounts from the login page
- The first-admin initialization flow still exists
- Users created manually by an administrator in the console still work

### Local Account Email Allowlist / Blocklist

If the site should only allow company email registration, or block disposable email providers, configure:

```text
Admin -> System Settings -> User Management -> Registration and Login -> Local Account Email Allowlist
Admin -> System Settings -> User Management -> Registration and Login -> Local Account Email Blocklist
```

These two lists only apply to **local accounts**: the email entered during public registration, and the local email a user re-binds under `Settings -> Security`. They do not restrict external identities returned by third-party SSO; email domain restrictions for external auth are configured per provider under `Admin -> External Auth`.

Entries can be full emails or exact domains:

```text
alice@example.com
example.com
@example.com
```

`example.com` and `@example.com` are equivalent, matching only `user@example.com`, not `user@sub.example.com`. Internationalized domains must be written in punycode form.

Rule order:

- Blocklist wins over allowlist
- An empty allowlist means no allowlist restriction
- An empty blocklist means no extra blocking
- With both lists empty, any valid email can be used for local registration and local email re-binding

## Email-Code MFA Policy

Administrators can enable email-code MFA in the console:

```text
Admin -> System Settings -> Auth and Cookies -> Require Email-Code MFA
```

Once enabled, users with a verified email can complete second-factor verification with an 8-digit email code after passing password or external identity. This feature depends on a fully working mail delivery configuration; the SMTP host and sender address must not be empty, and the SMTP username and password must be filled in together or left empty together.

Default rules:

- Email codes are valid for `10` minutes by default, but never longer than the remaining time of the current MFA login flow
- The same user cannot resend within `60` seconds by default
- With only `Require Email-Code MFA` enabled, users without TOTP whose email is verified use email codes
- If `Allow Email Fallback for TOTP Users` is also enabled, users with an authenticator can use email codes as an additional verification method

:::caution[Enable email fallback carefully]
Email codes rely on the security of the user's mailbox. High-security deployments usually offer them only to verified-email users without an authenticator; whether TOTP users may fall back to email should follow your security policy.
:::

When a user loses both the authenticator and all recovery codes, an administrator resets MFA under `Admin -> Users -> User Detail -> Security Actions -> Reset MFA`; see [Users and Teams](/en/admin/users-teams/#resetting-mfa).

## Passkey Switch

Administrators can temporarily disable the Passkey login entry:

```text
Admin -> System Settings -> User Management -> Registration and Login -> Allow Passkey Login
```

After disabling, users can no longer sign in with registered Passkeys, but existing Passkeys are not deleted. Re-enabling restores them.

Production deployments must first set `Admin -> System Settings -> Site -> Public Site URL` correctly and use HTTPS; local `localhost` / `127.0.0.1` debugging is the exception. Browsers usually expose full Passkey capability only in secure contexts.

## Password Login Switch

Administrators can disable local username/password sign-in while keeping external identity providers and Passkey sign-in available:

```text
Admin -> System Settings -> User Management -> Registration & Login -> Allow password sign-in
```

When disabled, the login page hides the local password input and sign-in button, and `POST /auth/login` rejects local password authentication. Registration, invitations, password reset, initial setup, administrator password assignment, authenticated password changes, and external-auth password linking remain available; existing accounts, passwords, and sessions are not deleted. Keep at least one working external provider or Passkey method before disabling password login, and verify that the administrator can still sign in through the retained method.

## External Auth Onboarding Order

The admin entry is `Admin -> External Auth`. Six provider types are currently supported:

| Type | Best for |
| --- | --- |
| `oidc` / OpenID Connect | Standard OIDC identity providers; the preferred choice |
| `generic_oauth2` / Generic OAuth2 | Providers with only an OAuth2 authorization-code + UserInfo interface, or requiring manually entered endpoints |
| `github` | GitHub OAuth App sign-in |
| `qq` | QQ Connect website sign-in |
| `google` | Google OIDC sign-in |
| `microsoft` | Microsoft Entra ID / Microsoft Account OIDC sign-in |

Recommended onboarding order:

1. Set `Admin -> System Settings -> Site -> Public Site URL` to the real external access address
2. Confirm the reverse proxy already handles HTTPS, Host, real client IPs, and large request bodies
3. Create the application on the identity provider side and prepare the Client ID (plus Client Secret for a confidential client)
4. Create the provider under `Admin -> External Auth`, then copy the redirect URI AsterDrive generates after saving
5. Register that redirect URI on the identity provider side
6. Decide the account binding policies (next section)
7. Run the full path once with a real account: login, binding, MFA, and email verification

:::caution[Set the public site URL first]
External auth redirect URIs depend on `Public Site URL`. If it is wrong, identity provider callbacks land on the wrong address. After changing the public site URL, update the redirect URI registered on the provider side too.
:::

## Account Binding Policies

AsterDrive does not treat email as the sole identity source. External identities match existing bindings first by `identity_namespace + subject`.

| Policy | Default | Description |
| --- | --- | --- |
| Require verified email | On | The provider must return a usable email with `email_verified=true`, otherwise the flow enters verification or fails |
| Auto-bind by verified email | Off | Auto-binding happens only when the provider returns `email_verified=true` and exactly one local user has that email |
| Auto-create local users | Off | Unbound external identities may create local regular users; still constrained by the public registration switch, email domains, and email verification policy |

Conservative suggestions:

- For trusted providers like OIDC, enabling "require verified email" is reasonable
- For providers that do not reliably return `email_verified=true`, do not enable "auto-bind by verified email"
- The GitHub provider only accepts a verified primary email; QQ returns no email; Microsoft does not treat `email` as a verified primary email — per-field configuration for these providers lives in [External Authentication](/en/reference/config/external-auth/)

If an external identity cannot sign in directly, the user either signs in and binds an existing account, or continues through email verification. Email verification depends on the external-login verification mail template under `Admin -> System Settings -> Mail Delivery`; see [Mail Delivery](/en/admin/mail/).

Deleting a provider deletes its external identity bindings, but does not delete existing local users. Users view and unbind their own external identities under `Settings -> Security -> External Identities`.

## Pre-launch Validation

- The test button on save only checks provider endpoint configuration and discovery / endpoint reachability; it does not complete a real authorization-code login for you
- Before going live, run once with a real account: login, auto-creation, auto-binding, MFA second factor, email verification
- Per-provider field descriptions, default scopes, claim mapping, and common errors (including the Microsoft `AADSTS` family) are in [External Authentication](/en/reference/config/external-auth/)

## Features That Depend on Mail

These features do not work without mail:

- Activation mail after public registration
- Password reset on the login page
- Email re-bind confirmation mail under `Settings -> Security`
- Email verification when external auth cannot match a local account directly
- Email-code MFA

:::caution[Set up mail before opening registration]
Do it in the wrong order and new accounts get created but never receive activation mail — stuck at "waiting for activation". For the configuration order, see [Mail Delivery](/en/admin/mail/).
:::
