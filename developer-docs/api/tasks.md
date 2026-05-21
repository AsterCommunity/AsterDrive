# 后台任务 API

以下路径都相对于 `/api/v1`，且都需要认证。

这组接口负责“列出现有任务、查看详情、重试失败任务”。真正创建任务的入口分散在其他模块里。

## 个人空间

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/tasks` | 分页列出当前用户个人空间任务 |
| `GET` | `/tasks/{id}` | 读取单个个人空间任务 |
| `POST` | `/tasks/{id}/retry` | 重试失败的个人空间任务 |

## 团队空间

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/tasks` | 分页列出指定团队任务 |
| `GET` | `/teams/{team_id}/tasks/{id}` | 读取单个团队任务 |
| `POST` | `/teams/{team_id}/tasks/{id}/retry` | 重试失败的团队任务 |

## 谁会创建这些任务

当前最常见的创建入口有：

- `POST /batch/archive-compress`
- `POST /teams/{team_id}/batch/archive-compress`
- `POST /files/{id}/extract`
- `POST /teams/{team_id}/files/{id}/extract`
- `GET /files/{id}/archive-preview`
- `GET /teams/{team_id}/files/{id}/archive-preview`
- `GET /s/{token}/archive-preview`
- `GET /s/{token}/files/{file_id}/archive-preview`

另外，系统内部还会创建：

- `thumbnail_generate`
- `media_metadata_extract`
- `storage_policy_temp_cleanup`
- `system_runtime`

这些任务通常没有创建者，API 返回的 `creator` 为 `null`，普通用户 `/tasks` 列表通常看不到；管理员可以在 `/api/v1/admin/tasks` 看全部任务。

`storage_policy_temp_cleanup` 只在管理员用 `DELETE /admin/policies/{id}?force=true` 强制删除存储策略，且仍有临时对象或 multipart upload 需要延后清理时创建。它会先等待预签名 URL 的安全窗口过期，再按删除前保存的策略快照清理对象。

## 分页

列表接口都使用 offset 分页参数：

- `limit`
- `offset`

当前实现细节：

- 默认 `limit = 20`
- 实际上限受运行时配置 `task_list_max_limit` 控制，默认 `100`
- 个人接口只会返回创建者为当前用户且 `team_id = null` 的任务
- 团队接口只会返回 `team_id = {team_id}` 的任务

## `TaskInfo`

列表和详情都会返回 `TaskInfo`，当前主要字段包括：

- `id`
- `kind`
- `status`
- `display_name`
- `creator`
- `team_id`
- `share_id`
- `progress_current`
- `progress_total`
- `progress_percent`
- `status_text`
- `attempt_count`
- `max_attempts`
- `last_error`
- `payload`
- `result`
- `steps`
- `can_retry`
- `lease_expires_at`
- `started_at`
- `finished_at`
- `expires_at`
- `created_at`
- `updated_at`

其中：

- `creator` 是创建者用户摘要；系统运行任务和缩略图任务通常为 `null`
- `payload` / `result` 已经是结构化对象，不再是旧文档里说的 `payload_json` / `result_json`
- `steps` 会给出更细的阶段状态、阶段进度和阶段文案
- `can_retry = true` 目前只在 `status = failed` 且失败类型允许手动重试时出现
- `progress_total <= 0` 时，成功任务的 `progress_percent` 会直接视为 `100`
- `expires_at` 表示任务临时产物什么时候可以清理，不表示 `background_task` 历史记录一定会在这个时间删库

## 当前任务类型

当前代码里的 `BackgroundTaskKind` 有七种：

- `archive_extract`
- `archive_compress`
- `archive_preview_generate`
- `thumbnail_generate`
- `media_metadata_extract`
- `storage_policy_temp_cleanup`
- `system_runtime`

当前 `BackgroundTaskStatus` 有六种：

- `pending`
- `processing`
- `retry`
- `succeeded`
- `failed`
- `canceled`

对普通用户来说，最常见的是前两种：

- `archive_extract`：解压归档文件到工作空间目录
- `archive_compress`：把一组选中资源打包并写回工作空间
- `archive_preview_generate`：异步扫描 ZIP 文件并把只读 manifest 缓存在实体属性里
- `media_metadata_extract`：异步解析图片 / 音频 / 视频基础元数据并把结果按 blob 缓存；`media_metadata_enabled` 是总开关，具体图片 / 音频 / 视频处理器、后缀绑定和 `ffprobe` 命令由 `media_processing_registry_json` 控制，缺失时缓存为 `unsupported`
- `storage_policy_temp_cleanup`：强制删除存储策略后，兜底清理遗留的临时对象和 multipart upload

## `POST /tasks/{id}/retry`

这条接口和团队对应版本都只接受失败态任务：

- 只有 `status = failed` 才能重试
- 成功重试后，任务会被重置回待执行状态
- 当前实现会先清掉该任务旧的临时目录，再做重置

如果任务当前不是 `failed`，会返回 `400`。

## 当前实现现状

有两件事别搞混：

- `/batch/archive-download` 及团队对应接口走的是“短期 stream ticket + 直接 ZIP 流下载”，不会创建 `background_task`
- `/batch/archive-compress` 和 `/files/{id}/extract` 才会真正创建这里能看到的后台任务
- `/files/{id}/archive-preview` 和公开分享归档预览接口第一次命中未生成缓存时，会创建 `archive_preview_generate`；接口本身返回 `202`，前端应稍后重试原接口，而不是轮询任务详情作为唯一入口

所以如果你只用了下载 ticket 打包链路，任务列表为空是正常现象；如果你用了压缩 / 解压链路，列表就应该能看到对应任务。
