# OrbStack 本地验收 Overlay

这个 overlay 只用于单节点 OrbStack Kubernetes smoke test，不是生产部署模板。它会：

- 把头像 PVC 从 `ReadWriteMany` 降为单节点可用的 `ReadWriteOnce`
- 创建使用 `emptyDir` 的临时 PostgreSQL 18、Redis 7.4 和 RustFS S3-compatible 存储
- 生成固定测试 Secret
- 使用本地镜像 `asterdrive:issue-399`，并设置 `imagePullPolicy: Never`

先在仓库根目录构建当前分支镜像，再应用 overlay：

```bash
docker build -t asterdrive:issue-399 .
kubectl -n asterdrive scale statefulset/asterdrive --replicas=0
kubectl -n asterdrive delete pvc asterdrive-avatars
kubectl apply -k deploy/kubernetes/overlays/orbstack
kubectl -n asterdrive rollout status deployment/postgres-local
kubectl -n asterdrive rollout status deployment/redis-local
kubectl -n asterdrive rollout status deployment/rustfs-local
kubectl -n asterdrive rollout status statefulset/asterdrive
```

全新 cluster 数据库没有默认存储策略，因此两个 Primary 正常启动后，`/health` 应返回 `200`，`/health/ready` 会保持 `503`。先在临时 RustFS 中创建 bucket，再通过任一 Primary 完成管理员 setup，并创建指向 `http://rustfs:9000` 的默认 S3-compatible 策略；首个管理员会自动回填到新建的默认策略组，两个 Primary 随后都应变为 Ready。

清理时删除整个 namespace：

```bash
kubectl delete namespace asterdrive
```
