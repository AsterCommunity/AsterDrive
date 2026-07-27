---
title: "Kubernetes 部署"
description: 使用 StatefulSet 部署 AsterDrive 多 Primary，并正确配置共享数据库、Redis、存储、稳定内部端点、健康检查和头像共享目录。
---

仓库同时提供 Kustomize 和 Helm，用于部署多个 AsterDrive Primary。两种入口都以[负载均衡与多实例](/deployment/load-balancing/)中的 cluster 契约为前提，并且**只管理 AsterDrive 自身**，不会接管生产 PostgreSQL/MySQL、Redis、对象存储、Ingress controller 或证书控制器的生命周期。

外围依赖不作为默认 subchart，不是因为它们不重要，而是因为数据库、Redis 和对象存储都是权威状态。把它们绑进应用 Chart 会让应用升级、回滚和卸载同时影响数据服务，也很难覆盖托管数据库、云对象存储、现有 Redis 集群和不同备份策略。仓库中的 OrbStack overlay 包含这些组件，仅用于本地 smoke 和故障注入。

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

示例使用 `/health` 作为 startup/liveness probe，使用 `/health/ready` 作为 readiness probe。readiness 始终检查共享数据库、真实 Redis cache 和 cluster 拓扑；系统完成初始化后还会执行默认存储 driver 的轻量 readiness 检查，失败的 Pod 不会进入对外 Service。

全新 cluster 数据库启动时没有默认存储策略。只要数据库、Redis 和拓扑健康，`/health/ready` 仍返回 `200`，响应状态依次为 `needs_admin` 或 `needs_storage`，让 Pod 进入对外 Service 并完成初始化；尚未完成产品初始化不等于 Pod 故障。先创建首个管理员，再用该管理员登录，前端会引导到存储策略页。创建一条所有 Primary 都可访问的共享存储策略并设为默认后，AsterDrive 会原子创建默认策略组、回填尚未分配策略组的管理员，并通过 Redis 通知其他 Primary 重载，响应状态随后变为 `ready`。普通注册、邀请接受和管理员创建用户在 `needs_storage` 阶段会收到明确的“系统初始化未完成”错误。

系统进入 `ready` 后，`/health/ready` 会检查默认策略存在、driver 可构造，以及 driver 提供的本地低成本前置条件。这个高频探针不会对 S3、OneDrive、SFTP 或远程 Follower 执行读写网络探测，因此远端存储服务中断不一定触发 `503`；生产环境还需要使用指标告警或独立 synthetic probe 监控真实对象读写。Redis、数据库或拓扑异常在任何初始化阶段都会返回 `503`。

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

## Kustomize 生产示例

`deploy/kubernetes/overlays/production-example/` 复用公共 base，并让最终生产清单包含：

- 固定版本镜像示例
- `automountServiceAccountToken: false`
- Restricted Pod Security Namespace 标签
- CPU/内存限制
- `DoNotSchedule` 节点拓扑分散
- NGINX Ingress 起点
- 只限制入站流量的 NetworkPolicy

应用前必须替换 RWX StorageClass、镜像版本或 digest、域名、TLS Secret 和 IngressClass。NetworkPolicy 默认允许 AsterDrive Namespace 内部访问，并允许带 `asterdrive.io/ingress-access=true` 标签的 Namespace 访问 3000 端口；请给实际 Ingress controller Namespace 加标签，或直接按集群入口修改策略。

该策略有意不限制出站，因为模板不知道数据库、Redis、对象存储、DNS、OAuth/OIDC、SMTP 和 remote Follower 的真实地址。要启用出站默认拒绝，先按实际环境逐项列出这些依赖，否则 readiness、登录、邮件、远端存储或下载链路会被自己切断。

首次部署时先实际创建 `asterdrive` Namespace，再执行整套 overlay 的 server-side dry-run。`--dry-run=server` 不会持久化同一次 Kustomize 输出中的 Namespace，若目标集群尚无该 Namespace，后续 StatefulSet、Service 和 PVC 校验会报 Namespace 不存在。

```bash
kubectl label namespace ingress-nginx asterdrive.io/ingress-access=true
kubectl apply -f deploy/kubernetes/base/namespace.yaml
kubectl kustomize deploy/kubernetes/overlays/production-example
kubectl apply --dry-run=server -k deploy/kubernetes/overlays/production-example
kubectl apply -k deploy/kubernetes/overlays/production-example
```

## Helm

Chart 位于 `deploy/helm/asterdrive/`，与 Kustomize base 使用相同的 cluster 默认值、探针、稳定内部端点、安全上下文、临时卷和 RWX 头像卷契约。

```bash
helm upgrade --install asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  --create-namespace \
  --set image.digest=sha256:REPLACE_WITH_IMAGE_DIGEST \
  --set avatarPersistence.storageClass=REPLACE_WITH_RWX_STORAGE_CLASS
```

Chart 默认引用 `asterdrive-cluster` Secret。数据库连接串、Redis endpoint 和应用密钥不进入普通 Helm values；通过集群现有 Secret 管理方案创建 Secret，需要改名时设置 `existingSecret`。所有 Primary 必须通过 Chart 创建或复用的 RWX PVC 共享头像目录；Chart 会拒绝关闭头像持久化。额外环境变量只能补充非权威配置，不能覆盖 cluster profile、每 Pod 内部端点、数据库、Redis、内部代理或认证密钥。启用 NetworkPolicy 后始终允许 Primary 互访，`allowSameNamespace` 仅控制是否额外放行整个 Namespace。Chart 还支持 Ingress、镜像 tag/digest、资源、调度和 PDB，但不提供 PostgreSQL、Redis 或对象存储 subchart。

生产多节点建议设置 `topologySpread.whenUnsatisfiable=DoNotSchedule`。默认保留 `ScheduleAnyway`，便于开发集群和单节点验证；严格设置在可用节点不足时会让新 Pod 保持 Pending，这是正确的故障域保护，不应通过把两个 Primary 挤回同一节点来掩盖容量不足。

## 本地校验

不连接业务集群也可以先渲染清单，并使用与 CI 相同的 kubeconform 做严格 schema 校验：

```bash
kubectl version --client
kubectl kustomize deploy/kubernetes > /tmp/asterdrive-base.yaml
kubectl kustomize deploy/kubernetes/overlays/production-example > /tmp/asterdrive-production.yaml
helm lint deploy/helm/asterdrive
helm template asterdrive deploy/helm/asterdrive --namespace asterdrive > /tmp/asterdrive-helm.yaml
docker run --rm -v /tmp:/manifests \
  ghcr.io/yannh/kubeconform:v0.7.0@sha256:85dbef6b4b312b99133decc9c6fc9495e9fc5f92293d4ff3b7e1b30f5611823c \
  -strict -summary \
  /manifests/asterdrive-base.yaml \
  /manifests/asterdrive-production.yaml \
  /manifests/asterdrive-helm.yaml
```

`kubectl apply --dry-run=client` 即使关闭 validation 仍可能执行 API discovery，因此不适合作为无 kubeconfig CI 的离线验证器。CI 会渲染 OrbStack、production overlay，以及 Helm 的默认、Ingress+NetworkPolicy+digest、existing PVC+三副本等边界组合，并使用固定版本的 kubeconform 执行严格 schema 校验。上述验证仍不代表外部数据库、Redis、RWX StorageClass、Ingress 或共享存储已经可用；真正部署前应连接目标集群，先创建目标 Namespace，再执行 `kubectl apply --dry-run=server`，上线后完成[多实例上线验收](/deployment/load-balancing/#上线验收)。
