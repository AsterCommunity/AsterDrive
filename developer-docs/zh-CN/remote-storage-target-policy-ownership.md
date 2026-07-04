# 远端存储目标与策略归属

本文记录 0.3 系列里 remote storage 的产品模型和工程边界。它不是一次性迁移清单，而是给后续 `driver_type = "remote"` 存储策略、远端节点和 follower 侧接收落点改造使用的约束。

## 目标模型

远端存储链路分成三层：

```text
Remote Node
  -> 连接、enrollment、transport、health、capabilities

Remote Storage Target
  -> follower 侧实际接收文件的存储落点

Remote Storage Policy
  -> 选择 remote node，并选择该 node 当前 binding 下的 remote storage target
```

这三个概念不能混在一个入口里。远端节点回答“这个 follower 怎么连、现在能做什么”；远端存储目标回答“文件落到 follower 的哪里”；远端存储策略回答“AsterDrive 文件写入时选择哪个 follower 和哪个落点”。

## 当前实现边界

当前代码使用 `remote_storage_target` 作为 API、DTO、repository、service 和 UI 命名。旧 `/ingress-profiles` route 仍作为兼容 alias 保留，不是新产品概念。

现有职责大致是：

- `src/services/managed_follower_service.rs`：远端节点记录、enrollment、transport、健康探测、能力缓存，以及删除前检查是否仍被 remote policy 引用。
- `src/services/remote_storage_target_service/**`：follower 侧接收落点 CRUD、primary 到 follower 的转发、driver descriptor、字段归一化、默认 target、driver 构造和能力过滤。
- `src/services/policy_service/**`：存储策略创建、更新、连接测试、策略组和策略引用约束；remote 策略目前主要绑定 remote node，target 选择语义还没有收回到 policy 工作流。
- `src/storage/remote_protocol/**`：内部存储协议的签名、HTTP / tunnel transport、wire model、能力响应和对象读写请求。
- `frontend-panel/src/pages/admin/AdminRemoteNodesPage.tsx`：现在是 remote node 管理和 remote storage target 管理的主要入口。
- `frontend-panel/src/pages/admin/AdminPoliciesPage.tsx`：现在是存储策略入口，但 remote policy 创建流程还没有完整承载 follower-side target 的选择或创建。

这说明当前行为可以工作，但产品入口偏向 remote node，导致管理员要在远端节点和存储策略两个页面之间来回切换，才能配置一条可用 remote policy。

## 归属规则

### Remote Node 只负责节点能力

远端节点页面和后端服务应该主要表达：

- 节点名称、启用状态和绑定状态
- `direct`、`reverse_tunnel`、`auto` transport
- enrollment token / command
- 健康检查、last error、tunnel status
- cached capabilities 和协议兼容性
- follower 声明的 target driver 能力

Remote Node 不应该成为主要的 storage target 创建入口。可以保留高级管理或只读概览，但新用户流程不应要求先去 remote node 页面手工创建 target。

### Remote Storage Target 负责 follower 落点

Remote Storage Target 描述 follower 侧写入对象时的实际落点，例如：

- local target path / base path
- object storage endpoint、bucket、prefix
- credential create / edit / preserve 规则
- max file size
- 是否是当前 binding 的默认 target
- driver descriptor 与字段校验

Target 是 follower 侧资源，但从 primary 的产品流程看，它应该被 remote storage policy 选择。多 primary binding 下，target 必须按 binding 隔离，不能把一个 follower 的全局默认落点误当成所有 primary 的默认值。

### Remote Storage Policy 负责最终选择

Remote policy 创建和编辑流程应该逐步变成：

1. 选择 `driver_type = "remote"`。
2. 选择 remote node。
3. 加载该 node 在当前 binding 下的 remote storage target 列表和 driver descriptors。
4. 选择已有 target，或在同一流程里创建一个 target。
5. 保存 policy 时显式记录 target 选择；如果暂时仍依赖旧 default target，也只能作为兼容 fallback。

Policy service 应该拥有“这条 remote policy 最终写到哪个 node / target”的产品语义。Remote node service 和 internal protocol 只提供能力、转发和 follower 侧执行。

## 兼容迁移顺序

### 第一阶段：产品语言（已完成）

- 用户可见 UI 使用 “remote storage target / 远端存储目标”。
- API、DB、DTO、service、repository 和前端组件命名使用 `remote_storage_target`。
- 旧 `/ingress-profiles` route 保留为 0.4.0 deprecated 兼容层。

### 第二阶段：服务边界（已完成）

- `remote_storage_target_service` 内部拆成 target CRUD、remote forwarding、descriptor、capability、normalization、driver build 等模块。
- 纯字段归一化和 descriptor 构造不依赖 `AppState`。
- primary 到 follower 的转发逻辑不和 follower 本地 target 持久化逻辑混在同一个函数里。

### 第三阶段：能力解释

- cached `last_capabilities` 的 product-level 解释走统一 resolver。
- remote policy validation、target create / update validation、descriptor listing 使用同一套 v4 fallback、unknown driver id 和 transport 限制规则。
- `src/storage/remote_protocol/**` 仍然只负责 wire 兼容，不决定 UI 是否展示某个 target driver。

### 第四阶段：Policy 工作流收口

- remote policy 表单加载 remote node 后，同步展示该 node 可用 target。
- 创建 policy 时可以在同一流程里选择或创建 target。
- remote node 详情页里的 target 管理降级为高级入口或只读概览。
- default target 只作为 UI 默认选择，不作为长期隐式业务 fallback。

### 第五阶段：API alias 与命名清理（已完成）

对外新增 target-named API，并保留旧 route 至少一个兼容窗口：

```text
GET  /api/v1/admin/remote-nodes/{id}/storage-targets
POST /api/v1/admin/remote-nodes/{id}/storage-targets
PATCH /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
DELETE /api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}
GET  /api/v1/admin/remote-nodes/{id}/storage-target-drivers
```

内部 follower 协议同理增加 `/targets` alias，旧 `/ingress-profiles` route 作为 deprecated 兼容入口保留。

数据库表和 entity 已通过新增迁移从旧表名改到 `remote_storage_targets`，不要修改既有 baseline migration。

## 验收检查

后续实现 #370 相关 PR 时至少检查：

- remote node 页面不再是普通管理员创建 target 的唯一入口。
- remote policy 创建 / 编辑流程可以完整选择 remote node 和 remote storage target。
- direct、reverse tunnel、auto transport 下 target 列表、创建、更新、能力过滤行为一致。
- v4 legacy fallback 和 unknown future driver id 仍然按 resolver 规则处理。
- 前端没有重新引入按 `driver_type` 推断能力的本地矩阵。
- 旧 `/ingress-profiles` route 在兼容窗口内继续可用，新增 alias 有测试覆盖。
- 文档、OpenAPI 和生成前端类型只在 API shape 真正变化时同步更新。
