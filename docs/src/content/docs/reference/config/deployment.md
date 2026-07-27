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

`internal_endpoint` 必须是当前 primary 可从其他 primary 直接访问、且唯一指向该实例的绝对 `http`/`https` URL，不能带 query 或 fragment；不要填写所有实例共用的负载均衡地址。`internal_proxy_secret` 至少 32 个字符，并且必须在所有 primary 上保持一致。两项都留空表示 direct-only cluster；只配置其中一项会在静态配置检查阶段失败。

`cluster` profile 会在启动、`/health/ready` 和 `aster_drive doctor` 中检查共享数据库、Redis 和存储拓扑。single 和 cluster 的全新数据库都不会自动创建存储策略；创建管理员后都进入 `needs_storage`，但 cluster 必须在管理端创建所有 Primary 都能访问的共享策略并设为默认。

共享依赖、静态密钥、上传与 SFTP 限制、reverse tunnel owner routing、健康检查、migration 锁、任务 lease 和 Ingress 要求统一见[负载均衡与多实例](/deploy/multi-instance/)。上传模式的具体选择见[上传与大文件](/using/upload-download/#cluster-部署时的上传选择)。

环境变量写法：

```bash
ASTER__DEPLOYMENT__PROFILE=cluster
```

需要在本地复现双 primary 的 reverse tunnel 路由和接管验收时，显式开启测试 feature：

```bash
cargo test --features multi-primary-e2e --test test_multi_primary_e2e reverse_tunnel_ -- --ignored
```
