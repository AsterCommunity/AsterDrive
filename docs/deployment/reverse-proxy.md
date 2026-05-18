# 反向代理（生产环境必需）

AsterDrive 不内置 TLS 终端。  
只要你准备把站点暴露到公网、开放 WebDAV，或者接外部 Office / WOPI 服务，前面就**必须**有一层反向代理来负责：

- HTTPS 证书
- HTTPS 相关安全响应头，并保留 AsterDrive 返回的页面基线 `Content-Security-Policy`
- HTTP 到 HTTPS 重定向
- 大文件上传体积限制
- SSE 长连接超时和缓冲
- WebDAV 方法 / 请求头透传
- 前端静态资源缓存头

不要把 `:3000` 直接裸露到公网。  
这只适合本机或内网临时引导；正式上线请把 AsterDrive 绑定到内网地址，然后让 Caddy / Nginx / Traefik 对外暴露 `443`。

## 上线前先对齐这几个值

- `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 填成真实的 `https://` 来源，多个公开域名逐项添加，例如 `https://drive.example.com`
- 静态引导项 `auth.bootstrap_insecure_cookies` 只在纯 HTTP 首次引导时临时设成 `true`
- 正式切到 HTTPS 后，把 `auth.bootstrap_insecure_cookies` 去掉，并确认运行时 `auth_cookie_secure` 已恢复为开启
- 首页响应头里能看到 AsterDrive 返回的页面基线 `Content-Security-Policy`，代理层没有删掉或覆盖成不兼容的策略
- 不要把站点自己的基线 CSP 直接改成全站 `sandbox`
- 代理层不要拦掉 WebDAV 的 `PROPFIND`、`MOVE`、`COPY`、`LOCK`、`UNLOCK`
- 代理层不要覆盖缩略图接口返回的 `ETag` / `Cache-Control`
- 代理层必须保留真实 `Host`，并正确传递公网协议。AsterDrive 会用请求 Host 在 `公开站点地址` 列表里做精确匹配，生成对应域名的分享、WebDAV 和 WOPI URL
- 如果你希望 AsterDrive 识别真实客户端 IP，那么前置反向代理必须是你自己控制并信任的那一跳。AsterDrive 只会在连接来源命中 `rate_limit.trusted_proxies` 时，才读取 `X-Forwarded-For` 里的客户端 IP；否则会忽略转发头，继续按实际连接来源处理
- `X-Forwarded-For` / `Forwarded` 这类头不能直接当成用户身份凭据。只有当请求确实来自你配置的代理、网关或 Docker 内网跳板时，它们才有意义；不要把公网段、第三方 CDN 或你不控制的上游放进 `trusted_proxies`
- 如果你需要按真实客户端 IP 做限流、审计或会话安全判断，先看 [访问限流](/config/rate-limit) 对 `trusted_proxies` 的说明，再把同一组代理地址 / CIDR 同步到你的反向代理拓扑里

本文默认：

- AsterDrive 监听 `127.0.0.1:3000`
- WebDAV 前缀使用默认值 `/webdav`
- 域名是 `drive.example.com`

如果你改了监听地址、域名或 WebDAV 前缀，把下面配置里的对应值一起改掉。

### 多域名入口

同一个实例可以有多个公开入口，例如：

```text
https://drive.example.com
https://panel.example.com
https://intranet-drive.example.net
```

后台里仍然只改 `公开站点地址` 这一项，但每个来源单独一行。系统只接受精确 HTTP(S) 来源；不要带路径、不要写 `*`、不要填不受你控制的域名。第一行是默认回退来源。

当请求从 `panel.example.com` 进来，并且这一行已经在列表里时，系统会生成 `https://panel.example.com/...` 形式的 WebDAV、分享和 WOPI URL。未匹配到的 Host 不会被直接信任，系统会回退第一行，避免任意 Host 污染对外链接。

## 关键路径速查

| 用途 | 路径 |
| --- | --- |
| 前端页面 / 管理后台 / 分享页 | `/` |
| API | `/api/v1/` |
| SSE 存储变更流 | `/api/v1/auth/events/storage` |
| WOPI 回调 | `/api/v1/wopi/` |
| WebDAV | `/webdav/` |
| 前端构建资源 | `/assets/` |
| 内置静态资源 | `/static/` |
| PDF.js 资源 | `/pdfjs/` |

## Caddy

Caddy 最省事，开箱就能处理 HTTPS 和 HTTP 到 HTTPS 跳转。

```caddyfile
drive.example.com {
    encode zstd gzip

    @frontend_assets path /assets/*
    header @frontend_assets Cache-Control "public, max-age=31536000, immutable"

    @embedded_static path /static/* /pdfjs/*
    header @embedded_static Cache-Control "public, max-age=86400"

    reverse_proxy 127.0.0.1:3000 {
        # SSE 需要尽快 flush，不要让代理层攒着不发
        flush_interval -1
    }
}
```

这份配置已经满足：

- 自动 HTTPS
- 自动 HTTP 到 HTTPS 跳转
- 浏览器页面保留 AsterDrive 返回的基线 CSP
- SSE 立即刷出
- WebDAV / WOPI / 普通 API 全站透传

补充说明：

- Caddy 默认不会像 Nginx 那样主动卡一个很小的请求体上限；如果你自己额外加了 `request_body` 限制，记得同步放开
- 缩略图接口本身会返回 `ETag` 和 `must-revalidate`，这里不要再额外改写成强缓存

## Nginx

Nginx 需要你自己处理 HTTPS、重定向、上传大小和 SSE。

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

    # 大文件上传不要被代理层截断
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

这份配置里最容易漏的点就是：

- `client_max_body_size 0`
- `proxy_request_buffering off`
- SSE 专门关掉 `proxy_buffering`
- `X-Forwarded-Proto` 必须保留成 `https`
- `X-Real-IP` 只是辅助头，AsterDrive 不把它当作安全判断依据
- `X-Forwarded-For` 只有在请求来源命中 `trusted_proxies` 时才会被应用读取
- 如果反向代理前面还有 CDN、L4 负载均衡或云厂商网关，要明确哪一跳是 AsterDrive 信任的“最后一层代理”，然后只把那一跳的 IP / CIDR 放进 `trusted_proxies`
- 没有配置 `trusted_proxies` 时，AsterDrive 会直接按实际连接来源处理，不会相信转发头里的客户端 IP

如果你单独给 `/webdav/` 做 location，也不要加 `limit_except` 去限制方法；否则 Finder、Windows、rclone 一类客户端可能无法正常使用 WebDAV。

## Traefik

Traefik 更适合 Docker / Compose 场景。  
它分成两部分：

- Traefik 自己的静态配置：负责 entrypoint、HTTPS 和超时
- AsterDrive 容器的 labels：负责 Host 路由和转发端口

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

`readTimeout: 0s` 这一类设置很关键。  
不然大文件上传和 SSE 很容易在代理层先超时。

### `docker-compose.yml` labels

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

Traefik 默认会补上常见的 `X-Forwarded-*` 头。  
你真正要注意的是：

- `web` 要跳到 `websecure`
- `websecure` 的超时不要设得太短
- 不要用 headers middleware 把 AsterDrive 返回的页面 CSP 覆盖掉
- 不要再给 WebDAV 或缩略图路由套一层会覆盖响应头的 middleware

如果你想把 `/assets/` 做成更激进的 `immutable` 缓存，建议单独再拆一个 router；避免把所有 `/api/v1/*` 设置为强缓存，否则可能缓存动态 API 响应并引发问题。

## CSP / 安全响应头

AsterDrive 现在会给前端 HTML 自动返回一条页面基线 `Content-Security-Policy`。
反向代理要做的是**保留它**，而不是自己随手覆盖一条更窄的策略。安全扫描如果还报“无 CSP”，先看代理有没有把上游响应头删掉，或者只测到了静态资源 / API 路径。

### 先把两类 CSP 分清楚

生产环境里现在有两层不同的策略，需要明确区分：

- **站点页面基线 CSP**：给 `/`、管理后台、分享页这类 HTML 页面用，主要约束脚本、样式、图片、iframe、worker 等资源加载来源
- **危险文件 inline 沙箱 CSP**：给脚本能力文件的原始 inline 响应用，当前后端会只在 `text/html`、`application/xhtml+xml`、`image/svg+xml` 这类响应上额外挂 `Content-Security-Policy: sandbox`

`sandbox` 本身是 **Document directive**，适合约束文档上下文，不是“把所有文件都改成 `sandbox`”的通用方案。  
如果你把站点自己的基线 CSP 直接改成全站 `sandbox`，登录页、后台、分享页这些正常 HTML 也会一起进沙箱，脚本、表单、存储和同源能力都会不可用，导致站点核心功能受影响。

所以部署时要按这个原则走：

- 反向代理保留 AsterDrive 给站点页面返回的**基线 CSP**
- 不要把全站 `Content-Security-Policy` 统一改写成 `sandbox`
- 不要在代理层移除上游对危险 inline 文件返回的 `Content-Security-Policy`
- 如果代理层和应用层都返回了 CSP，浏览器会**同时执行**这些策略；这是允许的，但策略会按更严格的结果生效

当前页面基线策略长这样：

```text
default-src 'self';
base-uri 'self';
object-src 'none';
frame-ancestors 'self';
script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: http: https:;
font-src 'self' data:;
connect-src 'self' http: https: ws: wss:;
media-src 'self' blob:;
worker-src 'self' blob:;
frame-src 'self' http: https:;
manifest-src 'self';
```

这条策略是按当前前端实际行为整理出来的，调整前请确认影响：

- `script-src 'unsafe-inline'` 现在要保留；自定义前端和占位符注入里仍可能出现内联脚本
- `style-src 'unsafe-inline'` 现在要保留；前端里有运行时内联样式和动态 `<style>`，强制移除可能导致样式失效
- `img-src 'self' data: blob: http: https:` 和 `media-src 'self' blob:` 要保留；缩略图、图片 / 视频预览、头像裁剪、外链图标会用到这些来源
- `connect-src 'self' http: https: ws: wss:` 要保留；预签名上传 / 下载、远程 follower、实时推送都会用到
- `worker-src 'self' blob:` 要保留；PDF 预览会用 worker，某些构建方式会走 blob worker
- `frame-src 'self' http: https:` 要保留；外部预览应用和 WOPI 入口可能不是同源 iframe

如果你一定要在网关层覆盖 CSP，至少先抄上面这条，再按你的真实部署往回收。不要直接套用只允许 `connect-src 'self'` 的通用模板，预签名上传、外部预览或远程节点很容易因此被拦截。

想继续收紧时，先用 `Content-Security-Policy-Report-Only` 跑一轮真实验收，再切强制模式。  
至少测试一次登录、上传、PDF 预览、文本预览、分享页、头像、以及外部预览应用 / WOPI。

## WebDAV 代理时不要漏掉什么

只要代理层是“整站透传”，一般没事。  
出问题通常发生在你自己手动加了额外限制：

- 限制了 `PROPFIND`、`LOCK`、`UNLOCK`
- 把 `Authorization` 或 `Destination` 一类头删掉了
- 把 `/webdav/` 改成了别的前缀，但客户端地址没一起改

如果你改了 `[webdav].prefix = "/dav"`，那代理层和客户端地址也都要一起跟着改成 `/dav/`。

## WOPI / Office 回调的额外要求

如果你接的是 OnlyOffice、Collabora 或其他 WOPI 服务，再多确认两件事：

- `public_site_url` 必须包含 WOPI 宿主能回连到的真实 HTTPS 来源；如果有多个入口，把它们逐项填进 `公开站点地址`
- 外部 Office 服务必须能访问到 `https://你的域名/api/v1/wopi/...`

最常见的错误现象就是：

- 打开方式按钮能显示，但点开后加载失败
- Office 页面能打开，但读不到文件
- 能打开，却保存不回 AsterDrive

## 缩略图缓存请保留重新验证语义

AsterDrive 的缩略图接口已经返回了：

- `ETag`
- `Cache-Control: public/private, max-age=0, must-revalidate`

所以代理层应该做的是：

- 保留这些响应头
- 允许浏览器用 `If-None-Match` 走 304 重新验证

而不是：

- 把缩略图一把改成 `immutable`
- 去掉 `ETag`
- 用 CDN 强行缓存成几小时不更新

## 上线后最少验收一次

1. 浏览器能通过 `https://你的域名/` 正常登录
2. 首页响应头里已经能看到 AsterDrive 返回的页面基线 `Content-Security-Policy`，代理层没有删掉或覆盖成不兼容的策略
3. `管理 -> 系统设置 -> 公开站点地址` 里每一行都是真实 `https://` 来源；如果有多个公开域名，分别从每个域名登录一次，确认 WebDAV 地址和分享链接使用当前域名
4. 上传一个大文件，确认不会被代理层截断
5. 打开两个浏览器标签页，确认文件变更能通过 SSE 刷新出来
6. 如果启用了 WebDAV，用真实客户端连一次
7. 如果启用了 WOPI，用真实 Office 文件试开并保存一次
