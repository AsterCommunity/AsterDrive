---
title: "Rate Limiting"
---

:::tip[This page covers `[rate_limit]` and `[network_trust]`]
Disabled by default. After enabling it, AsterDrive rate-limits login, public access, APIs, and write operations by source IP.
**It can be used behind a reverse proxy, but you must configure `network_trust.trusted_proxies`**. Without it, the application only sees the proxy IP and can easily treat all users as the same source.
:::

```toml
[network_trust]
trusted_proxies = []

[rate_limit]
enabled = false

[rate_limit.auth]
seconds_per_request = 2
burst_size = 5

[rate_limit.public]
seconds_per_request = 1
burst_size = 30

[rate_limit.api]
seconds_per_request = 1
burst_size = 120

[rate_limit.write]
seconds_per_request = 2
burst_size = 10
```

## When to Enable It

- The service is exposed directly to the public internet
- You want to slow brute-force attempts on login entry points
- You want to slow frequent probing of public share pages
- You want to control burst pressure from high-cost write operations

## What the Four Rule Groups Cover

| Group | Purpose |
| --- | --- |
| `auth` | Sensitive operations such as login, registration, token refresh, and share password verification |
| `public` | Public share pages and anonymous access |
| `api` | Most daily operations by logged-in users |
| `write` | High-cost write operations such as batch operations and admin-console actions |

## How to Understand the Two Knobs

| Setting | Purpose |
| --- | --- |
| `seconds_per_request` | Average time between allowed requests, meaning the token refill rate |
| `burst_size` | Number of burst requests allowed in a short time, meaning the token bucket capacity |

Example:

```toml
[rate_limit.auth]
seconds_per_request = 2
burst_size = 5
```

The same source IP can **send 5 authentication-related requests in a row first**, then its quota refills at "one request every 2 seconds".

## What Users See After It Triggers

- The server returns `429 Too Many Requests`
- The response includes a `Retry-After` header
- The frontend shows "Try again later"

## How to Configure It Behind a Reverse Proxy

The default `[network_trust].trusted_proxies = []` is the safest configuration. AsterDrive ignores `X-Forwarded-For` and rate-limits by the actual connection source IP, which prevents forged XFF from bypassing limits. Behind a reverse proxy, however, the server usually sees only the proxy address. See [reverse proxy](/en/deployment/reverse-proxy/#align-these-values-before-launch) for the full reverse proxy deployment notes.

If your deployment uses:

- Nginx / Caddy reverse proxy to AsterDrive
- Docker bridge networking
- Any topology where all requests enter through the same proxy address

Then put the proxy IPs / CIDRs **that you control** into `trusted_proxies`:

```toml
[network_trust]
trusted_proxies = ["127.0.0.1", "172.16.0.0/12"]

[rate_limit]
enabled = true
```

The rules are simple:

- AsterDrive reads the leftmost client IP in `X-Forwarded-For` only when the connection source IP matches `trusted_proxies`
- When the connection source is not trusted, `X-Forwarded-For` is ignored and the actual connection IP is still used for rate limiting
- `trusted_proxies` supports single IPs and CIDRs, such as `127.0.0.1`, `10.0.0.0/8`, and `172.16.0.0/12`
- Do not add public network ranges you do not control. That is equivalent to trusting someone else to report the real IP for you.
- This `trusted_proxies` list also affects client-IP reuse checks in authentication sessions, so it must match your actual reverse proxy topology. Do not configure it only for rate limiting while ignoring login safety.

If you do not want to handle this at the application layer, you can keep AsterDrive rate limiting disabled and delegate it to the reverse proxy, such as Nginx `limit_req`, Caddy `rate_limit`, or Traefik `RateLimit` middleware. Avoid configuring both sides too tightly, or troubleshooting becomes confusing.

## Practical Notes

- Start conservatively when enabling it for the first time. Do not set `burst_size` too low.
- When exposing public share pages, pay extra attention to `auth` and `public`.
- Behind a reverse proxy, first confirm that `trusted_proxies` covers the hop that proxies to AsterDrive. It is not necessarily the public ingress IP.
- If unsure, observe it in a test environment for a while before enabling it in production.
