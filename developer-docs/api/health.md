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
  "code": 0,
  "msg": "",
  "data": {
    "status": "ok",
    "version": "0.0.0",
    "build_time": "2026-03-22T00:00:00Z"
  }
}
```

`build_time` 来自编译期写入的 `ASTER_BUILD_TIME`。

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

这个接口更适合 Prometheus 等监控系统抓取，不建议直接暴露给公网。
