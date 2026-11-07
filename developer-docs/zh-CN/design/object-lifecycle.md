# 存储对象发布与回收

内容 hash 是 `(hash, policy_id, backing)` 的逻辑去重身份。物理对象 key 表达某一代字节的所有权；创建、迁移和失败补偿不能把共享 hash 路径当作本次操作独占的对象。

## 发布

所有对象命名由 `aster_drive_storage::object_path` 负责。它统一 blob、上传会话、分片、流式/relay/copy/promotion 暂存以及图片派生对象的命名与分类；provider prefix 拼接仍由 `object_key` 负责，OS 临时根仍由 Forge 负责。原有重复工厂和转发入口已经移除。

正常去重命中先在文件发布事务内锁定当前 blob，检查 hash、policy、大小及清理状态，然后 CAS 增加引用。命中路径不触发 connector 发布或对象拷贝；若清理先获胜，则回退到事务外准备独占候选。

未命中的上传和迁移先生成随机物理 key，在数据库事务外写完并校验候选。Local 候选保留两级目录分散。事务通过唯一约束、源状态 CAS 和引用计数选择赢家；落败者只删除自己的未采用候选。数据库提交结果不确定时保留候选，不能猜测提交失败后删除可能已被采用的内容。

迁移的流式和分片路径都先验证本次候选的完整长度、摘要，再发布 metadata。相同 hash 的另一发布者先提交时，迁移重新验证已提交赢家并合并，不能覆盖或清理赢家对象。

## 清理

`ref_count = 0 -> -1` 是 writer 数据库中的原子清理认领。计数修复不把该标记恢复为可认领状态；物理删除失败也保留认领。过期认领经过锁内重新计数后，按既有十分钟窗口重试原物理代，续写尝试时间避免所有 worker 同时重试。

回收 metadata 的 CAS 同时比较记录 ID、`ref_count = -1` 与持久化物理路径。旧删除请求只能退休它认领的那一代，不能退休后来指向其他 key 的 metadata。

历史本地策略可能共用同一物理路径。清理使用 writer 数据库中的 `idx_file_blobs_storage_path` 查找其他 metadata 所有者，并解析真实本地路径。其他所有者即使引用数为零或已经认领清理，也足以阻止当前操作删除共享字节；当前操作仅退休自己的 metadata。超过 128 个候选或无法确定远端物理身份时保守保留对象，查询和分配保持有界。

并发清理历史别名可能保留无引用对象，不能以此为由重新开放旧代给发布者。诊断和维护必须把保留对象与已释放物理字节区分开；`retained_shared` outcome 不计物理释放字节。

## Provider 会话和团队归档

Provider-resumable 完成在同一 SQL CAS 中提交 Completed、file_id 并清除 `object_temp_key`，原子转移临时清理权。进度重放使用已提交的完成状态，不再需要上传侧 key。历史 Completed session 同样不能授权团队清理删除永久对象。

未完成 session 在归档清理前先进行 writer 状态 CAS；并发完成先成功时，清理不能继续使用旧快照。归档只清理仍由 session 持有的对象，文件和 blob 引用按正常 purge 路径退休。

## 引用计数修复

Doctor、自动 reconcile 和后台 blob 维护共用 `reconcile_blob_reference_count`。锁定 blob 后再读取文件与非当前 revision 的引用真相，并在同一短事务内修复。MySQL 的锁定读取先于一致性快照读取；SQLite 通过无值变化 UPDATE 先取得写锁。旧审计报告仅用于展示，不能作为之后 UPDATE 的真相。

全量 repair 仍按页批量读取观察值；只有疑似漂移进入逐 blob 的锁内复核，避免健康库变成逐条 COUNT。

## 配额修复与当前准入

配额报告同样只是观察值。`repair_storage_usage` 对疑似漂移的 owner 在短事务内锁定配额计数行，再统计当前文件和历史 revision 后更新；与上传发布共用 owner 锁，不能用旧报告覆盖并发提交。健康 owner 按页聚合，PostgreSQL/MySQL fixture 中 1 个和 64 个健康 owner 均为 5 条 SQL。

覆盖写保留已有 blob 的存储目的地，但在 provider I/O 前和最终发布事务中检查当前组的启用状态、大小、扩展名与类别准入。流式接收先执行当前上限再哈希／写入；准入从策略快照读取，不新增逐文件数据库查询。

## 性能和升级边界

慢速 I/O 不持有数据库写事务。哈希使用 64 KiB 缓冲，跨盘 promotion 使用固定 1 MiB 缓冲；候选和 staging 源在事务完成前保留以支持重试。去重命中的发布阶段在 SQLite fixture 中，1 KiB 与 1 MiB 样本均为 29 条 SQL、0 次 connector 调用、0 个拷贝字节；这不是生产延迟或 RSS 基准。

现存对象按持久化路径继续读取，不执行存量内容搬迁。升级必须协调所有 writer 切换到独占候选与新清理契约，防止旧 writer 再创建共享 hash 路径。真实云服务及生产升级验收需要单独验证。
