# 回收站 API

以下路径都相对于 `/api/v1`，且都需要认证。

## 一览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/trash` | 列出回收站内容 |
| `POST` | `/trash/{entity_type}/{id}/restore` | 恢复单个文件或文件夹 |
| `DELETE` | `/trash/{entity_type}/{id}` | 彻底删除单个文件或文件夹 |
| `DELETE` | `/trash` | 清空当前用户回收站 |

其中 `entity_type` 只能是 `file` 或 `folder`。

`GET /trash` 当前支持这些分页参数：

- `folder_limit` / `folder_offset`
- `file_limit`
- `file_after_expires_at` / `file_after_id`

返回体会带：

- `folders`
- `files`
- `folders_total`
- `files_total`
- `next_file_cursor`

也就是说，回收站和普通目录列表一样，文件夹用 offset 分页，文件用 cursor 分页。回收站条目返回 `expires_at`，表示该条目按当前 `trash_retention_days` 计算出的自动清理时间。

## 恢复与清理规则

- `GET /trash` 会返回当前用户回收站里的 `folders` 和 `files`
- 恢复时，如果原父目录已经不存在，资源会回到根目录
- 如果恢复的是文件夹，会递归恢复其已删除子项
- `DELETE /trash/{entity_type}/{id}` 是永久删除
- `DELETE /trash` 会创建 `trash_purge_all` 后台任务，并返回 `TaskInfo`；真正清空由后台 dispatcher 执行

个人及团队回收站在同一作用域存在 `pending`、`processing`、`retry` 或 `canceling` 的 `trash_purge_all` 任务时，文件还原、目录还原及大目录还原入队均返回 **HTTP 409**，错误码为 `trash.purge_in_progress`。前端提示“回收站正在清空，任务结束后才能还原，请稍后重试”。团队限制覆盖所有成员，个人与团队、不同用户或不同团队之间互不影响。

限制以 writer 数据库任务记录为准，任务 lease 过期或进程重启不会提前放行；`succeeded`、`failed`、`canceled` 后自动放行，手动重试重新生效。`canceling` 表示尚未停止，必须等到取消被确认。清空入队和还原提交共享数据库作用域锁，先完成还原的资源不会被随后入队的清空删除；大目录还原的 `202` 仅表示已入队，最终提交还会检查清空任务，冲突时任务失败且不提交还原。清空的每个破坏性事务校验当前 lease/token，旧 worker 无权在接管或终态后继续删除。

永久删除时，文件会处理 Blob 引用计数、缩略图、版本与配额回收；文件夹则会递归清掉整棵目录树。

还需要注意一条实现语义：

- 永久删除会先提交业务记录删除和 Blob `ref_count` 递减
- 存储对象与缩略图的物理清理在事务后执行
- 只有对象确认已经删除后，才会移除 `file_blobs` 元数据
- 如果存储层临时删除失败，Blob 元数据会保留，通常表现为 `ref_count = 0`，后续由后台 maintenance 任务重试清理

## 自动清理

除了手动清空或永久删除，系统还会根据 `trash_retention_days` 清理过期条目；后台周期由运行时配置 `maintenance_cleanup_interval_secs` 控制，并带有少量 jitter，避免多个维护任务同时打到数据库和存储。
