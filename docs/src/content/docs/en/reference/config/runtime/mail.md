---
description: "The Mail Delivery group of System Settings: SMTP, sender, test mail, and the registration, password reset, email change, external login verification, and login email code templates."
title: "Mail Delivery"
---

Entry point: `Admin -> System Settings -> Mail Delivery`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group decides whether registration activation, password reset, and email address change emails can be sent. The most commonly changed items are:

- SMTP host, port, username, password
- Sender address and sender name
- Whether to enable SMTP encryption
- Test mail
- Registration activation, password reset, email address change, external login email verification, and login email code mail templates

:::caution[Check before enabling registration]
If the site will allow registration, password recovery, or email address changes, check mail configuration and `Public Site URL` **together**. Do not configure only one of them.

If external authentication allows users to continue binding or account creation through email verification, it also depends on this mail configuration group.
:::

See [mail](/en/admin/mail/) for detailed guidance.
