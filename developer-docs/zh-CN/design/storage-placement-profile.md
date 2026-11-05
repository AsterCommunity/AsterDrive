# Storage Placement Profile

## 状态

Issue #444 的实现基线。placement profile/rule/target 是唯一运行时事实源。`m20261105_000001_remove_expired_storage_compat` 删除已过期的 `storage_policy_group_items` 兼容投影。

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

管理端 simulator 与真实 resolver 共用同一执行路径。合法 dry-run 即使被准入或路由规则拒绝也返回 `200` 诊断结果，包含后端规范化分类、admission 状态、最终 decision 或 rejection code、已评估规则和排除目标；`400` 只表示文件名、大小等模拟输入本身非法。

## 升级与历史边界

- 历史 migration 保留一次性 item → rule/target 导入，保证新建数据库和历史升级可重放；当前 entity、CLI 数据库复制计划和运行时不再包含旧 item 表。
- 新清理 migration 删除旧 item 表，以及规则描述中的原始待删除提示；已经导入的 rule/target 和自定义描述会保留。
- 上传会话的 `placement_profile_id` 和 `placement_revision` 必须非空。缺少绑定的旧会话阻止升级，须在旧版本完成、取消并清理；不会自动删除会话、上传分片或云端对象。
- `placement_rule_id` 继续允许空值：文件夹显式策略覆盖不经过 placement rule，这属于当前产品语义。
- 团队绑定由 immutable snapshot 读取，缺失时明确报错，不再回数据库兜底。团队创建及绑定变更返回前更新本地 snapshot，并发布跨实例 topology reload。
- 新清理 migration 不支持向旧兼容 schema 回滚；降级需要恢复匹配版本的数据库备份。所有 writer 必须同步升级。

## 验收

至少覆盖 matcher 边界、admission allowlist、folder override、target draining/unavailable、weighted selection、profile revision、session binding、迁移前后 legacy routing 一致性和多 workspace copy policy 保留语义。
