# Custom Frontend

::: tip What this page covers
AsterDrive's frontend is replaceable: the official frontend is embedded into the binary, but you can override it with **your own frontend assets**. This page explains the override mechanism, placeholders in `index.html`, using "custom configuration" as a persistent layer for global variables, and CSP limitations.
It is for developers who want to replace or heavily customize the frontend, not for daily users or administrators.
:::

## Override Mechanism

All AsterDrive frontend routes (home page, `/assets/*`, `/static/*`, `/pdfjs/*`, `/favicon.svg`, PWA files, and SPA fallback) use the same loading order:

1. **Check `./frontend-override/` under the current working directory first**. If a file exists there, use it
2. **Fall back to the embedded official frontend** if the file is not found

In other words, you only need to put your frontend build output into `./frontend-override/`. AsterDrive will **prefer** loading all assets from there, without recompiling the binary.

::: warning Relative to the current working directory
`./frontend-override/` is resolved **relative to the working directory at startup**, not relative to the binary location:

- Local direct run: `frontend-override/` under the project root
- systemd: `WorkingDirectory/frontend-override/`
- Docker: `/frontend-override/` inside the container (default working directory is `/`, so you need to mount it there manually)

The simplest Docker approach is a volume mount: `-v /path/to/my-dist:/frontend-override:ro`
:::

The override is **file-level**: files present in your `dist/` are used, while missing files continue to fall back to the embedded official version. Replacing only `index.html` plus some assets is fine.

## Supported Placeholders in index.html

When loading `index.html`, AsterDrive replaces these strings before returning it to the browser:

| Placeholder | Source | Description |
| --- | --- | --- |
| `%ASTERDRIVE_VERSION%` | Binary version | Compile-time `CARGO_PKG_VERSION` |
| `%ASTERDRIVE_TITLE%` | Runtime configuration | `Site title` maintained under backend `Site Configuration` |
| `%ASTERDRIVE_DESCRIPTION%` | Runtime configuration | `Site description` |
| `%ASTERDRIVE_FAVICON_URL%` | Runtime configuration | `favicon` URL |
| `%ASTERDRIVE_WORDMARK_DARK_URL%` | Runtime configuration | Dark wordmark URL for light surfaces, defaulting to `/static/asterdrive/asterdrive-dark.svg` |
| `%ASTERDRIVE_WORDMARK_LIGHT_URL%` | Runtime configuration | Light wordmark URL for dark surfaces / the login hero, defaulting to `/static/asterdrive/asterdrive-light.svg` |
| `%ASTERDRIVE_CSP%` | Constant | Baseline page `Content-Security-Policy` |

All replacement values are HTML-entity escaped, so inserting them directly into `<title>` / `<meta>` is safe.

Typical usage:

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy" content="%ASTERDRIVE_CSP%" />
  <title>%ASTERDRIVE_TITLE%</title>
  <meta name="description" content="%ASTERDRIVE_DESCRIPTION%" />
  <link rel="icon" href="%ASTERDRIVE_FAVICON_URL%" />
  <link rel="preload" as="image" href="%ASTERDRIVE_WORDMARK_LIGHT_URL%" media="(min-width: 1024px), (prefers-color-scheme: dark)" />
  <link rel="preload" as="image" href="%ASTERDRIVE_WORDMARK_DARK_URL%" media="(max-width: 1023px) and (prefers-color-scheme: light)" />
  <meta name="generator" content="AsterDrive %ASTERDRIVE_VERSION%" />
</head>
<body>
  <div id="app"></div>
  <script type="module" src="/assets/index.js"></script>
</body>
</html>
```

## Use "Custom Configuration" to Persist Global Variables

Your frontend will probably need some site-wide persistent configuration: theme colors, brand name, third-party credentials, switches, and so on. AsterDrive provides `custom configuration` (entries in the `system_config` table with `source="custom"`) as the **officially recommended persistence layer**.

**Naming convention**: `{namespace}.{name}`

| Use | Example key |
| --- | --- |
| Theme color for your custom frontend | `my-frontend.theme.primary_color` |
| Feature switch | `my-frontend.feature.enable_xxx` |
| Third-party integration URL | `my-frontend.integration.xxx_api_url` |
| Customer-side brand copy | `my-frontend.brand.slogan` |

Use your frontend's identifier as `namespace` (preferably with `-`) to avoid conflicts with official system configuration or other custom frontends.

::: warning Do not use prefixes such as `wopi.` / `auth.` / `mail.`
These prefixes may be occupied by future system configuration. A private namespace such as `my-frontend.` / `acme-panel.` is safest.
:::

### Public Read API

Custom frontends should read consumer-side custom configuration from the public read-only endpoint:

| Operation | Endpoint |
| --- | --- |
| Read custom configuration visible to the current identity | `GET /api/v1/public/custom-config` |

Response example:

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "entries": {
      "my-frontend.theme.primary_color": "#6366f1",
      "my-frontend.feature.enable_beta_tab": "true"
    }
  }
}
```

This endpoint only returns entries with `source="custom"`, and only exposes key/value pairs under `entries`. Internal `system_config` fields such as `id`, `source`, and `updated_by` are not exposed to the frontend. Responses use `Cache-Control: public, max-age=60`.

Custom configuration has three visibility levels:

| Visibility | Behavior |
| --- | --- |
| `private` | Visible only in the admin console; never returned by the public read endpoint |
| `public` | Readable without login through `/api/v1/public/custom-config` |
| `authenticated` | Returned only when the request carries a valid access token |

If the request has no token, the endpoint returns only `public` entries. If the request explicitly carries an invalid token, the endpoint returns 401 instead of silently falling back to anonymous behavior.

### Admin API

Custom configuration and system configuration use **the same Admin API**. The difference is the `source` field; custom entries also have `visibility`:

| Operation | Endpoint |
| --- | --- |
| List all configuration (paginated) | `GET /api/v1/admin/config` |
| Read one key | `GET /api/v1/admin/config/{key}` |
| Write / update | `PUT /api/v1/admin/config/{key}` body `{"value": "...", "visibility": "public"}` |
| Delete | `DELETE /api/v1/admin/config/{key}` |

`visibility` is only accepted for custom configuration. Built-in system configuration cannot be made public through this field. When omitted, new custom configuration defaults to `private`, so existing or accidentally created entries are not exposed unexpectedly.

::: tip Do not put secrets in public configuration
Both `public` and `authenticated` are frontend-consumed visibility levels. Do not store API secrets, private keys, permanent tokens, or backend credentials there. For third-party services, prefer backend proxying or short-lived credentials.
:::

### Batch Operations from CLI

The operations CLI also supports custom configuration. `list` / `get` / `set` / `delete` / `validate` / `export` / `import` all work. See [Operations CLI](/en/deployment/ops-cli).

Typical scenario:

```bash
# Batch import configuration for your custom frontend during a maintenance window
./aster_drive config \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  import \
  --input-file ./my-frontend-config.json
```

Example input file:

```json
[
  { "key": "my-frontend.theme.primary_color", "value": "#6366f1", "visibility": "public" },
  { "key": "my-frontend.feature.enable_beta_tab", "value": "true", "visibility": "authenticated" }
]
```

## CSP Limitations

When AsterDrive returns `index.html`, it does two things:

- Adds the baseline page `Content-Security-Policy` response header
- Replaces `%ASTERDRIVE_CSP%` with the same policy suitable for `<meta http-equiv="Content-Security-Policy">`
- Replaces title, description, favicon, and wordmark placeholders so the pre-login HTML can use runtime branding

The response-header version has one extra directive: `frame-ancestors 'self'`. This is a browser restriction; `frame-ancestors` cannot take effect through a meta tag.

Key constraints in the current baseline policy:

- `default-src 'self'`: resources default to same-origin only
- `script-src 'self' 'unsafe-inline'`: inline scripts are allowed
- `style-src 'self' 'unsafe-inline'`: inline styles are allowed
- `img-src 'self' data: blob: http: https:`: images may be same-origin, data URI, blob, or HTTP(S) sources
- `font-src 'self' data:`: fonts are only same-origin or data URI
- `connect-src 'self' http: https: ws: wss: blob:`: XHR / fetch / WebSocket may connect to same-origin and HTTP(S) / WS(S) endpoints
- `media-src 'self' blob:`: media preview allows same-origin and blob
- `worker-src 'self' blob:`: workers allow same-origin and blob
- `frame-src 'self' http: https:`: iframes may embed HTTP(S) sources for WOPI, external previews, and similar uses
- `frame-ancestors 'self'`: this site may only be embedded by itself
- `object-src 'none'`: plugin objects are fully disabled

`http:` / `https:` are not relaxed casually. Browser direct upload, presigned download, remote followers, external preview apps, WOPI iframes, and PDF worker blobs all hit these source restrictions. You can tighten the policy, but test real upload, download, PDF preview, share pages, and external open methods afterward.

::: warning Third-party scripts / fonts / font libraries will be blocked by CSP
If your frontend uses Google Fonts, external CDN scripts, Sentry, GA, or similar third-party resources, **the browser will block them directly**.

There is currently no configurable CSP override mechanism. If you want external dependencies, the recommended options are:

1. Bundle dependencies into your own `dist/` (recommended)
2. Or **open an issue first** to discuss how to allow specific sources
:::

## PWA and Special Paths

These paths bypass SPA fallback and are handled as real files:

- `/sw.js`: Service Worker
- `/manifest.webmanifest`: PWA manifest
- `/workbox-*`: Workbox runtime
- `/pdfjs/*`: PDF.js assets (no SPA fallback; missing files return 404 directly)

Other paths fall back to SPA fallback and return `index.html` when no concrete file is found.

## Development Advice

- **Local development**: run the Vite dev server directly and proxy `/api` to AsterDrive; no need to touch `./frontend-override/`
- **Production replacement**: replace only `./frontend-override/`; do not change the binary
- **Coexisting with the official frontend**: the current version does not support A/B or multiple frontends; you must choose one
- **Version alignment**: binary upgrades may add new APIs or behavior changes; test your custom frontend after each upgrade

::: tip Want better custom frontend support in AsterDrive?
The current mechanism is **minimally usable**: it works, but it is rough. If you are building a custom frontend and have concrete extension needs such as public read-only configuration, custom CSP, or switching between multiple frontends, [open an issue](https://github.com/AptS-1547/AsterDrive/issues). This kind of feedback gets priority.
:::
