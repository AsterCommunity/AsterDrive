---
description: "The Site Configuration group of System Settings: public site URL, site title and description, favicon and logos, preview apps, and WOPI-related TTLs."
title: "Site Configuration"
---

Entry point: `Admin -> System Settings -> Site Configuration`. For other groups and when changes take effect, see [System Settings](/en/reference/config/runtime/).

If the site needs to be accessed externally, configure this group first.

- **`Public Site URL`**
  Enter the HTTP(S) origins users actually use to access the site. Fill one origin per input in the list, for example:

  ```text
  https://drive.example.com
  https://panel.example.com
  ```

  Each item should contain only the origin: protocol, domain, and optional port. Do not include paths, do not include `/api`, and do not use wildcards. The system uses these origins to generate share pages, mail links, WebDAV addresses, Office / WOPI preview URLs, and absolute URLs needed by later callbacks. When left empty, most browser pages can work from the current access address, but external entry points are more likely to generate wrong links. **Production deployments should explicitly configure the public site URL.**

  If the same instance is accessed through multiple domains, add all of them to this list. AsterDrive finds an exact matching origin in the list based on the current request Host. If matched, it uses that origin to generate links. If not matched, it uses the first item as the fallback origin.

:::caution[This is not a CORS allowlist]
  `Public Site URL` means "which public entry points belong to this AsterDrive instance", and it also participates in same-site CSRF trust decisions for cookie writes. It does not automatically allow browsers to call APIs cross-origin. Cross-origin access is configured separately under `Network Access -> Allowed CORS Origins`.
:::
- **`Site Title`, `Site Description`**
  Affect the title and description on login pages, share pages, and app pages.
- **`favicon`, light logo, dark logo**
  Affect branding shown in browser tabs, login pages, and the site header.
- **`Preview Apps`**
  Provide additional "open with" options for Office, PDF, spreadsheet, or other files. Built-in previewers, external URL templates, and WOPI opening methods are managed here together.
- **`WOPI-related TTLs`**
  Adjust these only when integrating online Office preview/editing services such as OnlyOffice. Normal deployments should keep the defaults.

:::tip[Recommended order for enabling WOPI]

1. Configure `Public Site URL` correctly first
2. Enable an existing app under `Preview Apps`, or import a new app through `WOPI Discovery`
3. Confirm the external Office / WOPI service can call back to `/api/v1/wopi/...` generated from `Public Site URL`
4. If browser cross-origin calls to the AsterDrive API are blocked, allow the corresponding origin under `Network Access`
5. Open real `docx` / `xlsx` / `pptx` files once and confirm they can be saved back to AsterDrive

:::

`WOPI access token TTL`, `WOPI lock TTL`, and `WOPI discovery cache duration` are all in this group. Adjust them manually only after you have connected a WOPI service and actually run into problems such as session expiry or discovery updates not taking effect in time.
