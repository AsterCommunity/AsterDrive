# 资源锁系统重构契约

本文档定义 AsterDrive 当前资源锁系统的实现架构。该系统已在 `v0.x` 阶段完成破坏性迁移；活动 `resource_locks` 行是唯一权威状态，文件和目录表不再保存 `is_locked` 投影。

锁系统同时服务以下入口：

- REST 文件与目录 mutation
- WebDAV Class 2 `LOCK` / `UNLOCK`、写方法的 `If` token 校验和 lock discovery
- WOPI `LOCK` / `UNLOCK` / `REFRESH_LOCK` / `GET_LOCK` / `UnlockAndRelock`
- 管理员强制解锁与过期锁清理
- 文件、目录和工作空间列表的只读锁状态展示

重构完成后必须只有一份锁真相：`resource_locks` 中仍然活动的锁记录。缓存和 API 投影都不能成为第二份权威状态。

## 设计结论

本次重构采用以下不可回退的决定：

- 删除 `files.is_locked` 和 `folders.is_locked` 数据库列。
- 删除围绕这两个字段的 set/clear/synchronization 逻辑。
- 删除公开模型中的裸 `is_locked: bool`，改为强类型 `ResourceLockState`。
- 使用显式 workspace lock namespace 行作为所有锁 mutation 的数据库串行化锚点。
- 使用 `namespace generation` 作为缓存版本，不使用 Redis 锁、缓存 CAS 或事件投递保证正确性。
- mutation 只信任 writer database transaction；缓存只服务列表、详情和其他非权威读取。
- Folder/Workspace 深度锁按资源层次定义，`path` 只保留为协议呈现信息，不能作为锁身份的唯一来源。
- REST、WebDAV、WOPI 和 admin 共享 Drive-owned 的锁生命周期核心，不保留多套事务实现。

## 规范依据

WebDAV 协议行为以 RFC Editor 发布的 [RFC 4918](https://www.rfc-editor.org/rfc/rfc4918) 为准：

- Section 6.1 定义锁模型和冲突锁。
- Section 7.4 定义 collection write lock 对成员 URL 的保护范围。
- Section 7.5 要求修改所有被锁资源时提交对应 lock token。
- Section 9.10.3 要求 `Depth: infinity` 覆盖整个层次，并且 LOCK / UNLOCK 不能部分成功。
- Section 9.10.4 要求对未映射非目录 URL 的成功 LOCK 创建空资源。
- Section 9.10.5 定义 shared / exclusive lock compatibility。

Forge 负责这些产品中立的协议规则、解析、规划和响应 grammar。Drive 负责把协议计划映射到 workspace、数据库资源、权限、锁持久化和事务。

## 已消除的旧设计问题

### 派生 boolean 被当作权威状态

`files.is_locked` 和 `folders.is_locked` 只能表达资源自身是否曾被标记，不能表达：

- Personal workspace root lock
- Team workspace root lock
- Folder `Depth: infinity` 对后代的覆盖
- shared lock 集合
- lock timeout
- MOVE / COPY overwrite 后锁根重绑定

锁超时、清理失败或并发替代锁都可能使 boolean 与真实锁记录漂移。维护它需要每个创建、删除、refresh、cleanup 和 rebind 路径执行额外同步，却仍然不能表达完整语义。

### 加锁顺序不一致

旧实现中部分入口先锁 File/Folder 行再访问 `resource_locks`，另一些入口先 `SELECT resource_lock FOR UPDATE` 再等待目标行。当前实现已统一为 namespace、目标、ancestor、锁记录的顺序。

### 层次锁缺少共同串行化点

旧实现缺少 workspace 级共同互斥点，两个并发事务可能同时创建互相冲突的层次锁。当前 `resource_lock_namespaces` 行是共同串行化点。

### 协议入口复制生命周期实现

旧实现由 REST/WOPI service 和 WebDAV backend 分别维护生命周期。当前 workspace root、folder 和 file 都进入 Drive-owned typed lifecycle，协议 adapter 不再复制锁事务。

### lock-null 创建曾与锁获取分步提交

旧 WebDAV handler 先提交空文件，再在另一个 transaction 中获取锁，锁插入失败时可能留下未加锁的空资源。当前 Drive backend 在同一个 writer transaction 中创建空文件元数据和锁记录；Forge 只提供 RFC 适用的 mutation credential 与协议响应语义。

## 领域模型

锁的 workspace、根、覆盖范围、模式和来源是独立维度。

```rust
pub enum LockWorkspace {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

pub enum LockRoot {
    WorkspaceRoot,
    Folder { folder_id: i64 },
    File { file_id: i64 },
}

pub enum LockDepth {
    Resource,
    Infinity,
}

pub enum LockMode {
    Exclusive,
    Shared,
}

pub enum LockOrigin {
    Product,
    WebDav,
    Wopi,
}

pub struct LockTarget {
    pub workspace: LockWorkspace,
    pub root: LockRoot,
    pub depth: LockDepth,
}
```

这些类型必须保证：

- File/Folder 必须属于 `LockTarget.workspace`。
- `WorkspaceRoot` 的真实身份完全来自 workspace，不再使用 `PersonalRoot` / `TeamRoot` 伪实体。
- File 的 `Infinity` 可以保留为 WebDAV 请求呈现值，但冲突覆盖等价于 `Resource`。
- Folder `Infinity` 覆盖自身和所有当前及未来后代。
- WorkspaceRoot `Infinity` 覆盖该 workspace 内所有资源。
- `LockOrigin` 不决定协议响应，但决定 owner payload 的解析与允许的生命周期操作。

## 持久化模型

### `resource_lock_namespaces`

每个 personal/team workspace 有且只有一条 namespace 行。

```text
id                BIGINT primary key
workspace_type    VARCHAR(16) not null
workspace_id      BIGINT not null
generation        BIGINT not null default 0
created_at        datetime not null
updated_at        datetime not null

unique(workspace_type, workspace_id)
```

namespace 行承担两个职责：

- `SELECT ... FOR UPDATE` 是同一 workspace 锁 mutation 的共同串行化点。
- `generation` 是已提交锁投影的缓存版本。

namespace 不是权限快照，也不保存 actor。用户或团队删除时必须在同一产品删除 contract 中清理对应 namespace 和锁记录。

### `resource_locks`

目标表结构：

```text
id                BIGINT primary key
token             VARCHAR unique not null
namespace_id      BIGINT not null references resource_lock_namespaces(id)
root_kind         VARCHAR(16) not null
root_folder_id    BIGINT null references folders(id)
root_file_id      BIGINT null references files(id)
depth             VARCHAR(16) not null
mode              VARCHAR(16) not null
origin            VARCHAR(16) not null
holder_user_id    BIGINT null
owner_info        TEXT null
lockroot_path     VARCHAR null
timeout_at        datetime null
created_at        datetime not null
```

应用层和 migration 必须校验 root 列组合：

| `root_kind` | `root_folder_id` | `root_file_id` |
| --- | --- | --- |
| `workspace_root` | null | null |
| `folder` | non-null | null |
| `file` | null | non-null |

`lockroot_path` 是 WebDAV lock-root URI 的 canonical presentation snapshot。层次冲突不能只通过字符串前缀判断；MOVE/rebind 可以更新呈现路径，但资源身份由 workspace 和 root foreign key 决定。

### 索引

至少需要：

- unique token
- `(namespace_id, timeout_at)`
- `(namespace_id, root_kind, root_folder_id)`
- `(namespace_id, root_kind, root_file_id)`
- `(holder_user_id, timeout_at)`，用于 WebDAV 用户锁配额
- `(namespace_id, lockroot_path)`，用于 WebDAV discovery 和兼容诊断

## 权威事务顺序

所有可能改变锁或被锁资源的事务采用同一顺序：

```text
begin writer transaction
-> resolve workspace and namespace
-> SELECT namespace FOR UPDATE
-> lock storage usage when the mutation changes stored resources
-> lock resource rows in deterministic order
-> SELECT relevant resource_locks FOR UPDATE
-> validate timeout, hierarchy, mode, owner and submitted credentials
-> mutate resource and/or resource_locks
-> increment namespace generation when lock projection changed
-> commit
-> run non-authoritative audit/observation/cache cleanup
```

资源行顺序要求：

- 多路径操作先按 workspace key 排序。
- 同一 workspace 内按 canonical resource identity 排序，不按请求到达顺序。
- ancestor 必须从 workspace root 向目标方向锁定。
- source/destination 同时存在时先排序后获取，不能由 COPY/MOVE 分支自行决定顺序。
- 任何入口都不能先持有 `resource_locks` 行再等待 namespace 或目标资源行。

## 生命周期操作

### Acquire

Acquire command 至少携带：

```rust
pub struct AcquireLockCommand {
    pub target: LockTarget,
    pub mode: LockMode,
    pub origin: LockOrigin,
    pub holder_user_id: Option<i64>,
    pub owner_info: Option<ResourceLockOwnerInfo>,
    pub timeout_at: Option<DateTime<Utc>>,
    pub presentation_path: Option<String>,
}
```

事务必须：

1. 锁 namespace。
2. 锁目标和必要 ancestor 行。
3. 删除或忽略确认已过期的冲突候选。
4. 查询同 workspace 内覆盖目标或被目标覆盖的活动锁。
5. 应用 shared/exclusive compatibility。
6. 校验 owner 锁配额。
7. 创建锁记录。
8. 递增 generation。

### Unlock / Force unlock

token/id 选择器可以先做非锁定 snapshot 读取以定位 workspace，但事务内必须重新验证：

1. 根据 snapshot 锁 namespace。
2. 锁 snapshot 指向的目标资源。
3. 重新 `SELECT resource_lock FOR UPDATE`。
4. 校验 token/id、namespace 和 root identity 未变化。
5. 删除锁并递增 generation。

若 snapshot 与事务内记录不一致，当前事务必须中止并从新的 snapshot 重新开始；不能在持有旧目标行时继续追锁新目标。

### Refresh

Refresh 必须在 namespace transaction 内锁定目标和锁记录，重新校验 token、scope 和 timeout 后更新 `timeout_at`。timeout 改变会改变只读投影，因此必须递增 generation。

### Expired cleanup

cleanup 按 namespace 分组处理：

1. 加载存在过期候选的 namespace ID。
2. 每个 namespace 独立开启短 writer transaction。
3. 锁 namespace。
4. 重新删除仍然过期的锁。
5. 仅在 `rows_affected > 0` 时递增 generation。

返回和审计的 removed count 必须来自实际删除行数，不能来自事务外预查询数量。

### Resource mutation

文件内容、元数据、删除、MOVE、COPY overwrite、版本恢复、WOPI write-back 和 WebDAV mutation 都必须使用同一个 transaction-aware evaluator：

```rust
pub async fn enforce_mutation_locks_on(
    txn: &DatabaseTransaction,
    target: &LockTarget,
    submitted: &SubmittedLockCredentials,
) -> Result<()>;
```

该入口必须检查 direct、ancestor folder infinity 和 workspace root infinity 锁。普通 REST mutation 没有 WebDAV/WOPI token 时，任何覆盖目标的活动协议锁都按冲突处理。

mutation credential contract：

- Forge 解析 `If` header，并只下发对实际冲突 lock-root URI 生效的正向 token；`Not <token>` 只参与条件求值，不成为 mutation credential。
- Drive 的最终 writer transaction 只接收 owned `LockMutationCredentials`，再转换为借用的 evaluator 输入；不接受 `validated=true` 或 `skip_lock_check` 旁路。
- Product lock 可以由匹配的 holder user 满足；WebDAV/WOPI lock 必须由对应内部 lock token 满足，不能仅凭同一用户身份越过协议锁。
- WOPI opaque lock value 只用于 WOPI header 比较；比较成功后传入的是对应 `resource_locks.token`，不是 opaque value。

## WebDAV lock-null 原子边界

Forge 继续负责 RFC 4918 LOCK request planning、适用 lock token 的筛选和 HTTP 状态映射。Drive backend 负责路径、workspace、storage staging 和最终 writer transaction，并实现以下状态机：

```text
目标已存在
  -> 在同一 namespace transaction 内获取锁

目标未映射且允许创建 lock-null file
  -> transaction 外 staging 空 storage object
  -> 锁 namespace
  -> 锁 storage usage
  -> 校验父集合 membership mutation credential
  -> 创建 blob/file 元数据
  -> 创建锁并递增 generation
  -> 一个 commit 对外可见
```

初次解析为已存在、但事务内重解析发现目标被并发删除时，backend 回滚该次事务，在事务外 staging 一次空对象并重试。若重试时另一请求已经创建同一路径，backend 锁定现有资源并只清理本请求未使用的 staging object。

存储驱动的 object write 不参与数据库事务，因此 Drive 使用明确区分 shared dedup object 与 owned non-dedup object 的 `PreparedEmptyFile`：

- 已知数据库失败时，只清理本请求持有的 non-dedup object。
- dedup 空对象是共享对象，不由失败请求删除。
- database commit outcome uncertain 时保留 staged object，避免删除可能已经提交资源引用的对象。
- cleanup 按 staging ownership 执行，不按 WebDAV 路径盲删。

父集合未映射时 Forge 返回 HTTP 409；目标为 collection URL 且未映射时返回 404。锁冲突仍使用 423，不能与父集合缺失混用。

Forge backend port 通过 acquire result 表达目标是否已存在，通用 handler 不推断 Drive 文件事务或 cleanup ownership。

## 只读锁投影

公开文件/目录模型使用：

```rust
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ResourceLockState {
    Unlocked,
    Direct {
        mode: LockMode,
        expires_at: Option<DateTime<Utc>>,
    },
    Inherited {
        root: LockRootSummary,
        mode: LockMode,
        expires_at: Option<DateTime<Utc>>,
    },
}
```

`ResourceLockState` 是响应投影，不写回 File/Folder entity。前端通过 `state != unlocked` 渲染锁图标，不再维护另一个 capability 或 lock matrix。

admin 锁列表仍直接读取锁表，并可展示 token、origin、owner payload 等受保护详情；普通文件列表的 projection 不包含 token、owner XML 或 WOPI payload。

## 缓存契约

### 缓存不是互斥量

`aster_forge_cache` 的 Redis backend 在连接失败时会短暂回退到本地 memory。这个 availability contract 适合读取缓存，不适合分布式锁。锁系统禁止使用：

- Redis `SET NX` 作为 workspace lock
- cache `set_bytes_if_absent` 作为事务 claim
- pub/sub event 作为正确性前提
- cached `Unlocked` 结果直接放行 mutation

### Generation-keyed projection

缓存 key：

```text
resource_lock_projection:v1:personal:<user_id>:g:<generation>
resource_lock_projection:v1:team:<team_id>:g:<generation>
```

缓存 value 只包含：

- generation
- root identity
- depth
- mode
- timeout_at

缓存不包含 token、owner XML、WOPI lock value 或权限判断结果。

### Cache fill

读取流程：

1. 从 reader DB 读取 namespace generation。
2. 使用 generation key 查缓存。
3. miss 时批量读取该 namespace 的活动锁根。
4. 再次读取 generation。
5. 两次 generation 相同才把 projection 写入对应 generation key。
6. generation 改变时重试一次；再次变化则直接使用当前 DB 结果，不写缓存。

旧 reader 写入旧 generation key 不会污染新请求，因为新请求只读取新 generation key。事件总线可以用于提前回收旧 key，但不是一致性依赖。

### TTL

缓存 TTL 为：

```text
min(configured maximum projection TTL, earliest timeout_at - now)
```

读取缓存后仍要按当前时间过滤过期项。无 timeout 锁使用配置的 maximum TTL。缓存不可用、反序列化失败或读取超时都回退 DB，不改变协议或 mutation 结果。

## 模块边界

目标模块结构：

```text
src/services/files/lock/
  domain.rs          LockWorkspace/Root/Depth/Mode/Origin/Target
  resolve.rs         File/Folder -> verified LockTarget
  lifecycle.rs       acquire/unlock/refresh/force unlock
  enforcement.rs     transaction-aware mutation checks
  projection.rs      batch ResourceLockState calculation
  cache.rs           generation-keyed projection cache
  cleanup.rs         namespace-grouped expiration cleanup
  models.rs          public/admin presentation models

src/db/repository/
  lock_namespace_repo.rs
  lock_repo.rs

src/webdav/
  protocol adapter only; no duplicate lifecycle transaction

src/services/preview/wopi/
  WOPI header/state mapping; shared Drive lifecycle underneath
```

禁止添加只改名转发的薄包装函数。抽取的函数必须拥有事务不变量、领域转换、批量查询或协议映射中的至少一项真实职责。

## 破坏性迁移

迁移按以下顺序执行：

1. 创建 namespace 表和新的 lock root/mode/depth/origin 列。
2. 为现有锁从 File/Folder 归属或旧 PersonalRoot/TeamRoot 类型回填 workspace。
3. 为每个被引用 workspace 创建 namespace，并回填 `namespace_id`。
4. 将旧 `shared` / `deep` boolean 转换为 enum 值。
5. 将旧 entity target 转换为 workspace_root/folder/file root 列。
6. 拒绝 workspace 无法解析、root 不存在、枚举非法或 token 重复的数据；migration 不静默丢锁。
7. 切换代码到新模型。
8. 删除旧 `entity_type`、`entity_id`、`shared`、`deep` 列。
9. 删除 `files.is_locked` 和 `folders.is_locked`。

`down` migration 只有在数据能无损映射回旧模型时才执行。存在 workspace root、shared set 或新 origin 语义无法表达时，应明确失败，不伪造旧状态。

## API 与前端迁移

破坏性 API 变化：

- File/Folder DTO 删除 `is_locked`。
- 新增 `lock_state`。
- admin lock DTO 使用 `workspace`、`root`、`depth`、`mode`、`origin`，删除旧 `entity_type`、`entity_id`、`shared`、`deep`。
- OpenAPI 导出后重新生成 TypeScript SDK。
- 前端只消费后端 `lock_state`，不推断父级或 workspace 锁覆盖关系。

## 测试矩阵

### 领域与 repository

- 所有 LockWorkspace/Root/Depth/Mode/Origin 序列化值
- File/Folder workspace 归属校验
- namespace unique 和 generation increment
- root 列组合约束
- active timeout 边界：`timeout_at < now` 过期，等于/晚于 now 活动
- SQLite migration/backfill
- PostgreSQL/MySQL SQL 和行锁路径

### 生命周期

- File、Folder、PersonalRoot、TeamRoot acquire
- token unlock、owner unlock、force unlock、refresh
- shared/shared 成功，shared/exclusive 和 exclusive/* 冲突
- expired replacement
- target/namespace 不存在
- DB failure rollback
- generation 只在投影实际变化时递增

### 层次与并发

- Folder resource lock 不覆盖 child
- Folder infinity lock 覆盖现有和随后创建的 child
- Workspace root infinity lock 覆盖整个 personal/team workspace
- 同资源不同 mount path 仍冲突
- 并发 parent infinity LOCK 与 child LOCK 只能一个成功
- acquire 与 unlock 不死锁
- refresh 与 cleanup 不删除已续期锁
- MOVE/rebind 与 unlock 不删除错误目标的锁
- 多路径 mutation 使用确定锁顺序

并发测试必须使用 barrier/failpoint 证明两个任务进入指定临界区，不能只用 `join!` 假设竞争发生。

### 缓存

- hit/miss 和 batch projection
- 旧 generation fill 不能污染新 generation 读取
- 最早 timeout 限制 TTL
- cached entry 按当前时间过滤
- Redis/memory backend parity
- cache failure 回退 DB
- 缓存中不存在 token 和 owner payload
- mutation 路径不访问 cache

### 协议和产品集成

- REST/WOPI/WebDAV 对同一锁的交叉冲突
- WebDAV lock-null 成功原子创建
- WebDAV lock-null lock failure 不遗留资源
- WebDAV RFC 4918 shared/exclusive/depth/UNLOCK 行为
- WOPI LOCK/REFRESH/UnlockAndRelock/GET_LOCK
- REST 文件、目录、版本、删除、移动和覆盖写入
- admin 强制解锁和 expired cleanup count
- Litmus resource、lockbomb、protected suites

## 验收条件

重构只有在以下条件全部满足后才算完成：

- 仓库中不存在 File/Folder entity 的 `is_locked` 字段或同步 helper。
- 所有 mutation 使用 writer transaction 内的权威 lock evaluator。
- WebDAV backend 不再复制 Drive lifecycle transaction。
- TeamRoot 和 PersonalRoot 经过统一 lifecycle。
- 并发重叠层次锁有确定结果，不产生双成功。
- 缓存故障只影响性能，不影响写入正确性和协议状态。
- OpenAPI、生成 TypeScript、前端锁展示和管理员锁页同步完成。
- SQLite、PostgreSQL、MySQL 相关边界均有验证。
- WebDAV/WOPI focused tests、全 WebDAV 测试、严格 Clippy 和 Litmus 基线通过。
