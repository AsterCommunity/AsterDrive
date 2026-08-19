---
description: "The User Management group of System Settings: public registration, email activation, password and Passkey sign-in, local email allow/block lists, avatars, and Gravatar."
title: "User Management"
---

Entry point: `Admin -> System Settings -> User Management`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group controls account entry points and avatar-related behavior.

- **`Allow Public User Registration`** - After disabling it, the login page only supports existing-account login and administrator initialization. New accounts can only be created by administrators.
- **`Require Email Activation After Registration`** - After enabling it, normal users created through public registration must click the activation email before logging in.
- **`Allow Password Sign-In`** - Enabled by default. Disabling it blocks local username/password sign-in as a first factor; external authentication and Passkey sign-in remain available when enabled. Existing passwords and accounts are not deleted.
- **`Allow Passkey Sign-In`** - After disabling it, users cannot sign in with registered Passkeys. Existing Passkeys are not deleted and can be used again after re-enabling the setting.
- **`Local Account Email Allowlist`** - Restricts email addresses or exact domains allowed for local registration and local email changes. Empty means no allowlist restriction.
- **`Local Account Email Blocklist`** - Blocks email addresses or exact domains for local registration and local email changes. The blocklist overrides the allowlist.
- **`Avatar Directory`** - User-uploaded avatars are written to this local directory. Relative paths resolve under server-side `./data`.
- **`Avatar Upload Size Limit`** - Avatar files exceeding this limit are rejected directly.
- **`Gravatar Base URL`** - If official Gravatar access is unstable, change it to a proxy or mirror.

The local email allowlist / blocklist applies only to local-account flows, not third-party SSO. Entries can be `alice@example.com`, `example.com`, or `@example.com`; domain entries are exact matches, so `example.com` does not automatically match `sub.example.com`. Internationalized domains must use punycode.
