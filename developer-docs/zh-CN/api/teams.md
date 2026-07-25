# 团队与团队空间 API

以下路径都相对于 `/api/v1`，且都需要认证。

团队相关能力分成两层：

- 团队自身：团队资料、成员、团队审计
- 团队工作空间：文件、文件夹、上传、搜索、标签、分享、WebDAV 账号、回收站、后台任务

## 团队自身

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/teams` | 列出当前用户可见的团队 |
| `POST` | `/teams` | 创建团队 |
| `GET` | `/teams/{id}` | 读取团队详情 |
| `PATCH` | `/teams/{id}` | 更新团队名称或描述 |
| `DELETE` | `/teams/{id}` |归档团队 |
| `POST` | `/teams/{id}/restore` | 恢复已归档团队 |
| `GET` | `/teams/{id}/audit-logs` | 查看团队审计记录 |
| `GET` | `/teams/{id}/members` | 分页查看团队成员 |
| `POST` | `/teams/{id}/members` | 添加团队成员 |
| `PATCH` | `/teams/{id}/members/{member_user_id}` | 调整成员角色 |
| `DELETE` | `/teams/{id}/members/{member_user_id}` | 移除成员 |

当前实现要点：

- `GET /teams` 支持 `archived=true`，用来查看已归档团队
- `POST /teams` 目前仍只允许系统管理员调用；这条用户侧入口会把调用者自己设为团队 `owner`
- 如果要由系统管理员“替别人创建团队并指定初始团队管理员”，使用 `/admin/teams`；admin 创建入口会把目标用户加入团队并赋予 `admin` 角色
- `DELETE /teams/{id}` 是归档，不是物理删除；超过 `team_archive_retention_days` 后才会被后台清理
- `GET /teams/{id}/audit-logs` 需要团队 `owner` 或 `admin`，支持 `user_id`、`action`、`after`、`before`、`limit`、`offset`
- `GET /teams/{id}/members` 支持 `keyword`、`role`、`status`、`limit`、`offset`、`sort_by`、`sort_order`
- `POST /teams/{id}/members` 可用 `user_id` 或 `identifier` 指定目标用户，二选一；`role` 不传时默认 `member`
- 成员分页返回除了 `items` / `total` / `limit` / `offset`，还会带 `owner_count` 和 `manager_count`

## 团队工作空间

团队工作空间统一挂在：

```text
/api/v1/teams/{team_id}
```

这套接口不是独立实现的一套“团队版文件系统”，而是把个人空间已有的文件 / 文件夹 / 上传 / 搜索 / 标签 / 分享 / 回收站 / 后台任务 / WebDAV 账号语义切到团队作用域。

## 目录与文件

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/folders` | 列出团队根目录 |
| `POST` | `/teams/{team_id}/folders` | 在团队空间创建文件夹 |
| `GET` | `/teams/{team_id}/folders/{id}` | 列出团队子目录内容 |
| `GET` | `/teams/{team_id}/folders/{id}/info` | 读取团队文件夹完整信息 |
| `GET` | `/teams/{team_id}/folders/{id}/ancestors` | 读取团队面包屑祖先链 |
| `PATCH` | `/teams/{team_id}/folders/{id}` | 重命名、移动、设置目录策略 |
| `DELETE` | `/teams/{team_id}/folders/{id}` | 软删除团队文件夹 |
| `POST` | `/teams/{team_id}/folders/{id}/lock` | 锁定 / 解锁团队文件夹 |
| `POST` | `/teams/{team_id}/folders/{id}/copy` | 递归复制团队文件夹 |
| `POST` | `/teams/{team_id}/files/upload` | 团队空间 multipart 直传 |
| `POST` | `/teams/{team_id}/files/new` | 在团队空间创建空文件 |
| `POST` | `/teams/{team_id}/files/upload/init` | 协商团队上传模式 |
| `GET` | `/teams/{team_id}/files/upload/sessions` | 列出团队空间可恢复上传 session |
| `PUT` | `/teams/{team_id}/files/upload/{upload_id}/{chunk_number}` | 上传团队分片 |
| `POST` | `/teams/{team_id}/files/upload/{upload_id}/presign-parts` | 批量申请团队对象存储 / remote multipart part URL |
| `POST` | `/teams/{team_id}/files/upload/{upload_id}/complete` | 完成团队上传 |
| `GET` | `/teams/{team_id}/files/upload/{upload_id}` | 查询团队上传进度 |
| `DELETE` | `/teams/{team_id}/files/upload/{upload_id}` | 取消团队上传 |
| `GET` | `/teams/{team_id}/files/{id}` | 获取团队文件元信息 |
| `GET` | `/teams/{team_id}/files/{id}/archive-preview` | 获取团队归档只读预览清单 |
| `GET` | `/teams/{team_id}/files/{id}/direct-link` | 生成团队文件直接下载链接 token |
| `POST` | `/teams/{team_id}/files/{id}/preview-link` | 生成团队文件短期预览链接 |
| `POST` | `/teams/{team_id}/files/{id}/resource-handle` | 解析团队文件的前端资源合同 |
| `POST` | `/teams/{team_id}/files/{id}/wopi/open` | 为团队文件创建 WOPI 启动会话 |
| `GET` | `/teams/{team_id}/files/{id}/download` | 下载团队文件 |
| `GET` | `/teams/{team_id}/files/{id}/thumbnail` | 获取团队文件缩略图 |
| `GET` | `/teams/{team_id}/files/{id}/image-preview` | 获取团队文件图片预览 WebP |
| `GET` | `/teams/{team_id}/files/{id}/media-metadata` | 获取团队文件媒体元数据 |
| `PUT` | `/teams/{team_id}/files/{id}/content` | 覆盖团队文件内容 |
| `POST` | `/teams/{team_id}/files/{id}/extract` | 把团队归档文件解包成后台任务 |
| `PATCH` | `/teams/{team_id}/files/{id}` | 重命名或移动团队文件 |
| `DELETE` | `/teams/{team_id}/files/{id}` | 软删除团队文件 |
| `POST` | `/teams/{team_id}/files/{id}/lock` | 锁定 / 解锁团队文件 |
| `POST` | `/teams/{team_id}/files/{id}/copy` | 复制团队文件 |
| `GET` | `/teams/{team_id}/files/{id}/versions` | 列出团队文件历史版本 |
| `POST` | `/teams/{team_id}/files/{id}/versions/{version_id}/restore` | 恢复团队文件版本 |
| `DELETE` | `/teams/{team_id}/files/{id}/versions/{version_id}` | 删除团队文件版本 |

这部分请求体、分页参数、上传模式、锁语义、版本语义都和个人空间一致，直接对照这些文档看就行：

- [文件 API](./files.md)
- [文件夹 API](./folders.md)

## 批量、搜索、标签、分享、回收站、后台任务与 WebDAV

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/teams/{team_id}/batch/delete` | 批量删除团队文件和文件夹 |
| `POST` | `/teams/{team_id}/batch/move` | 批量移动团队文件和文件夹 |
| `POST` | `/teams/{team_id}/batch/copy` | 批量复制团队文件和文件夹 |
| `POST` | `/teams/{team_id}/batch/archive-compress` | 创建团队空间压缩归档后台任务 |
| `POST` | `/teams/{team_id}/batch/archive-download` | 创建团队空间 ZIP 下载 ticket |
| `GET` | `/teams/{team_id}/batch/archive-download/{token}` | 下载团队空间 ZIP |
| `GET` | `/teams/{team_id}/search` | 搜索团队工作空间 |
| `GET` | `/teams/{team_id}/tags` | 列出团队标签 |
| `POST` | `/teams/{team_id}/tags` | 创建团队标签 |
| `PATCH` | `/teams/{team_id}/tags/{tag_id}` | 重命名或修改团队标签颜色 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}` | 删除团队标签 |
| `GET` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | 列出团队文件或文件夹已绑定标签 |
| `PUT` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | 替换团队文件或文件夹的完整标签集合 |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | 给团队文件或文件夹附加一个标签 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | 从团队文件或文件夹移除一个标签 |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/batch` | 给多个团队文件 / 文件夹附加一个标签 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/batch` | 从多个团队文件 / 文件夹移除一个标签 |
| `POST` | `/teams/{team_id}/shares` | 为团队文件或文件夹创建分享 |
| `GET` | `/teams/{team_id}/shares` | 列出当前用户在该团队创建的分享 |
| `PATCH` | `/teams/{team_id}/shares/{id}` | 编辑团队分享 |
| `DELETE` | `/teams/{team_id}/shares/{id}` | 删除团队分享 |
| `POST` | `/teams/{team_id}/shares/batch-delete` | 批量删除团队分享 |
| `GET` | `/teams/{team_id}/trash` | 列出团队回收站 |
| `POST` | `/teams/{team_id}/trash/{entity_type}/{id}/restore` | 恢复团队回收站条目 |
| `DELETE` | `/teams/{team_id}/trash/{entity_type}/{id}` | 彻底删除团队回收站条目 |
| `DELETE` | `/teams/{team_id}/trash` | 清空团队回收站 |
| `GET` | `/teams/{team_id}/tasks` | 查看该团队作用域下的后台任务 |
| `POST` | `/teams/{team_id}/tasks/offline-download` | 创建团队空间链接导入任务 |
| `GET` | `/teams/{team_id}/tasks/{id}` | 读取单个团队任务 |
| `POST` | `/teams/{team_id}/tasks/{id}/retry` | 重试失败的团队任务 |
| `GET` | `/teams/{team_id}/webdav-accounts` | 列出团队 WebDAV 账号 |
| `POST` | `/teams/{team_id}/webdav-accounts` | 创建团队 WebDAV 账号 |
| `DELETE` | `/teams/{team_id}/webdav-accounts/{account_id}` | 删除团队 WebDAV 账号 |
| `POST` | `/teams/{team_id}/webdav-accounts/{account_id}/toggle` | 启用或停用团队 WebDAV 账号 |

这几组能力同样复用个人空间契约：

- [批量操作 API](./batch.md)
- [搜索 API](./search.md)
- [标签 API](./tags.md)
- [分享 API](./shares.md)
- [回收站 API](./trash.md)
- [后台任务 API](./tasks.md)
- [WebDAV](./webdav.md)
- [WOPI](./wopi.md)

有几条团队特有语义需要额外记住：

- 团队分享的公开 REST 访问仍然走 `/api/v1/s/{token}`，前端公开页面仍然是 `/s/:token`，不是 `/teams/{team_id}/s/{token}`
- 文件写入时会优先使用目录级 `policy_id`；没有目录覆盖时，再按 `teams.policy_group_id` 的规则解析实际存储策略
- 团队文件的 WOPI 启动入口虽然是 `/teams/{team_id}/files/{id}/wopi/open`，但真正回调时仍然走统一的 `/api/v1/wopi/files/{id}`；团队作用域信息保存在 access token 里
- 团队批量打包下载 ticket 只能在对应团队路由下消费，不能拿去个人 `/batch/archive-download/{token}` 复用
- 团队 `GET /teams/{team_id}/files/upload/sessions` 和个人空间恢复接口返回相同结构，但只列出该团队作用域下当前用户发起、仍未过期且可恢复的 session
- 团队上传初始化同样支持 `frontend_client_id`；恢复接口也支持同名 query 过滤，只列出同一前端实例创建的 session
- 团队 `POST /teams/{team_id}/tasks/offline-download` 创建 `offline_download` 任务，把远端 HTTP/HTTPS 链接导入团队空间；请求体和个人 `/tasks/offline-download` 一致
- 团队 WebDAV 账号用同一个 WebDAV 挂载入口认证，但账号的空间作用域是团队；普通成员只能管理自己创建的团队 WebDAV 账号，团队 `owner` / `admin` 可以管理该团队全部账号
- 团队 `POST /teams/{team_id}/files/{id}/extract` 语义和个人空间一致：创建 `archive_extract` 任务，不会同步阻塞到解包完成
- 团队 `POST /teams/{team_id}/files/{id}/extract` 支持 `filename_encoding`，且 `target_folder_id = null` 时默认解包到源压缩包所在目录
- 团队 `POST /teams/{team_id}/batch/archive-compress` 语义和个人空间一致：创建 `archive_compress` 任务，把打包结果写回团队工作空间；`target_folder_id = null` 时优先写回选中项的共同父目录，没有共同父目录时写回团队根目录
- 团队 `GET /teams/{team_id}/files/{id}/archive-preview` 语义和个人空间一致：目前支持 ZIP，只读返回 manifest；缓存未生成时创建或复用 `archive_preview_generate` 任务并返回 `202`

团队文件的 `GET /teams/{team_id}/files/{id}/direct-link` 语义和个人空间一致：接口只返回 token，真正下载仍然走根路径 `/d/{token}/{filename}`。默认 inline 直链由 AsterDrive 流式返回；追加 `?download=1` 后会复用附件下载分流，命中 `presigned` 策略时返回 `302`。

团队文件的 `POST /teams/{team_id}/files/{id}/preview-link` 也和个人空间一致：接口返回 `PreviewLinkInfo`，真正预览内容走根路径 `/pv/{token}/{filename}`。

团队文件的 `POST /teams/{team_id}/files/{id}/resource-handle` 复用个人文件的用途、交付模式、表示形式和响应合同，区别只是 `identity.scope = "team"`，并且 `request.url` 指向团队作用域路径。完整字段见 [文件 API 的 resource handle](./files.md#post-filesidresource-handle)。
