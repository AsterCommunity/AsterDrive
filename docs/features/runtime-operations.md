---
description: AsterDrive 系统与运维功能地图，覆盖启动配置、运行时系统设置、后台任务、邮件、监控、审计、CLI、备份、升级和故障排查。
---

# 系统与运维

系统与运维负责让服务稳定运行、可配置、可观察、可恢复。它横跨启动配置、数据库运行时配置、后台任务、健康检查、日志、邮件、审计和 CLI。

## 能力边界

| 能力 | 说明 | 相关文档 |
| --- | --- | --- |
| 启动配置 | 监听地址、数据库、日志、WebDAV 前缀、节点模式、限流和缓存 | [配置总览](/config/)、[服务器](/config/server)、[数据库](/config/database) |
| 运行时系统设置 | 管理后台热更新的站点、注册、邮件、WOPI、回收站、任务保留等配置 | [系统设置](/config/runtime) |
| 后台任务 | 缩略图、归档、迁移、离线下载、清理、周期任务和重试 | [系统设置](/config/runtime)、[运维 CLI](/deployment/ops-cli) |
| 邮件 | SMTP、模版、发件队列和测试邮件 | [邮件](/config/mail) |
| 监控 | 健康检查、ready 检查、Prometheus metrics 和 Grafana dashboard | [监控与 Grafana](/deployment/monitoring) |
| 审计 | 管理操作、团队审计和可筛选的审计日志 | [管理后台](/guide/admin-console) |
| CLI | doctor、离线配置、节点 enroll、跨数据库迁移 | [运维 CLI](/deployment/ops-cli) |
| 备份升级 | 数据库、配置、本地上传目录、版本升级和回滚 | [备份与恢复](/deployment/backup)、[升级与版本迁移](/deployment/upgrade) |

## 后端模块

| 模块 | 负责内容 |
| --- | --- |
| `config::loader`、`config_service` | 静态配置加载和运行时系统设置 |
| `runtime::startup`、`runtime::tasks` | primary/follower 启动和周期后台任务 |
| `task_service` | 用户可见后台任务、调度、重试和清理 |
| `mail::sender`、`mail::outbox`、`mail::template` | 邮件投递和模版 |
| `ops::health`、`readiness_service`、`metrics` | 健康检查、ready 和指标 |
| `audit_service` | 审计日志记录、查询和展示 |
| `cli::*` | 离线运维命令 |

## 配置边界

- 启动前必须知道的东西放 `config.toml` 或 `ASTER__...` 环境变量。
- 管理员运行时能调整的东西放数据库 `system_config`。
- 系统默认配置定义集中在 `src/config/definitions.rs`。
- 新增用户可见后台任务要走 `task_service::create_task_record()` 并唤醒 dispatcher。

## 排障方向

- 服务起不来：先看配置路径、数据库连接、端口占用、目录权限。
- ready 不通过：看迁移、默认策略、运行模式和 follower 绑定状态。
- 后台任务堆积：看任务 dispatcher、任务保留、失败原因和重试次数。
- 管理后台进不去但要改配置：用 [运维 CLI](/deployment/ops-cli) 的 `config` 子命令。
