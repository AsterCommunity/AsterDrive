# 远端存储目标与策略归属

本文记录 AsterDrive 当前已经落地的 remote storage 产品模型、服务边界和兼容契约。它用于约束 `driver_type = "remote"` 的存储策略、远端节点和 follower 侧远端存储目标，避免后续改动把 node、target 和 policy 的职责重新混在一起。

## 当前模型

远端存储链路分成三层：

```text
Remote Node
  -> 连接、enrollment、transport、health、capabilities

Remote Storage Target
  -> follower 当前 primary binding 下实际接收文件的存储落点

Remote Storage Policy
  -> 选择 remote node，并显式选择该 node 的 remote storage target
```

这三个概念不能合并成一个“远端存储配置”。远端节点回答“这个 follower 怎么连、现在能做什么”；远端存储目标回答“文件落到 follower 的哪里”；远端存储策略回答“AsterDrive 文件写入时选择哪个 follower 和哪个落点”。

当前代码、API、数据库、service 和 UI 统一使用 `remote_storage_target` 命名。旧 `/ingress-profiles`、`/ingress-profile-drivers` 和 `/storage-target-drivers` route 已移除；内部协议 V6 只通过 `remote_storage_target.connector_ids` 协商能力。

## 当前所有权边界

### Remote Node：连接和节点能力

`src/services/remote/remote_node.rs` 负责远端节点记录、连接方式、transport、健康状态、能力缓存，以及删除节点前的引用检查。它不负责替存储策略决定最终 target。

节点 enrollment 由 `src/services/remote/node_enrollment.rs` 和 `src/services/remote/enrollment.rs` 负责。enrollment token、命令和绑定建立属于节点接入流程，不应塞进 target CRUD 或 policy 校验。

Remote Node 层主要表达：

- 节点名称、启用状态和 binding 状态；
- `direct`、`reverse_tunnel`、`auto` transport；
- enrollment token / command；
- 健康检查、last error 和 tunnel status；
- cached capabilities、协议兼容区间和 follower 声明的 target connector 能力。

### Remote Storage Target：follower 落点

`src/services/remote/storage_target/**` 负责 target CRUD、primary 到 follower 的转发以及 follower-owned connector catalog 的读取。descriptor、localization、字段归一化和 driver 构造由 follower 自己的 connector registry 负责；primary 不要求安装同一 connector，也不决定某条 policy 最终绑定哪个 target。

Remote Storage Target 描述 follower 侧写入对象时的实际落点。target 与普通 storage policy 共用 `StorageConnectionInput`：`connector_config` 由 connector 自己声明 schema，`credential` 由统一 credential input 承载，运行时通过 follower 自己的 `StorageConnectorRegistry` 构造 driver。当前可用 connector 由 follower capability 中的 `connector_ids` 和签名 catalog 共同确定，不在 primary remote service 或前端维护 provider 字段镜像。

Target 属于当前 primary 与 follower 的 binding。多 primary 场景下，不能把 follower 的某个全局默认值当成所有 primary 共用的 target。credential 的创建、修改和保留规则也应停留在 target 层，不进入 storage policy 的通用 options。

### Remote Storage Policy：最终 node + target 选择

`src/services/storage_policy/policy/**` 拥有最终的产品选择和校验语义。新建 remote policy 时必须同时提供：

- `remote_node_id`；
- `remote_storage_target_key`。

保存前会加载所选 follower target，并确认：

- target 属于所选 remote node 的当前 binding；
- target 不存在 `last_error`；
- `applied_revision >= desired_revision`，即 follower 已应用最新配置；
- remote node 的协议、基础对象能力、CORS 和 transport 满足 policy options。

Remote policy 的 `remote_node_id`、`remote_storage_target_key` 和 `base_path` 共同定义物理对象 namespace，显式绑定后不可原地修改。传输策略、大小限制、名称等不改变对象位置的字段仍可更新。需要更换 node、target 或 base path 时，必须创建新 policy，并通过 policy-to-policy storage migration 迁移已有 blob。仅有一个升级例外：0.5.1 生成且 target key 为空的旧 policy 可以在 node 和 base path 不变时补选一次已应用 target；补选后同样锁定。

Primary 是 policy 引用的权威。删除 target 前会在 storage-topology lock 内检查同一 `remote_node_id + remote_storage_target_key` 的 policy 引用；存在任何引用时拒绝删除。被引用 target 的 connector config 同样不可修改，但显示名称以及不改变 connector config 的凭据轮换仍允许。这样保护链保持为 `blob -> policy -> target`，follower 不需要复制 primary 的文件账本。

非 remote policy 携带 `remote_storage_target_key` 会被拒绝。route 只负责接收 DTO、权限检查和调用 service；target 选择与校验不能回流到 handler，也不能由前端 `driver_type` 矩阵替代。

### Remote Protocol：wire 与 transport

`src/storage/remote_protocol/**` 只负责内部协议模型、签名、path encoding、HTTP / reverse tunnel transport、能力 wire model 和响应解析，不决定：

- UI 展示哪些字段；
- policy 默认选择哪个 target；
- target 是否属于当前 policy；
- 管理员应该从哪个产品入口创建 target。

这些产品语义分别属于 descriptor / target service 和 policy service。

## 当前管理端工作流

Remote policy 创建把 node 与 target 选择收口到同一流程：

1. 选择 `driver_type = "remote"`。
2. 选择 remote node。
3. 加载该 node 当前 binding 下的 target 列表和 follower 返回的 connector descriptors。
4. 优先保留当前 target；新建策略时选择列表第一项作为表单初值。
5. 选择已有 target，或在 policy 流程里快速创建一个 target。
6. 快速创建成功后自动选中新 target，再保存 policy 的 `remote_node_id` 和 `remote_storage_target_key`。

编辑已有 remote policy 时，node、target 和 base path 只读，并明确引导管理员创建新 policy 后使用 storage migration。target 删除或物理配置更新若仍存在 policy 引用，会返回 `remote_storage_target.referenced`；直接修改 remote policy 物理绑定会返回 `policy.remote_storage_location_immutable`。

Policy 表单中的 target 管理视图使用只读列表加创建能力，避免在创建 policy 时同时承载完整的 target 编辑和删除操作。远端节点管理页仍保留完整 target 管理入口，适合运维人员检查、创建、更新和删除 follower target。

Target 创建表单必须按 follower 返回的 connector descriptors 和字段描述渲染。前端不得用 Local / S3 白名单或字段矩阵重新推断 capabilities；descriptor 缺失时只能保守地不展示对应创建能力。

## 旧数据兼容边界

早期 remote policy 可能只有 `remote_node_id`，没有 `remote_storage_target_key`。target 归 follower binding 所有，primary 无法在数据库 migration 中可靠推断原策略意图，因此不做隐式映射：这类策略必须由管理员重新编辑并选择 target。运行时、容量查询和 follower 对象请求都会拒绝空 target key，不再回退到 binding 级默认值。

## 协议与能力兼容

当前内部存储协议为 `v6`，兼容下限为 `v6`，primary 与 follower 声明的版本区间必须有交集。

`v6` 通过 `remote_storage_target.connector_ids` 声明远程 target 支持的 connector，并通过签名 `/api/v1/internal/storage/target-connectors?locale=...` 返回 follower-owned descriptor 与 localization catalog。unknown future connector id 不会绕过 follower 当前注册的 descriptor；target list、create / update 校验和 policy 表单使用 follower 返回的 catalog。

## 当前 API

Primary 管理 API 使用 target 命名：

```text
GET    /api/v1/admin/remote-nodes/{id}/storage-targets
POST   /api/v1/admin/remote-nodes/{id}/storage-targets
PATCH  /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
DELETE /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
GET    /api/v1/admin/remote-nodes/{id}/storage-target-connectors
```

Follower 内部协议使用 `/api/v1/internal/storage/targets`。旧 `/ingress-profiles` route 返回 `404`。数据库表和 entity 已通过追加 migration 从旧表名迁移到 `remote_storage_targets`；不要修改既有 baseline migration。

## 变更验收清单

后续修改 remote node、target 或 remote policy 时至少检查：

- 新建 remote policy 同时持久化 `remote_node_id` 和 `remote_storage_target_key`；
- target 必须属于所选节点、没有 `last_error`，并且 applied revision 已追上 desired revision；
- 非 remote policy 不接受 target key；
- 旧 policy 的空 target key 不再回退，必须显式选择有效 target；
- remote policy 创建后不能修改 node、target 或 base path，换位置必须新建 policy 并迁移；
- target 被任何同 node + target key 的 policy 引用时不能删除或改变 connector config；
- policy 创建 / 编辑可以选择 target，并能在同一流程快速创建后自动选中；
- remote node 页面继续提供完整 target 管理，而 policy 页面不重复实现完整编辑器；
- direct、reverse tunnel、auto transport 下 target 列表、创建、更新和能力过滤一致；
- `v6` 显式 connector 能力和 unknown future connector id 都按 resolver 规则处理；
- 前端没有重新引入按 `driver_type` 推断字段或能力的本地矩阵；
- 旧 `/ingress-profiles` route 保持 `404`，target-named API 和兼容边界有测试覆盖；
- 文档、OpenAPI 和生成前端类型只在真实 API shape 变化时同步更新。
