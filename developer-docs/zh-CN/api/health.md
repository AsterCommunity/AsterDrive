# 健康检查 API

健康检查路径不在 `/api/v1` 下，而是直接挂在根路径。

这组接口在 `primary` 和 `follower` 两种节点模式下都会注册。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` / `HEAD` | `/health` | 存活检查 |
| `GET` / `HEAD` | `/health/ready` | 就绪检查，包含数据库、setup 状态和节点模式对应的轻量前置条件 |
| `GET` | `/health/memory` | 堆内存统计，仅 `debug_assertions + openapi feature` 构建注册 |
| `GET` | `/health/metrics` | Prometheus 指标，需要 `metrics` feature 和显式静态启用 |

## `GET /health`

典型响应：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "status": "ok",
    "version": "0.0.0",
    "build_time": "2026-03-22T00:00:00Z"
  }
}
```

`build_time` 来自编译期传入的 `ASTER_BUILD_TIME` 环境变量。构建时未设置该变量，则由
构建脚本回退为当前 UTC 时间。

`HEAD /health` 语义相同，只是不返回响应体。

## `GET /health/ready`

这条接口不是只看数据库。所有 deployment profile 都会检查数据库和实际 cache backend。`single` profile 在配置的远端 cache 启动失败并回退到健康的 memory cache 时保持 ready，同时在诊断健康报告中标记为 degraded；要求共享运行时状态的 profile 则要求配置和实际 backend 都是 Redis。随后按节点模式继续：

- `primary`：检查动态 cluster 拓扑和权威 setup 状态。`needs_admin` / `needs_storage` 直接以 `200` 返回；只有进入 `ready` 后才要求默认存储策略存在、driver 可实例化，并执行该 driver 的轻量 readiness 检查
- `follower`：检查 follower 当前的存储驱动和绑定所需状态

`/health/ready` 是高频探针路径，不会对 S3 / remote 等远端存储执行写入、读取或删除对象的网络探测。需要验证 S3 凭证、bucket 权限和远端对象写删能力时，使用管理端存储策略的“测试连接”接口。

`primary` 成功响应里的 `data.status` 有三种值：

- `needs_admin`：`200`，基础依赖健康，但还没有首个管理员
- `needs_storage`：`200`，管理员已存在，但默认存储策略组或管理员绑定尚未完成
- `ready`：`200`，产品初始化完成，默认 driver 的轻量 readiness 检查也已通过

错误响应：

- 数据库不可用：`503`，消息是 `Database unavailable`
- 实际 cache 不健康：`503`，消息是 `Cache unavailable`；cluster 中实际 backend 与配置不一致也返回 `503`，并要求配置和实际 backend 都是 Redis
- cluster 拓扑、默认策略、driver 构造或轻量 readiness 检查失败：`503`，消息是 `Storage unavailable`

部署建议：

- 用 `/health` 做 liveness / 基础探活
- 用 `/health/ready` 做 Kubernetes readiness；最终上线验收还要确认 `data.status = "ready"`

## `GET /health/memory`

只有 `debug_assertions + openapi feature` 构建会注册这个接口。

返回当前堆分配量与峰值，单位是 MB 字符串。

## `GET /health/metrics`

需要编译 `metrics` feature 并显式设置 `[observability.prometheus].enabled = true` 后才注册，默认关闭。输出格式为 Prometheus text exposition。

`metrics` feature 不在默认 feature 里。需要 Prometheus 指标时按需编译：

```bash
cargo build --release --features metrics
```

或者使用包含 `metrics` 的完整构建：

```bash
cargo build --release --features full
```

静态 Basic Auth 位于 `[observability.prometheus.basic_auth]`；配置、hash 生成、环境覆盖与 Prometheus 抓取示例见[监控文档](https://drive.docs.astercosm.com/ops/monitoring/)。primary/follower 使用同一规则，health/readiness 不受保护。错误凭据、重复/畸形 Authorization 返回统一 401 + Basic challenge，认证先于导出/503；响应均 no-store。无 `metrics` 或 OTLP-only 构建不注册此端点。编译能力与部署启用分别控制，不能因 push 能力出现匿名 pull 入口。

### 当前指标

HTTP 和数据库：

| 指标 | 标签 | 说明 |
| --- | --- | --- |
| `http_requests_total` | `method`, `route`, `status` | HTTP 请求总数，`route` 使用 Actix route pattern 或低基数 fallback |
| `http_request_duration_seconds` | `method`, `route`, `status` | HTTP 请求耗时直方图 |
| `db_queries_total` | `backend`, `kind`, `status` | SeaORM 查询总数，`kind` 按 SQL 首词粗分类 |
| `db_query_duration_seconds` | `backend`, `kind`, `status` | SeaORM 查询耗时直方图 |

认证、上传、下载和后台任务：

| 指标 | 标签 | 说明 |
| --- | --- | --- |
| `auth_events_total` | `action`, `status`, `reason` | 登录和 refresh token 事件，包含成功与主要失败原因 |
| `file_uploads_total` | `mode`, `status` | 文件上传结果，覆盖 stream / chunked / presigned / presigned multipart / provider resumable 等 transport |
| `file_downloads_total` | `source`, `outcome`, `range` | 文件下载结果，区分登录下载、公开分享、直链、预览链接和 share stream |
| `upload_sessions_total` | `mode` | 创建的上传 session 数量 |
| `upload_session_events_total` | `mode`, `event`, `status` | 上传 session 生命周期事件，例如 complete / cancel |
| `background_tasks_total` | `kind`, `status` | 后台任务状态转换统计 |
| `background_task_retries_total` | `kind` | 后台任务 retry 次数 |
| `background_tasks_pending` | 无 | 当前 `Pending` / `Retry` 后台任务积压量 |

存储驱动和公开分享回滚：

| 指标 | 标签 | 说明 |
| --- | --- | --- |
| `storage_driver_operations_total` | `driver`, `operation`, `status`, `kind` | 存储驱动操作次数，失败时 `kind` 来自存储错误分类 |
| `storage_driver_operation_duration_seconds` | `driver`, `operation`, `status`, `kind` | 存储驱动操作耗时直方图 |
| `share_download_rollback_events_total` | `event` | 公开分享流式下载中断后的下载计数回滚队列事件 |
| `share_download_rollback_pending` | 无 | 待处理分享下载计数回滚量 |

进程指标：

| 指标 | 标签 | 说明 |
| --- | --- | --- |
| `process_memory_rss_bytes` | 无 | 当前进程 RSS 内存，单位 bytes |
| `process_cpu_milliseconds_total` | 无 | 当前进程累计 CPU time，单位 milliseconds |
| `process_uptime_seconds` | 无 | 当前进程从 metrics 初始化开始计算的运行时长，单位 seconds |

`process_cpu_milliseconds_total` 直接暴露毫秒值，查询时不需要再从 seconds 手动换算。`process_uptime_seconds` 不使用 Unix epoch，而是基于本进程启动后的单调时间更新。

### 新增业务指标与统计口径

基线为 `c5989c2`。既有 HTTP/DB/auth/task/system、upload/download attempts、capacity admissions/probe cache、stream upload、driver、avatar 等指标继续复用，完整旧产品 schema 由 `crates/aster_drive_metrics/tests/fixtures/pre_instance_schema.json` 约束。新增项在同一实例级 metric set 中注册一次，业务层不依赖 Prometheus/OTLP 类型。

| 指标 | 类型 / 单位 | 标签 / 采集位置 |
| --- | --- | --- |
| `file_transfer_bytes_total` | Counter / bytes | `direction,source,mode`；HTTP body 读取/输出流，约每 1 MiB 批量提交，终止时提交尾数 |
| `file_transfers_in_flight` | Gauge / count | 同上；RAII 所有退出路径归还 |
| `file_transfer_duration_seconds` | Histogram / seconds | 同上加 `outcome`；持续到 EOF、错误或 drop，只有一个终止结果 |
| `upload_phase_duration_seconds` | Histogram / seconds | `mode,phase,status`；ingest、assemble、finalize、complete；completed replay 不重复记录 |
| `upload_sessions_active` | Gauge / count | `mode,state`；未完成持久会话，包括 failed/TTL expired |
| `upload_session_oldest_age_seconds` | Gauge / seconds | 同上；从创建时间算，expired 表示 TTL 已过，不直接断言可强制清理 |
| `upload_admission_rejections_total` | Counter / count | `reason=quota/target_capacity/staging_capacity/capacity_unavailable/concurrency`；最终准入拒绝一次，非每候选目标一次 |
| `storage_capacity_bytes` | Gauge / bytes | `scope,kind`；staging 文件系统或最近已观察目标的独立最小值 |
| `storage_capacity_known` | Gauge / boolean | 同上；未知、不支持或过期必须过滤，不能当 0 容量 |
| `upload_staging_bytes` | Gauge / bytes | `kind=declared_reservations/staged_reservations/safety_floor`；前两者为持久会话声明量，不伪装成目录实际占用 |
| `blob_cleanup_pending` | Gauge / count | stored backing 的 ref_count<=0 行，包含处理中 claim；排除 virtual-empty |
| `blob_cleanup_oldest_age_seconds` | Gauge / seconds | 独立 `cleanup_pending_since`，归零时开始、重试保留、重新引用时清空 |
| `blob_cleanup_objects_total` | Counter / count | `outcome`；只记录实际 claim 的 cleanup 尝试，非重复查看已删除行 |
| `blob_cleanup_retired_backing_bytes_total` | Counter / bytes | 物理删除确认且 metadata 移除后的 backing 记账字节；不等于磁盘块实际释放，失败/重复清理不重复累计 |
| `remote_nodes` | Gauge / count | `status=disabled/unknown/probe_failed/probe_ok`；沿用最近 probe 事实，非 scrape 时探测 |
| `remote_tunnel_connections` | Gauge / count | 本节点已建立的 streaming lanes；关闭、drop 释放；poll-only 连接不冒充持久 socket |
| `remote_tunnel_reconnects_total` | Counter / count | `reason`；follower 连接循环的重连/错误重试 |
| `remote_requests_total` | Counter / count | `transport,operation,status,reason`；primary 出站 wire 与 follower reverse forwarding，不能和 driver operations 相加 |
| `remote_request_duration_seconds` | Histogram / seconds | `transport,operation,status`；本节点观察到的 wire 请求周期，文件 body 另看 transfer 生命周期 |
| `observability_snapshot_valid` | Gauge / boolean | `source=database/staging_filesystem`；最近一次采样是否成功 |
| `observability_snapshot_last_success_timestamp_seconds` | Gauge / Unix seconds | 同上；不因失败刷新成功时间 |

`direction` 是本节点 body 的 ingress/egress；`source=file/share/webdav/wopi/remote`。`mode=stream/chunked/multipart/resumable/range/presigned` 只表达实际边界。primary 的 presigned/redirect 不经过数据面，不记录实际字节；若 remote presigned 由 AD follower 真正承载，follower 按 `source=remote` 记录。不要跨 primary 用户边界和 follower remote I/O 重复求和。计数是应用 body 字节，不含 TLS/HTTP 帧开销；失败前已读取/输出的字节仍计入。`cancelled` 代表未到 EOF 就被丢弃，不能单凭它判定是客户端还是下游主动取消。

新 duration buckets 显式覆盖毫秒到分钟/小时；既有 histogram buckets 不变。没有新增用户、文件、对象 key、URL 或凭据标签。既有 topology labels 受 provider 系列预算约束。

Primary 每 60 秒执行受控 DB 聚合，至多等待 5 秒；follower 不查询 primary 业务积压。staging 容量在 blocking worker 采样；目标容量读取既有 admission probe cache 的近 60 秒观察，不额外远程探测。scrape 仅导出内存快照。成功样本清零已消失的有限分组；失败通过 valid/last-success 标识，dashboard 必须过滤失败或过期样本。

`observed_target_minimum` 的 available/total/used 分别取最小值，可能来自不同目标，不能彼此求比率，也不能与 staging filesystem 相加。它不是所有策略的容量总量；共享盘/桶不重复累计。`staging_filesystem` 是整个文件系统使用量，可能包含其他数据，不是 staging 目录大小。

待清理时间在所有 feature 组合中与引用计数同一 SQL 写入；MySQL 的 timestamp SET 排在 ref_count SET 之前，避免左到右赋值重复应用减量。升级前已存在的 pending 行以当时 updated_at 回填，不能恢复丢失的历史首次归零时间；升级后重试不再重置年龄。migration 只增加观测列/索引，不改变 ref-count、quota、版本和删除事务规则。

workspace 聚合配额指标和独立 WebDAV/WOPI operation 指标仍属于 #636 的 P1；本轮交付 P0 数据面和生命周期。tracing/Tempo 留在 #638 后续批次。

### PromQL 示例

用户边界的每秒 body bytes：

```prometheus
sum by (direction) (rate(file_transfer_bytes_total{source!="remote"}[5m]))
```

传输失败率（取消单独统计，不伪装成读写失败）：

```prometheus
sum(rate(file_transfer_duration_seconds_count{source!="remote",outcome="failure"}[5m]))
/
clamp_min(sum(rate(file_transfer_duration_seconds_count{source!="remote"}[5m])), 1e-9)
```

上传阶段 p95：

```prometheus
histogram_quantile(0.95, sum by (le,phase,mode) (rate(upload_phase_duration_seconds_bucket{status="success"}[5m])))
```

每个实例的最老上传/清理积压；使用同样有效性条件过滤两种 gauge：

```prometheus
max by (job,instance) (upload_session_oldest_age_seconds)
and on (job,instance) (observability_snapshot_valid{source="database"} == 1)
and on (job,instance) (time() - observability_snapshot_last_success_timestamp_seconds{source="database"} < 120)
```

```prometheus
blob_cleanup_oldest_age_seconds
and on (job,instance) (observability_snapshot_valid{source="database"} == 1)
and on (job,instance) (time() - observability_snapshot_last_success_timestamp_seconds{source="database"} < 120)
```

受支持的 staging 盘可用量与隧道异常：

```prometheus
storage_capacity_bytes{scope="staging_filesystem",kind="available"}
and on (job,instance,scope,kind) (storage_capacity_known == 1)
and on (job,instance) (observability_snapshot_valid{source="staging_filesystem"} == 1)
```

```prometheus
sum by (job,instance,reason) (rate(remote_tunnel_reconnects_total[5m]))
```

```prometheus
sum by (job,instance,transport,reason) (rate(remote_requests_total{status="failure"}[5m]))
```
