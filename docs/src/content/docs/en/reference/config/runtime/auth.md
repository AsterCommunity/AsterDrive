---
description: "The Authentication and Cookies group of System Settings: cookie security, token TTLs, activation/email-change/reset link TTLs and cooldowns, and email-code MFA."
title: "Authentication and Cookies"
---

Entry point: `Admin -> System Settings -> Authentication and Cookies`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group decides browser login behavior and session safety.

- **`Authentication Cookie Sent Only Over HTTPS`** - Keep enabled in production. Disable temporarily only for local or intranet plain-HTTP trial runs.
- **`Access Token TTL`, `Refresh Token TTL`** - Control how long login state is maintained.
- **`Registration Activation Link TTL`**
- **`Email Address Change Link TTL`**
- **`Password Reset Link TTL`**
- **`Verification Email Resend Cooldown`**
- **`Password Reset Request Cooldown`**
- **`Require Email Code MFA`** - Requires working mail delivery. After enabling it, verified-email users without TOTP can complete second-factor verification with an 8-digit email code after password or external identity login.
- **`Allow TOTP Email Fallback`** - Allows users who already have an authenticator to choose email code on the MFA login page. Security-sensitive sites can keep it disabled.
- **`Email Login Code TTL`** - Default is `10` minutes; actual validity never exceeds the remaining lifetime of the current MFA login flow.
- **`Email Login Code Resend Cooldown`** - Default is `60` seconds.

For normal deployments, you usually only need to confirm cookie security requirements and link TTLs match your site policy.

:::caution[Email codes depend on mail security]
Email-code MFA is useful only when SMTP delivery is reliable and user email addresses are verified. Before enabling it, send a test mail under `Mail Delivery` and confirm the `Login Email Code` template matches your site's wording.
:::
