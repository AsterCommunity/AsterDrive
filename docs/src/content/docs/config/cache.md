---
title: "缓存"
---

:::tip[这一篇覆盖 `[cache]`]
单机部署保持默认（内存缓存）就够了。只有多实例部署、希望共享缓存时才考虑 Redis。
不确定要不要引入 Redis？多数单机部署不需要引入 Redis。
:::

```toml
[cache]
backend = "memory"
endpoint = ""
default_ttl = 3600
```

## 大多数部署直接保持默认

单机、NAS、小团队部署，内存缓存够用。**只有这两种情况才值得上 Redis**：

- 多实例部署
- 多个应用实例之间需要共享缓存

## 选项一览

| 选项 | 默认值 | 作用 |
| --- | --- | --- |
| `backend` | `"memory"` | `memory` 或 `redis` |
| `endpoint` | `""` | Redis 连接地址，仅 `backend = "redis"` 时使用 |
| `default_ttl` | `3600` | 默认 TTL，单位秒 |

## Redis 认证

现有完整 URL 字符串继续兼容：

```toml
endpoint = "redis://encoded-user:encoded-password@cache.internal:6379/0"
```

用户名或密码含保留字符时，推荐直接传原始凭据：

```toml
endpoint = { base_url = "redis://cache.internal:6379/0", username = "RAW_USERNAME", password = "RAW_PASSWORD" }
```

无 ACL 用户名、只有密码的 Redis 使用 `username = ""`。原始凭据不要预编码，`base_url` 不能包含 userinfo；AsterDrive 和 Forge 的 Debug/配置序列化不会输出 username/password。生产环境应限制配置文件权限，Kubernetes 建议通过 Secret 挂载完整配置。

对应的结构化环境变量：

```bash
ASTER__CACHE__ENDPOINT__BASE_URL=redis://cache.internal:6379/0
ASTER__CACHE__ENDPOINT__USERNAME=
ASTER__CACHE__ENDPOINT__PASSWORD=RAW_PASSWORD
```

## Redis 连不上会怎样

把 `backend` 设成 `redis` 但启动时 Redis 连不上时，AsterDrive 会让 Forge cache 构造器使用 `ReturnError` 策略，直接返回连接错误并终止启动。这样 single 和 cluster 都不会在运维人员以为正在使用 Redis 时悄悄改成进程本地缓存，编排器也能在 Redis 恢复后重试启动。

如果 Redis backend 已经成功建立，运行期短暂断线会进入受控 fallback/circuit 状态：`/health` 继续表示进程存活，`/health/ready` 返回失败；连接恢复后 backend 会重新变为 ready。

## 对应环境变量

```bash
ASTER__CACHE__BACKEND=memory
ASTER__CACHE__ENDPOINT=redis://127.0.0.1:6379/0
ASTER__CACHE__DEFAULT_TTL=3600
```
