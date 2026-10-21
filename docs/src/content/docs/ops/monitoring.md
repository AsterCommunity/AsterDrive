---
description: AsterDrive Prometheus 指标、Grafana dashboard、Docker 本地观测栈和上线监控注意事项。
title: "监控与 Grafana"
---

:::tip[这一篇覆盖什么]
这一页讲 AsterDrive 自带 Prometheus 指标怎么打开、怎么被 Prometheus 采集，以及如何导入官方 Grafana dashboard。  
如果你只是上线前做最后检查，也建议重点阅读“上线安全边界”部分，确认 metrics 入口没有直接暴露到公网。
:::

AsterDrive 的 Prometheus 指标是可选能力，默认构建不启用。启用后，服务会在健康检查路径下注册：

```text
GET /health/metrics
```

输出是 Prometheus text exposition，覆盖 HTTP、数据库、上传下载、后台任务、存储驱动、认证事件、分享下载回滚和进程资源。

## 当前能力与证据

本页是 Drive 观测能力与启用方式的消费者事实源；[Forge 消费者索引](https://github.com/AsterCommunity/AsterForge/blob/master/docs/guide/reference-projects.md)只记录公共机制和版本证据。以下核对日期为 **2026-10-22**，基于已合并代码；后续计划不代表当前发行包已经提供。

| 信号 | 当前支持 | 启用与限制 |
| --- | --- | --- |
| 本地 text / JSON logs | 已接入 Forge logging | `[logging]`；stdout 或文件；`RUST_LOG` 优先；配置见[日志](/reference/config/logging/) |
| Prometheus metrics | 已接入实例级 Forge provider | `metrics` feature + `observability.prometheus.enabled`；primary/follower 均可挂载；默认关闭 |
| OTLP metrics | 已接入，与 Prometheus 可独立或同时启用 | `otlp-metrics` feature + `observability.otlp_metrics.enabled`；HTTP/protobuf；默认关闭 |
| OTLP traces / 跨节点 trace / trace-log 关联 | 未支持 | Forge #45 / Drive #638；已有 `tracing` span 和 request_id 不代表 OTel trace 导出 |
| OTLP logs | 未支持 | 本地 JSON logs 不等于 OTLP logs exporter |
| Jemalloc heap profiling / allocator 统计 | 有独立可选能力 | 平台与 allocator 限制见[堆画像](https://drive.docs.astercosm.com/developer/testing/jemalloc-profiling/)；不等于 continuous profiling 平台导出 |
| Continuous profiling 平台导出 | 未支持 | 不提供看似生效的配置开关 |

### 固定版本与接入点

| 项目 | 可核查证据 |
| --- | --- |
| Drive revision | [`aa1f4257bc805762726c4debe6f2ca7d235ffecf`](https://github.com/AsterCommunity/AsterDrive/commit/aa1f4257bc805762726c4debe6f2ca7d235ffecf) |
| Forge resolved revision | [`59e62915d652503ff2a20efd504774665af59c6f`](https://github.com/AsterCommunity/AsterForge/commit/59e62915d652503ff2a20efd504774665af59c6f)；[Cargo.lock](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/Cargo.lock) 与 [Cargo.toml](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/Cargo.toml) 固定 Git 依赖，无本地 path/patch |
| 编译能力 | [产品 metrics crate](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/crates/aster_drive_metrics/Cargo.toml)：`metrics` 接入 allocator/runtime-health，`prometheus` / `otlp` 分别转发 exporter；根 feature 见上面的 Cargo.toml |
| 配置与校验 | [ObservabilityConfig](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/config/metrics.rs)、[配置示例](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/config.example.toml)；`ASTER__` 覆盖、重启生效、未知字段与未编译能力拒绝 |
| 产品 adapter / 采样 | [typed metric set 与 recorder](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/crates/aster_drive_metrics/src/lib.rs)、[后台任务](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/runtime/tasks.rs)；业务 SQL、容量采样、标签含义属于 Drive |
| HTTP / 角色注入 | [health routes](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/api/routes/health.rs)、[认证 middleware](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/api/middleware/metrics_auth.rs)、[primary/follower components](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/runtime/components.rs) |
| 生命周期 | [runtime assembly](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/src/runtime/assembly.rs)；runtime producers 与 audit recovery 完成后 shutdown metrics，保留原始 runtime 错误；日志 guard 在 main 最后释放 |
| Forge 公共合同 | [metrics](https://github.com/AsterCommunity/AsterForge/blob/59e62915d652503ff2a20efd504774665af59c6f/docs/crates/aster_forge_metrics.md)、[observability](https://github.com/AsterCommunity/AsterForge/blob/59e62915d652503ff2a20efd504774665af59c6f/docs/crates/aster_forge_observability.md)、[logging](https://github.com/AsterCommunity/AsterForge/blob/59e62915d652503ff2a20efd504774665af59c6f/docs/crates/aster_forge_logging.md) |

### 指标 schema 与兼容边界

完整声明分别由 [Forge 通用 schema](https://github.com/AsterCommunity/AsterForge/blob/59e62915d652503ff2a20efd504774665af59c6f/crates/aster_forge_metrics/src/lib.rs)及其 [infrastructure recorder](https://github.com/AsterCommunity/AsterForge/blob/59e62915d652503ff2a20efd504774665af59c6f/crates/aster_forge_metrics/src/shared.rs)、[Drive 产品 metric set](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/crates/aster_drive_metrics/src/lib.rs)拥有，涵盖通用 HTTP/DB/auth/task/config/system/health 和产品传输、上传阶段/积压、容量、清理、远端链路、avatar 等。描述包含 kind、HELP、unit、labels、buckets；不维护另一份手工指标清单。

[旧产品 schema fixture](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/crates/aster_drive_metrics/tests/fixtures/pre_instance_schema.json)只是迁移前兼容子集，不是当前全量 schema。`migration_preserves_the_entire_pre_instance_product_schema` 比较旧名称、类型、HELP、标签和 buckets；[Grafana 合同测试](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/crates/aster_drive_metrics/tests/grafana_dashboard.rs)检查 dashboard。旧 API 到 provider/instruments 的替换见 Forge metrics 合同的“API 迁移”。已交付迁移没有计划内重命名；任何新增改名、单位、标签或 bucket 变化都必须单独列迁移说明及测试，不能借更换 exporter 静默改变。

`process_cpu_milliseconds_total` 保留现有毫秒口径，不能按 seconds 直接解释。产品字节指标与逻辑文件大小、内部 driver/remote I/O 不混加；`blob_cleanup_retired_backing_bytes_total` 是确认删除 backing 的记账字节，不保证磁盘实际释放量。容量 minima 各维度独立，不相加、不互相比率；无支持或采样失败时结合 validity/last-success 判断，不能把未知当零。

HTTP histogram 衡量 handler/header 阶段，文件 transfer histogram 覆盖实际流到 EOF/error/drop，两者不能互换。presigned/redirect 不经过 AD 的数据面不可见；采样 trace 也不能替代未采样的吞吐 counter。独立 worker 角色尚未交付，不从 primary/follower 验证推断其可用。

### 验证、发行与维护

- **历史实现证据**：[PR #642](https://github.com/AsterCommunity/AsterDrive/pull/642) 于 2026-10-16 合并，使用 Forge `2beaf986f1740629d3fd8913f865e4134d31e203`。其本地测试记录含四种 feature/config 组合、两种角色的端点和认证、schema/Grafana、上传及三库 migration；该 PR 明确未等待新 CI，不把它写成 CI 全绿。
- **当前依赖证据**：上表记录的 Forge revision 已更新到 `59e62915d652503ff2a20efd504774665af59c6f`。本次发布准备通过了 `full` 下 965 项 Drive 定向测试、16 项指标/schema/Grafana 测试、PostgreSQL 与 MySQL 观测 migration 往返、PostgreSQL 中断索引恢复，以及 `full` 下 workspace/全 target Clippy。这不代表重跑了 #642 的全部 feature 组合，也不证明 collector 交付或生产 rollout。
- **分发证据**：[镜像 workflow](https://github.com/AsterCommunity/AsterDrive/blob/aa1f4257bc805762726c4debe6f2ca7d235ffecf/.github/workflows/docker-image.yml) 定义 `-metrics` 构建；OTLP 目前按本页源码/自建镜像方式启用。workflow 定义不证明某个 tag 已可下载；本次未核验具体发行包、镜像 digest 或生产 rollout。
- **检查入口**：`bun docs/scripts/check-observability-docs.mjs` 离线检查中英版本链接、固定 Drive 路径与当前 Forge lock revision；`bash scripts/test-metrics-features.sh` 验证当前四组合，配置与认证测试由脚本调用。链接/静态检查不证明运行行为或 collector 送达。
- **更新责任**：改 feature、配置、schema、导出协议或生命周期的 PR 同步本页中英文和 Forge 索引；固定 revision 必须指向已有公开提交，测试、发行和生产状态分别更新。文档构建执行版本检查；不会联网拉取消费者。

已交付指标与抓取认证见 #636/#637（均由 #642 关闭）；剩余 tracing 接入见 [Drive #638](https://github.com/AsterCommunity/AsterDrive/issues/638) / [Forge #45](https://github.com/AsterCommunity/AsterForge/issues/45)。本事实源与索引交付由 [Drive #639](https://github.com/AsterCommunity/AsterDrive/issues/639) / [Forge #46](https://github.com/AsterCommunity/AsterForge/issues/46) 跟踪；Forge #44 继续补库级测试及模板验收，不要求 Drive 重复迁移。

## 启用 metrics feature

从源码构建时使用：

```bash
cargo build --release --features metrics
```

或者使用包含全部可选能力的构建：

```bash
cargo build --release --features full
```

先设置 `[observability.prometheus].enabled = true` 并重启；专用 Basic Auth 配置见下文。

启动后先确认实例能返回指标：

```bash
curl http://<asterdrive-host>:3000/health/metrics
```

如果返回 404，检查二进制的 `metrics` feature 和静态 `[observability.prometheus].enabled` 是否都已开启。
如果返回 HTML 或代理错误，先查反向代理路径和后端端口。

## Prometheus scrape 配置

Prometheus 与 AsterDrive 在同一台主机或同一网络内时，可以这样采集：

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - 127.0.0.1:3000
```

如果 Prometheus 跑在 Docker 里，而 AsterDrive 跑在宿主机上，Docker Desktop 下通常使用：

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - host.docker.internal:3000
```

Linux Docker 如果没有 `host.docker.internal`，给 Prometheus 容器加：

```yaml
extra_hosts:
  - host.docker.internal:host-gateway
```

然后仍然使用 `host.docker.internal:3000` 作为 target。

## Docker Compose 示例

下面是最小 Prometheus + Grafana compose 示例，适用于 Prometheus / Grafana 以 Docker 运行、AsterDrive 在宿主机监听 `3000` 端口的场景。生产部署时请按实际网络、端口、持久化目录和访问控制调整。

```yaml
services:
  prometheus:
    image: prom/prometheus:v3.7.3 # 示例固定到已验证版本；升级前请按你的部署窗口验证配置兼容性。
    command:
      - --config.file=/etc/prometheus/prometheus.yml
      - --storage.tsdb.path=/prometheus
      - --web.enable-lifecycle
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    extra_hosts:
      - host.docker.internal:host-gateway

  grafana:
    image: grafana/grafana:12.3.0 # 示例固定到已验证版本；升级前请按你的部署窗口验证配置兼容性。
    ports:
      - "3300:3000"
    environment:
      - GF_SECURITY_ADMIN_USER=admin
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana-data:/var/lib/grafana
    depends_on:
      - prometheus

volumes:
  prometheus-data:
  grafana-data:
```

`prometheus.yml`：

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: asterdrive
    metrics_path: /health/metrics
    static_configs:
      - targets:
          - host.docker.internal:3000
```

示例启动后访问：

- Prometheus：`http://localhost:9090`
- Grafana：`http://localhost:3300`

Grafana 示例账号是 `admin` / `admin`。生产环境应改成独立强密码，并限制 Grafana 管理入口的访问来源。

## Grafana dashboard

AsterDrive 文档站提供可直接导入的 Grafana dashboard JSON。官方文档站和自托管文档站都会在站点根路径下分发这个文件：

```text
/grafana/asterdrive-overview.json
```

在线文档站完整地址：

```text
https://drive.docs.astercosm.com/grafana/asterdrive-overview.json
```

导入方式：

1. 打开 Grafana
2. 进入 `Dashboards -> New -> Import`
3. 粘贴上面的 JSON URL，或者下载 JSON 后上传
4. 选择你的 Prometheus datasource
5. 导入 `AsterDrive Overview`

这个 dashboard 会提供：

- 总览：目标存活、进程 uptime、RSS、CPU、HTTP RPS、5xx 比例
- HTTP：按 route/status 的请求量、p95/p99 延迟、错误比例、慢路由
- 数据库：SeaORM 查询量、错误量、p95 和平均耗时
- 存储驱动：操作量、not_found、硬失败、p95 和平均耗时
- 传输：上传、下载、上传 session 生命周期、传输失败
- 后台任务：积压量、状态转换、retry、分享下载回滚队列
- 认证：登录和 refresh token 事件、认证失败比例
- 进程：RSS、CPU core 用量、uptime

dashboard 的查询基于 AsterDrive 当前指标标签：

- `job`
- `instance`
- `method`
- `route`
- `status`
- `backend`
- `kind`
- `driver`
- `operation`
- `mode`
- `source`
- `outcome`
- `range`
- `action`
- `reason`
- `event`

其中 `job` 和 `instance` 来自 Prometheus scrape target，不是 AsterDrive 自己写入的业务标签。

## 显式开启与抓取认证

观测默认关闭。Cargo feature 提供能力，静态配置决定是否运行；修改配置后重启。旧版仅编译 `metrics` 就暴露端点的默认行为已改为显式启用。静态配置现在拒绝未知字段；升级时删除废弃配置段，将未发布的 `[metrics.basic_auth]` 改为 `[observability.prometheus.basic_auth]`，防止认证拼错后静默失效。错误保留安全字段名或解析字节位置，不输出配置值。

```toml
[observability.prometheus]
enabled = true

[observability.prometheus.basic_auth]
enabled = true
username = "prometheus"
password_hash = '$argon2id$v=19$...' # 替换为完整 verifier
```

用 CLI 从 stdin 生成 verifier，不经数据库、日志初始化或命令行密码参数：

```bash
# Bash；输入不会回显或进入命令历史
read -r -s -p 'Metrics password: ' METRICS_PASSWORD
printf '%s' "$METRICS_PASSWORD" | ./aster_drive hash-password
unset METRICS_PASSWORD
```

支持 Argon2id v19，验证上限为 64 MiB、3 passes、4 lanes、32-byte output。username 不允许冒号或控制字符；空/畸形/超预算 hash 启动拒绝，不生成替代密码，也不回退匿名。环境覆盖示例：`ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__PASSWORD_HASH`；PHC 值用 shell 单引号保护 `$`。

Prometheus 使用原始密码文件，服务端配置只存 verifier：

```yaml
scrape_configs:
  - job_name: asterdrive
    scheme: https
    metrics_path: /health/metrics
    basic_auth:
      username: prometheus
      password_file: /etc/prometheus/asterdrive-metrics.pass
    static_configs:
      - targets: [drive.example.com]
```

通过 HTTPS 或受信任的 TLS 终止代理传输 Basic Auth。关闭认证后已开启的端点允许匿名抓取；普通 JWT/cookie 不能替代抓取凭据。错误、畸形、重复 Authorization 都是空 body 的 401 + `WWW-Authenticate: Basic realm="AsterDrive metrics"`。认证先于导出与初始化状态检查；正确凭据后仍遵循成功 200、导出不可用 503。指标与认证失败响应均 `Cache-Control: no-store`；health/readiness 不受保护。

密码验证在独立、有界 blocking executor 中执行，最多两个任务；忙时立即返回统一 401，不排队，也不与用户登录共用额度。取消 HTTP 请求不提前释放仍在执行的密码工作额度。 对语法有效的 Basic 凭据，无论用户名是否匹配都执行密码验证，以保持相同的验证路径；错误用户名也消耗这两个额度，密集扫描可能让合法抓取暂时收到 401。

## 可选 OTLP metrics

现有 `-metrics` 镜像提供 Prometheus pull。需要 OTLP 时从源码构建，或使用 `docker build --build-arg CARGO_FEATURES=server,cli,metrics,otlp-metrics -t asterdrive:observability .`；静态 exporter 配置仍默认关闭。

```bash
cargo build --release --features metrics,otlp-metrics
```

```toml
[observability]
environment = "production"

[observability.prometheus]
enabled = true # OTLP-only 时显式保持 false

[observability.otlp_metrics]
enabled = true
endpoint = "https://collector.example.com/v1/metrics"
interval_seconds = 60
timeout_seconds = 5
# headers = { Authorization = "Bearer <collector credential>" }
```

一个实例级指标内核、一次业务记录，分别供两个 reader 导出。只支持 OTLP HTTP/protobuf metrics；这不启用 traces、Tempo 或 OTLP logs。配置不被 `OTEL_*`、代理或 SDK credential 环境变量覆盖；HTTPS 使用系统信任，不跟随重定向。collector 不可达不阻止文件服务启动或 readiness；失败在本地记录，并在下一周期重试累计快照。

OTLP resource 包含 service/version/environment/role，以及每次启动生成、同一次运行保持不变的 `service.instance.id`（复用 config-sync runtime_id）。不需要手填实例 ID。Prometheus 的 `job/instance` 来自 scrape target，业务标签不添加这些身份。

| 构建能力 | 配置 | HTTP 指标端点 |
| --- | --- | --- |
| 默认 / 无 metrics | exporters 均关闭 | 不存在 |
| `metrics` | prometheus=true | 存在，可选 Basic Auth |
| `otlp-metrics` | otlp_metrics=true | 不存在 |
| 两个 feature | 分别开启 | 只有 prometheus=true 才存在 |

启用未编译的能力启动失败；编译了但未配置启用时为 Noop，不启动采样/推送。新增 blob 待清理起始时间作为持久状态始终随引用计数维护，保证以后开启观测时年龄正确，不受 metrics feature 影响；没有因此增加业务 DB 往返。


本地冒烟验证可将 metrics endpoint 指向 `http://127.0.0.1:4318/v1/metrics`，并用以下配置启动 OpenTelemetry Collector。只接收 metrics 并打印诊断，不启用 traces，也不提供持久化；本次文档更新未实际运行 Collector。

```yaml
receivers:
  otlp:
    protocols:
      http:
        endpoint: 0.0.0.0:4318
exporters:
  debug:
    verbosity: basic
service:
  pipelines:
    metrics:
      receivers: [otlp]
      exporters: [debug]
```

## 上线安全边界

按上面的静态配置启用专用 Basic Auth，并按部署需要配合网络访问限制。

生产环境必须做到：

- `/health/metrics` 不直接暴露到公网
- 只允许 Prometheus、Grafana Agent、VictoriaMetrics Agent 等采集端访问
- 反向代理对 `/health/metrics` 单独加来源 IP 限制，或者只在内网监听
- 不要用普通网页登录态保护 metrics；采集端不应该依赖浏览器会话

反向代理里要把普通用户入口和 metrics 入口分开处理。公开站点可以访问 `/health`，但 `/health/metrics` 应该只给监控网络看。

## 观察重点

上线后第一天建议重点看：

- `http_requests_total` 里 5xx 是否持续增长
- `http_request_duration_seconds` 的 p95/p99 是否随时间抬升
- `db_queries_total{status="error"}` 是否非零
- `db_query_duration_seconds` 是否出现异常慢查询分布
- `storage_driver_operations_total{status="failure",kind!="not_found"}` 是否非零
- `background_tasks_pending` 是否持续堆积
- `background_task_retries_total` 是否持续增长
- `share_download_rollback_pending` 是否持续堆积
- `process_memory_rss_bytes` 是否单向增长且空闲后不回落

`storage_driver_operations_total{status="failure",kind="not_found"}` 建议单独观察。缩略图、缓存或对象探测场景里，`not_found` 常常只是预期的 miss。告警规则更适合优先关注 `storage_driver_operations_total{status="failure",kind!="not_found"}` 这类硬失败。

## 压测期间的指标口径

跑 `tests/performance/k6` 里的下载、Range 下载、WebDAV GET、WebDAV Range GET、WebDAV `PROPFIND Depth: 1` 时，建议同时抓取 `/health/metrics`。k6 负责客户端 p95/p99、吞吐和错误率；Prometheus 指标负责解释瓶颈落在哪里。

重点对照：

- `http_request_duration_seconds`：确认慢的是 REST 下载、WebDAV GET 还是 PROPFIND。
- `db_query_duration_seconds` / `db_queries_total{status="error"}`：确认大目录枚举和权限快照有没有打爆数据库。
- `storage_driver_operations_total` 与 storage driver latency 指标：区分对象存储限流、range read 延迟和普通 not_found miss。
- `process_memory_rss_bytes`：长时间并发下载和大目录 PROPFIND 下确认 RSS 是否持续抬升。
- remote follower 场景：同时采 primary 和 follower，单看一端很容易误判 tunnel 或上游对象存储瓶颈。
