# 远端存储目标与策略归属

本文记录 AsterDrive `0.4.0` 当前已经落地的 remote storage 产品模型、服务边界和兼容契约。它用于约束 `driver_type = "remote"` 的存储策略、远端节点和 follower 侧远端存储目标，避免后续改动把 node、target 和 policy 的职责重新混在一起。

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

当前代码、API、数据库、service 和 UI 统一使用 `remote_storage_target` 命名。旧 `/ingress-profiles` route 已在 `0.4.0` 移除；内部协议为了兼容 `v4` / `v5` 仍会把能力字段序列化为 `managed_ingress`，但这只是 wire 兼容字段，不是另一套产品概念。

## 当前所有权边界

### Remote Node：连接和节点能力

`src/services/remote/remote_node.rs` 负责远端节点记录、连接方式、transport、健康状态、能力缓存，以及删除节点前的引用检查。它不负责替存储策略决定最终 target。

节点 enrollment 由 `src/services/remote/node_enrollment.rs` 和 `src/services/remote/enrollment.rs` 负责。enrollment token、命令和绑定建立属于节点接入流程，不应塞进 target CRUD 或 policy 校验。

Remote Node 层主要表达：

- 节点名称、启用状态和 binding 状态；
- `direct`、`reverse_tunnel`、`auto` transport；
- enrollment token / command；
- 健康检查、last error 和 tunnel status；
- cached capabilities、协议兼容区间和 follower 声明的 target driver 能力。

### Remote Storage Target：follower 落点

`src/services/remote/storage_target/**` 负责 target CRUD、primary 到 follower 的转发、descriptor、字段归一化、driver 构造和 follower 能力过滤。它提供“有哪些 target、哪些 driver 可以创建、怎样把配置转给 follower”的能力，不决定某条 policy 最终绑定哪个 target。

Remote Storage Target 描述 follower 侧写入对象时的实际落点。当前内部 target driver 为：

- `local`：使用 `base_path` 指向 follower 本地文件系统；
- `s3`：使用 endpoint、bucket、credential 和 `base_path` 指向 follower 管理的 S3-compatible 存储。

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

非 remote policy 携带 `remote_storage_target_key` 会被拒绝。route 只负责接收 DTO、权限检查和调用 service；target 选择与校验不能回流到 handler，也不能由前端 `driver_type` 矩阵替代。

### Remote Protocol：wire 与 transport

`src/storage/remote_protocol/**` 只负责内部协议模型、签名、path encoding、HTTP / reverse tunnel transport、能力 wire model 和响应解析。它可以保留 `managed_ingress` 这类兼容字段，但不决定：

- UI 展示哪些字段；
- policy 默认选择哪个 target；
- target 是否属于当前 policy；
- 管理员应该从哪个产品入口创建 target。

这些产品语义分别属于 descriptor / target service 和 policy service。

## 当前管理端工作流

Remote policy 创建和编辑已经把 node 与 target 选择收口到同一流程：

1. 选择 `driver_type = "remote"`。
2. 选择 remote node。
3. 加载该 node 当前 binding 下的 target 列表和 follower 返回的 target driver descriptors。
4. 优先保留当前 target；没有当前值时选择 default target，再回退到列表第一项。
5. 选择已有 target，或在 policy 流程里快速创建一个 target。
6. 快速创建成功后自动选中新 target，再保存 policy 的 `remote_node_id` 和 `remote_storage_target_key`。

Policy 表单中的 target 管理视图使用只读列表加创建能力，避免在创建 policy 时同时承载完整的 target 编辑和删除操作。远端节点管理页仍保留完整 target 管理入口，适合运维人员检查、创建、更新和删除 follower target。

Target 创建表单必须按 follower 返回的 driver descriptors 和字段描述渲染。前端不得用 Local / S3 白名单或字段矩阵重新推断 capabilities；descriptor 缺失时只能保守地不展示对应创建能力。

## 旧数据兼容边界

早期 remote policy 可能只有 `remote_node_id`，没有 `remote_storage_target_key`。当前实现保留一条有限兼容路径：

- 编辑旧 policy 且没有改变 remote binding 时，可以暂时保留空 target key；
- 运行时对这类旧数据回退到 follower binding 的 default target；
- 新建 remote policy 必须显式选择 target；
- 修改 remote node 或 target 等 remote binding 时，也必须补齐显式 target key。

这个 fallback 只用于平滑读取和保存历史数据，不是新功能可以继续依赖的默认选择机制。default target 可以帮助 UI 做初始选择，但新 policy 的权威合同仍是数据库里保存的 `remote_storage_target_key`。

## 协议与能力兼容

当前内部存储协议为 `v5`，兼容下限为 `v4`，本地支持区间是 `v4-v5`。primary 与 follower 声明的版本区间必须有交集。

`v5` 增加远程存储目标 driver 能力。Rust 字段为 `remote_storage_target`，wire JSON 为兼容旧节点仍使用 `managed_ingress` 并接受新名字作为 alias：

- `v4` 没有声明该能力时，resolver 按旧协议把 Local 和 S3 视为可用；
- `v5` 缺少该能力时不再应用隐式 fallback；
- unknown future driver id 不会绕过当前注册的 descriptor；
- target list、create / update 校验和 policy 表单必须共用 backend capability resolver 的解释结果。

## 当前 API

Primary 管理 API 使用 target 命名：

```text
GET    /api/v1/admin/remote-nodes/{id}/storage-targets
POST   /api/v1/admin/remote-nodes/{id}/storage-targets
PATCH  /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
DELETE /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
GET    /api/v1/admin/remote-nodes/{id}/storage-target-drivers
```

Follower 内部协议使用 `/api/v1/internal/storage/targets`。旧 `/ingress-profiles` route 返回 `404`。数据库表和 entity 已通过追加 migration 从旧表名迁移到 `remote_storage_targets`；不要修改既有 baseline migration。

## 变更验收清单

后续修改 remote node、target 或 remote policy 时至少检查：

- 新建 remote policy 同时持久化 `remote_node_id` 和 `remote_storage_target_key`；
- target 必须属于所选节点、没有 `last_error`，并且 applied revision 已追上 desired revision；
- 非 remote policy 不接受 target key；
- 旧 policy 空 target 的 fallback 只在不改变 remote binding 时保留；
- policy 创建 / 编辑可以选择 target，并能在同一流程快速创建后自动选中；
- remote node 页面继续提供完整 target 管理，而 policy 页面不重复实现完整编辑器；
- direct、reverse tunnel、auto transport 下 target 列表、创建、更新和能力过滤一致；
- `v4` legacy fallback、`v5` 显式能力和 unknown future driver id 都按 resolver 规则处理；
- 前端没有重新引入按 `driver_type` 推断字段或能力的本地矩阵；
- 旧 `/ingress-profiles` route 保持 `404`，target-named API 和兼容边界有测试覆盖；
- 文档、OpenAPI 和生成前端类型只在真实 API shape 变化时同步更新。
