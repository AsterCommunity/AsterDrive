---
description: "The Network Access group of System Settings: browser cross-origin access rules (CORS), including HTTP(S) site origins and browser-extension origins."
title: "Network Access"
---

Entry point: `Admin -> System Settings -> Network Access`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

This group mainly handles browser cross-site access rules (CORS).

Change it only in these scenarios:

- The browser page and AsterDrive are not under the same domain
- You want another site to call AsterDrive directly from the browser
- A browser extension needs to access WebDAV or the API directly

`Allowed CORS origins` is an array of complete origins, with one item per input. Examples include `https://panel.example.com` and `chrome-extension://extension-id`. HTTP(S) sites and Chrome/Edge, Firefox, and Safari Web Extension origins are supported. For an extension, configure its complete extension ID instead of allowing every extension by scheme.

CORS is disabled by default and the allowlist is empty. In that state, the server neither adds CORS response headers nor rejects requests merely because they carry an `Origin` header. Once CORS is enabled, only exact allowlist matches are accepted. A single `*` item permits any origin, but it cannot be combined with cross-origin credentials.

:::tip[Same-site deployments usually do not need changes]
Most deployments where "frontend pages and APIs are on the same site" do not need to touch this group.

When connecting an external WOPI service, the most common issue is not here. It is usually that the Office service cannot call back to the WOPI URL generated from `Public Site URL`. Add an origin here only when the browser console clearly reports a CORS error for the AsterDrive API.
:::
