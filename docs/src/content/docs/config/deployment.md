---
title: "部署模式"
description: AsterDrive single 与 cluster 部署模式，以及 cluster 模式的共享依赖和拓扑检查。
---

`[deployment]` 声明当前实例采用单实例还是多 primary 集群部署。默认值保持单实例体验：

```toml
[deployment]
profile = "single"
```

可选值：

| 值 | 用途 |
| --- | --- |
| `single` | 默认模式；适合 SQLite、memory cache、本地存储和单 primary reverse tunnel |
| `cluster` | 多 primary 部署；启用共享依赖和拓扑兼容性检查 |

## cluster 前置条件

```toml
[deployment]
profile = "cluster"
internal_endpoint = "http://primary-a:3000"
internal_proxy_secret = "replace-with-at-least-32-random-characters"

[database]
url = "postgres://aster:password@postgres/asterdrive"

[cache]
backend = "redis"
endpoint = "redis://redis:6379/0"

[config_sync]
backend = "redis"
endpoint = "redis://redis:6379/0"
topic = "aster_drive.config_reload"
```

所有 primary 实例还需要：

- 连接同一份 PostgreSQL 或 MySQL 权威数据库
- 使用同一个 Redis cache endpoint
- 使用同一个 config sync endpoint 和 topic
- 使用 S3、Azure Blob、OneDrive、SFTP 等所有实例均可访问的存储；`local` policy 不属于共享存储
- 如果使用 `reverse_tunnel` 或空 `base_url` 的 `auto` 节点，每个 primary 都要配置自己的 `internal_endpoint`，并在所有 primary 上设置相同的 `internal_proxy_secret`

reverse tunnel 的连接/lane/pending state 仍属于单个 primary 进程，但 cluster 模式会把 owner directory、lease/fencing token 写入共享数据库，并通过 authenticated streaming proxy 将非 owner primary 的请求转发到 owner。文件 body 走 owner primary 的 HTTP data plane，不经过 Redis 或数据库。

`internal_endpoint` 必须是当前 primary 可从其他 primary 访问的绝对 `http`/`https` URL，不能带 query 或 fragment；`internal_proxy_secret` 至少 32 个字符，并且必须在所有 primary 之间保持一致。两项都留空时表示 cluster 的 direct-only 拓扑；只配置其中一项会在静态配置检查阶段失败。

`cluster` profile 会在启动、`/health/ready` 和 `aster_drive doctor` 中检查这些组合。它用于声明部署意图和阻止已知不兼容拓扑；负载均衡和跨实例 storage SSE 仍是独立能力，reverse tunnel owner routing 由上面的 owner directory 和 proxy 配置启用。

全新 cluster 数据库不会自动创建本地默认 policy。先在管理端创建并设定一个共享存储 policy；在此之前 `/health/ready` 会保持非 ready 状态。

环境变量写法：

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```

需要在本地复现双 primary 的 reverse tunnel 路由和接管验收时，显式开启测试 feature：

```bash
cargo test --features multi-primary-e2e --test test_multi_primary_e2e reverse_tunnel_ -- --ignored
```
