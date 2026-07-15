# 健康检查 API

健康检查路径不在 `/api/v1` 下，而是直接挂在根路径。

这组接口在 `primary` 和 `follower` 两种节点模式下都会注册。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` / `HEAD` | `/health` | 存活检查 |
| `GET` / `HEAD` | `/health/ready` | 就绪检查，包含数据库和存储可用性 |
| `GET` | `/health/memory` | 堆内存统计，仅 `debug_assertions + openapi feature` 构建注册 |
| `GET` | `/health/metrics` | Prometheus 指标，仅 `metrics` feature 启用时存在 |

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

这条接口不是只看数据库。当前逻辑会先 `ping` 数据库，再做节点模式对应的轻量存储就绪检查：

- `primary`：检查主节点默认存储策略存在、驱动可实例化，以及本地存储目录这类低成本前置条件
- `follower`：检查 follower 当前的存储驱动和绑定所需状态

`/health/ready` 是高频探针路径，不会对 S3 / remote 等远端存储执行写入、读取或删除对象的网络探测。需要验证 S3 凭证、bucket 权限和远端对象写删能力时，使用管理端存储策略的“测试连接”接口。

返回语义：

- 全部就绪：`200`
- 数据库不可用：`503`，消息是 `Database unavailable`
- 存储不可用：`503`，消息是 `Storage unavailable`

部署建议：

- 用 `/health` 做 liveness / 基础探活
- 用 `/health/ready` 做 readiness / 上线前探针

## `GET /health/memory`

只有 `debug_assertions + openapi feature` 构建会注册这个接口。

返回当前堆分配量与峰值，单位是 MB 字符串。

## `GET /health/metrics`

只有在编译时启用了 `metrics` feature 才会注册，输出格式为 Prometheus text exposition。

`metrics` feature 不在默认 feature 里。需要 Prometheus 指标时按需编译：

```bash
cargo build --release --features metrics
```

或者使用包含 `metrics` 的完整构建：

```bash
cargo build --release --features full
```

当前不会在应用层给 `/health/metrics` 做鉴权。生产环境必须通过反向代理、防火墙、安全组或内网监听限制访问，只允许 Prometheus / Grafana Agent / VictoriaMetrics Agent 等采集端访问，不要把这个接口无差别暴露到公网。

反向代理建议：

- 独立匹配 `/health/metrics`，只允许监控系统来源 IP
- 或者只在内网域名 / 内网监听端口暴露
- 不要复用普通用户登录态保护这个接口，Prometheus scrape 应保持简单、稳定、可自动化

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
| `file_uploads_total` | `mode`, `status` | 文件上传结果，覆盖 direct / chunked / presigned / presigned multipart 等模式 |
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
