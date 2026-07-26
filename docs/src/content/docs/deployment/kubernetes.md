---
title: "Kubernetes 部署"
description: 使用 StatefulSet 部署 AsterDrive 多 Primary，并正确配置共享数据库、Redis、存储、稳定内部端点、健康检查和头像共享目录。
---

仓库提供了 `deploy/kubernetes/` 示例，用于部署两个 AsterDrive Primary。它以 [负载均衡与多实例](/deployment/load-balancing/)中的 cluster 契约为前提，不会替你创建生产 PostgreSQL/MySQL、Redis、对象存储或 Ingress controller。

## 为什么使用 StatefulSet

AsterDrive 的普通 HTTP 请求不依赖 Pod 身份，但 reverse tunnel owner 路由需要每个 Primary 有唯一且稳定的 `deployment.internal_endpoint`。示例通过 StatefulSet Pod 名称和 headless Service 生成：

```text
http://asterdrive-0.asterdrive-headless.asterdrive.svc.cluster.local:3000
http://asterdrive-1.asterdrive-headless.asterdrive.svc.cluster.local:3000
```

StatefulSet 在这里仅用于稳定 DNS。权威状态仍在共享数据库、Redis 和存储数据面中，不应依赖 Pod 本地磁盘。

## 部署前必须准备

1. 一套所有 Primary 共用的 PostgreSQL 或 MySQL，cluster 不接受 SQLite。
2. 一套 Redis，同时供 cache 和 config sync 使用。
3. 一套所有 Primary 可访问的存储策略，例如 S3-compatible、Azure Blob、OneDrive 或 remote Follower；cluster 不接受 local policy。
4. 一个支持 `ReadWriteMany` 的 PVC，用于共享默认头像目录 `/data/avatar`。不使用上传头像时可以移除这项挂载。
5. 一组在所有 Primary 上完全一致的认证和加密密钥。
6. 一个只在集群可信网络内可达的 Primary 间内部端点，以及至少 32 个字符的共享内部代理密钥。

必需 Secret 键、基础渲染命令和清单边界记录在 `deploy/kubernetes/README.md`。`secret.example.yaml` 只用于展示字段，不在 kustomization 中；请通过现有 Secret 管理方案创建同名 Secret。

## 探针与终止

示例使用 `/health` 作为 startup/liveness probe，使用 `/health/ready` 作为 readiness probe。readiness 会检查共享数据库、真实 Redis cache 和默认存储，失败的 Pod 不会进入对外 Service。

全新 cluster 数据库启动时没有默认存储策略，因此 `/health/ready` 会先返回 `503`。此时仍可通过任一 Primary 的 `/api/v1/auth/setup` 创建首个管理员；管理员会暂时没有策略组。随后创建一条所有 Primary 都可访问的共享存储策略并设为默认，AsterDrive 会原子创建默认策略组、回填尚未分配策略组的管理员，并通过 Redis 通知其他 Primary 重载。普通注册、邀请接受和管理员创建用户仍要求默认策略组已经存在。

Pod 收到终止信号前先执行 10 秒 `preStop` 等待 endpoint 摘流量，总 `terminationGracePeriodSeconds` 为 45 秒。已经建立的 SSE、上传、下载、WebDAV 或 reverse tunnel 连接仍可能中断，客户端需要重连或重试。

## 存储卷

示例给每个 Pod 单独的 `emptyDir` 作为 `/data`，再用 RWX PVC 覆盖 `/data/avatar`。这样 config 和临时目录不会被多个进程并发写入，头像仍可跨实例读取。

不要把共享 PVC 当作启用 local policy 或 Pod-local staging 的办法。cluster 会按驱动和上传策略拒绝这些路径；共享对象存储或 connector-native multipart 才是多 Primary 上传的数据面。

## Ingress

`ingress.example.yaml` 是 NGINX Ingress 示例，没有加入 kustomization。上线前替换域名、TLS Secret 和 IngressClass，并确认 controller 支持：

- SSE、下载、上传和 WebDAV 流式传输，不缓冲请求或响应
- reverse tunnel 的 WebSocket Upgrade
- 足够大的 request body 上限
- 足够长的 read/write/idle timeout
- 正确的 `Host`、公网协议和可信代理链
- 必要时在入口执行 cluster-wide 全局限流

## 本地校验

不连接业务集群也可以先验证渲染结果：

```bash
kubectl version --client
kubectl kustomize deploy/kubernetes
kubectl apply --dry-run=client -k deploy/kubernetes
```

客户端 dry-run 只验证本地清单与 kubectl 能识别的 schema，不代表外部数据库、Redis、RWX StorageClass、Ingress 或共享存储已经可用。实际集群上线后还要执行[多实例上线验收](/deployment/load-balancing/#上线验收)。
