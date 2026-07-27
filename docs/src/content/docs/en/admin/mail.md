---
description: "AsterDrive mail delivery configuration: SMTP options, recommended setup order, test mail, the 7 built-in mail templates, public site URL coupling, and a troubleshooting checklist for missing mail."
title: "Mail Delivery"
---

:::tip[This page covers the console's "Mail Delivery" group]
Mail configuration does not live in `config.toml`; it is entirely under `Admin -> System Settings -> Mail Delivery`.
Before opening registration, password reset, or email re-binding, get this group working first — **mail first, registration second**.
:::

Features that depend on mail:

- Email activation after public registration
- Password reset on the login page
- Email re-binding under `Settings -> Security`
- Email verification when external auth cannot match a local account directly
- Email-code MFA in login second-factor verification
- Administrator test mail

Entry:

```text
Admin -> System Settings -> Mail Delivery
```

## Recommended Order

1. Fill in the SMTP server, port, and encryption mode
2. Fill in username and password as needed
3. Fill in the sender address and sender name
4. **Send a test mail to yourself first**
5. Then try registration activation, password reset, and email re-binding
6. If you plan to enable external-auth email verification or email-code MFA, run each real flow once too

:::caution[The cost of doing it backwards]
Open public registration before mail works, and a batch of accounts gets created that never receive activation mail — all stuck at "waiting for activation".
:::

## Options at a Glance

| Option | Purpose |
| --- | --- |
| `mail_smtp_host` | SMTP server address |
| `mail_smtp_port` | SMTP port, default `587` |
| `mail_security` | Encryption mode; `465` usually means implicit SSL/TLS, other ports use STARTTLS |
| `mail_smtp_username` | SMTP login username |
| `mail_smtp_password` | SMTP login password |
| `mail_from_address` | Sender email shown to recipients |
| `mail_from_name` | Sender name shown to recipients |

:::tip[Handling username and password]

- SMTP needs no authentication — leave both empty
- Authentication required — **fill both together**; never just one

:::

If you rarely deal with mail systems: think of SMTP as simply "the connection info for the outgoing mail server".

## Confirming Mail Actually Sends

The `Mail Delivery` page has a `Send Test Mail` button.

Common usage:

- Send directly to the current administrator's email
- Temporarily switch to another external mailbox to confirm non-intranet domains also receive it

After the test passes, do two more things:

1. On the login page, try "register and receive activation mail" or "reset password" once
2. Confirm `Admin -> System Settings -> Site -> Public Site URL` is set correctly

## What Mail Templates Can Change

There are currently 7 built-in template groups:

- Registration activation
- Email re-bind confirmation
- Password reset
- Password reset result notification
- Old-email change notification
- External login email verification
- Login email code

Each group can change separately:

- Subject
- Body (HTML)

:::tip[No need to guess variable names]
The right side of the page lists the magic variables available for the current template — copy from there.
:::

## Why `Public Site URL` Must Be Configured Together

Activation links, password reset links, and email re-bind confirmation links all need to generate **addresses that open from the outside**.

If the real access address is:

```text
https://drive.example.com
```

Set it here:

```text
Admin -> System Settings -> Site -> Public Site URL
```

If the same instance has multiple public entries, add them one by one in the list. Background flows like mail, which have no current browser Host, use the first entry as the default origin — put the origin you most want users to click first.

:::caution[Only fill the site root]
No paths, no `/api` — just the origin layer, e.g. `https://drive.example.com`.
:::

## What You See When It Is Not Configured Right

| Symptom | Likely broken step |
| --- | --- |
| New users can register but never receive activation mail | SMTP unreachable, or rejected by the recipient side |
| Password reset button works, but no reset link in the mailbox | Same as above, or `Public Site URL` is not set |
| User can start an email re-bind but the new mailbox gets no confirmation | Same as above |
| External login reaches email verification but no mail arrives | SMTP not working, or the external-login verification template / public site URL is wrong |
| The MFA page can send an email code but nothing arrives | SMTP not working, or the login email code template was broken by edits |
| Test mail fails | SMTP configuration wrong, or the network egress is blocked |

Troubleshooting checklist:

1. SMTP host, port, encryption, username and password
2. Whether the sender address is allowed by the SMTP service
3. Whether `Public Site URL` is a real external HTTP(S) origin; with multiple entries, whether the mail default origin is on the first row
4. Check both inbox and spam folders
5. After mail works again, resend the activation mail or re-bind confirmation mail
