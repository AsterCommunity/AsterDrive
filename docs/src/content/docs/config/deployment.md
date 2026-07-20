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
- 远端节点使用 `direct` transport

当前 reverse tunnel registry 属于单个 primary 进程，因此 `reverse_tunnel`，以及空 `base_url` 的 `auto` 节点，仅适用于 `single` profile。跨 primary tunnel owner routing 落地后再扩展这项契约。

`cluster` profile 会在启动、`/health/ready` 和 `aster_drive doctor` 中检查这些组合。它用于声明部署意图和阻止已知不兼容拓扑；负载均衡、跨实例 storage SSE 和 reverse tunnel owner routing 仍是独立能力。

全新 cluster 数据库不会自动创建本地默认 policy。先在管理端创建并设定一个共享存储 policy；在此之前 `/health/ready` 会保持非 ready 状态。

环境变量写法：

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```
