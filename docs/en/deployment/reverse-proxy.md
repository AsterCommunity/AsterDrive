# Reverse Proxy (Required in Production)

AsterDrive does not include a TLS terminator.  
If you plan to expose the site to the public internet, enable WebDAV, or connect an external Office / WOPI service, you **must** put a reverse proxy in front to handle:

- HTTPS certificates
- HTTPS-related security response headers, while preserving the browser page baseline `Content-Security-Policy` returned by AsterDrive
- HTTP to HTTPS redirects
- large file upload body limits
- SSE long-connection timeout and buffering
- WebDAV method / request header passthrough
- frontend static asset cache headers

Do not expose `:3000` directly to the public internet.  
That is only suitable for temporary local or intranet bootstrap. For production launch, bind AsterDrive to an internal address, then expose `443` externally through Caddy / Nginx / Traefik.

## Align These Values Before Launch

- Set `Admin -> System Settings -> Site Configuration -> Public Site URL` to a real `https://` origin. Add multiple public domains one by one, for example `https://drive.example.com`.
- Only set static bootstrap option `auth.bootstrap_insecure_cookies` to `true` temporarily during plain HTTP first bootstrap.
- After switching to HTTPS, remove `auth.bootstrap_insecure_cookies` and confirm runtime `auth_cookie_secure` has been restored to enabled.
- The home page response headers should include the browser page baseline `Content-Security-Policy` returned by AsterDrive. The proxy must not remove it or overwrite it with an incompatible policy.
- Do not rewrite the site's own baseline CSP into a site-wide `sandbox`.
- The proxy must not block WebDAV methods such as `PROPFIND`, `MOVE`, `COPY`, `LOCK`, and `UNLOCK`.
- The proxy must not overwrite `ETag` / `Cache-Control` returned by thumbnail endpoints.
- The proxy must preserve the real `Host` and correctly pass the public protocol. AsterDrive uses the request Host to perform exact matching against the `Public Site URL` list, then generates sharing, WebDAV, and WOPI URLs for the corresponding domain.
- If you want AsterDrive to identify the real client IP, the front reverse proxy must be the hop you control and trust. AsterDrive only reads client IPs from `X-Forwarded-For` when the connection source matches `network_trust.trusted_proxies`; otherwise, it ignores forwarded headers and continues using the actual connection source.
- Headers such as `X-Forwarded-For` / `Forwarded` must not be treated directly as user identity credentials. They are only meaningful when the request really comes from a proxy, gateway, or Docker intranet hop you configured. Do not put public CIDR ranges, third-party CDNs, or upstreams you do not control into `trusted_proxies`.
- If you need rate limiting, audit, or session security decisions based on the real client IP, first read the `[network_trust].trusted_proxies` explanation in [Rate Limiting](/en/config/rate-limit), then synchronize the same proxy addresses / CIDRs into your reverse proxy topology.

This page assumes:

- AsterDrive listens on `127.0.0.1:3000`.
- The WebDAV prefix uses the default `/webdav`.
- The domain is `drive.example.com`.

If you changed the listen address, domain, or WebDAV prefix, update the corresponding values in the examples below.

### Multiple Domain Entries

One instance can have multiple public entries, for example:

```text
https://drive.example.com
https://panel.example.com
https://intranet-drive.example.net
```

In the admin panel, still only change `Public Site URL`, with each origin on its own line. The system only accepts exact HTTP(S) origins. Do not include paths, do not write `*`, and do not enter domains you do not control. The first line is the default fallback origin.

When a request enters through `panel.example.com`, and that line is already in the list, the system generates WebDAV, sharing, and WOPI URLs in the form `https://panel.example.com/...`. Unmatched Hosts are not trusted directly; the system falls back to the first line to avoid arbitrary Host header pollution in external links.

## Key Path Quick Reference

| Purpose | Path |
| --- | --- |
| Frontend page / admin panel / sharing page | `/` |
| API | `/api/v1/` |
| SSE storage change stream | `/api/v1/auth/events/storage` |
| WOPI callbacks | `/api/v1/wopi/` |
| WebDAV | `/webdav/` |
| Frontend build assets | `/assets/` |
| Embedded static assets | `/static/` |
| PDF.js assets | `/pdfjs/` |

## Caddy

Caddy is the simplest option and handles HTTPS plus HTTP to HTTPS redirects out of the box.

```caddyfile
drive.example.com {
    encode zstd gzip

    @frontend_assets path /assets/*
    header @frontend_assets Cache-Control "public, max-age=31536000, immutable"

    @embedded_static path /static/* /pdfjs/*
    header @embedded_static Cache-Control "public, max-age=86400"

    reverse_proxy 127.0.0.1:3000 {
        # SSE must flush quickly; do not let the proxy buffer events.
        flush_interval -1
    }
}
```

This configuration already provides:

- automatic HTTPS
- automatic HTTP to HTTPS redirect
- preservation of the baseline CSP returned by AsterDrive for browser pages
- immediate SSE flushing
- whole-site passthrough for WebDAV / WOPI / normal APIs

Notes:

- Caddy does not impose a very small request body limit by default like Nginx often does. If you add your own `request_body` limit, remember to raise it accordingly.
- Thumbnail endpoints return `ETag` and `must-revalidate` themselves. Do not rewrite them here into aggressive cache headers.

## Nginx

With Nginx, you handle HTTPS, redirects, upload size, and SSE yourself.

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 80;
    server_name drive.example.com;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name drive.example.com;

    ssl_certificate     /etc/letsencrypt/live/drive.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/drive.example.com/privkey.pem;

    # Do not let the proxy truncate large file uploads.
    client_max_body_size 0;

    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection $connection_upgrade;
    proxy_request_buffering off;
    proxy_read_timeout 3600s;
    proxy_send_timeout 3600s;
    send_timeout 3600s;

    location = /api/v1/auth/events/storage {
        proxy_pass http://127.0.0.1:3000;
        proxy_buffering off;
        proxy_cache off;
        add_header X-Accel-Buffering no always;
    }

    location ^~ /assets/ {
        proxy_pass http://127.0.0.1:3000;
        expires 1y;
        add_header Cache-Control "public, max-age=31536000, immutable" always;
    }

    location ^~ /static/ {
        proxy_pass http://127.0.0.1:3000;
        expires 1d;
        add_header Cache-Control "public, max-age=86400" always;
    }

    location ^~ /pdfjs/ {
        proxy_pass http://127.0.0.1:3000;
        expires 1d;
        add_header Cache-Control "public, max-age=86400" always;
    }

    location / {
        proxy_pass http://127.0.0.1:3000;
    }
}
```

The easiest things to miss in this config are:

- `client_max_body_size 0`
- `proxy_request_buffering off`
- disabling `proxy_buffering` specifically for SSE
- preserving `X-Forwarded-Proto` as `https`
- `X-Real-IP` is only a helper header; AsterDrive does not use it as a security decision source
- `X-Forwarded-For` is only read by the application when the request source matches `trusted_proxies`
- if another CDN, L4 load balancer, or cloud gateway sits in front of the reverse proxy, decide which hop is the "last proxy layer" trusted by AsterDrive, then put only that hop's IP / CIDR into `trusted_proxies`
- without `trusted_proxies`, AsterDrive uses the actual connection source directly and does not trust client IPs from forwarded headers

If you create a separate `location` for `/webdav/`, do not add `limit_except` to restrict methods. Otherwise clients such as Finder, Windows, and rclone may not work correctly with WebDAV.

## Traefik

Traefik is better suited to Docker / Compose scenarios.  
It has two parts:

- Traefik's static configuration: entrypoints, HTTPS, and timeouts
- AsterDrive container labels: Host routing and forwarding port

### `traefik.yml`

```yaml
entryPoints:
  web:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: websecure
          scheme: https
  websecure:
    address: ":443"
    transport:
      respondingTimeouts:
        readTimeout: 0s
        writeTimeout: 0s
        idleTimeout: 3600s

providers:
  docker:
    exposedByDefault: false

certificatesResolvers:
  letsencrypt:
    acme:
      email: ops@example.com
      storage: /letsencrypt/acme.json
      httpChallenge:
        entryPoint: web
```

Settings such as `readTimeout: 0s` are important.  
Otherwise, large uploads and SSE can time out at the proxy layer first.

### `docker-compose.yml` Labels

```yaml
services:
  asterdrive:
    image: ghcr.io/apts-1547/asterdrive:latest
    labels:
      - traefik.enable=true

      - traefik.http.routers.asterdrive.rule=Host(`drive.example.com`)
      - traefik.http.routers.asterdrive.entrypoints=websecure
      - traefik.http.routers.asterdrive.tls=true
      - traefik.http.routers.asterdrive.tls.certresolver=letsencrypt
      - traefik.http.routers.asterdrive.service=asterdrive

      - traefik.http.routers.asterdrive-assets.rule=Host(`drive.example.com`) && (PathPrefix(`/assets/`) || PathPrefix(`/static/`) || PathPrefix(`/pdfjs/`))
      - traefik.http.routers.asterdrive-assets.entrypoints=websecure
      - traefik.http.routers.asterdrive-assets.tls=true
      - traefik.http.routers.asterdrive-assets.tls.certresolver=letsencrypt
      - traefik.http.routers.asterdrive-assets.priority=100
      - traefik.http.routers.asterdrive-assets.middlewares=asterdrive-static-cache
      - traefik.http.routers.asterdrive-assets.service=asterdrive

      - traefik.http.middlewares.asterdrive-static-cache.headers.customresponseheaders.Cache-Control=public, max-age=86400

      - traefik.http.services.asterdrive.loadbalancer.server.port=3000
```

Traefik fills common `X-Forwarded-*` headers by default.  
What you need to watch:

- `web` must redirect to `websecure`
- `websecure` timeouts must not be too short
- do not use a headers middleware to overwrite the page CSP returned by AsterDrive
- do not wrap WebDAV or thumbnail routes in another middleware that overwrites response headers

If you want `/assets/` to use more aggressive `immutable` caching, split out a separate router. Avoid applying strong caching to all `/api/v1/*`, because that may cache dynamic API responses and cause problems.

## CSP / Security Response Headers

AsterDrive now automatically returns a browser page baseline `Content-Security-Policy` for frontend HTML.
The reverse proxy should **preserve it**, not casually overwrite it with a narrower policy. If a security scanner still reports "no CSP", first check whether the proxy removed the upstream response header, or whether the scan only hit static asset / API paths.

### Separate the Two CSP Types First

Production now has two different policy layers that must be kept distinct:

- **Site page baseline CSP**: used for HTML pages such as `/`, admin panel, and sharing pages. It mainly restricts script, style, image, iframe, worker, and other resource loading sources.
- **Dangerous file inline sandbox CSP**: used for raw inline responses of files with script capability. The backend currently adds `Content-Security-Policy: sandbox` only to responses such as `text/html`, `application/xhtml+xml`, and `image/svg+xml`.

`sandbox` itself is a **Document directive**. It is suitable for restricting document contexts, not a general solution for "turn every file into `sandbox`".  
If you rewrite the site's own baseline CSP into a site-wide `sandbox`, normal HTML such as the login page, admin panel, and sharing pages will also enter a sandbox. Scripts, forms, storage, and same-origin capabilities become unavailable, breaking core site functionality.

Deployment should follow these principles:

- The reverse proxy preserves the **baseline CSP** returned by AsterDrive for site pages.
- Do not rewrite the whole site's `Content-Security-Policy` to `sandbox`.
- Do not remove the upstream `Content-Security-Policy` returned for dangerous inline files at the proxy layer.
- If both proxy layer and application layer return CSP, browsers enforce **both** policies. This is allowed, but the effective result is the stricter combination.

The current page baseline policy is:

```text
default-src 'self';
base-uri 'self';
object-src 'none';
frame-ancestors 'self';
script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: http: https:;
font-src 'self' data:;
connect-src 'self' http: https: ws: wss: blob:;
media-src 'self' blob:;
worker-src 'self' blob:;
frame-src 'self' http: https:;
manifest-src 'self';
```

This policy is based on current frontend behavior. Confirm impact before changing it:

- `script-src 'unsafe-inline'` must remain for now; custom frontends and placeholder injection may still include inline scripts.
- `style-src 'unsafe-inline'` must remain for now; the frontend uses runtime inline styles and dynamic `<style>`, and removing it may break styling.
- `img-src 'self' data: blob: http: https:` and `media-src 'self' blob:` must remain; thumbnails, image / video preview, avatar cropping, and external-link icons use these sources.
- `connect-src 'self' http: https: ws: wss: blob:` must remain; presigned upload / download, remote followers, and realtime push use these.
- `worker-src 'self' blob:` must remain; PDF preview uses workers, and some build styles use blob workers.
- `frame-src 'self' http: https:` must remain; external preview applications and WOPI entries may use cross-origin iframes.

If you must overwrite CSP at the gateway layer, start by copying the policy above, then tighten it according to your real deployment. Do not directly apply a generic template that only allows `connect-src 'self'`; presigned upload, external preview, or follower nodes can easily be blocked by that.

When tightening further, first run `Content-Security-Policy-Report-Only` through a real acceptance pass, then switch to enforcement mode.  
At minimum, test login, upload, PDF preview, text preview, sharing page, avatar, and external preview application / WOPI once.

## Do Not Miss These When Proxying WebDAV

If the proxy is whole-site passthrough, WebDAV is usually fine.  
Problems usually come from manually added restrictions:

- restricting `PROPFIND`, `LOCK`, or `UNLOCK`
- removing headers such as `Authorization` or `Destination`
- changing `/webdav/` to another prefix without updating client addresses too

If you change `[webdav].prefix = "/dav"`, update both the proxy layer and client addresses to `/dav/`.

## Extra Requirements for WOPI / Office Callbacks

If you integrate OnlyOffice, Collabora, or another WOPI service, confirm two more things:

- `public_site_url` must include a real HTTPS origin that the WOPI host can connect back to. If there are multiple entries, add them one by one to `Public Site URL`.
- The external Office service must be able to access `https://your-domain/api/v1/wopi/...`.

The most common symptoms are:

- opener buttons are visible, but loading fails after clicking
- the Office page opens, but cannot read the file
- the file opens, but cannot save back to AsterDrive

## Preserve Revalidation Semantics for Thumbnail Cache

AsterDrive thumbnail endpoints already return:

- `ETag`
- `Cache-Control: public/private, max-age=0, must-revalidate`

So the proxy layer should:

- preserve these response headers
- allow the browser to use `If-None-Match` for 304 revalidation

Do not:

- rewrite thumbnails to `immutable`
- remove `ETag`
- force CDN caching for hours without revalidation

## Minimum Post-Launch Acceptance

1. The browser can log in normally through `https://your-domain/`.
2. The home page response headers include the browser page baseline `Content-Security-Policy` returned by AsterDrive, and the proxy has not removed it or overwritten it with an incompatible policy.
3. Every line in `Admin -> System Settings -> Public Site URL` is a real `https://` origin. If there are multiple public domains, log in through each one and confirm WebDAV addresses and sharing links use the current domain.
4. Upload a large file and confirm it is not truncated by the proxy.
5. Open two browser tabs and confirm file changes refresh through SSE.
6. If WebDAV is enabled, connect once with a real client.
7. If WOPI is enabled, open and save a real Office file once.
