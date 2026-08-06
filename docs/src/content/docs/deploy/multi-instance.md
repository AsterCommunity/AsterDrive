---
title: "负载均衡与多实例"
description: AsterDrive 多 Primary 负载均衡部署契约，覆盖共享依赖、存储与上传限制、配置同步、健康检查、任务协调和上线验收。
---

:::caution[先声明 cluster profile]
只有在所有 Primary 都设置 `[deployment].profile = "cluster"`，并通过启动检查、拓扑检查和 readiness 检查后，才应把它们放进同一个负载均衡 upstream。Follower 是远程存储节点，不是承载普通用户请求的 Primary 扩容副本。
:::

## 支持的拓扑

```text
浏览器 / WebDAV / WOPI / Follower tunnel
                    │
              Ingress / LB
             ┌──────┴──────┐
          Primary A     Primary B
             └──────┬──────┘
        PostgreSQL / MySQL + Redis
                    │
          共享存储 / 远程 Follower
```

负载均衡器可以把新的 HTTP 请求分配到任意 ready Primary。AsterDrive 支持的多实例路径不把 sticky session 当作正确性条件：认证和业务元数据以共享数据库为准，缓存与跨实例通知使用 Redis，存储内容必须位于所有 Primary 都能访问的数据面。

长连接仍会在建立后停留在某一个 Primary。Primary 退出时，SSE、WebDAV 请求、正在传输的 HTTP body 或 reverse tunnel 连接需要由客户端重连或重试；负载均衡器不能把一条已经建立的连接无缝搬到另一台实例。

## 所有 Primary 必须共享什么

| 项目 | 要求 |
| --- | --- |
| 权威数据库 | 使用同一份 PostgreSQL 或 MySQL；cluster 不接受 SQLite |
| Cache | `[cache].backend = "redis"`，并连接同一份共享缓存 |
| 配置与事件通知 | `[config_sync].backend = "redis"`，所有实例使用相同 endpoint 和 topic |
| 存储数据面 | 默认策略以及用户、团队可能命中的策略必须从每个 Primary 可访问 |
| 静态密钥 | `jwt_secret`、`share_cookie_secret`、`direct_link_secret`、`mfa_secret_key`、`storage_credential_secret_key` 在所有 Primary 上保持一致 |
| 内部代理密钥 | 使用 reverse tunnel 时，所有 Primary 使用相同的 `internal_proxy_secret` |

`config_sync` 只通知其他进程重新读取数据库里的运行时配置，不会同步 `config.toml`、环境变量或 Secret。除监听地址、实例名和 `deployment.internal_endpoint` 这类实例专属值外，建议让所有 Primary 使用同一份静态配置模板。

用户上传的头像仍保存在 `avatar_dir`。如果启用上传头像，多实例部署需要把该目录挂载为所有 Primary 可读写的共享目录，并使用一致路径；否则在 Primary A 上传的头像可能无法从 Primary B 读取。Gravatar 不依赖这个目录。

## 存储与上传限制

cluster 模式会直接拒绝创建或保留 `local` 存储策略。即使每个 Pod 使用相同路径名，或底层准备了 RWX/NFS 挂载，当前版本仍按驱动类型拒绝 `local` policy；共享文件系统目前只适合 `avatar_dir` 等明确记录的本地目录，不会把 `local` policy 变成 cluster 支持路径。

| 存储路径 | cluster 行为 |
| --- | --- |
| S3-compatible、腾讯云 COS、华为云 OBS、Azure Blob | connector-native multipart、预签名和浏览器直传可用 |
| OneDrive | provider resumable 的服务端中继或浏览器直传可用 |
| 远程 Follower | relay / presigned 可用；reverse tunnel 还需要内部代理配置 |
| SFTP | 所有 Primary 都能访问同一 SFTP 服务时可用于单请求直传；需要 stream staging 的可恢复分片上传会被拒绝 |
| `local` policy | cluster 中拒绝创建、启用或通过拓扑检查 |

限制的核心不是文件大小，而是上传会话的临时状态归谁所有。AsterDrive 会在创建会话前拒绝需要 Pod-local offset/stream staging 的路径，拒绝后不会留下 upload session 或暂存文件。connector-native multipart、预签名、浏览器直传以及远程 relay/presigned 把临时状态放在共享数据库或存储数据面，因此可以跨 Primary 继续。

不要用 sticky session、相同的 `upload_temp_dir` 字符串或每个 Pod 各自的本地卷绕过这项检查。上传模式的选择和排查见[上传与大文件](/using/upload-download/#cluster-部署时的上传选择)。

## 配置、事件与一致性

运行时配置、存储策略、策略组、存储凭据、远程节点拓扑和用户策略组绑定以数据库为权威。写入实例完成数据库事务后，通过 Redis 通知其他实例重新加载对应 snapshot。跨实例 Storage SSE 使用独立 Redis topic，不与配置 reload 共用 channel。

Redis pub/sub 不保存历史消息。某个 Primary 断线时，客户端会收到 `sync.required` 并从权威 API 刷新；订阅恢复后，实例会重新加载完整运行时配置和存储拓扑，再继续接收新事件。完整故障语义见[配置同步](/reference/config/config-sync/#redis-故障时会怎样)。

## Reverse Tunnel 路由

reverse tunnel 的 WebSocket、lane 和 pending request 仍由接收连接的 owner Primary 持有。共享数据库保存 owner lease 和 fencing token；请求落到非 owner Primary 时，AsterDrive 通过 authenticated streaming proxy 转发到 owner。

每个 Primary 的 `deployment.internal_endpoint` 必须是其他 Primary 能直接访问、且唯一指向该实例的绝对 HTTP(S) URL。不要把所有实例都填成同一个公开负载均衡地址，否则内部转发可能再次落到非 owner。所有 Primary 使用相同且至少 32 个字符的 `internal_proxy_secret`，并把内部端点限制在可信网络内。

两项都留空表示 direct-only cluster。此时 direct Follower 可用，已启用的 reverse tunnel 或空 `base_url` 的 `auto` 节点会让拓扑检查失败。

## 健康检查与流量摘除

| 探针 | 用途 | cluster 行为 |
| --- | --- | --- |
| `/health` | liveness | 只表示进程存活；Redis 短暂中断时仍返回 `200` |
| `/health/ready` | readiness | 始终检查数据库、实际 Redis cache 和拓扑；初始化完成后再执行默认存储 driver 的轻量 readiness 检查 |

Kubernetes、Ingress controller 和其他负载均衡器应只把 `/health/ready` 返回成功的 Primary 放进 upstream，不要用 `/health` 代替 readiness。启动时 Redis 初始化失败会由 Forge cache 构造器直接返回错误并终止启动，由编排器在 Redis 恢复后重启；已经成功创建的 Redis backend 在运行期短暂断线时会保持 liveness、使 readiness 失败，并自行重连恢复 ready。

全新数据库在 single 和 cluster 下都不创建默认存储策略。基础依赖健康时，`/health/ready` 在初始化期间返回 `200`，响应状态为 `needs_admin` 或 `needs_storage`，使管理员能通过普通负载均衡入口完成初始化。共享存储策略设为默认并完成管理员策略组回填后，状态变为 `ready`；此后默认 driver 的轻量 readiness 检查失败会返回 `503`。该探针不对远端存储执行对象读写，真实数据面可用性还需要单独监控。

## Migration、调度器与后台任务

每个 Primary 都可以在启动时执行 migration。PostgreSQL 使用事务级 advisory lock，MySQL 使用 named lock，把 migration history 检查和 DDL 串行化；数据库账号仍需具备 DDL 权限。

周期任务由共享数据库 lease 选出一个 owner，standby 在 owner 退出或 lease 过期后接管。普通后台任务通过数据库 claim、lease 和 fencing token 防止两个 Primary 同时提交同一任务结果。实例故障时，正在使用本地临时文件的任务可能从重试点重新执行，而不是从中断的字节位置继续。

## 限流不是全局计数

AsterDrive 的 HTTP Governor 和 WebDAV IP token bucket 在每个进程内独立计数。两个 Primary 后面的同一客户端可能分别消耗两份 burst 配额，所以应用内配置不等于 cluster-wide 全局限流。

如果需要严格的全局入口配额，在 Ingress、API gateway 或负载均衡层执行；AsterDrive 内部限流仍可作为单实例保护。无论在哪一层限流，都要正确配置 `network_trust.trusted_proxies`，只信任实际连接到 AsterDrive 的最后一跳代理。详见[访问限流](/reference/config/rate-limit/#多实例计数边界)。

## 负载均衡器要求

- 保留真实 `Host` 和公网协议，按可信代理链传递客户端 IP
- 支持 SSE、WebDAV、下载和上传的流式传输，关闭会破坏流式请求的缓冲
- reverse tunnel 经过该入口时支持 WebSocket Upgrade
- 上传、下载、SSE、WebDAV 和 WOPI 使用足够长的 read/write/idle timeout
- request body 上限覆盖实际上传和 WebDAV 写入需求
- 只把 readiness 成功的实例加入 upstream，并在终止前先摘流量再优雅关闭
- 不用 sticky session 掩盖共享状态、共享存储或 Pod-local staging 问题

代理示例和请求头细节见[反向代理](/deploy/reverse-proxy/)。

## 上线验收

至少完成以下验证：

1. 同时启动两个 Primary，确认全新数据库只执行一次 migration，两个实例以 `needs_admin` 进入 Service；通过负载均衡入口创建管理员和默认共享存储后，确认两个实例都变为 `ready`。
2. 通过负载均衡入口重复登录、刷新 token、创建目录、上传、下载和 WebDAV 读写，确认请求切换实例后仍正确。
3. 在 Primary A 修改运行时配置、存储策略、策略组和用户绑定，确认 Primary B 无需重启即可使用新状态。
4. 停止 Redis，确认 `/health` 保持 `200`、`/health/ready` 变为 `503`、SSE 收到 `sync.required`；恢复后确认 readiness 和订阅自动恢复。
5. 停止当前调度 owner，确认 standby 接管；执行一个后台任务并确认只有一个最终结果。
6. 使用 reverse tunnel 时，让请求命中非 owner Primary，确认文件流经 owner 返回，并验证旧 fencing token 被拒绝。
7. 验证实际使用的每一种上传策略；特别确认 SFTP 大文件或其他 staging 路径会在创建会话前明确失败。
8. 对上传头像功能做跨实例读取测试，并确认 Ingress/LB 层的全局限流符合预期。

完整生产检查清单见[生产上线检查](/ops/launch-checklist/)。仓库内置的双 Primary StatefulSet、Service、PDB、PVC 和 Ingress 示例见 [Kubernetes 部署](/deploy/kubernetes/)。
