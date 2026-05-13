# 部署方式选择

::: tip 这页只负责选路
完整部署文档已经统一放到 [部署概览](/deployment/) 下面。这里保留一个短入口，是为了让旧链接和“使用指南”里的部署入口都能落到同一个选择页。
:::

如果只是本机、内网或临时试用，直接看 [快速开始](./getting-started)。  
如果准备长期运行、挂域名、接 HTTPS、备份和升级，直接从 [部署概览](/deployment/) 开始。

## 先选运行方式

| 方式 | 适合谁 | 下一步 |
| --- | --- | --- |
| Docker | NAS、家用服务器、小团队、已有容器环境 | [Docker 部署](/deployment/docker) |
| Docker 从节点 | 想把另一台 AsterDrive 接成远程存储后端 | [Docker 从节点](/deployment/docker-follower) |
| systemd | 云主机、物理机、长期稳定运行 | [systemd 部署](/deployment/systemd) |
| 直接运行二进制 | 本地测试、临时验证 | [快速开始](./getting-started) |

第一次部署，优先选 Docker。长期跑在 Linux 服务器上，优先选 systemd。

## 上线前必须想清楚

正式部署不是把容器跑起来就完事。至少先确认这些东西：

- 数据目录：`config.toml`、数据库、本地上传目录要能跟着升级和重启保留下来
- 访问方式：公网入口应该通过反向代理提供 HTTPS
- 公开站点地址：分享、邮件、WOPI 和跨源访问都依赖它
- WebDAV：如果要给 Finder、Windows、rclone 或同步工具用，代理层要放行对应方法和上传大小
- 存储位置：本地磁盘、S3 / MinIO、远程节点分别有不同维护成本
- 备份恢复：别等出事了才第一次读备份文档，真会气死自己

这些内容在 [部署概览](/deployment/) 里按顺序讲，别在这里硬啃一份重复长文。

## 常见下一步

- 想先跑起来：看 [快速开始](./getting-started)
- 想正式上线：看 [部署概览](/deployment/)
- 想挂 HTTPS：看 [反向代理](/deployment/reverse-proxy)
- 想确认启动后自动完成了什么：看 [首次启动检查](/deployment/runtime-behavior)
- 想在命令行做检查、离线配置或跨数据库迁移：看 [运维 CLI](/deployment/ops-cli)
- 想备份和恢复：看 [备份与恢复](/deployment/backup)
- 想升级：看 [升级与版本迁移](/deployment/upgrade)
- 想接远程存储后端：先看 [远程节点](./remote-nodes)
