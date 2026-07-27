---
description: "AsterDrive account and security: email verification and change, password changes, MFA (authenticator, recovery codes, email codes), Passkeys, external identities, and login device management."
title: "Account and Security"
---

:::tip[What this page covers]
Everything under `Settings -> Security`: email, password, MFA, Passkeys, external identities, and login devices. Profile and interface preferences live in settings too and are covered here as well.
:::

## Profile and Interface

`Settings -> Profile` lets you change your display name and avatar. Avatars support upload-and-crop, Gravatar, or clearing the current one. Email status shows here, but changing the bound email happens under `Settings -> Security`.

`Settings -> Interface` lets you adjust:

- Light / dark / follow system
- Theme color
- Display language
- Default file browser view
- Single-click or double-click to open files and folders
- Whether to enable real-time file change sync
- Display timezone

"Real-time file change sync" here means the web page refreshes the current view via live push. It is not a desktop local-folder sync client — keep the two apart.

## Email Verification and Change

`Settings -> Security` shows whether your current email is verified. If it is verified, you can:

- Enter a new email
- Send a confirmation email to the new address
- Resend the confirmation email when needed

The new email only takes effect after you click the confirmation link. When the page shows a "pending new email", the change flow is not finished yet.

## Changing Your Password

Changing your password requires entering the current password first. In the current version, after a successful password change, the current browser session stays logged in, while sessions on other devices are invalidated and must log in again.

If the administrator requires you to change your password, you land directly on a "change password" page after login. There you can only view basic account info, log out, or enter the current password and set a new one; if the administrator reset it, the current password is a temporary one. Normal file, team, share, and admin features stay unavailable until the change succeeds, after which the system returns to the normal signed-in state.

## Multi-Factor Authentication (MFA)

The `Multi-Factor Authentication` tab adds a second factor to your account. What users can bind themselves is a TOTP authenticator app, such as 1Password, Bitwarden, Google Authenticator, or Microsoft Authenticator.

When enabling MFA, the system asks you to:

- Scan the QR code with the authenticator app, or enter the secret manually
- Enter the current 6-digit code to finish binding
- Download or copy the recovery codes

:::caution[Recovery codes are shown only once]
Recovery codes are displayed in plain text only when generated, and each can be used only once. Save them in a password manager, encrypted note, or another safe place. If you lose the authenticator, you can complete second-factor verification on the login page with a recovery code; regenerate the codes after logging in.
:::

With MFA enabled, both password login and external identity login require the second factor. Passkey login already completes user verification through device unlock or a security key, so it does not enter the TOTP challenge. Disabling MFA or regenerating recovery codes also requires entering the current TOTP code or an unused recovery code to confirm.

If the administrator enabled email-code MFA and your email is verified, the login second-factor page may also show `Email Code`. After you click send, the system sends an 8-digit one-time code to your verified email. The code is valid for 10 minutes by default, but never longer than the remaining time of this login's second-factor flow; the same user cannot resend within 60 seconds by default. If you already enabled an authenticator, whether email codes work as a fallback depends on administrator configuration — high-security sites may disable this fallback.

If you lose both the authenticator and all recovery codes, and the site has no usable email-code method, contact the administrator to reset MFA from the user details page.

## Passkeys

The `Passkey` tab manages passwordless sign-in methods:

- Add a new Passkey
- Rename existing Passkeys
- View creation and last-used times
- Delete Passkeys you no longer use

When adding one, the browser opens the system's own verification window. After it succeeds, the login page can sign you in with device unlock, fingerprint, face, or a security key, depending on your browser and system.

If you added a Passkey but the login page suddenly stops showing the Passkey entry, the current browser environment usually does not support it, or the administrator temporarily disabled site-wide Passkey login. That switch does not delete your registered Passkeys; they work again once the administrator re-enables it.

## External Identities

The `External Identities` tab lists the external sign-in identities bound to your account. After unbinding, that identity can no longer sign in to this account directly; if the administrator enabled automatic binding by verified email, it may bind again later when the rules match.

## Login Devices

Under `Login Devices` you can:

- View devices currently still signed in
- Remove a single device
- Sign out all other devices at once

If you remove the current device, the current browser signs out immediately.
