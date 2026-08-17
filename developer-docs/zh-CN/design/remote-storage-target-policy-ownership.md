# 远端存储目标与策略归属

本文记录 remote storage 的产品模型、服务边界，以及 `0.5.0` target connector 迁移契约。它约束远端节点、follower 侧存储目标和 `asterdrive.storage.remote` 存储策略之间的职责，避免三者重新混成一套配置。

## 产品模型

```text
Remote Node
  -> enrollment、transport、health、capabilities

Remote Storage Target
  -> 当前 primary binding 在 follower 上实际接收文件的 connector 落点

Remote Storage Policy
  -> 选择 remote node，并显式选择该 node 的 target
```

节点回答“怎么连接 follower”；target 回答“文件落到 follower 哪里”；policy 回答“AsterDrive 文件写到哪个 follower 的哪个落点”。Target 不属于普通 storage policy connector，不能并入 policy 配置。

## 所有权边界

### Remote Node

`src/services/remote/remote_node.rs` 拥有节点记录、`direct` / `reverse_tunnel` / `auto` transport、健康状态、能力缓存和引用检查。Enrollment token、命令与 binding 建立由 `src/services/remote/node_enrollment.rs` 和 `src/services/remote/enrollment.rs` 负责。

Remote Node 不替 policy 选择 target，也不解释 connector 配置。

### Remote Storage Target

`src/services/remote/storage_target/**` 拥有：

- connector registry 和稳定 `connector_id`；
- config / credential schema 版本；
- descriptor、字段 scope、默认值与归一化；
- credential 校验、加密存储和 saved-secret 保留；
- runtime driver 构造、revision 与 reconciliation；
- target CRUD 和 primary 到 follower 的转发。

内置 connector 为：

- `asterdrive.remote-target.local`：config v1，包含 `base_path`；
- `asterdrive.remote-target.s3`：config v1，包含 `endpoint`、`bucket`、`base_path`；credential v1 包含 `s3_access_key_id`、`s3_secret_access_key`。

新增 connector 通过 registry 注册自己的 descriptor、schema、归一化和 driver 构造，不增加 core enum、tagged request 或前端 connector 白名单。

Target 属于当前 primary 与 follower 的 binding。多 primary 场景下，不能把 follower 的全局默认值当成所有 primary 共用 target。

### Remote Storage Policy

`src/services/storage_policy/policy/**` 拥有最终 node + target 选择。新建 remote policy 必须同时保存：

- `remote_node_id`；
- `remote_storage_target_key`。

保存前校验 target 属于该 node 当前 binding、connector 可用、没有 `last_error`，且 `applied_revision >= desired_revision`。非 remote policy 携带 target key 会被拒绝。

### Remote Protocol

`src/storage/remote_protocol/**` 只负责签名、path encoding、HTTP / reverse tunnel transport、能力 wire model 和响应解析。它不决定 UI 字段、policy 初始选择或 target 所有权。

## 持久化与 credential

`remote_storage_targets` 保存核心状态和非敏感 connector envelope：

```json
{
  "format_version": 1,
  "connector_id": "asterdrive.remote-target.s3",
  "schema_version": 1,
  "values": {
    "endpoint": "https://HOST",
    "bucket": "BUCKET",
    "base_path": "PREFIX"
  }
}
```

`remote_storage_target_credentials` 按 target 唯一保存 connector ID、schema version、revision 和 authenticated ciphertext。加密 AAD 绑定 target ID、connector ID 和 schema version；API、日志和 `Debug` 不回显 credential values。

未知 connector 的 envelope 原样保留。列表返回 `connector_available = false` 和确定的 unavailable / misconfigured 状态；运行时不猜测其结构，管理员仍可删除数据。

## 0.5.0 数据转换

Schema migration 只增加 connector 列和 credential 表，不改历史 migration。应用在配置加载和 schema migration 之后、监听端口之前，用全局 migration lock 和单个数据库事务转换旧行：

- `local` 映射为 `asterdrive.remote-target.local` config v1；空 credential 不创建记录；
- `s3` 映射为 `asterdrive.remote-target.s3` config v1，并把旧 access/secret 加密写入 credential 表；
- 所有旧明文和扁平配置列在同一事务内清空；
- unknown driver、部分 destination payload、不完整 credential、错误密钥、credential 冲突或 orphan 记录中止启动并回滚整批转换；
- 已转换行会验证 envelope、credential metadata、AAD 和可解密性，重复启动保持幂等。

旧扁平 entity 字段和转换代码只服务 AsterDrive `0.5.0` 已发布数据，`0.6.0` 必须整体删除；runtime、API 和前端不读取这些字段，也不保留兼容 alias。

## V6 协议

当前内部存储协议为 `v6`，支持区间固定为 `v6-v6`。

- capability wire 字段只使用 `remote_storage_target.connector_ids`；unknown future connector ID 可解析并保留，本地 resolver 只暴露 registry 中实际可用的 connector；
- v4 / v5 不在支持区间内，不翻译 `managed_ingress`、`driver_types` 或 Local/S3 wire 名称；
- 旧节点在 protocol range 校验阶段失败，不进入 target descriptor、CRUD 或 runtime 构造；
- Rust API、CRUD、runtime 和前端不接受旧 Local/S3 enum。

## 管理端工作流

Remote policy 表单先选择 node，再加载该 binding 的 target 列表和 connector descriptors。当前 target 优先，其次 default target，最后列表第一项。Policy 流程只提供只读列表和快速创建；远端节点页保留完整 target 创建、编辑和删除。

表单和列表完全由 descriptor 驱动：

- `connector_config` 与 `static_credential` scope 分别构造 payload；
- text、secret、boolean、number、select、default、required 和数值范围由 descriptor 表达；
- 编辑同 connector 时空 secret 表示保留；切换 connector 时必须提交完整新 credential；
- label、badge 和非敏感摘要来自 descriptor metadata；
- descriptor 缺失时保留 persisted data，并关闭编辑入口。

前端不根据 Local/S3 或 connector ID 建立字段、能力和展示矩阵。

## API

```text
GET    /api/v1/admin/remote-nodes/{id}/storage-targets
POST   /api/v1/admin/remote-nodes/{id}/storage-targets
PATCH  /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
DELETE /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
GET    /api/v1/admin/remote-nodes/{id}/storage-target-connectors
```

Descriptor endpoint、响应和内部类型统一使用 connector contract，不保留旧 `storage-target-drivers` URL。Follower 内部协议使用 `/api/v1/internal/storage/targets`；旧 `/ingress-profiles` route 保持 `404`。

## 验收清单

- target config 使用稳定 connector ID 和 versioned envelope，credential 独立加密；
- Local/S3 旧行原子转换，明文清空，错误时整批回滚，重复运行幂等；
- config / credential connector、schema、AAD 或密钥不匹配时失败；
- create、edit、connector switch、saved-secret 与 reconciliation 共用 connector contract；
- unavailable connector 数据保留并返回确定状态；
- v6 unknown connector ID 保留、registry 过滤和 v5 protocol rejection 都有协议测试；
- direct、reverse tunnel、auto 下 CRUD 和能力过滤一致；
- 前端 descriptor 字段类型、默认值、required、非法值、切换和 generic presentation 有测试；
- OpenAPI、生成 TypeScript、SQLite migration、后端 focused/integration tests 与前端 type/lint/unit tests同步验证。
