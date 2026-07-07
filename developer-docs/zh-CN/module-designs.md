# AsterDrive 关键模块设计说明

本文补充 [`architecture.md`](./architecture.md) 的全局视角，专门解释几个已经落地、但仅靠看目录和接口名很难迅速建立正确心智模型的核心模块。

这里描述的是当前仓库的实现设计，不是未来计划，也不是理想化重写方案。

当前覆盖七块：

- 统一工作空间存储链路
- 分享服务
- 后台任务系统
- 存储策略迁移任务
- 管理员文件 / Blob 可观测
- `doctor` / 一致性审计
- 跨数据库迁移 CLI

如果你只是想知道“请求从哪里进来、代码应该改在哪一层”，先看 [`architecture.md`](./architecture.md)。如果你已经知道入口，但还不清楚模块内部为什么这么拆、边界为什么这样收，继续看本文。

## 1. 统一工作空间存储链路

对应代码主要在：

- `src/services/workspace/scope/`
- `src/services/workspace/storage/`
- `src/services/workspace/models.rs`
- `src/services/workspace/storage_core/`
- `src/services/files/file/*`
- `src/services/files/folder/*`

### 设计目标

这条链路解决的是一个很具体的问题：个人空间和团队空间的文件语义高度相似，但权限、配额归属、默认策略组归属并不一样。

如果为两类空间分别维护两套完整 service，会很快遇到几个坏处：

- 文件、文件夹、上传、分享、回收站、任务这些规则需要重复实现
- 新功能很容易先补一条链路，另一条链路补漏
- 行为回归时，个人空间和团队空间会逐渐漂移

当前设计的核心思路是：

1. route 层只负责把请求映射成一个 `WorkspaceStorageScope`
2. service 层尽量复用统一的文件主链路
3. 只有确实依赖空间身份差异的地方，才在 scope 分支里展开

### 核心抽象：`WorkspaceStorageScope`

`WorkspaceStorageScope` 只有两种形态：

- `Personal { user_id }`
- `Team { team_id, actor_user_id }`

这个类型不是为了省参数，而是为了把“资源属于谁”和“操作人是谁”这两个维度固定下来。

在个人空间里，两者通常是同一个用户；在团队空间里，两者不再相同：

- `team_id` 表示资源归属的工作空间
- `actor_user_id` 表示发起操作的成员

因此很多函数只要接收一个 `scope`，就已经拿到了后续权限判断所需的最小完整上下文。

### 分层拆法

统一存储链路目前按三层拆开：

1. `workspace::scope`
   负责 scope 访问校验，以及“某个 file/folder 是否属于这个空间”
2. `workspace::storage`
   负责把上传、落盘、预上传 blob、multipart 等入口拼成统一工作流
3. `workspace::storage_core`
   负责策略解析、配额读写、blob / 文件记录创建等较稳定的核心动作

这样拆的原因不是“按文件大小切模块”，而是为了把三类变化频率不同的逻辑分开：

- scope 规则随团队功能演进而变化
- 上传方式和接入路径会继续增加
- 核心落盘与计费规则相对稳定，应该成为底座

### 关键工作流

不管入口是 REST 上传、目录上传、WebDAV flush，还是后台归档导入，这条链路都尽量收敛到同一套语义：

1. 先确认 scope 可访问
2. 校验目标文件夹是否在当前空间内
3. 解析文件最终应落到哪条存储策略
4. 根据驱动类型和策略选上传方式
5. 创建或复用 blob
6. 创建 / 覆盖 `files` 记录
7. 更新个人或团队的 `storage_used`
8. 事务外做存储副作用清理、变更通知等补充动作

这里有几个容易误解的点：

- 个人空间与团队空间共用同一条文件主链路，但配额归属不同
- 文件夹可以覆盖策略；否则才回退到用户默认策略或团队策略组
- 本地存储支持可选内容去重；对象存储、OneDrive 和 Remote 路径默认不做内容去重
- “上传成功”不是只看对象写进了存储，还必须包含数据库状态转换和配额落账

### 为什么策略解析放在统一链路里

策略选择不是纯配置读取，而是业务语义的一部分。它同时影响：

- 文件内容最终写到哪种存储驱动
- 是否允许当前大小的文件
- 是否启用本地内容去重
- 配额统计记到谁头上

如果把策略解析下沉到 route 层，调用方必须自己维护“文件夹覆盖优先于默认策略组”这套规则，很容易出现同一个仓库里多套策略决策。

当前约束是：

1. 先看目标文件夹是否显式绑定策略
2. 个人空间走用户绑定策略 / 策略组
3. 团队空间走团队绑定策略组

这样做的结果是，策略决策始终和工作空间语义同处一层，不会散落在 API handler、repo 或驱动实现里。

### 为什么配额检查是两段式

文件写入涉及外部存储副作用，单纯依赖事务内最后一刻再判断会让用户在明显超额时仍走完整上传流程，体验和资源消耗都不好。

因此当前设计故意做成两段：

- 事务外先做一次 fast-fail
- 事务内在最终写入前再做权威校验

这意味着外层检查只负责减少无效工作，不负责给出最终一致性承诺；最终承诺仍然由事务内检查和原子更新承担。

### 关键约束

这条链路目前依赖几个不应轻易破坏的约束：

- scope 校验必须先于资源访问
- `files` / `folders` 的空间归属判断不能绕过 `ensure_*_scope`
- 配额写入必须和文件记录创建保持一致
- 本地去重只是一种存储策略能力，不是全局默认行为
- 事务内完成数据库落账，事务外处理不可回滚的副作用

如果未来要加新的文件入口，优先问题不是“这个接口放哪”，而是“它能不能复用这条统一链路”；只有在业务语义确实不同的时候才应该新开支线。

## 2. 分享服务

对应代码主要在：

- `src/services/share/mod.rs`
- `src/services/share/management.rs`
- `src/services/share/content.rs`
- `src/services/share/shared.rs`

### 设计目标

分享服务解决的是“把内部资源暴露给匿名或半匿名访问者”这件事，但又不能把内部权限模型直接搬给分享页使用。

所以当前设计刻意把分享拆成两条路径：

- 管理路径：创建、更新、删除、列举分享，走已登录用户 / 团队成员权限
- 公开访问路径：通过 token 读取分享内容，只认分享自身状态，不认原始登录态

这两条路径共用一份 `shares` 数据，但不共用认证前提。

### 分享对象模型

REST 创建分享时使用 `target: { type, id }` 描述目标；服务层会把它映射到持久化表里的单目标模型。一个分享只能指向一种资源：

- 文件分享：`file_id` 有值，`folder_id` 为空
- 文件夹分享：`folder_id` 有值，`file_id` 为空

这不是为了偷懒，而是为了把下载计数、过期时间、密码校验、公开 token 这些状态统一挂在“单个可公开资源”上。

相比“把文件夹下每个子文件都复制一份分享状态”，这种设计更容易保证：

- 状态集中
- 生命周期单一
- 公开 token 稳定
- 计数语义清晰

### 为什么创建分享时要锁资源

`create_share_in_scope()` 在事务里会先锁住目标 file / folder，再检查是否已存在活跃分享。

这是为了防并发下的重复创建。

当前想保证的语义是：

- 同一空间内，同一资源最多只保留一个活跃分享
- 过期分享可以被删除并重新创建

如果不在事务里锁资源，两次并发请求都可能在“还没看到对方新建记录”时通过校验，最后写出两条活跃分享。

### 公开访问为什么不直接信任目标资源

公开访问路径会先加载分享，再反查目标 file / folder，并重新验证几件事：

- 分享是否过期
- 下载次数是否已达上限
- 目标资源是否仍存在
- 目标资源是否仍在分享声明的空间里
- 如果是文件夹分享，子文件 / 子目录是否仍在分享根目录子树内

这层校验不能省，因为分享 token 本身只表达“曾经有人允许公开访问这个资源”，不表达“资源现在一定还有效”。

### 文件夹分享的边界控制

文件夹分享最容易出安全问题的地方不是根目录本身，而是“子资源如何被限定在分享子树里”。

当前设计采用两层约束：

1. 先校验目标 file / folder 与分享本身属于同一工作空间
2. 再用 `verify_folder_in_scope()` 校验它是否在分享根目录的后代树内

这样做的原因是，单靠 `team_id` 或 `user_id` 一致并不能证明它真的属于这个分享范围。

### 密码和计数设计

分享密码不会明文存储，而是按普通认证密码一样做哈希。

公开访问时的几个状态拆分也比较刻意：

- `view_count` 只表示被查看次数
- `download_count` 只表示实际下载次数
- `max_downloads = 0` 表示不限次，而不是“禁止下载”

下载计数的增加和回滚在 repo 层有专门原子操作，是因为它需要在高并发公开访问下尽量减少“超限后仍多放出几次下载”的窗口。

### 为什么分享服务不接管原资源生命周期

当前分享服务有意不做“被分享文件的版本快照固定”或“分享时复制出独立只读副本”。

这意味着：

- 分享看到的是当前资源状态
- 如果源资源被移到回收站或超出分享范围，公开访问会失效
- 分享不是归档快照，只是一个受约束的公开入口

这个取舍的优点是实现简单、存储成本低、分享更新即时可见；代价是它天然继承源资源生命周期。

如果未来要引入“不可变分享”或“带版本固定的公开快照”，那会是另一套产品语义，不应该直接塞进现有分享链路。

## 3. 后台任务系统

对应代码主要在：

- `src/services/task/mod.rs`
- `src/services/task/dispatch.rs` 和 `src/services/task/dispatch/`
- `src/services/task/runtime.rs`
- `src/services/task/storage_policy_cleanup.rs`
- `src/services/task/storage_migration.rs`
- `src/db/repository/background_task_repo/`
- `src/db/repository/storage_migration_checkpoint_repo.rs`

### 设计目标

后台任务系统当前承担两类事情：

- 用户可见、可能较耗时的业务任务，例如归档压缩、解压、缩略图生成
- 系统周期任务的执行记录，例如清理、派发和巡检

它不是一个独立的任务服务，也不是外部消息队列，而是单体进程里的持久化任务子系统。

核心目标是：

- 在单体进程内提供可恢复的异步执行
- 让任务状态可被 API / 管理界面直接查询
- 避免 worker 重启或并发执行时把旧结果回写到新状态上

### 为什么 `background_tasks` 表既是队列也是历史表

当前没有额外引入“队列表 + 历史表”双模型，而是让 `background_tasks` 同时承担：

- 待执行任务队列
- 处理中任务租约状态
- 已完成 / 已失败任务记录
- UI 展示所需的进度、步骤、错误和结果摘要

这个设计的优点是：

- 状态源单一
- API / Admin 页面不需要跨表拼装
- retry、清理、归档保留期都可以直接基于同一条记录操作

代价是表结构字段会比较重，但对当前单体规模是可接受的。

### 认领模型：租约 + fencing token

这套系统最关键的设计点不是“怎么跑任务”，而是“怎么阻止旧 worker 覆盖新 worker 的结果”。

当前做法是：

1. dispatcher 先从数据库挑出可认领任务
2. 认领时原子递增 `processing_token`
3. 之后所有心跳、进度更新、完成写回、失败写回都带上这个 token
4. 只有 token 仍匹配时，这些写操作才会命中

这就是典型的 fencing token 思路。

只要旧 worker 的 token 过期，它后续的数据库写回就应该全部失败，不能再把状态覆写成“成功”或“失败”。

### 为什么还需要进程内 `TaskLeaseGuard`

单靠 fencing token 只能阻止旧 worker 回写数据库，不能阻止旧 worker 继续在本地做副作用。

例如压缩 / 解压这种可能跑在 `spawn_blocking` 里的长任务，如果只知道“最终写库会失败”，但业务逻辑继续往临时目录里写文件，依然会产生资源浪费甚至冲突。

因此当前又加了一层 `TaskLeaseGuard`：

- 只要心跳或状态写库成功，就刷新本地 lease
- 如果 lease 丢失或连续太久没续上，就要求执行流主动终止

所以这里有两层保护：

- `processing_token` 负责防止旧 worker 回写数据库
- `TaskLeaseGuard` 负责让旧 worker 尽快停下本地执行

### 执行上下文和关闭语义

业务任务入口不直接接收 `TaskLeaseGuard`，而是接收 `TaskExecutionContext`。这个上下文把两件事绑在一起：

- 当前 processing token 对应的 lease guard
- 进程 graceful shutdown 的 cancellation token

任务实现和长耗时 helper 应该调用 `context.ensure_active()`、`context.sleep_or_shutdown()` 或 `context.shutdown_requested()`。这样无论任务是在普通 async 流程、下载轮询，还是 `spawn_blocking` 里的压缩 / 解压循环中运行，只要服务开始关闭，执行流都能主动停下来。

需要注意，Tokio 不能强行中断已经开始执行的 `spawn_blocking` 闭包。runtime shutdown 的 grace 期只是在等待 worker 协作退出；如果阻塞闭包内部没有周期性检查 `TaskExecutionContext`，超过 grace 后 abort 的也只是外层 async handle，已经进入执行中的阻塞工作仍可能继续占用线程直到自然返回。因此压缩、解压、批量复制等阻塞长循环必须在循环内部放置 `context.ensure_active()` 检查点，不能只依赖外层 future 被取消。

`TaskLeaseGuard` 仍然存在，但它是较底层的 fencing / 心跳实现细节。进度写库、runtime metadata 写库、完成写库这类需要 processing token 的 helper 可以继续接收 guard；新业务任务和会等待 I/O、sleep、长循环的 helper 不应该把裸 guard 当作执行上下文。

graceful shutdown 不是业务失败。worker 因 `TaskExecutionContext` 收到 shutdown 而退出时，dispatcher 会用当前 processing token 把任务从 `Processing` 释放回 `Retry`，同时清空本次 lease 字段并唤醒 dispatcher。这个释放不会增加 `attempt_count`，也不会写入 `last_error`。如果 token 已经不匹配，释放会被 fencing 条件挡住，旧 worker 不会覆盖新 worker 的状态。

### 心跳和 stale reclaim

dispatcher 会定期续心跳，数据库里同时记录：

- `last_heartbeat_at`
- `lease_expires_at`

这样一来，如果进程崩溃、任务卡死或节点切换，新的 dispatcher 可以把超过 stale 阈值的任务重新认领。

但这里又故意做了一个保守处理：

- 心跳写入偶发失败时，不立即把任务判死
- 只有 lease 真正过期，当前 worker 才自我终止

这是为了避免瞬时数据库抖动把长任务误判成死任务，然后触发双 worker 并跑。

### 重试模型

任务失败后不会无脑重跑，而是基于：

- 当前尝试次数
- 任务种类
- 错误是否可重试

来决定进入：

- `Failed`
- `Retry`
- 或者被新 lease 接手继续处理

也就是说，“失败”在这里不是单一语义，而是带重试预算和 lease 状态的结果。

### 分 lane 调度

dispatcher 不是用一个全局并发池无差别捞所有任务，而是按任务类型分 lane：

- `Archive`：`archive_compress`、`archive_extract`、`archive_preview_generate`，并发上限来自 `background_task_archive_max_concurrency`
- `Thumbnail`：`thumbnail_generate`、`image_preview_generate`、`media_metadata_extract`，并发上限来自 `background_task_thumbnail_max_concurrency`
- `StorageMigration`：`storage_policy_migration`，并发上限来自 `background_task_storage_migration_max_concurrency`
- `Fallback`：`storage_policy_temp_cleanup`、`trash_purge_all`、`blob_maintenance` 和系统运行记录的兜底 lane，并发上限来自 `background_task_max_concurrency`

归档预览虽然是只读扫描，但也会触达对象存储和 ZIP 解析，所以和压缩 / 解压共享 archive 并发预算。图片预览生成和媒体元数据解析也会读取原始对象并进行 CPU 解析，因此和缩略图共享 thumbnail lane。

archive 和 thumbnail lane 会在单轮 dispatch 里快速继续捞下一批，避免大量同类任务只靠下一次周期 tick 慢慢推进。StorageMigration lane 独立限流，避免大规模策略迁移占满归档、缩略图或普通维护任务的并发预算。Fallback lane 更保守，避免维护型任务抢走太多资源。

`storage_policy_temp_cleanup` 是强制删除存储策略后的兜底清理任务：当策略下仍有上传 session，且管理员用 `force=true` 删除策略时，服务端会先清理可立即处理的 session；如果还有临时对象或 multipart upload 需要等预签名 URL 自然失效后再删，就会创建这个任务延后处理。

`trash_purge_all` 是用户或团队清空回收站时创建的后台任务。它放在 fallback lane，是因为它主要做数据库批量遍历、文件物理清理和一次最终同步事件发布，不应该占用 archive 或 thumbnail 的专用并发预算。

`blob_maintenance` 是管理员发起的 blob 完整性检查、引用计数修复或孤儿 blob 清理任务。它放在 fallback lane，是因为它属于维护型任务，不应该占用 archive、thumbnail 或 storage migration 的专用并发预算。

`storage_policy_migration` 是管理员发起的跨策略 blob 迁移任务。它单独放进 StorageMigration lane，是因为它会长时间读取源驱动、写入目标驱动、更新 blob 引用，并且需要自己的恢复 checkpoint；如果混进 fallback lane，很容易和清理类任务互相拖慢。

### 为什么系统周期任务也记进同一张表

`runtime.rs` 会把值得留痕的系统周期任务结果记录到 `background_tasks` 表，但它们的 `kind` 是 `SystemRuntime`，不会再被 dispatcher 执行。空轮询会返回 `Quiet`，不会写表；连续健康的 `system-health-check` 成功结果会刷新最近一条成功记录，而不是每次新增一行。

这样做是为了保留统一观测面：

- 用户后台任务和系统任务都能在一个界面里看到
- 运维排障时不需要同时翻日志和另一套任务历史表
- 保留期清理逻辑可以复用

这不是把系统任务“伪装成普通任务”，而是借用同一套展示和留痕基础设施。

### 步骤和结果为什么存 JSON

任务步骤、输入负载、执行结果目前都序列化到 JSON 字段，而不是为每种任务建独立子表。

原因很现实：

- 任务种类还在增加
- 每类任务的步骤结构不同
- UI 需要的是有限的通用展示，而不是关系型深查询

因此当前设计更偏向“持久化状态机快照”，而不是“任务编排系统”。

只要任务种类继续保持有限、结果查询主要面向 UI，这个取舍是划算的。

## 4. 存储策略迁移任务

对应代码主要在：

- `src/api/routes/admin/storage_migrations.rs`
- `src/services/task/storage_migration.rs`
- `src/db/repository/storage_migration_checkpoint_repo.rs`
- `src/entities/storage_migration_checkpoint.rs`

这不是“把策略配置从 A 改到 B”，而是把 `file_blobs.policy_id = source_policy_id` 的实际对象内容迁到目标策略，并在数据库里把 blob 指向新的策略和存储路径。

### 为什么需要 dry-run

`POST /admin/storage-migrations/dry-run` 不创建任务，只做预检查和估算：

- 源策略下有多少 blob、总字节数是多少
- 多少 blob 是可按内容 SHA-256 合并判断的，多少是 opaque hash
- 目标策略里已经有多少匹配 hash
- 目标驱动是否支持 stream upload
- 目标驱动是否能完成一次写删探测

这一步解决的是“任务创建前就知道它大概会做什么、目标是否明显不可写”。它不提供最终一致性保证；真正执行时还会重新校验策略更新时间和目标驱动能力。

### 为什么有 checkpoint

迁移任务可能很长，不能只靠 `background_tasks` 一行记录表示进度。当前实现会在 `storage_migration_checkpoints` 表里为每个迁移任务写一条记录：

- `task_id` 绑定对应后台任务
- `source_policy_id` / `target_policy_id` 固定迁移方向
- `plan_hash` 固定创建时的策略版本和参数
- `stage` 表示准备、迁移、完成阶段
- `last_processed_blob_id` 和各类计数支持恢复后继续往后扫

任务失败或进程退出后，管理员通过 `/admin/storage-migrations/{task_id}/resume` 触发重试，dispatcher 重新认领任务后会基于 checkpoint 继续推进。

### 合并、跳过和失败

迁移单个 blob 时，服务会重新读取最新 blob 记录：

- blob 已经不在源策略下：记为 skipped
- 目标策略下已有相同内容 hash 的 blob：优先合并引用
- 否则从源驱动读取对象，用 stream upload 写入目标驱动，再更新数据库引用
- 单个 blob 失败会推进失败计数，并让任务按重试策略处理

第一版实现不支持 `delete_source_after_success = true`；这个字段已经在 API 里保留，但传 true 会被拒绝，避免文档或前端误以为会自动清理旧策略里的对象。

## 5. 管理员文件 / Blob 可观测

对应代码主要在：

- `src/api/routes/admin/files.rs`
- `src/services/admin.rs`
- `src/db/repository/file_repo/`

这组接口服务的是管理员排障和迁移前后检查，不是普通文件业务入口。

当前有两条查询线：

- 文件视角：`/admin/files` 和 `/admin/files/{id}`，展示文件记录、当前 blob、版本摘要
- Blob 视角：`/admin/file-blobs` 和 `/admin/file-blobs/{id}`，展示 blob 记录、hash 类型、引用计数、引用它的文件和版本

它们都走 reader 连接，只读，不会触发业务副作用。常见用途是：

- 查某个策略下到底还有哪些 blob
- 查某个 blob 被哪些文件或历史版本引用
- 在存储迁移前后确认 blob 的策略、路径、引用计数变化
- 排查内容 SHA-256 blob 和 opaque blob 在去重 / 迁移中的行为差异

这里的 `hash_kind` 是展示层派生值，不是数据库字段：64 位十六进制字符串视为 `content_sha256`，其他值视为 `opaque`。

## 6. `doctor` / 一致性审计

对应代码主要在：

- `src/cli/doctor.rs`
- `src/cli/doctor/execute.rs`
- `src/services/ops/integrity.rs`
- `src/storage/driver.rs`

### 设计目标

`doctor` 不是给日常业务路径调用的 service，而是一个偏运维的系统诊断入口。

它要解决的问题是：

- 部署环境是否可用
- 迁移是否完整
- 运行时配置是否能加载
- 存储和数据库之间是否出现了长期漂移

这类问题不适合塞进在线请求里，也不适合完全依赖后台任务默默修，因为运维往往需要一个“可以主动触发、可以拿到结构化报告”的入口。

### 分层设计

`doctor` 当前分成两层：

1. `src/cli/doctor.rs`
   负责参数解析、模式选择、报告聚合、人类可读输出和 JSON 输出
2. `src/services/ops/integrity.rs`
   负责真正的深度审计和部分修复逻辑

这种拆法的目的，是把“如何向用户展示检查结果”和“如何计算系统真实状态”分离开。

所以如果你想新增检查项：

- 纯展示和参数逻辑改 CLI 层
- 真正的数据库 / 存储审计逻辑改 integrity service

### 浅检查与深检查

`doctor` 的默认检查偏“环境可运行性”，例如：

- 数据库连接
- migration 状态
- SQLite 搜索加速能力
- runtime config 快照加载
- 公共站点 URL / 邮件 / 预览应用配置
- 存储策略基本可用性

`--deep` 才会进入一致性审计，当前包括：

- `storage_usage`
- `blob_ref_counts`
- `storage_objects`
- `folder_tree`

这样拆的原因是，深检查往往更慢，且可能触达对象存储，不应该成为每次简单巡检的默认成本。

### 为什么一致性审计单独做批量扫描

很多漂移问题无法靠单条业务请求即时发现，比如：

- `users.storage_used` 和真实文件占用不一致
- `file_blobs.ref_count` 和真实引用数不一致
- 存储里有孤儿对象，但数据库里已经没有记录
- 目录树出现跨工作空间父子关系或环

这些问题的共同特征是：

- 依赖全局视角
- 需要跨表聚合
- 不适合在在线请求里边写边校正

所以 integrity service 的实现整体偏“离线批量核对”，并且会分批扫描大表，避免一次性把全量记录都读进内存。

### `--fix` 的边界

当前 `doctor --deep --fix` 只会自动修两类可以确定性回写的漂移：

- `storage_used`
- `file_blobs.ref_count`

它不会自动修：

- 目录树结构损坏
- 存储里多余或缺失的对象
- 需要人工判断的跨范围问题

这是一个故意保守的边界。

能自动修的前提不是“理论上能改”，而是“修复动作足够确定，不会把原本还能人工排查的数据破坏掉”。

### 为什么对象扫描要走存储驱动抽象

`audit_storage_objects()` 不直接假定底层一定是本地文件系统或某种固定 S3 SDK，而是复用 `StorageDriver` 暴露出来的遍历能力。

这样做有两个好处：

- 审计逻辑不需要知道驱动细节
- 以后新增驱动时，只要实现同样的遍历接口，`doctor` 就能复用

也就是说，`doctor` 的对象审计依赖的是“驱动可枚举”这个能力，而不是某个具体存储后端的实现细节。

### 当前设计的取舍

`doctor` 并不试图做成实时自愈系统。

当前设计更像：

- 一个可以人工触发的结构化巡检入口
- 一个把“环境检查”和“数据漂移审计”聚合起来的运维界面后端
- 一个只对确定性问题做有限自动修复的工具

这意味着它的价值主要在于：

- 上线后首轮体检
- 存储 / 迁移 / 配额异常排查
- 清理任务或手工修复前的事实确认

而不是替代平时的监控、日志和报警。

## 7. 跨数据库迁移 CLI

对应代码主要在：

- `src/cli/database_migration.rs`
- `src/cli/database_migration/apply.rs`
- `src/cli/database_migration/checkpoint.rs`
- `src/cli/database_migration/schema.rs`
- `src/cli/database_migration/verify.rs`

### 设计目标

`database-migrate` 解决的是“把一个已经运行过的 AsterDrive 实例从一个数据库后端搬到另一个后端”的问题，例如 SQLite 迁到 PostgreSQL，或者 MySQL 迁到 PostgreSQL。

它不是在线业务请求，也不是 SeaORM migration 的替代品。它做的是：

- 连接源库和目标库
- 校验两端数据库后端和 migration 状态
- 在目标库准备 schema
- 按固定表顺序复制业务数据
- 维护断点续传检查点
- 复制后做数量、唯一约束、外键约束校验

### 为什么单独做 CLI

跨库迁移需要长时间持有外部连接、展示进度、处理中断恢复，还需要在真实业务停机窗口内运行。把它塞进 HTTP Admin API 会带来几个不必要的问题：

- HTTP 超时和反向代理限制会干扰长任务
- 失败恢复需要额外设计远程控制面
- 迁移期间本来就不适合让普通业务继续写入

所以当前实现直接做成离线 CLI：命令行负责用户交互和报告，迁移逻辑负责确定性复制与校验。

### 表复制顺序

迁移不是简单按字母序 dump 表。`COPY_TABLE_ORDER` 固定了复制顺序，先复制被依赖的基础表，再复制引用它们的业务表，例如：

- `managed_followers`
- `storage_policies`
- `storage_policy_groups`
- `storage_policy_group_items`
- `follower_enrollment_sessions`
- `users`
- `user_profiles`
- `auth_sessions`
- `passkeys`
- `mfa_factors`
- `mfa_recovery_codes`
- `mfa_login_flows`
- `mfa_email_codes`
- `mfa_totp_setup_flows`
- `teams`
- `team_members`
- `folders`
- `webdav_accounts`
- `file_blobs`
- `blob_media_metadata`
- `files`
- `file_versions`
- `shares`
- `upload_sessions`
- `upload_session_parts`
- `contact_verification_tokens`
- `external_auth_providers`
- `external_auth_identities`
- `external_auth_login_flows`
- `external_auth_email_verification_flows`
- `master_bindings`
- `remote_storage_targets`
- `system_config`
- `audit_logs`
- `mail_outbox`
- `background_tasks`
- `storage_migration_checkpoints`
- `entity_properties`
- `resource_locks`
- `wopi_sessions`

这个顺序必须和外键关系一起维护。新增表时，别只加 migration 和 entity，还要评估它是否应该进入 `COPY_TABLE_ORDER`，以及应该插在哪个位置。

### 断点续传模型

迁移检查点存在目标库的 `aster_cli_database_migrations` 表里。

这张表不是业务表，而是 CLI 自己的执行状态，用来记录：

- 当前迁移 key
- 当前阶段
- 正在复制的表
- 已复制到的游标 / 批次状态
- 整体执行状态

这样设计的原因很直接：跨库迁移可能因为网络、权限、磁盘、容器重启等原因中断。只要目标库还在，下一次运行可以基于检查点继续，而不是无脑从头拷。

### 模式选择

当前有三种运行模式：

- 默认 apply：准备目标 schema、复制数据、执行校验
- `--dry-run`：只做计划和预检，不写业务数据
- `--verify-only`：只校验已有目标库数据，不执行复制

另外可以用 `ASTER_CLI_PROGRESS` 控制进度输出，用 `ASTER_CLI_COPY_BATCH_SIZE` 调整复制批大小。测试里还保留了 `ASTER_CLI_FAIL_AFTER_BATCHES` 用于模拟中断恢复。

### 校验边界

复制后校验会关注：

- 源表和目标表行数是否一致
- 目标库唯一约束是否存在冲突
- 目标库外键约束是否存在违反
- 自增序列是否需要重置

它不会替你判断业务层面的“是否应该迁移某些历史数据”。这条链路的目标是忠实复制当前数据库状态，而不是做清洗或重构。

## 什么时候应该继续扩展本文

如果某个模块同时满足下面两条，就值得继续加到这份文档里：

- 它已经成为主链路或运维主链路的一部分
- 只看代码签名很难理解它的设计约束和取舍

当前下一批很可能值得补的，是这些方向：

- WebDAV 协议层与数据库锁系统
- WOPI session 与目标解析
- 团队空间模型与成员权限边界
- 运行时配置定义、快照和热更新链路
