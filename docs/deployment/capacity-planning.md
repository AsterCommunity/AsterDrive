---
description: AsterDrive 容量规划参考，按文件数量、数据库大小、对象存储、内存、临时磁盘和后台任务并发估算生产部署资源。
---

# 容量规划参考

::: tip 先别找万能数字
这一页给的是规划口径和保守估算，不是所有机器都通用的上限。

AsterDrive 的真实容量取决于数据库后端、存储策略、文件名长度、版本数量、搜索索引、后台任务、上传模式和并发访问。正式上线前，仍然要结合 [性能基准与压测](/deployment/performance-benchmarking) 在自己的环境里跑一轮。
:::

## 先分清三类容量

| 容量类型 | 主要增长来源 | 放在哪里 | 规划重点 |
| --- | --- | --- | --- |
| 元数据容量 | 用户、团队、文件夹、文件记录、Blob、版本、分享、任务、审计、搜索索引 | 数据库 | 文件数量、版本数量、索引和历史记录保留时间 |
| 对象数据容量 | 文件内容、历史版本内容、缩略图、预览缓存、头像 | 本地磁盘、S3 / MinIO、远程 follower | 实际文件字节数、版本保留、回收站保留、对象存储冗余 |
| 运行时容量 | 上传缓冲、临时分片、归档 staging、后台任务、数据库连接、缓存 | 进程内存和临时目录 | 并发上传、后台任务并发、单文件大小、临时目录剩余空间 |

数据库不是文件内容仓库。一个 10 MiB 文件和一个 10 GiB 文件在数据库里的记录大小差别通常很小；真正影响数据库大小的是“有多少条记录”和“这些记录保留多久”。

## 文件数量怎么放大元数据

一个普通文件通常会带来：

- 1 条 `files` 记录
- 1 条或复用 1 条 `file_blobs` 记录
- 可能有 0 到多条 `file_versions` 记录
- 可能有分享、锁、属性、缩略图、媒体元数据、后台任务和审计记录
- 在 SQLite 下，还可能进入文件名 / 文件夹名搜索加速索引

一个文件夹通常会带来：

- 1 条 `folders` 记录
- 可能有分享、锁、属性和审计记录
- 在 SQLite 下，也可能进入搜索加速索引

按生产规划保守估算，可以先用这组口径：

| 项目 | 低变更、少版本 | 有搜索、分享、任务和少量版本 | 版本 / 审计 / 属性较重 |
| --- | --- | --- | --- |
| 每个文件或文件夹的数据库预算 | `2-4 KiB` | `4-8 KiB` | `8-20 KiB+` |
| 10 万文件/文件夹 | `200-400 MiB` | `400-800 MiB` | `800 MiB-2 GiB+` |
| 100 万文件/文件夹 | `2-4 GiB` | `4-8 GiB` | `8-20 GiB+` |

这张表故意偏保守。文件名很短、没有版本、没有审计长期保留时，实际可能更小；如果你长期保留版本、分享、审计日志和后台任务历史，实际也可能更大。

## 数据库后端选择

| 规模 / 使用方式 | 建议 |
| --- | --- |
| 个人、NAS、小团队，文件数量在几万到十几万级，写入并发不高 | SQLite 可以先用，备份和迁移最简单 |
| 文件数量进入几十万级，目录列表、搜索、后台任务开始频繁 | 继续用 SQLite 前先跑压测；同时准备迁移 PostgreSQL / MySQL 的窗口 |
| 文件数量预期接近或超过百万级，多用户持续上传、搜索、WebDAV、后台任务并发明显 | 优先 PostgreSQL 或 MySQL |
| 需要数据库托管备份、读写隔离、长期审计、较复杂运维观测 | 优先 PostgreSQL 或 MySQL |

SQLite 不是“不能用”，但它更适合单机和轻并发。只要你已经知道未来会有大量文件、很多用户同时操作、或者需要长期保留版本和审计，别等数据库已经很大再迁移，猫猫，迁移窗口不是许愿池。

## 对象存储容量

对象数据容量不要按“当前文件列表大小”估，要按生命周期估：

```text
对象数据预算 =
  当前可见文件内容
  + 回收站保留期内的文件内容
  + 历史版本内容
  + 缩略图 / 预览 / 媒体处理缓存
  + 未完成上传和临时对象
  + 存储后端自己的冗余、版本化或复制开销
```

几个常见放大点：

- 回收站保留时间越长，删除后的对象越晚释放。
- 文件版本越多，编辑类工作流越容易把对象数据放大。
- S3 / MinIO 如果启用 bucket versioning，AsterDrive 删除对象后，底层桶也可能继续保留旧版本。
- 远程 follower 的容量要看 follower 真实落点，不能只看 primary 的磁盘。
- 存储迁移期间，源策略和目标策略可能同时持有对象，短时间内要预留接近双份容量。

建议本地存储至少预留：

- 日常增长量的 `30` 天缓冲
- 一次最大存储迁移或批量导入所需的临时空间
- `data/.tmp` 和 `data/.uploads` 的上传临时空间
- 文件系统自身保留空间，尤其是 ext4 / XFS / ZFS / Btrfs 快照场景

## 内存容量

AsterDrive 的常规请求路径以流式处理为主，内存不会按文件总大小线性增长。真正容易把内存和临时空间顶上去的是：

- 并发 direct / chunked 上传
- 大文件下载和 WebDAV 客户端重试
- 缩略图、图片预览和媒体元数据任务
- 压缩包预览、压缩、解压
- 离线下载
- 大目录列表、搜索和管理员统计
- 数据库连接池和对象存储 HTTP 连接

保守起步建议：

| 部署类型 | 建议内存 | 说明 |
| --- | --- | --- |
| 个人测试、小 NAS | `512 MiB-1 GiB` | 功能能跑，但不适合开很多后台任务并发 |
| 家庭 / 小团队 | `1-2 GiB` | 更适合 WebDAV、缩略图和少量并发上传 |
| 多用户、S3、远程节点或频繁后台处理 | `4 GiB+` | 给数据库、缓存、归档任务和对象存储连接留余量 |
| 大量文件、长期压测或生产实例 | 按监控结果扩容 | 看 RSS、p95、任务积压和数据库慢查询，不要只看空闲内存 |

默认后台任务并发比较克制：

| 配置 | 默认值 | 容量影响 |
| --- | --- | --- |
| `background_task_max_concurrency` | `1` | fallback lane 通用任务并发 |
| `background_task_archive_max_concurrency` | `2` | 压缩、解压、归档预览更吃 CPU、内存和临时磁盘 |
| `background_task_thumbnail_max_concurrency` | `1` | 图片 / 媒体处理并发 |
| `background_task_storage_migration_max_concurrency` | `1` | 存储迁移并发，主要吃对象存储吞吐 |
| `offline_download_max_concurrency` | `1` | 离线下载并发，吃网络和临时空间 |

不要只因为 CPU 还有空闲就盲目调高这些值。归档和媒体任务会吃临时目录，存储迁移会吃源/目标存储吞吐，离线下载会吃网络和磁盘。

## 临时磁盘容量

临时目录通常包括：

- `data/.uploads`：chunked 上传分片、组装前临时状态
- `data/.tmp`：归档、预览、转换、下载等运行时临时文件
- 离线下载工具自己的临时目录
- follower 受管 ingress profile 对应目录

默认限制里有几个要记住：

| 项目 | 默认值 | 规划含义 |
| --- | --- | --- |
| `thumbnail_max_source_bytes` | `64 MiB` | 超过这个大小的源文件默认不生成缩略图 |
| `thumbnail_max_dimension` | `400 px` | 生成缩略图时，默认会把最大边长限制在这个尺寸以内 |
| `image_preview_max_dimension` | `1600 px` | 生成图片预览图时，默认会把最大边长限制在这个尺寸以内 |
| `media_metadata_max_source_bytes` | `256 MiB` | 超过这个大小的媒体默认不提取元数据 |
| `archive_extract_max_source_bytes` | `512 MiB` | 单个解压源文件默认上限 |
| `archive_extract_max_staging_bytes` | `2 GiB` | 单个解压任务 staging 默认上限 |
| `archive_extract_max_uncompressed_bytes` | `1 GiB` | 单个解压任务默认未压缩总量上限 |
| `archive_build_max_temp_bytes` | `2 GiB` | 单个压缩任务默认临时空间预算 |
| `offline_download_max_file_size_bytes` | `1 GiB` | 单个离线下载文件默认上限 |

临时磁盘估算可以先按：

```text
临时空间预算 =
  同时进行的 chunked 上传总大小
  + 同时运行的归档 staging 上限
  + 同时运行的离线下载上限
  + 预览 / 缩略图 / 媒体处理临时文件
  + 20%-30% 安全余量
```

如果用户经常上传大文件，优先让大文件走对象存储 presigned / multipart，减少 primary 本地临时目录压力。

## 版本、回收站和审计保留

容量增长最容易被低估的是“已经看不见但还没清掉”的数据：

- 回收站里的文件仍然占对象存储容量。
- 历史版本可能继续引用旧 Blob。
- 审计日志、后台任务历史、分享下载计数和系统运行记录会持续写数据库。
- 未完成上传会在过期前占用临时空间或对象存储 multipart 状态。

上线后建议检查这些运行时配置：

- `trash_retention_days`
- `max_versions_per_file`
- `task_retention_hours`
- `audit_retention_days`
- `maintenance_cleanup_interval_secs`
- `blob_reconcile_interval_secs`

如果你要长期保留审计日志，数据库预算要按审计写入量单独算，不要混在文件数量估算里。

## 观测与估算命令

### 统计元数据行数

下面的 SQL 可以作为第一轮容量盘点：

```sql
SELECT 'users' AS table_name, COUNT(*) AS rows FROM users
UNION ALL SELECT 'teams', COUNT(*) FROM teams
UNION ALL SELECT 'folders', COUNT(*) FROM folders
UNION ALL SELECT 'files', COUNT(*) FROM files
UNION ALL SELECT 'file_blobs', COUNT(*) FROM file_blobs
UNION ALL SELECT 'file_versions', COUNT(*) FROM file_versions
UNION ALL SELECT 'shares', COUNT(*) FROM shares
UNION ALL SELECT 'upload_sessions', COUNT(*) FROM upload_sessions
UNION ALL SELECT 'background_tasks', COUNT(*) FROM background_tasks
UNION ALL SELECT 'audit_logs', COUNT(*) FROM audit_logs;
```

如果某张表在你的版本里还不存在，删掉对应行再跑。别在生产高峰期拿复杂统计反复扫大表，真要查就挑低峰。

### 查看数据库大小

SQLite：

```bash
du -h data/asterdrive.db data/asterdrive.db-wal 2>/dev/null
sqlite3 data/asterdrive.db "PRAGMA page_count; PRAGMA page_size;"
```

PostgreSQL：

```sql
SELECT pg_size_pretty(pg_database_size(current_database())) AS database_size;

SELECT
  relname,
  pg_size_pretty(pg_total_relation_size(relid)) AS total_size
FROM pg_catalog.pg_statio_user_tables
ORDER BY pg_total_relation_size(relid) DESC
LIMIT 20;
```

MySQL：

```sql
SELECT
  table_name,
  ROUND((data_length + index_length) / 1024 / 1024, 2) AS size_mb
FROM information_schema.tables
WHERE table_schema = DATABASE()
ORDER BY data_length + index_length DESC
LIMIT 20;
```

### 看对象和配额一致性

先跑普通检查：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc"
```

需要检查对象目录、Blob 引用和真实存储占用时跑深度检查：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep
```

`--deep` 会更慢，尤其是对象很多时，不要在业务高峰期随手跑。

## 扩容触发点

看到这些信号时，就该扩容或调整策略：

| 信号 | 处理 |
| --- | --- |
| 数据库文件持续增长，文件数量进入几十万级 | 评估 PostgreSQL / MySQL，缩短任务和审计保留 |
| 目录列表或搜索 p95 持续抬升 | 跑压测，检查索引、SQLite FTS、数据库慢查询 |
| `background_tasks_pending` 长期堆积 | 增加对应 lane 并发，或降低任务产生速度；同时确认内存和临时空间 |
| `process_memory_rss_bytes` 单向增长且空闲不回落 | 跑长稳测试，保留日志和监控样本，排查泄漏或过高并发 |
| `data/.uploads` 或 `data/.tmp` 经常逼近磁盘上限 | 降低并发、清理失败上传、改用对象存储 multipart、扩大临时目录 |
| 本地存储剩余空间低于 20%-30% | 扩容、迁移策略、缩短回收站 / 版本保留，或接入 S3 / follower |
| 存储迁移 dry-run 提示容量不足 | 不要硬跑；先扩目标容量或分批迁移 |

## 一套实用起步配置

如果你只是要一个初始判断，可以按下面走：

1. **单人 / 家庭**：SQLite + 本地存储，`1 GiB` 内存起步，定期备份整个 `data/`。
2. **小团队**：SQLite 仍可起步，但文件数量接近几十万前先跑压测；内存给到 `2 GiB` 更稳。
3. **团队持续使用或文件数预期百万级**：从一开始就用 PostgreSQL / MySQL，存储走本地大盘、S3 或 follower，内存至少 `4 GiB` 起步。
4. **频繁归档、预览、离线下载、存储迁移**：先按后台任务和临时目录容量规划，再决定并发，不要反过来。

最后，容量规划不是一次性文档。上线后至少每月看一次数据库大小、对象存储增长、RSS、后台任务积压和备份恢复演练结果。文件系统这种东西，最怕的不是变大，是你不知道它怎么变大的。
