---
description: AsterDrive 远程节点概念与管理，讲清主控与从节点的角色边界、内部存储协议版本、接入路径选择，以及禁用等管理操作的语义。
title: "远程节点"
---

:::tip[这一篇讲什么]
这一篇讲远程节点的**概念和管理边界**：主控和从节点各自是什么、协议版本怎么兼容、禁用一条节点链路会发生什么。

完整的接入部署流程已经收敛到场景页 [Follower 存储节点部署](/deploy/follower-node/)；远程存储策略的策略组分流和验收见 [远程节点存储策略教程](/admin/storage-backends/remote-follower/)。
:::

## 先把概念说清楚

AsterDrive 的远程节点能力，本质上是让**另一台 AsterDrive** 充当存储后端。

- **主控节点**：负责登录、前端、管理后台、分享、WebDAV、存储策略和远程节点管理
- **从节点**：只提供 `/health`、`/health/ready` 和内部远程存储协议；接收主控节点签名后的对象请求，再按主控节点下发的**远程存储目标**把对象落到 follower 本地目录或 S3

当前内部远程存储协议版本是 `v5`，当前主控支持与 `v4` 到 `v5` 的 follower 通信。主控测试连接和绑定远程策略时，会比较双方声明的协议版本范围，并读取 follower 暴露的服务端版本、对象读写能力、Range 能力、compose 能力、metadata 能力，以及浏览器直传所需的 CORS 契约。只要版本范围有交集即可继续；`v2` / `v3` follower 需要先升级。

默认情况下，AsterDrive 跑在 `primary` 模式。
只有把 `[server].start_mode` 切成 `follower`，它才会变成从节点。

:::caution[Follower 不是 Primary]
从节点不是第二个登录站点，也不是第二套管理后台。

它的目标只有一个：**给主控节点提供远程对象存储落点**。
主控可以是 single profile 的单个 Primary，也可以是共享数据库、Redis 和存储的 cluster profile。Follower 本身不会承载普通用户请求，也不会单独提供控制面；多 Primary 的要求见[负载均衡与多实例](/deploy/multi-instance/)。
:::

## 接入路径怎么选

| 你的情况 | 去哪里 |
| --- | --- |
| 第一次接从节点，想完整走一遍 | [Follower 存储节点部署](/deploy/follower-node/) |
| 要决定 follower 暴露公网、放进 Tailscale / VPN 还是走反向通道 | [从节点网络部署方式](/deploy/follower-node/network/) |
| 已经接入完成，要配策略组分流和验收 | [远程节点存储策略教程](/admin/storage-backends/remote-follower/) |

两条接入路径的区别只在 enroll 怎么完成：

- **Docker 自动 enroll**：容器启动时读取一次性 `ASTER_BOOTSTRAP_REMOTE_*` 环境变量自动完成，推荐
- **手动 enroll**：在从节点工作目录执行 `aster_drive node enroll`，然后重启服务

两种方式最后都要回主控完成"测试连接 → 默认远程存储目标 → remote 存储策略"这条共同路径。

## 常见判断题

### 从节点能不能开给普通用户登录？

不能。当前设计中，从节点不作为普通用户登录入口。

`follower` 模式只暴露：

- `/health`
- `/health/ready`
- 内部远程存储 API

### 远程存储目标能不能再选一个 remote 策略？

不能。
从节点接收入站对象时，落点必须能在 follower 这一侧直接写入，例如 `local` 或 `s3`；不能再套一层 `remote`。

### `base_url` 留空能不能 enroll？

能，但要看传输方式：

- `direct`：只能先保存记录和完成 enroll；没有 `base_url` 时不能测试连接，也不能承接远程存储流量
- `reverse_tunnel`：follower 重启后会主动连主控；通道在线后可以测试连接、下发远程存储目标，并承接 `relay_stream` 远程策略
- `auto`：`base_url` 为空时等同于反向通道；填了 `base_url` 时等同于直连

无论哪种方式，远程 `presigned` 上传/下载都需要直连和浏览器可访问的 follower `base_url`。

### enroll 成功后为什么还得重启？

因为当前版本只把绑定写进数据库，不会对正在运行的 follower 进程做热刷新。
**写入成功 ≠ 已经生效**，重启之后才真正开始接流量。Docker 自动 enroll 路径在首次启动时就完成绑定，不受这条限制。

### 禁用远程节点会发生什么？

主控节点的远程策略会停止使用它；从节点也会拒绝对应的签名入站请求。
禁用会实际停止这条链路，而不只是隐藏后台记录。

### 一个 follower 能不能同时绑给多套主控？

不能。follower 只绑定一套 AsterDrive 控制面身份。
cluster 中的多个 Primary 共享同一套控制面（数据库、静态密钥和公开/LB 入口），可以共用 follower；彼此独立的 AsterDrive 部署不能同时绑定同一台 follower。
