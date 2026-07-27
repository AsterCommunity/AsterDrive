---
description: AsterDrive 部署场景选择，按单实例 Docker、单实例 systemd、多实例、Kubernetes、Follower 存储节点和反向代理六条主路径组织，并列出部署前必须确认的数据、访问方式、WebDAV 和存储事项。
title: "部署概览"
---

:::tip[这一页干什么]
先按规模和环境选一条部署场景主路径，再跟着对应场景页从头走到上线验收：

| 场景 | 适合谁 | 主路径 |
| --- | --- | --- |
| 单实例 Docker | NAS、单机、小团队、已有容器环境 | [单实例 Docker 部署](/deploy/docker/) |
| 单实例 systemd / 二进制 | 云主机、物理机、长期稳定运行 | [单实例 systemd 部署](/deploy/systemd/) |
| 多实例与负载均衡 | 多个 Primary、共享数据面 | [负载均衡与多实例](/deploy/multi-instance/) |
| Kubernetes | 容器编排、Ingress、StatefulSet | [Kubernetes 部署](/deploy/kubernetes/) |
| Follower 存储节点 | 把另一台 AsterDrive 接成远程存储节点 | [Docker 部署从节点](/deploy/follower-node/) |
| 反向代理与公网入口 | 任何正式上线的部署 | [反向代理](/deploy/reverse-proxy/) |

还没决定选哪条？先看 [部署方式选择](/start/choose-deployment/)。
:::

AsterDrive 是单服务交付：

- 浏览器页面
- 公开分享页
- 管理后台
- WebDAV
- 文件预览与 WOPI 入口

都由同一个进程提供。  
部署时最重要的事只有三件：

- 让服务稳定运行
- 把数据保存好
- 让上传、WebDAV 和外部打开方式在你的网络环境里可用

## 部署前先确认这四件事

无论走哪条场景路径，这四件事都一样。

### 数据目录

重启或升级后必须保留下来的内容：

- `data/config.toml`
- 数据库
- 本地上传目录

如果你启用了上传头像，或额外配置了其他本地 `local` 存储策略，还要一起保留：

- `avatar_dir` 对应的本地目录（默认通常是 `data/avatar`）
- 你自定义的本地存储根目录

服务运行时还会使用临时目录：

- `data/.tmp`
- `data/.uploads`

这两个目录通常不需要备份，但要保证本地磁盘有可用空间。

### 访问方式

正式上线时，**必须**通过反向代理提供 HTTPS，并保持：

```toml
[auth]
bootstrap_insecure_cookies = false
```

如果只是本地或内网 HTTP 首次引导，可以临时设成 `true`，让系统把浏览器 Cookie 的 HTTPS 要求初始化成关闭。  
等正式切到 HTTPS 后，再到后台系统设置里把它改回开启。

如果站点要对外访问，最好同时确认：

- 首页响应头里能看到 AsterDrive 返回的页面基线 `Content-Security-Policy`，代理层没有删掉或覆盖成不兼容的策略
- `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 已经填成真实的 `https://` 来源；多个公开域名逐项添加
- 如果要开放注册、找回密码或邮箱改绑，`管理 -> 系统设置 -> 邮件投递` 已经发通过测试邮件

### WebDAV

如果你需要 Finder、Windows 或同步工具接入，部署时就要一起考虑：

- WebDAV 路径
- 反向代理
- 上传大小限制

### 在线预览 / WOPI

如果你准备把 Office 文件交给外部服务打开，部署时还要一起确认：

- `公开站点地址` 已经填成真实 `https://` 来源
- `站点配置 -> 预览应用` 已经配置好对应打开方式
- 外部 Office / WOPI 服务能访问到 `公开站点地址` 对应的 AsterDrive 地址；如果浏览器跨源调用 AsterDrive API 被拦，再到 `网络访问` 放行对应来源

### 存储位置

- 本地磁盘：部署最简单
- S3 / MinIO：适合对象存储场景

## 首次启动会自动完成什么

只要服务成功启动，就会自动完成这些准备：

- 生成默认 `data/config.toml`
- 连接数据库并自动更新数据库结构
- single 和 cluster 都不自动创建存储策略；创建首个管理员后统一进入 `needs_storage`
- 管理员把第一条合适的策略设为默认时，系统原子创建或协调默认策略组，并回填未分配管理员；single 可以选择本地存储，cluster 必须选择所有 Primary 都能访问的共享存储
- 初始化系统设置默认项
- 启动邮件派发、后台任务派发、周期清理和底层文件一致性检查任务

## 上线后先验收这几项

完整清单见 [首次启动检查](/ops/first-check/#启动后马上检查这些项)。

部署完最少跑通这几项：

1. `/health` 和 `/health/ready` 返回正常
2. 首页能正常打开并登录
3. 能创建文件夹并上传一个文件
4. 管理后台能打开

其他角色级（WebDAV、WOPI、邮件、回收站等）按 [首次启动检查](/ops/first-check/#启动后马上检查这些项) 对应章节验。

## 部署之后：运维生命周期

上线只是开始。这些主题属于日常运维，不在部署场景页里展开：

- [生产上线检查清单](/ops/launch-checklist/)：正式上线前的完整验收
- [备份与恢复](/ops/backup/)：备份策略、恢复顺序和恢复后校验
- [升级与版本迁移](/ops/upgrade/)：版本升级路径和注意事项
- [监控与 Grafana](/ops/monitoring/)：Prometheus 指标和 dashboard
- [容量规划参考](/ops/capacity/)：文件数量、数据库、内存和临时磁盘估算
- [故障排查](/ops/troubleshooting/)：按症状定位问题
- [运维 CLI](/ops/cli/)：doctor、离线配置、节点接入和数据库迁移
