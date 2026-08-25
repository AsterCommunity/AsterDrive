# Storage Placement Profile

## 状态

Issue #444 的实现基线。当前版本保留旧的 `storage_policy_group_items` 兼容投影；该路径标记为 deprecated，计划在 `0.6.0` 正式移除。

## 运行模型

`StoragePolicy` 表示具体存储端点；profile/rule/target 表示上传准入和放置拓扑。用户和团队在自身记录上绑定 profile，profile 不拥有成员列表。

启动和 topology reload 时，后端把版本化 typed payload 解析为 immutable `PolicySnapshot`。上传热路径只做内存匹配和 target selection，不逐次查询 profile、rule、target 或 assignment 表。

```text
workspace scope
  -> assigned profile
  -> admission
  -> ordered rule
  -> eligible target
  -> first_available / weighted_random
  -> existing upload finalization
```

新 blob 只在创建时选择 policy。跨 workspace copy/move 复用源 blob 和源 `policy_id`；已有 blob 不随 profile 更新流动。Range PUT 的协议和 session 细节由独立 issue 负责。

## 兼容路径

- 旧 `storage_policy_group_items` 只在 migration 中读取一次，转换为单 target、weight=100、`first_available` 的 rule。
- runtime snapshot、service、repository 业务路径和上传链路不读取旧 item 表；新 mutation 只写 placement rule/target。
- 旧 admin `items` 输入仅作为 0.6.0 前的请求转换器，不会写回旧表；输出与运行时事实来自新 rule/target。
- 所有兼容代码必须带 `TODO(0.6.0)` 删除计划；不得继续扩展旧 item 模型。
- 新功能应优先写 placement rule/target，不能建立新的旧 item 专用行为。

## 验收

至少覆盖 matcher 边界、admission deny precedence、folder override、target draining/unavailable、weighted selection、profile revision、session binding、迁移前后 legacy routing 一致性和多 workspace copy policy 保留语义。
