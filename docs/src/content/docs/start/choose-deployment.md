---
description: AsterDrive 部署方式选择：按试用、单实例、多实例、远程存储节点和 Kubernetes 分流到对应部署场景页，并列出上线前必须确认的事项。
title: "部署方式选择"
---

:::tip[这页只负责选路]
完整部署步骤都在 [部署](/deploy/) 分区。这一页帮你按规模和环境挑一条主路径，挑完直接跳过去。
:::

如果只是本机、内网或临时试用，不用选，直接 [快速开始](/start/quick-trial/)。

## 按规模选

| 你的规模 | 推荐路径 | 场景页 |
| --- | --- | --- |
| 本机试用、临时验证 | 直接跑官方镜像，纯 HTTP 即可 | [快速开始](/start/quick-trial/) |
| 个人、家庭、小团队单实例 | Docker Compose 或 systemd 二选一 | [单实例 Docker](/deploy/docker/)、[单实例 systemd](/deploy/systemd/) |
| 多用户、需要多 Primary 承载流量 | cluster profile + 负载均衡 | [多实例与负载均衡](/deploy/multi-instance/) |
| 已有 Kubernetes 平台 | 复用仓库内置 manifest | [Kubernetes 部署](/deploy/kubernetes/) |
| 想把另一台 AsterDrive 接成远程存储 | 主控不动，新机器跑 follower | [Follower 存储节点](/deploy/follower-node/) |

第一次部署，优先选 Docker。长期跑在 Linux 服务器上，优先选 systemd。多 Primary 不是"单实例跑两份"，先看 [多实例与负载均衡](/deploy/multi-instance/) 的契约再动手。

## 按环境补充

| 环境 | 额外要看 |
| --- | --- |
| 任何正式公网入口 | [反向代理](/deploy/reverse-proxy/)：HTTPS、上传大小、WebDAV 方法透传 |
| follower 在另一个网络 | [从节点网络部署方式](/deploy/follower-node/network/)：公网、VPN、Docker 网络、反向通道怎么选 |
| 对象存储（S3 / MinIO / R2 / COS / Azure / OneDrive / SFTP） | [存储后端](/admin/storage-backends/) 对应教程，部署后再接 |

## 上线前需要确认

正式部署不只是启动容器，还需要提前确认以下事项：

- 数据目录：`config.toml`、数据库、本地上传目录要能跟着升级和重启保留下来
- 访问方式：公网入口应该通过反向代理提供 HTTPS
- 公开站点地址：分享、邮件、WOPI 和跨源访问都依赖它
- WebDAV：如果要给 Finder、Windows、rclone 或同步工具用，代理层要放行对应方法和上传大小
- 存储位置：不同存储策略后端有不同维护成本
- 备份恢复：上线前先确认备份和恢复流程，避免故障发生后才临时补做准备

## 部署完之后

- 启动后自动完成了什么、马上该验什么 → [首次启动检查](/ops/first-check/)
- 正式上线前的完整清单 → [生产上线检查清单](/ops/launch-checklist/)
- 备份和恢复 → [备份与恢复](/ops/backup/)
- 版本升级 → [升级与版本迁移](/ops/upgrade/)
