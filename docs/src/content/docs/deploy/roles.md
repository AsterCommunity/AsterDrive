---
title: "API 与 worker 分离"
description: "同一二进制的 all/api/worker 职责、共享状态、取消、并发和迁移契约。"
---

`./aster_drive`、`./aster_drive serve`、`./aster_drive serve all` 默认组合完整服务。
SQLite + memory cache + local storage 仍可单进程部署，不需要额外 worker。

| 命令 | 职责 |
| --- | --- |
| `serve all` | 业务 HTTP、生产任务、本地维护、消费、周期调度、邮件 |
| `serve api` | REST/前端/WebDAV/WOPI、上传下载、任务查询/取消、SSE、本地资源维护 |
| `serve worker` | 持久化任务消费、单 owner 周期调度与清理、邮件；HTTP 仅 `/health*` |

Follower 仍由 `server.start_mode = "follower"` 决定。默认命令与 `serve all` 使用该配置；
Follower 配置搭配显式 `serve api` / `serve worker` 会在启动时报错。
部署 profile（single/cluster）、节点模式（primary/follower）和进程职责是三个不同概念。

## 共享数据与恢复

分离部署使用 cluster profile，共享 PostgreSQL/MySQL、Redis cache/config sync、认证与凭据密钥和存储数据面。
API 与 worker 使用相同版本、相同业务 handler、repository 和产品事务。

| 数据 | 位置与可达性 |
| --- | --- |
| task/payload/progress/lease、文件/配额/版本、发布 checkpoint | 权威数据库；不存文件 body |
| blob、归档/缩略图等已发布产物 | 所有角色可访问的 connector；local policy 必须是 operator 提供的共享文件系统 |
| upload staging / chunk、头像 | 沿用 cluster 共享路径要求；不能放在互不相通的 Pod 私有目录 |
| task scratch | 本次 token 专用临时目录；可为 Pod 私有，接管后从持久输入重建 |
| 配置与 storage SSE | Redis 通知 + 权威配置重读/重连；进程内 Notify 仅优化延迟 |
| reverse tunnel | API/all 接受 follower 连接；worker 经数据库 owner discovery 和认证流代理访问 owner |

API 接受异步任务前持久化输入。普通消费者不持有 scheduler 全局租约：每条 lane 按本进程空槽认领，
事务内锁权威额度行、选择候选并更新 processing token。空队列自适应轮询不超过配置的 idle max
（当前配置校验上限164秒）；跨进程不依赖 Notify。worker 为0时任务保持排队，API 同步操作继续可用；
原有任务创建/准入策略不变，没有额外的无限内存队列。

处理是至少一次语义。旧 token 不能覆盖进度、checkpoint 或终态。
归档压缩、解包与离线下载把发布记录与文件/目录、配额、版本、活动变更放在同一事务中；
重试复用已发布结果，不重复收费或创建第二份逻辑文件。失败与取消均保留已提交的结果。
这不承诺对任意外部副作用 exactly-once，也不保证已删除的未处理输入可以重建。

## 协作取消

任务中心确认后调用个人 `POST /api/v1/tasks/{id}/cancel` 或团队
`POST /api/v1/teams/{team_id}/tasks/{id}/cancel`，沿用 workspace 权限。
`can_cancel` 是后端能力字段，系统维护任务不提供此入口。

- Pending/Retry 直接变为 Canceled，不执行 handler。
- Processing 先变为 Canceling，UI 显示“取消中”并继续轮询；worker 在安全边界确认 Canceled。
- Canceling/Canceled 重复取消幂等；Succeeded/Failed 返回409。
- worker 崩溃后，替代者在租约过期后接管 Canceling，只确认取消，不重新执行业务。
- 取消停止后续工作，保留已提交文件和变更，不自动回滚；Canceling 不可重试，确认 Canceled 后可手动重试同记录并复用 checkpoint。

正常 lease TTL 为60秒、heartbeat为10秒；强杀的接管窗口为 lease 剩余时间加轮询/数据库恢复耗时。
阻塞的外部调用可能延迟协作停止，不能在发出取消请求时显示“已经停止”。

## 并发与探针

原有 `background_task_max_concurrency`、`background_task_archive_max_concurrency`、
`background_task_thumbnail_max_concurrency`、`offline_download_max_concurrency`、
`background_task_storage_migration_max_concurrency` 现在分别表示每个 worker 的对应 lane 上限。
共享额度通过系统设置 `task_fallback_cluster_concurrency`、`task_archive_cluster_concurrency`、
`task_thumbnail_cluster_concurrency`、`task_offline_download_cluster_concurrency`、
`task_storage_migration_cluster_concurrency` 控制，0表示不设集群总额度，最大65536。
claim 事务重读数据库权威值，陈旧的进程内 snapshot 不得绕过额度；降低额度不强杀已运行任务。

`/health` 与 `/health/live` 反映本地进程活性，暂时数据库/Redis 故障不直接触发 liveness 重启。
`/health/ready` 需要初始化和关键依赖健康，worker 额外检查消费者和 scheduler 活性；
空队列或健康 standby scheduler 不要求持有领导租约。关闭后立即停止 claim、退出 readiness，
默认 drain20秒，再协作取消5秒，最后关闭 audit/DB。外层任务组另留5秒退出余量。
静态 `[worker]` 的 `drain_timeout_secs` / `cancellation_timeout_secs` 可调整，总和不得超过24小时。
Pod grace 必须覆盖 preStop、业务 HTTP drain、worker budget 与审计收尾。

metrics 构建并启用 Prometheus 后，`/health/metrics` 提供队列深度、最老等待时间
`aster_task_pending_oldest_age_seconds`、执行并发、retry/failure 和
`aster_task_lease_takeovers_total`。资源 role 区分 all/api/worker/follower；不以 runtime ID 作为无界指标标签。

## 部署与强制迁移

Helm 默认 `split.enabled=false`。开启后独立配置 `split.api` / `split.worker` 的
`replicaCount`、`resources`、调度、PDB、grace 和 `autoscaling`；业务 Service 只选择 API。
worker 可缩到0（HPA 关闭）；HPA 启用后由 HPA 管理 replicas。
Kustomize 使用 `deploy/kubernetes/overlays/split`，两角色保持 StatefulSet 的稳定内部地址。

本次是统一版本迁移，不提供旧 Forge API 或混合版本 task payload/schema 兼容层：

1. 备份数据库和存储，停止旧 worker/all 的新认领并完成 drain；停止旧业务写入。
2. 所有角色更新到同一发行版本、密钥、cluster 配置和共享路径。保留原队列和输入，不清空任务表。
3. 首个新进程启动执行受协调迁移（PostgreSQL advisory lock、MySQL named lock、SQLite 单 writer）；
   新增 `task_resource_publications`，跟随 task history 清理。其他进程等待同一迁移锁，初始化成功才 Ready。
4. 先验证 API 同步操作及排队状态，再启动 worker，检查处理结果、配额、版本、审计及 SSE。
5. 需要切回完整形态时，drain worker 后以**同一新版本** `serve all` 启动，并调整业务 Service。
   若要降回旧发行版，需停机并恢复匹配的数据库/存储备份，不能直接启动旧程序处理新状态。

角色可分别扩缩，不要求重启另一角色。cluster profile 的存储可达性检查继续生效；
相同路径但不同私有卷不构成共享存储，单 SQLite/memory cache 不支持跨 Pod 分离拓扑。
