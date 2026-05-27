---
description: AsterDrive Prometheus 指标、Grafana dashboard、Docker 本地观测栈和上线监控注意事项。
---

# 监控与 Grafana

::: tip 这一篇覆盖什么
这一页讲 AsterDrive 自带 Prometheus 指标怎么打开、怎么被 Prometheus 采集，以及如何导入官方 Grafana dashboard。  
如果你只是上线前做最后检查，也建议重点阅读“上线安全边界”部分，确认 metrics 入口没有直接暴露到公网。
:::

AsterDrive 的 Prometheus 指标是可选能力，默认构建不启用。启用后，服务会在健康检查路径下注册：

```text
GET /health/metrics
```

输出是 Prometheus text exposition，覆盖 HTTP、数据库、上传下载、后台任务、存储驱动、认证事件、分享下载回滚和进程资源。

## 启用 metrics feature

从源码构建时使用：

```bash
cargo build --release --features metrics
```

或者使用包含全部可选能力的构建：

```bash
cargo build --release --features full
```

启动后先确认实例能返回指标：

```bash
curl http://<asterdrive-host>:3000/health/metrics
```

如果返回 404，通常是当前二进制没有启用 `metrics` feature。  
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

AsterDrive 文档站提供可直接导入的 Grafana dashboard JSON。官方文档站和自托管 VitePress 文档站都会在站点根路径下分发这个文件：

```text
/grafana/asterdrive-overview.json
```

在线文档站完整地址：

```text
https://drive.astercosm.com/grafana/asterdrive-overview.json
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

## 上线安全边界

AsterDrive 当前不对 `/health/metrics` 做应用层鉴权。这个选择是为了让 Prometheus scrape 简单稳定，访问控制应该交给网络边界。

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
