# 访问限流

::: tip 这一篇覆盖 `[rate_limit]` 和 `[network_trust]`
默认关闭。打开后按访问来源 IP 对登录、公开访问、API、写操作分别限流。
**反向代理后面可以用，但要配 `network_trust.trusted_proxies`**——不配的话，应用只能看到代理 IP，很容易把所有用户当成同一个来源。
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

## 什么时候建议开

- 服务直接暴露在公网
- 想拦登录入口的暴力尝试
- 想拦公开分享页被频繁探测
- 想控制高成本写操作的瞬时压力

## 四组规则分别管什么

| 分组 | 作用 |
| --- | --- |
| `auth` | 登录、注册、刷新令牌、分享密码验证等敏感操作 |
| `public` | 公开分享页和匿名访问 |
| `api` | 已登录用户的大多数日常操作 |
| `write` | 批量操作、管理后台等高成本写操作 |

## 两个旋钮怎么理解

| 设置项 | 作用 |
| --- | --- |
| `seconds_per_request` | 平均多久允许一次请求（令牌补充速率） |
| `burst_size` | 短时间内允许的突发请求数（令牌桶上限） |

例：

```toml
[rate_limit.auth]
seconds_per_request = 2
burst_size = 5
```

同一来源 IP 在认证类访问上可以**先连续发出 5 个请求**，之后按"每 2 秒一个"补充配额。

## 触发后用户看到什么

- 服务端返回 `429 Too Many Requests`
- 响应头带 `Retry-After`
- 前端会显示"稍后再试"

## 反向代理后面怎么配

默认 `[network_trust].trusted_proxies = []` 是最安全的：AsterDrive 忽略 `X-Forwarded-For`，直接按实际连接来源 IP 限流。这样不会被伪造 XFF 绕过，但反代后通常只能看到代理地址。反向代理部署的更完整说明见 [反向代理](/deployment/reverse-proxy#上线前先对齐这几个值)。

如果你的部署是：

- Nginx / Caddy 反代到 AsterDrive
- Docker 网桥
- 任何让所有请求都从同一个代理地址进入的网络拓扑

那就把你**自己控制的代理 IP / CIDR** 放进 `trusted_proxies`：

```toml
[network_trust]
trusted_proxies = ["127.0.0.1", "172.16.0.0/12"]

[rate_limit]
enabled = true
```

规则很简单：

- 只有连接来源 IP 命中 `trusted_proxies` 时，AsterDrive 才会读取 `X-Forwarded-For` 最左侧的客户端 IP
- 连接来源不可信时，`X-Forwarded-For` 会被忽略，继续按实际连接 IP 限流
- `trusted_proxies` 支持单 IP 和 CIDR，例如 `127.0.0.1`、`10.0.0.0/8`、`172.16.0.0/12`
- 不要把不受你控制的公网段放进去，这等于相信别人帮你报真实 IP
- 这组 `trusted_proxies` 也会影响认证会话里按客户端 IP 做的复用判断，所以它必须和你的真实反向代理拓扑一致，不能只顾限流不顾登录安全

如果你不想在应用层处理这件事，也可以继续关掉 AsterDrive 限流，把限流交给反向代理（Nginx `limit_req`、Caddy `rate_limit`、Traefik `RateLimit` 中间件）。但不建议两边都配置得很紧，否则排障时容易混淆。

## 几条经验

- 第一次启用保守一点，`burst_size` 不要设得太小
- 对外开放公开分享页时，重点关注 `auth` 和 `public`
- 反代后先确认 `trusted_proxies` 覆盖的是代理到 AsterDrive 的那一跳，不一定是公网入口 IP
- 不确定时先在测试环境观察一段时间再上
