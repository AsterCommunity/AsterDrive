# AsterDrive Helm Chart

这个 Chart 只部署 AsterDrive，不安装 PostgreSQL/MySQL、Redis、对象存储、Ingress controller 或证书控制器。这样应用升级不会意外接管外围权威数据的生命周期，也不会把本地 smoke 环境的单副本组件伪装成生产依赖。

## 前置条件

- Kubernetes 1.27 或更高版本。
- 所有 Primary 共用的 PostgreSQL/MySQL、Redis 和共享存储数据面。
- 已创建的 Secret，默认名为 `asterdrive-cluster`，字段见 `../../kubernetes/secret.example.yaml`。
- 启用头像上传时，需要支持 `ReadWriteMany` 的 StorageClass 或现有 PVC。
- 使用 Ingress 时，需要已安装并正确配置支持长连接、SSE、WebSocket 和大文件流式传输的 controller。

## 安装

生产环境应先固定镜像版本或 digest，并明确 RWX StorageClass：

```bash
helm upgrade --install asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  --create-namespace \
  --set image.digest=sha256:REPLACE_WITH_IMAGE_DIGEST \
  --set avatarPersistence.storageClass=REPLACE_WITH_RWX_STORAGE_CLASS
```

如果 Secret 使用其他名称，设置 `existingSecret`。Chart 不接收数据库密码、Redis 密码和应用密钥作为普通 values；请用 External Secrets、Sealed Secrets、SOPS 或集群现有密钥方案创建 Secret。

## 关键边界

- 默认运行两个 cluster-profile Primary，共用数据库、Redis cache/config sync 和存储策略。
- StatefulSet 只为每个 Primary 提供稳定内部 DNS；业务权威状态不写在 Pod 本地卷中。
- `/data` 与 `/tmp` 是 Pod 本地 `emptyDir`；`/data/avatar` 可使用 Chart 创建或外部提供的 RWX PVC。
- `networkPolicy.enabled=false`，因为 Chart 不知道实际 Ingress Namespace。启用后默认允许同 Namespace 的 Primary 间访问，并可通过 `networkPolicy.ingressFrom` 增加入口来源。
- NetworkPolicy 不限制出站。生产环境启用出站默认拒绝前，必须完整列出 DNS、数据库、Redis、对象存储、OAuth/OIDC、SMTP 和远端节点依赖。
- 默认 topology spread 是软约束。多节点生产集群可设置 `topologySpread.whenUnsatisfiable=DoNotSchedule`，防止两个 Primary 落在同一节点。
- `values.yaml` 中的资源限制只是起点，应根据上传并发、预览转换、WebDAV/WOPI 和后台任务负载压测后调整。

## 渲染验证

```bash
helm lint deploy/helm/asterdrive
helm template asterdrive deploy/helm/asterdrive --namespace asterdrive
helm template asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  -f deploy/helm/asterdrive/ci/ingress-network-policy-values.yaml
helm template asterdrive deploy/helm/asterdrive \
  --namespace asterdrive \
  -f deploy/helm/asterdrive/ci/existing-pvc-values.yaml
```
