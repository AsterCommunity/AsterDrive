# 批量操作 API

以下路径都相对于 `/api/v1`，且都需要认证。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/batch/delete` | 批量删除文件和文件夹 |
| `POST` | `/batch/move` | 批量移动文件和文件夹 |
| `POST` | `/batch/copy` | 批量复制文件和文件夹 |
| `POST` | `/batch/archive-compress` | 创建压缩归档后台任务 |
| `POST` | `/batch/archive-download` | 创建批量打包下载 ticket |
| `GET` | `/batch/archive-download/{token}` | 根据 ticket 流式下载 ZIP |

## 请求体结构

这组接口里的选择类请求体都使用混合资源选择：

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10, 11]
}
```

其中：

- `file_ids` 和 `folder_ids` 可以同时存在
- 单次总项目数上限是 1000
- 每个条目独立执行，不会因为一个失败就让整批全部回滚

## 返回结果

其中：

- `POST /batch/delete`
- `POST /batch/move`
- `POST /batch/copy`

会返回 `BatchResult` 风格的数据，包含：

- `succeeded`
- `failed`
- `errors`

这也是前端批量操作条和批量 toast 汇总提示的依据。

而：

- `POST /batch/archive-compress` 返回 `TaskInfo`
- `POST /batch/archive-download` 返回 `StreamTicketInfo`

## `POST /batch/delete`

行为：

- 文件和文件夹会走和单项删除一致的软删除逻辑
- 删除结果逐项统计
- 某一项失败不会阻断其他项继续执行

## `POST /batch/move`

请求体还会带目标目录：

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "target_folder_id": 99
}
```

行为：

- 支持把文件和文件夹一起移动到同一个目标目录
- `target_folder_id = null` 表示移动到根目录
- 前端拖拽移动和批量移动共用这类能力

## `POST /batch/copy`

请求体还会带目标目录：

```json
{
  "file_ids": [1],
  "folder_ids": [10],
  "target_folder_id": 99
}
```

行为：

- 文件复制不会物理复制 Blob，只增加引用计数
- 文件夹复制会递归复制目录树
- 与单项复制一样，目标位置同名时会自动生成副本名

## 打包下载与压缩任务

### `POST /batch/archive-compress`

这条接口会创建一个 `archive_compress` 后台任务，把选中的文件 / 文件夹打成 ZIP 后再写回当前工作空间。

请求体：

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "archive_name": "workspace-export",
  "target_folder_id": 99
}
```

当前语义：

- `target_folder_id = null` 时，服务端会先看选中项是否都来自同一个父目录；如果是，就把生成的压缩包写回这个共同父目录，否则写回根目录
- 返回的是 `TaskInfo`，不是文件流
- 这条链路会出现在 [`后台任务 API`](./tasks.md) 里
- 生成完成后，任务结果会带最终产物文件的路径和文件 ID
- 团队空间也有对应的 `/teams/{team_id}/batch/archive-compress`

### `POST /batch/archive-download`

请求体和其他批量接口一样，也支持混合资源，并可额外指定压缩包名：

```json
{
  "file_ids": [1, 2],
  "folder_ids": [10],
  "archive_name": "workspace-export"
}
```

成功后返回的不是文件流，而是一张短期 stream ticket：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "token": "st_xxxxx",
    "download_path": "/api/v1/batch/archive-download/st_xxxxx",
    "expires_at": "2026-04-12T12:00:00Z"
  }
}
```

当前语义：

- `archive_name` 为空时会自动推导；最终文件名总是 `.zip`
- ticket 默认 5 分钟过期
- `download_path` 可能是相对路径，也可能在配置了 `public_site_url` 后直接返回绝对 URL
- ticket 绑定当前用户和当前工作空间，不能拿个人空间 ticket 去团队接口下载，也不能换人复用
- 这条链路当前是“短期 ticket + 直接流式压缩下载”，不会创建 `/tasks` 里的后台任务记录

### `GET /batch/archive-download/{token}`

拿着上一步返回的 `download_path` 发起 `GET`，返回原始 `application/zip` 流。

当前实现细节：

- 空目录会被保留在 ZIP 里
- 多选文件夹时会按当前目录树打包
- 同级重名根项会在 ZIP 根目录内自动避让命名
- 只会打包当前仍处于活动状态、且属于当前工作空间可见范围内的文件和文件夹

## 使用场景

这组接口主要服务当前前端已经实现的：

- 多选批量删除
- 多选批量复制
- 多选批量移动
- 拖拽多个项目一起移动
- 多选打包下载
- 多选压缩成 ZIP 并写回工作空间

## 相关文档

- [文件 API](./files.md)
- [文件夹 API](./folders.md)
- [核心流程](../../../docs/guide/core-workflows.md)
