# AsterDrive Kubernetes 多 Primary 部署

这组清单部署两个 AsterDrive Primary，使用 StatefulSet 的稳定 Pod DNS 生成每实例唯一的 `deployment.internal_endpoint`。生产 Kustomize 和 Helm 入口只管理 AsterDrive；外部 PostgreSQL/MySQL、Redis、对象存储、Ingress controller 和证书控制器仍由部署者提供。

目录用途：

- `base/`：AsterDrive 多 Primary 的公共资源。
- `overlays/production-example/`：固定镜像、Pod Security、严格拓扑分散、资源限制、Ingress 和入站 NetworkPolicy 的生产起点。
- `overlays/orbstack/`：带临时 PostgreSQL、Redis、RustFS 和固定测试密钥的本地 smoke fixture，不能用于生产。
- `../helm/asterdrive/`：与生产 overlay 表达同一运行契约的 Helm Chart。

## 部署前准备

- 准备所有 Primary 共用的 PostgreSQL 或 MySQL。
- 准备 Redis，并让 cache 与 config sync 连接同一套可用服务。
- 准备 S3-compatible、Azure Blob、OneDrive 或 remote Follower 等共享存储策略；cluster 不接受 local policy。
- 准备支持 `ReadWriteMany` 的 StorageClass，供 `/data/avatar` 跨 Pod 共享。若不允许上传头像，可从 StatefulSet 和 kustomization 中移除 avatar PVC。
- 使用 Secret 管理器或 GitOps 密钥方案创建名为 `asterdrive-cluster` 的 Secret。`secret.example.yaml` 只列出必需键，不在 kustomization 中，也不应原样部署。
- 将镜像标签固定到准备上线的版本，不要在生产环境长期跟随浮动标签。

## 必需 Secret 键

| 键 | 用途 |
| --- | --- |
| `ASTER__DATABASE__URL` | 共享 PostgreSQL/MySQL 连接串 |
| `ASTER__CACHE__ENDPOINT` | Redis cache endpoint |
| `ASTER__CONFIG_SYNC__ENDPOINT` | Redis config sync endpoint |
| `ASTER__DEPLOYMENT__INTERNAL_PROXY_SECRET` | Primary 间 reverse tunnel 转发认证，至少 32 个字符 |
| `ASTER__AUTH__JWT_SECRET` | JWT 签名密钥 |
| `ASTER__AUTH__SHARE_COOKIE_SECRET` | 分享 Cookie 密钥 |
| `ASTER__AUTH__DIRECT_LINK_SECRET` | 直链签名密钥 |
| `ASTER__AUTH__MFA_SECRET_KEY` | MFA 加密密钥 |
| `ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY` | 存储凭据加密密钥 |
| `ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET` | WebDAV 认证缓存 HMAC 密钥 |

这些密钥在所有 Primary 上必须一致。自动 access token / refresh token 轮换仍写入共享数据库，不要为每个 Pod 生成不同静态密钥。

## 应用与验证

先创建真实 Secret。生产环境从 overlay 开始，替换镜像、RWX StorageClass、域名和网络入口标签后再应用。首次部署要先实际创建 Namespace；同一份 Kustomize 输出中的 Namespace 在 server-side dry-run 时不会持久化，否则后续 namespaced resources 会因为目标 Namespace 尚不存在而校验失败：

```bash
kubectl apply -f deploy/kubernetes/base/namespace.yaml
kubectl kustomize deploy/kubernetes/overlays/production-example
kubectl apply --dry-run=server -k deploy/kubernetes/overlays/production-example
kubectl apply -k deploy/kubernetes/overlays/production-example
kubectl -n asterdrive rollout status statefulset/asterdrive
kubectl -n asterdrive get pods,svc,pvc,pdb
```

`base/` 保留未注册的 `ingress.example.yaml`，适合自定义 overlay；`production-example/` 已注册一份需要显式替换的 NGINX Ingress。无论使用哪种方式，都要按实际 controller 调整流式传输、WebSocket、body size 和 timeout。

也可以使用 Helm：

```bash
helm upgrade --install asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  --create-namespace \
  --set image.digest=sha256:REPLACE_WITH_IMAGE_DIGEST \
  --set avatarPersistence.storageClass=REPLACE_WITH_RWX_STORAGE_CLASS
```

Chart 默认引用 `asterdrive-cluster` Secret，不生成 Secret，也不提供 PostgreSQL、Redis 或对象存储 subchart。完整 values 和边界见 `../helm/asterdrive/README.md`。

## 清单边界

- `/health` 用于 startup/liveness，`/health/ready` 用于 readiness；初始化期间基础依赖健康的 Pod 会以 `needs_admin` 或 `needs_storage` 状态进入对外 Service，完成初始化后才探测默认存储。
- Pod 终止前先等待 10 秒让 endpoint 摘流量，进程总优雅终止窗口为 45 秒。
- `/data` 是每 Pod 临时卷，只保存 config、普通临时文件和被 cluster 拒绝使用的上传 staging 目录；`/data/avatar` 由独立 RWX PVC 覆盖。
- StatefulSet 不表示 AsterDrive 把业务状态放在 Pod 磁盘。它只提供稳定、唯一的内部 DNS，供 reverse tunnel owner proxy 使用。
- `internal_endpoint` 只应在集群内部可达，不要指向公开 Ingress 或对外负载均衡地址。
- 应用内 rate limit 按进程计数；严格全局限流应放在 Ingress、gateway 或 LB。
- 生产示例的 NetworkPolicy 只限制入站，保留外部依赖所需的出站流量。启用出站默认拒绝前要先完整枚举 DNS、数据库、Redis、存储、身份提供方、SMTP 和远端节点。

完整限制与验收步骤见用户文档的“负载均衡与多实例”和“Kubernetes 部署”。
