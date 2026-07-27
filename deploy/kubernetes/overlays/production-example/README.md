# 生产部署 Overlay 示例

这个 overlay 只创建 AsterDrive 自身的 Namespace、ConfigMap、Service、StatefulSet、PDB、头像 PVC、Ingress 和 NetworkPolicy。PostgreSQL/MySQL、Redis、对象存储、Ingress controller、证书签发器以及 Secret 生命周期由集群现有基础设施负责。

应用前必须完成以下替换：

1. 将 `kustomization.yaml` 中的镜像标签固定到实际发布版本，或在 GitOps 层固定为镜像 digest。
2. 将 `REPLACE_WITH_RWX_STORAGE_CLASS` 替换为支持 `ReadWriteMany` 的 StorageClass。
3. 修改 `ingress.yaml` 中的域名、TLS Secret、IngressClass 和 controller 注解。
4. 给 Ingress controller 所在 Namespace 添加 `asterdrive.io/ingress-access=true` 标签，或按实际流量入口修改 `network-policy.yaml`。
5. 通过 External Secrets、Sealed Secrets、SOPS 或其他 Secret 管理方案创建 `asterdrive-cluster`，字段见 `../../secret.example.yaml`。
6. 按压测结果调整 CPU、内存和副本数。示例的限制值只是上线起点，不是容量承诺。

```bash
kubectl label namespace ingress-nginx asterdrive.io/ingress-access=true
kubectl kustomize deploy/kubernetes/overlays/production-example
kubectl apply --dry-run=server -k deploy/kubernetes/overlays/production-example
kubectl apply -k deploy/kubernetes/overlays/production-example
```

NetworkPolicy 只限制入站流量，保留到外部数据库、Redis、对象存储、DNS 和身份提供方的出站访问。若要启用默认拒绝出站策略，必须先按实际服务 CIDR、Namespace 和端口完整列出依赖，不能直接照搬通用模板。
