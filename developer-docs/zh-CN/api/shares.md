# 分享 API

分享接口分成两块：自己管理分享，以及公开访问分享内容。

以下路径都相对于 `/api/v1`。

## 自己的分享

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/shares` | 创建分享 |
| `GET` | `/shares` | 列出当前用户创建的分享 |
| `PATCH` | `/shares/{id}` | 编辑已有分享 |
| `DELETE` | `/shares/{id}` | 删除分享 |
| `POST` | `/shares/batch-delete` | 批量删除分享 |

创建请求示例：

```json
{
  "target": {
    "type": "file",
    "id": 1
  },
  "password": "123456",
  "expires_at": "2026-03-31T12:00:00Z",
  "max_downloads": 10
}
```

要点：

- `target.type` 只能是 `file` 或 `folder`，`target.id` 是对应资源 ID
- 当前请求体已经不再接受旧的顶层 `file_id` / `folder_id`
- 同一资源同一时间只允许一个活跃分享
- `max_downloads = 0` 表示不限次数
- 空密码等价于不设密码
- `GET /shares` 现在是分页接口，支持 `limit` 和 `offset`

编辑请求示例：

```json
{
  "password": "new-secret",
  "expires_at": "2026-04-02T12:00:00Z",
  "max_downloads": 5
}
```

编辑语义：

- `password` 不传：保留现有密码
- `password = ""`：移除密码
- `password = "xxx"`：替换为新密码
- `expires_at = null`：改为永不过期
- `max_downloads = 0`：改为不限次数

批量删除请求示例：

```json
{
  "share_ids": [1, 2, 3]
}
```

批量删除行为：

- 单次总项目数上限是 1000
- 每个 share 独立执行，不会因为一个失败而整批回滚
- 返回结果使用和其他 batch 接口一致的 `BatchResult` 结构

## 公开访问

下面这组 `/s/...` 仍然是“相对于 `/api/v1`”的 REST 路径，也就是完整地址实际是 `/api/v1/s/{token}/*`。
前端公开页面路由才是根路径 `/s/:token`。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/s/{token}` | 读取分享公开信息 |
| `POST` | `/s/{token}/verify` | 校验分享密码 |
| `POST` | `/s/{token}/preview-link` | 为分享文件生成短期预览链接 |
| `GET` | `/s/{token}/archive-preview` | 为分享文件获取归档预览清单 |
| `POST` | `/s/{token}/stream-session` | 为分享文件生成短期流式播放 session |
| `GET` | `/s/{token}/download` | 下载分享文件 |
| `POST` | `/s/{token}/archive-download` | 为分享范围内的文件 / 文件夹创建 ZIP 下载 ticket |
| `GET` | `/s/{token}/archive-download/{ticket}` | 消费 ticket 并流式下载 ZIP |
| `GET` | `/s/{token}/stream/{session_token}/{filename}` | 使用 stream session 流式读取分享文件 |
| `GET` | `/s/{token}/content` | 读取分享文件夹根层内容 |
| `GET` | `/s/{token}/folders/{folder_id}/content` | 浏览分享目录树中的子目录 |
| `GET` | `/s/{token}/folders/{folder_id}/ancestors` | 读取分享根目录以内的相对祖先链 |
| `GET` | `/s/{token}/files/{file_id}/download` | 下载分享文件夹中的子文件 |
| `POST` | `/s/{token}/files/{file_id}/preview-link` | 为分享目录树中的子文件生成短期预览链接 |
| `GET` | `/s/{token}/files/{file_id}/archive-preview` | 为分享目录树中的子文件获取归档预览清单 |
| `POST` | `/s/{token}/files/{file_id}/stream-session` | 为分享目录树中的子文件生成短期流式播放 session |
| `GET` | `/s/{token}/thumbnail` | 获取分享文件缩略图 |
| `GET` | `/s/{token}/image-preview` | 获取分享文件图片预览 WebP |
| `GET` | `/s/{token}/media-metadata` | 获取分享文件媒体元数据 |
| `GET` | `/s/{token}/files/{file_id}/thumbnail` | 获取分享目录树中子文件的缩略图 |
| `GET` | `/s/{token}/files/{file_id}/image-preview` | 获取分享目录树中子文件的图片预览 WebP |
| `GET` | `/s/{token}/files/{file_id}/media-metadata` | 获取分享目录树中子文件的媒体元数据 |
| `GET` | `/s/{token}/avatar/{size}` | 获取分享拥有者已上传头像 |

其中：

- `/verify` 成功后会写入 1 小时有效的 `aster_share_<token>` Cookie
- `/preview-link` 和 `/files/{file_id}/preview-link` 也会校验这枚 Cookie；受密码保护的分享必须先过 `/verify`
- `/download` 只适用于文件分享
- `/archive-download` 接收分享范围内的混合 `file_ids` / `folder_ids` 和可选 `archive_name`，返回短时效 `StreamTicketInfo`
- `/archive-download/{ticket}` 返回原始 ZIP 流，不走统一 JSON 包装，也不会创建普通 `/tasks` 后台任务
- `/preview-link` 只适用于文件分享；返回的 `PreviewLinkInfo.path` 最终指向根路径 `/pv/{token}/{filename}`
- `/archive-preview` 适用于支持的归档文件分享；缓存未生成时返回 `202` 并排队 `archive_preview_generate` 任务
- `/archive-preview` 和 `/files/{file_id}/archive-preview` 支持 `filename_encoding` query，取值与登录态文件接口一致：`auto`、`utf8`、`gb18030`、`cp437`、`cp850`、`shift_jis`、`big5`、`euc_kr`、`windows_1252`
- `/stream-session` 只适用于文件分享；返回的 `ShareStreamSessionInfo.path` 最终指向 `/api/v1/s/{token}/stream/{session_token}/{filename}`
- `/media-metadata` 只适用于文件分享；缓存未生成时返回 `202` 并排队 `media_metadata_extract` 任务
- `/image-preview` 只适用于服务端当前支持图片预览的文件分享；返回 WebP 原始响应，带 `ETag`
- `/content` 只返回文件夹分享的根目录内容
- `/folders/{folder_id}/content` 用于继续浏览分享目录树中的子目录
- `/folders/{folder_id}/ancestors` 只返回分享根目录以内的相对祖先链，不会把拥有者工作空间里的上级目录暴露给访客
- `/files/{file_id}/download` 用于下载分享文件夹树中的子文件
- `/files/{file_id}/preview-link` 用于分享目录树里子文件的短期预览
- `/files/{file_id}/archive-preview` 用于分享目录树里子归档文件的只读预览
- `/files/{file_id}/stream-session` 用于分享目录树里子文件的短期流式播放 session
- `/thumbnail` 只适用于服务端当前支持生成缩略图的文件分享
- `/files/{file_id}/thumbnail` 只适用于分享目录树中服务端当前支持生成缩略图的文件
- `/files/{file_id}/image-preview` 用于分享目录树里子图片文件的 WebP 预览
- `/files/{file_id}/media-metadata` 用于分享目录树里子文件的媒体元数据；缓存未生成时返回 `202` 并排队 `media_metadata_extract` 任务
- `/avatar/{size}` 只返回分享拥有者“已上传头像”的二进制资源，当前支持 `512` 和 `1024`

`POST /s/{token}/stream-session` 和 `POST /s/{token}/files/{file_id}/stream-session` 的成功响应示例：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "path": "/api/v1/s/share_token/stream/session_token/audio.mp3",
    "expires_at": "2026-04-12T15:00:00Z"
  }
}
```

当前实现细节：

- stream session 默认 3 小时过期，由运行时配置 `share_stream_session_ttl_secs` 控制；允许范围是 5 分钟到 24 小时
- `path` 可能是相对路径，也可能在配置了 `public_site_url` 后返回绝对 URL
- stream session 读取支持 `Range`，返回的是原始文件流，不走统一 JSON 包装
- 同一个 stream session 只会对分享下载计数做一次占用；如果响应构建失败，会尝试回滚计数
- 创建 session 和消费 session 都会校验分享密码 Cookie；消费流时会忽略下载次数上限的二次校验，避免 session 已发出后播放中途被重复拦截

分享归档下载请求示例：

```json
{
  "file_ids": [11, 12],
  "folder_ids": [21],
  "archive_name": "shared-selection.zip"
}
```

创建和消费 ticket 都受分享密码 Cookie、分享范围、下载次数和 `archive_download_share_enabled` 约束；关闭开关时返回 `archive_download.share_disabled`。ticket 会绑定到创建它的分享 token，不能换到另一个 `/s/{token}` 下消费。受密码保护的分享必须先调用 `/verify`，包括 ancestors 和归档下载接口。

文件夹分享的两个内容接口还支持和普通目录列表一致的参数：

- `folder_limit` / `folder_offset`
- `file_limit`
- `sort_by` / `sort_order`
- `file_after_value` / `file_after_id`

返回体同样会带 `next_file_cursor`。

当前边界直接记一句就够：

- 公开页已经支持在分享目录树内继续进入子文件夹浏览
- 子目录访问、子文件下载和子文件缩略图都会校验是否仍处在分享根目录范围内
- 子目录 ancestors 只会返回分享根目录以内的相对路径；越界访问返回 `403`
- 归档下载里的每个 file / folder 也必须位于分享范围内
- 子文件图片预览也会校验是否仍处在分享根目录范围内
- 子文件预览链接也会校验是否仍处在分享根目录范围内
- 子文件归档预览也会校验是否仍处在分享根目录范围内
- 子文件 stream session 也会校验是否仍处在分享根目录范围内
- 越过分享范围访问其他目录或文件会返回 `403`
- 如果拥有者当前头像来源是 `gravatar` 或 `none`，前端应直接使用 `GET /s/{token}` 返回的 `shared_by.avatar.url_*`

前端公开页路径是：

```text
/s/:token
```

分享归档预览和登录用户文件归档预览使用同一个 manifest 结构；区别是需要 `archive_preview_share_enabled = true`，并且公开响应使用 `Cache-Control: public, max-age=0, must-revalidate`。受密码保护的分享仍需先通过 `/verify` 写入分享 Cookie。
