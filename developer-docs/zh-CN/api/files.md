# 文件 API

以下路径都相对于 `/api/v1`，且都需要认证。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/files/upload` | 普通 multipart 直传 |
| `POST` | `/files/new` | 创建空文件 |
| `POST` | `/files/upload/init` | 协商上传模式 |
| `GET` | `/files/upload/sessions` | 列出当前用户可恢复的上传 session |
| `PUT` | `/files/upload/{upload_id}/{chunk_number}` | 上传单个分片 |
| `POST` | `/files/upload/{upload_id}/presign-parts` | 为 S3 multipart 上传批量申请分片 URL |
| `POST` | `/files/upload/{upload_id}/complete` | 组装分片或确认预签名上传 |
| `GET` | `/files/upload/{upload_id}` | 查询上传进度 |
| `DELETE` | `/files/upload/{upload_id}` | 取消上传 |
| `GET` | `/files/{id}` | 获取文件元信息 |
| `GET` | `/files/{id}/archive-preview` | 获取归档只读预览清单 |
| `GET` | `/files/{id}/direct-link` | 生成直接下载链接 token |
| `POST` | `/files/{id}/preview-link` | 生成短期预览链接 |
| `POST` | `/files/{id}/wopi/open` | 为指定 WOPI 预览器创建启动会话 |
| `GET` | `/files/{id}/download` | 下载文件内容 |
| `GET` | `/files/{id}/thumbnail` | 获取缩略图 |
| `GET` | `/files/{id}/image-preview` | 获取图片预览 WebP |
| `GET` | `/files/{id}/media-metadata` | 获取图片 / 音频 / 视频媒体元数据 |
| `PUT` | `/files/{id}/content` | 覆盖文件内容并写入版本历史 |
| `POST` | `/files/{id}/extract` | 把归档文件解包成后台任务 |
| `PATCH` | `/files/{id}` | 重命名或移动文件 |
| `DELETE` | `/files/{id}` | 软删除到回收站 |
| `POST` | `/files/{id}/lock` | 简化锁定 / 解锁 |
| `POST` | `/files/{id}/copy` | 复制文件 |
| `GET` | `/files/{id}/versions` | 列出历史版本 |
| `POST` | `/files/{id}/versions/{version_id}/restore` | 恢复某个版本 |
| `DELETE` | `/files/{id}/versions/{version_id}` | 删除某个版本 |

## 上传

上传的入口主要有两类：

- `POST /files/upload/init`：先协商模式
- `POST /files/upload`：直接走普通 multipart 上传
- `GET /files/upload/sessions`：刷新页面后恢复仍未完成的上传 session

这两条入口都支持目录上传语义：

- `POST /files/upload` 可通过 query 传 `folder_id`
- `POST /files/upload` 可通过 query 传 `relative_path`
- `POST /files/upload` 可通过 query 传 `declared_size`
- `POST /files/upload/init` 可在请求体里传 `relative_path`
- `POST /files/upload/init` 可在请求体里传 `frontend_client_id`
- `GET /files/upload/sessions` 可通过 query 传 `frontend_client_id`，只列出同一前端实例创建的可恢复 session
- `folder_id = null` 或不传时表示上传到根目录
- `declared_size` 是可选的客户端声明大小；当前前端普通 multipart 直传会带上它
- `frontend_client_id` 是前端实例 UUID，只用于断点续传列表过滤；用户 / 团队作用域仍然由登录态和路由决定
- 服务端会按相对路径自动创建缺失目录、复用已存在目录
- `relative_path` 中的空 segment 会被拒绝，例如 `docs//bad.txt`

协商接口会返回四种模式之一：

- `direct`：小文件直接上传
- `chunked`：大文件分片上传，可断点续传
- `presigned`：S3 单次预签名 `PUT`
- `presigned_multipart`：S3 multipart 直传，客户端需要再申请每个 part 的 URL

前端仍然只会看到这四种模式，不会额外出现一个 `relay_stream` 模式。S3 和 Remote 传输策略由存储策略控制：

- `options.s3_upload_strategy`：控制 S3 策略
- `options.remote_upload_strategy`：控制 remote follower 策略
- `relay_stream`：`init` 仍返回 `direct` / `chunked`，但服务端直接把字节流中继到 S3 / follower，不落本地临时文件
- `presigned`：`init` 才会返回 `presigned` / `presigned_multipart`

缺省时 S3 和 Remote 上传都会回退为 `relay_stream`。旧配置 `{"presigned_upload":true}` 仍兼容，等价于 `{"s3_upload_strategy":"presigned"}`；旧的 `{"s3_upload_strategy":"proxy_tempfile"}` 会回退为 `relay_stream`。使用预签名模式时，对象存储侧或 follower 内部存储接口还必须配置好浏览器可用的 CORS。Remote 预签名上传只适用于可直连的远端节点；如果远端节点解析为 reverse tunnel，服务端会拒绝 `remote_upload_strategy = "presigned"` 这类策略组合。

### 直传、分片和完成阶段

- `POST /files/upload`：普通 multipart 上传；空文件会报错，同目录同名文件不会覆盖。若命中的 S3 / Remote 策略是 `relay_stream`，这里会直接把请求体中继到对应驱动
- `POST /files/new`：创建一个 0 字节空文件，适合“新建文本文件”这类前端动作
- `GET /files/upload/sessions`：列出当前用户个人空间下未过期、状态为 `uploading` / `assembling` / `presigned` 的 session，按 `updated_at` 和 `upload_id` 倒序返回；传 `frontend_client_id` 时只返回同一前端实例创建的 session
- `PUT /files/upload/{upload_id}/{chunk_number}`：上传单个分片，`chunk_number` 从 `0` 开始
- `POST /files/upload/{upload_id}/presign-parts`：只用于 `presigned_multipart`，请求体里传 `part_numbers`
- `GET /files/upload/{upload_id}`：查询上传进度，也是前端断点续传依赖的接口；返回会带 `status`、`received_count`、`chunks_on_disk`、`chunk_size`、`total_chunks`、`filename`
- `POST /files/upload/{upload_id}/complete`：完成 `chunked`、`presigned` 或 `presigned_multipart` 上传

`GET /files/upload/sessions` 返回的是 `RecoverableUploadSessionResponse` 数组，主要字段包括：

- `upload_id`
- `mode`：`chunked`、`presigned` 或 `presigned_multipart`
- `status`
- `filename`
- `total_size`
- `chunk_size`
- `total_chunks`
- `received_count`
- `folder_id`
- `chunks_on_disk`
- `completed_parts`
- `expires_at`
- `updated_at`

其中 `completed_parts` 用于恢复 `relay_stream` multipart 或 `presigned_multipart` 已完成的 part 记录；普通本地 chunked 上传主要看 `chunks_on_disk`。

完成阶段的服务端行为分两类：

- 本地路径：会校验大小和配额；若 local 策略开启了 `content_dedup`，还会计算 SHA-256 并做 Blob 去重，否则直接创建独立 Blob
- 所有 S3 / Remote 路径（`relay_stream` / `presigned` / `presigned_multipart`）：都会校验大小和配额，但不会做 Blob 去重；最终会使用上传 session 派生的占位 hash 和 `files/{upload_id}` 风格的对象路径为每次上传创建独立 Blob；这些路径都不会回读对象计算 SHA-256

`POST /files/new` 创建空文件时也遵循同样规则：只有 local 显式开启 `content_dedup` 才会复用 0 字节 Blob，S3 始终创建独立 Blob。

`relay_stream` 的 multipart 场景下，服务端会把每个 part 的 `part_number + etag` 持久化到数据库；`complete` 时直接使用这些服务端记录完成 S3 / Remote multipart，不依赖客户端再回传 `parts`。

对 `presigned_multipart` 来说，`complete` 请求体需要带对象存储返回的 `parts` 列表；其他模式可以不带请求体。

## 文件操作

- `GET /files/{id}`：读取文件元信息；已进回收站的文件会按“找不到”处理
- `GET /files/{id}/archive-preview`：读取归档预览清单；缓存未生成时返回 `202` 并排队 `archive_preview_generate` 任务
- `GET /files/{id}/direct-link`：返回一个短 token；真正下载走根路径 `/d/{token}/{filename}`。默认按 inline 流式返回；追加 `?download=1` 后复用附件下载分流，命中 S3 / Remote 的 `presigned` 策略时会返回 `302`
- `POST /files/{id}/preview-link`：返回一个短期预览链接；真正读取内容走根路径 `/pv/{token}/{filename}`
- `POST /files/{id}/wopi/open`：为配置成 `provider = "wopi"` 的预览器创建一次 WOPI 启动会话
- `GET /files/{id}/download`：下载文件；默认是流式响应，若命中的 S3 / Remote 策略把下载策略设为 `presigned`，则会在鉴权后返回 `302` 重定向到短时效的对象存储 GET URL；支持 `If-None-Match`，命中时返回 `304`
- `GET /files/{id}/thumbnail`：读取缩略图（仅服务端当前支持的类型）；若后台仍在生成，会先返回 `202` 和 `Retry-After`
- `GET /files/{id}/image-preview`：为图片预览返回 WebP 原始响应，不走统一 JSON 包装；成功响应带 `ETag`，支持 `If-None-Match` 命中返回 `304`
- `GET /files/{id}/media-metadata`：读取按 blob 缓存的媒体元数据；缓存未生成时返回 `202` 和 `Retry-After`
- `PUT /files/{id}/content`：覆盖已有文件内容，是当前编辑现有文件的核心接口
- `POST /files/{id}/extract`：把 .zip 文件解包成后台任务，结果会出现在 `/tasks`
- `PATCH /files/{id}`：改名或移动
- `DELETE /files/{id}`：软删除到回收站

`FileInfo` / 文件列表条目现在还会带文件分类字段：

- `extension`：小写最终扩展名，不带点；无扩展名时为空字符串
- `compound_extension`：小写复合扩展名，例如 `tar.gz`；只有命中受支持复合扩展时才有值
- `file_category`：`image`、`video`、`audio`、`document`、`spreadsheet`、`presentation`、`archive`、`code`、`other`

这些字段会在创建、上传、覆盖写入和重命名时由服务端重新分类；搜索过滤直接依赖这些持久化字段。

`GET /files/{id}` 和团队空间的 `GET /teams/{team_id}/files/{id}` 详情响应还会带 `storage_used`。这个字段是文件详情页使用的配额口径：当前 `size` 加上所有历史版本大小。目录列表里的文件条目不会返回这个字段。

其中 `PUT /files/{id}/content` 支持 `If-Match`，会检查锁状态，成功后自动生成历史版本，并返回新的 `ETag`。

### `PATCH /files/{id}`

请求体：

```json
{
  "name": "renamed.pdf",
  "folder_id": 5
}
```

当前实现支持：

- 改名
- 移动到其他文件夹
- `folder_id = null` 时移回根目录

当前限制：

- 目标位置同名冲突会报错
- 被锁定文件不能修改

### `DELETE /files/{id}`

这是软删除，文件会进入回收站，而不是立刻删物理内容。

## 缩略图

当前缩略图能力主要来自运行时的 media processing registry，并由 `/public/thumbnail-support` 暴露给匿名态前端。默认内置 `images` 处理器覆盖常见图片格式；内置 `lofty` 处理器启用音频封面用途时也会暴露音频后缀；如果启用且运行环境可找到 `vips_cli` / `ffmpeg_cli`，缩略图支持列表也会包含对应配置里的扩展名。

存储策略也可以通过 `storage_native_processing_enabled = true`、`thumbnail_processor = "storage_native"` 和 `thumbnail_extensions` 暴露策略级原生缩略图 / 图片预览能力。内置 `tencent_cos` 策略可通过 COS CI 暴露这项能力；内置 Local、S3-compatible 和 Remote 策略不暴露原生缩略图或图片预览能力。

接口统一返回 WebP，并按 Blob、processor、processor version 和实际最大边长复用缓存。源文件大小上限由 `thumbnail_max_source_bytes` 控制；生成后最长边由运行时配置 `thumbnail_max_dimension` 控制。

默认缩略图尺寸继续使用旧的缓存 version namespace；非默认尺寸会在 derivative version 后追加 `-d{dimension}`。这样调整运行时尺寸后，不会覆盖或误用另一种尺寸的缓存结果。

## 图片预览

`GET /files/{id}/image-preview` 和团队空间的 `GET /teams/{team_id}/files/{id}/image-preview` 直接返回 WebP 图片数据，用于文件预览面板展示大图。它和缩略图接口不是同一个缓存尺寸：

- 缩略图面向文件列表和卡片，可能异步生成并返回 `202`
- 图片预览面向预览器；命中缓存时返回 WebP，缓存未命中时创建或复用 `image_preview_generate` 后台任务，并返回 `202` 和 `Retry-After`
- 图片预览生成后最长边由运行时配置 `image_preview_max_dimension` 控制
- 成功响应是 `image/webp`，带 `ETag` 和 `Cache-Control: private, max-age=0, must-revalidate`
- 不支持的文件类型会返回文件/缩略图分域错误，不会退回原始文件流
- 支持能力来自 `/public/thumbnail-support` 暴露的图片缩略图 / 图片预览能力并集，可来自后端媒体处理器，也可来自启用策略级原生处理的 Tencent COS
- 前端默认优先使用原图还是预览图由 `/public/frontend-config` 里的 `media.image_preview_preference` 控制

图片预览缓存使用和缩略图相同的尺寸感知 derivative version 规则。

## 媒体元数据

`GET /files/{id}/media-metadata` 返回按 Blob 缓存的媒体元数据；团队空间对应接口是 `GET /teams/{team_id}/files/{id}/media-metadata`。缓存未生成时接口返回 `202` 和 `Retry-After`，后台会创建 `media_metadata_extract` 任务，前端稍后重试同一接口即可。

当前图片元数据由内置 `images` 处理器读取尺寸和格式；音频元数据由内置 `lofty` 处理器读取标题、艺术家、专辑、时长、采样率、声道、码率、曲目号和内嵌封面存在性等信息；视频元数据由 `ffprobe_cli` 处理器通过服务端 `ffprobe` 读取时长、尺寸、编码、容器和帧率。`media_metadata_enabled` 是总开关；图片 / 音频 / 视频是否参与解析、命中的后缀，以及 `ffprobe` 的命令名或绝对路径，都统一在 `media_processing_registry_json` 里配置。若运行环境找不到配置的 `ffprobe`，视频会返回并缓存为 `unsupported`；配置修正且命令可用后，旧的 missing-probe unsupported 缓存会被重新探测。

存储策略可以通过 `storage_native_processing_enabled = true`、`storage_native_media_metadata_enabled = true` 和 `media_metadata_extensions` 启用策略级原生媒体元数据。内置 `tencent_cos` 策略可通过 COS CI 暴露音频 / 视频元数据；内置 Local、S3-compatible 和 Remote 策略不暴露原生媒体元数据能力。

音频内嵌封面不单独开音乐封面缓存。`lofty` 处理器具备 `thumbnail:audio` 用途时，客户端继续复用现有 thumbnail 路径获取封面图；响应里的 `has_embedded_picture` 和 MIME 用于播放器元数据展示和兜底判断。

### `GET /files/{id}/archive-preview`

这条接口为支持的归档文件返回只读清单，不解压、不写入工作空间：

可选查询参数 `filename_encoding` 控制 ZIP entry name 的解码方式：
`auto`（默认）、`utf8`、`gb18030`、`cp437`、`cp850`、`shift_jis`、
`big5`、`euc_kr`、`windows_1252`。例如：
`GET /files/{id}/archive-preview?filename_encoding=gb18030`。显式设置后会覆盖
自动检测；服务端会复用同一份 ZIP 原始清单缓存，并按请求编码重建显示清单。
这个参数只改变文件名解码行为，不改变其他限制或生成语义；设置为 `cp437`、
`cp850`、`gb18030`、`shift_jis`、`big5`、`euc_kr` 或 `windows_1252`
时会强制走对应兼容解码路径。这些兼容编码只在显式选择时使用，默认 `auto`
不会无限尝试所有编码。

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "schema_version": 2,
    "format": "zip",
    "source_blob_id": 42,
    "source_hash": "abc...",
    "generated_at": "2026-05-18T12:00:00Z",
    "entry_count": 2,
    "file_count": 1,
    "directory_count": 1,
    "total_uncompressed_size": 128,
    "truncated": false,
    "entries": [
      {
        "path": "docs/readme.txt",
        "name": "readme.txt",
        "parent": "docs",
        "kind": "file",
        "size": 128,
        "compressed_size": 64,
        "modified_at": "2026-05-18T12:00:00Z"
      }
    ]
  }
}
```

当前实现细节：

- 目前支持 `.zip` 以及对应 MIME 类型；其他格式返回带 `archive_preview.unsupported_type` 子码的 `400`
- 默认关闭，需要同时打开 `archive_preview_enabled` 和 `archive_preview_user_enabled`
- 首次请求如果没有可用缓存，会创建或复用 `archive_preview_generate` 后台任务，返回 `202`、`Retry-After: 2` 和空成功响应
- 任务完成后，原始清单缓存在 `entity_properties` 的 `system.archive_preview / zip_raw_manifest.v2`
- 成功响应带 `ETag`，支持 `If-None-Match` 命中返回 `304`
- 限制由 `archive_preview_max_source_bytes`、`archive_preview_max_entries`、`archive_preview_max_manifest_bytes`、`archive_preview_max_duration_secs` 以及归档解压相关上限共同控制
- 对支持 Range 的存储驱动，生成任务会优先用范围读取扫描归档元数据；必要时才下载到临时文件扫描

### `GET /files/{id}/direct-link`

这个接口只返回：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "token": "..."
  }
}
```

拿到 token 后，实际下载地址是：

```text
/d/{token}/{filename}
```

其中：

- `filename` 必须和当前文件名匹配；URL 编码后的同名也可以
- 这个下载入口不走 `/api/v1`，返回原始文件流，不是 JSON
- 不带 `?download=1` 时按 inline 处理，仍由 AsterDrive 服务端流式返回
- 追加 `?download=1` 可以强制走附件下载；这条路径会复用普通下载的附件分流逻辑，命中 S3 / Remote 的 `presigned` 策略时返回 `302`

### `POST /files/{id}/preview-link`

这个接口会返回：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "path": "/pv/...",
    "expires_at": "2026-04-09T12:00:00Z",
    "max_uses": 5
  }
}
```

要点：

- `path` 是实际预览入口；如果配置了 `public_site_url`，这里可能已经是完整绝对 URL
- 真实预览内容不走 `/api/v1`，而是走根路径 `/pv/{token}/{filename}`
- 当前预览链接默认 5 分钟过期，且最多使用 5 次
- 这个能力主要给内联预览器、Office 在线预览桥接和只读浏览场景使用，不等价于长期分享链接

### `POST /files/{id}/wopi/open`

请求体很简单：

```json
{
  "app_key": "custom.onlyoffice"
}
```

成功后返回 `WopiLaunchSession`：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "access_token": "...",
    "access_token_ttl": 1775995200000,
    "action_url": "https://office.example.com/...&WOPISrc=https%3A%2F%2Fdrive.example.com%2Fapi%2Fv1%2Fwopi%2Ffiles%2F1",
    "form_fields": {},
    "mode": "iframe"
  }
}
```

要点：

- `app_key` 必须指向 `/public/preview-apps` 里当前启用、且 `provider = "wopi"` 的预览器
- `action_url` 会带上实际回调入口 `WOPISrc`
- 当前实现里的 `access_token_ttl` 按 WOPI 规范返回“过期时间的 Unix 毫秒时间戳”，不是“剩余多少秒”
- 生成 WOPI 启动会话时要求系统已配置 `public_site_url`
- 真实 WOPI 回调不走这条路径，而是走 `/api/v1/wopi/files/{id}` 及其 `/contents` 变体，详细见 [WOPI](./wopi.md)

### `POST /files/{id}/extract`

请求体：

```json
{
  "target_folder_id": 12,
  "output_folder_name": "docs-unpacked",
  "filename_encoding": "auto"
}
```

要点：

- 这条接口不会同步返回解包结果，而是创建一个 `archive_extract` 后台任务
- 当前支持 `.zip` 文件名的源文件
- `target_folder_id = null` 时，解包结果会写到源压缩包所在目录；如果源压缩包在根目录，就写到根目录
- `output_folder_name` 不传时，服务端会为解包结果推导输出目录名
- `filename_encoding` 可选，默认 `auto`；支持值和 `GET /files/{id}/archive-preview` 的同名 query 参数一致，用于兼容非 UTF-8 ZIP entry name
- 真正的解包进度、失败原因和最终输出目录信息，要去 [`后台任务 API`](./tasks.md) 里看对应 `TaskInfo`

## 锁与复制

### `POST /files/{id}/lock`

这是简化的 REST 锁接口：`locked = true` 表示加锁，`locked = false` 表示解锁。底层真实锁记录仍保存在 `resource_locks`。

### `POST /files/{id}/copy`

复制文件不会物理复制 Blob，只增加引用计数；目标目录同名时会自动生成副本名。`folder_id = null` 表示复制到根目录。

## 版本历史

历史版本主要来自覆盖写入，例如：

- `PUT /files/{id}/content`
- WebDAV `PUT` 覆盖已有文件

对应接口：

- `GET /files/{id}/versions`
- `POST /files/{id}/versions/{version_id}/restore`
- `DELETE /files/{id}/versions/{version_id}`

当前语义要记住一条：恢复版本不会额外生成一条“回滚前版本”，被恢复的版本记录会直接消失，因为它已经重新变成当前版本。
