---
description: AsterDrive 多实例运行时配置同步说明，覆盖 Redis 通知后端、权威数据库、config.toml、环境变量、CLI 和故障边界。
title: "配置同步"
---

:::tip[这一篇覆盖 `[config_sync]`]
单实例部署保持默认关闭即可。只有多个 AsterDrive 进程共享同一份数据库，并且需要让后台系统设置和 `aster_drive config` 修改及时传播到其他实例时，才需要启用配置同步。
:::

```toml
[config_sync]
backend = "disabled"
endpoint = ""
topic = "aster_drive.config_reload"
```

## 它解决什么问题

管理员在一个实例修改系统设置后，该实例会立即更新自己的运行时配置；其他实例不会自动知道数据库已经变化。

启用配置同步后，写入实例会通过 Redis pub/sub 发送一条 reload 通知。其他实例收到通知后，重新从权威数据库加载完整运行时配置。

```text
管理 API / config CLI
        │
        ├─ 写入共享数据库
        ├─ 更新本进程 snapshot
        └─ 发布 Redis reload 通知
                    │
              其他 AsterDrive 实例
                    │
                    └─ 从共享数据库全量 reload
```

Redis **不保存配置值**，也不替代数据库。通知中的 key 只用于观测和清理派生缓存；接收方仍从数据库加载权威值。

## 单实例保持默认

单机、NAS 或只有一个 AsterDrive 进程时，不需要 Redis：

```toml
[config_sync]
backend = "disabled"
endpoint = ""
topic = "aster_drive.config_reload"
```

关闭时，后台系统设置和 CLI 写入仍然正常，只是不发送跨进程通知。

## 多实例配置

所有实例必须满足：

- 连接同一份 PostgreSQL、MySQL 或其他共享权威数据库
- 连接同一个 Redis 服务
- 使用相同的 `topic`
- 每个实例都启用 `[config_sync]`

```toml
[config_sync]
backend = "redis"
endpoint = "redis://127.0.0.1:6379/"
topic = "aster_drive.config_reload"
```

如果 Redis 需要认证或 TLS，按 Redis URL 的标准格式写入 `endpoint`，并限制配置文件读取权限。

:::caution[SQLite 不适合跨主机多实例]
配置同步不会把本地 SQLite 文件复制到其他主机。多个实例必须真正访问同一份权威数据库；不要给每台机器各放一份 SQLite，然后期待 Redis 帮你同步配置值。
:::

## 配置项

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `backend` | `"disabled"` | `disabled` 或 `redis` |
| `endpoint` | `""` | Redis URL，仅 `backend = "redis"` 时使用 |
| `topic` | `"aster_drive.config_reload"` | 产品级 reload topic；同一组实例必须一致 |

对应环境变量：

```bash
ASTER__CONFIG_SYNC__BACKEND=redis
ASTER__CONFIG_SYNC__ENDPOINT=redis://127.0.0.1:6379/
ASTER__CONFIG_SYNC__TOPIC=aster_drive.config_reload
```

启用 Redis 配置同步时，AsterDrive 的跨实例 Storage SSE 会使用同一组实例配置派生出的独立 topic：`aster_drive.config_reload` 对应 `aster_drive.storage_events`。配置 reload 和文件/目录变更事件不会共用 Redis channel，所有实例必须保持相同的 `topic` 才能互相接收事件。

修改 `[config_sync]` 后需要重启进程。这一组是静态启动配置，不在后台系统设置里动态修改。

## 哪些写入会发送通知

这些入口在数据库写入成功后发布 reload 通知：

- 管理后台或管理 API 修改、删除系统设置
- `aster_drive config set`
- `aster_drive config delete`
- `aster_drive config import`

一次操作联动修改多个配置项时，只发布一条包含全部 changed keys 的通知。实例启动时的 migration、默认值补种和配置修复不会发布通知；实例会在启动流程中直接加载完整 snapshot。

## Redis 故障时会怎样

`[config_sync]` 和 `[cache]` 的故障语义不同：

- `[cache].backend = "redis"` 连接失败时，缓存可以回退到进程内 memory cache
- `[config_sync].backend = "redis"` 的 backend、endpoint URL 等配置无效，导致通知后端无法构造时，实例启动失败
- Redis URL 合法但服务暂时不可达时，实例可以完成启动；订阅 supervisor 会记录 `disconnected`，使用有界指数退避和抖动自动重连
- 管理 API 或 CLI 已完成数据库写入，但随后发布通知失败时，命令会返回错误；本地值已经写入，其他实例可能要等重启或下一条成功通知后才重新加载

运行期间 Redis 短暂断开时，config reload 和 Storage SSE 的订阅都会进入 `disconnected` / `reconnecting` 状态。Drive 会向本地存活的 SSE 客户端发送一次 `sync.required`，提示前端从权威 API 刷新。Redis 恢复后无需重启实例：订阅进入 `recovered`，config reload 会从权威数据库执行一次完整 snapshot reconcile，Storage SSE 则继续接收新的跨实例事件。Redis pub/sub 不提供历史回放，断线窗口内丢失的具体 Storage SSE 事件由 `sync.required` 的全量刷新覆盖。

:::caution[Redis pub/sub 不补发历史消息]
Redis pub/sub 不是持久消息队列。某个实例离线期间错过的通知不会在它恢复后重放；但实例每次启动都会从数据库全量加载，所以重启后会回到权威状态。
:::

## 和缓存 Redis 的关系

`[cache]` 与 `[config_sync]` 可以使用同一个 Redis 服务，也可以使用不同服务或不同数据库 URL，但它们解决的是两件事：

- `[cache]`：共享缓存内容和 TTL
- `[config_sync]`：通知其他进程重新加载数据库配置

多实例部署如果只配置 Redis cache、没有配置 config sync，缓存可以共享，但后台系统设置仍可能只在处理写请求的实例立即生效。

## 上线验证

至少启动两个连接同一数据库和 Redis 的实例，然后：

1. 在实例 A 修改一个不需要重启的系统设置，例如站点标题。
2. 通过实例 B 刷新对应页面，确认设置及时生效。
3. 在任一实例日志和指标里确认订阅连接保持稳定，或在短暂故障后进入 `recovered`。
4. 用 CLI 修改一个测试用自定义配置，再从另一实例读取。
5. 暂停 Redis，确认出现 `disconnected` / `reconnecting`，且 SSE 客户端收到 `sync.required`；恢复 Redis 后确认出现 `recovered`，并验证下一次配置修改和文件变更都能继续同步。

上线前也应把 Redis 可用性和所有实例的 topic 一致性加入 [生产上线检查](/deployment/production-checklist/)。

## 常见问题

### 后台修改后只有一个实例生效

依次检查：

- 所有实例是否启用了 `backend = "redis"`
- `endpoint` 是否都能从容器或主机内部访问
- `topic` 是否完全一致
- 实例是否连接同一份数据库
- 日志和指标里是否持续出现 Redis 连接错误或 `disconnected` / `reconnecting`，以及之后是否缺少 `recovered`

### 可以只给 primary 配吗

不建议。所有会承载读取流量、运行后台任务或作为 follower 读取运行时配置的实例都应启用同一组同步配置。

### 可以用它同步静态 `config.toml` 吗

不可以。它只同步数据库支持的运行时系统设置。监听地址、数据库 URL、节点模式、日志、WebDAV 前缀、cache 和 config sync 自身仍需分别部署，并在修改后重启实例。
