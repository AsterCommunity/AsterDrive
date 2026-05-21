---
description: AsterDrive 部署概览，适合 Docker、systemd、反向代理、首次启动检查、备份恢复、升级和故障排查。
---

# 部署概览

::: tip 从哪里开始
按部署方式直接跳走：
- **Docker / NAS / 小团队** → [Docker 部署](/deployment/docker)
- **Linux 服务器长期运行** → [systemd 部署](/deployment/systemd)
- **想命令行做检查、离线改配置、跨数据库迁移** → [运维 CLI](/deployment/ops-cli)

本页把"部署前要想清楚的四件事"梳一遍，是给第一次部署的人看的。
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

## 推荐方式

| 方式 | 适合谁 |
| --- | --- |
| [Docker](/deployment/docker) | NAS、单机、小团队、已有容器环境 |
| [Docker 从节点](/deployment/docker-follower) | 想把另一台 AsterDrive 直接接成 Docker follower |
| [systemd](/deployment/systemd) | 云主机、物理机、长期稳定运行 |
| 直接运行二进制 | 本地测试、临时验证 |

## 部署前先确认这四件事

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
- 自动创建默认本地存储策略 `Local Default`
- 自动创建默认策略组 `Default Policy Group`
- 初始化系统设置默认项
- 启动邮件派发、后台任务派发、周期清理和底层文件一致性检查任务

## 上线后先验收这几项

完整清单见 [首次启动检查](/deployment/runtime-behavior#启动后马上检查这些项)。

部署完最少跑通这几项：

1. `/health` 和 `/health/ready` 返回正常
2. 首页能正常打开并登录
3. 能创建文件夹并上传一个文件
4. 管理后台能打开

其他角色级（WebDAV、WOPI、邮件、回收站等）按 [首次启动检查](/deployment/runtime-behavior#启动后马上检查这些项) 对应章节验。

## 下一步看哪里

- 用 Docker：看 [Docker 部署](/deployment/docker)
- 用 Docker 跑远程从节点：看 [Docker 部署从节点](/deployment/docker-follower)
- 用 systemd：看 [systemd 部署](/deployment/systemd)
- 准备备份、恢复和恢复后校验：看 [备份与恢复](/deployment/backup)
- 想在命令行里做部署检查、离线配置或跨数据库迁移：看 [运维 CLI](/deployment/ops-cli)
- 准备挂 HTTPS：看 [反向代理](/deployment/reverse-proxy)
- 准备接 Prometheus / Grafana：看 [监控与 Grafana](/deployment/monitoring)
- 想确认首次启动到底自动做了哪些事：看 [首次启动检查](/deployment/runtime-behavior)
- 准备升级：看 [升级与浏览器缓存](/deployment/frontend-assets)
- 想建立或复跑性能基准：看 [性能基准与压测](/deployment/performance-benchmarking)
