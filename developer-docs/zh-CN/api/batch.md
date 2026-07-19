# 批量操作 API

以下路径都相对于 `/api/v1`，且都需要认证。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/batch/delete` | 批量删除文件和文件夹 |
| `POST` | `/batch/move` | 批量移动文件和文件夹 |
| `POST` | `/batch/copy` | 批量复制文件和文件夹 |
| `POST` | `/workspace-transfer/copy` | 在不同工作空间之间复制文件和文件夹 |
| `POST` | `/workspace-transfer/move` | 在不同工作空间之间移动文件和文件夹 |
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
- `/batch/move` 至少需要两个选中资源；单个文件使用 `PATCH /files/{id}`，单个文件夹使用 `PATCH /folders/{id}`
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

- `/batch/copy` 至少需要两个选中资源；单个文件或文件夹使用对应的单项复制接口
- 文件复制不会物理复制 Blob，只增加引用计数
- 文件夹复制会递归复制目录树
- 与单项复制一样，目标位置同名时会自动生成副本名

## 跨工作空间复制与移动

个人空间和团队空间使用独立的资源作用域。跨作用域操作不复用 `/batch/copy` 或
`/batch/move`，而是使用下面两条接口显式声明源空间和目标空间：

- `POST /workspace-transfer/copy`
- `POST /workspace-transfer/move`

### 请求体

两条接口使用相同的请求结构：

```json
{
  "source_workspace": { "kind": "personal" },
  "file_ids": [1, 2],
  "folder_ids": [10],
  "destination_workspace": { "kind": "team", "team_id": 42 },
  "target_folder_id": null
}
```

`kind = "personal"` 表示当前用户的个人空间；`kind = "team"` 必须同时带一个正数
`team_id`。`target_folder_id = null` 表示目标空间的根目录。

### 权限与作用域校验

- 操作者必须同时有权访问源空间和目标空间。
- 所有源文件和源文件夹都必须属于 `source_workspace`。
- 目标文件夹必须属于 `destination_workspace`。
- 个人空间不会接受其他用户的个人资源。
- 团队空间会重新检查当前成员关系，不能只依赖前端显示的团队列表。
- 选中的资源为空、ID 非正数、团队 ID 非正数或批量超过 1000 项时，请求直接返回校验错误。

### Copy 语义

`/workspace-transfer/copy` 保留源资源，按目标空间重新创建文件和文件夹记录：

- 文件夹递归复制子文件夹和文件。
- 目标位置发生同名冲突时自动分配副本名。
- 目标空间重新计算归属、创建者和配额。
- Blob 内容可以复用现有引用，不要求把相同内容再次写入物理存储。
- 只向目标空间发送创建/变更事件。

### Move 语义

`/workspace-transfer/move` 在跨空间时采用“复制成功后删除源资源”的顺序：

1. 先按 copy 规则创建目标资源。
2. 只有全部选中项目复制成功后，才把源资源放入回收站。
3. 复制阶段出现任何项目失败时，不删除源资源；返回结果会保留逐项错误。

因此，调用方不能把一次部分成功的 move 当成完整移动。应该根据返回的
`succeeded`、`failed` 和 `errors` 刷新源空间与目标空间，并向用户展示未完成项目。

跨空间 move 还会拒绝直接移动已锁定的选中文件或文件夹。源资源进入回收站后，仍遵守
现有回收站恢复、永久删除和配额清理规则。

同一个工作空间的单项 move 使用文件/文件夹 PATCH 接口，多项 move 才使用普通 `/batch/move`；不会经过跨空间复制流程。

### 返回结果与事件

两条接口都返回普通 `BatchResult`。跨空间 move 成功后通常会看到：

- 目标空间的文件/文件夹创建事件；
- 源空间的文件/文件夹进入回收站事件；
- 一条带源空间和目标空间信息的批量移动审计记录。

如果复制没有全部成功，源空间不会发出删除事件，但目标空间可能已经存在已成功复制的项目；
客户端应按返回结果重新加载两个空间，而不是只更新当前列表中的一行。

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
- [核心流程](https://drive.astercosm.com/guide/core-workflows/)
