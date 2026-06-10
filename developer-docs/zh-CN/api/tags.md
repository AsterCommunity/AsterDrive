# 标签 API

以下路径都相对于 `/api/v1`，且都需要认证。

标签按工作空间隔离：

- 个人标签挂在 `/tags`
- 团队标签挂在 `/teams/{team_id}/tags`

个人标签库和团队标签库互不混用。个人空间创建的标签不能绑定到团队文件或文件夹，团队标签也不能绑定到个人空间资源。

## 接口列表

### 个人空间

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/tags` | 列出当前用户个人标签 |
| `POST` | `/tags` | 创建个人标签 |
| `PATCH` | `/tags/{tag_id}` | 重命名或修改个人标签颜色 |
| `DELETE` | `/tags/{tag_id}` | 删除个人标签，并解除已有绑定 |
| `GET` | `/tags/{entity_type}/{entity_id}` | 列出某个文件或文件夹已绑定标签 |
| `PUT` | `/tags/{entity_type}/{entity_id}` | 替换某个文件或文件夹的完整标签集合 |
| `PUT` | `/tags/{tag_id}/{entity_type}/{entity_id}` | 给单个文件或文件夹附加一个标签 |
| `DELETE` | `/tags/{tag_id}/{entity_type}/{entity_id}` | 从单个文件或文件夹移除一个标签 |
| `PUT` | `/tags/{tag_id}/batch` | 给多个文件和文件夹批量附加一个标签 |
| `DELETE` | `/tags/{tag_id}/batch` | 从多个文件和文件夹批量移除一个标签 |

### 团队空间

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/teams/{team_id}/tags` | 列出团队标签 |
| `POST` | `/teams/{team_id}/tags` | 创建团队标签 |
| `PATCH` | `/teams/{team_id}/tags/{tag_id}` | 重命名或修改团队标签颜色 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}` | 删除团队标签，并解除已有绑定 |
| `GET` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | 列出某个团队文件或文件夹已绑定标签 |
| `PUT` | `/teams/{team_id}/tags/{entity_type}/{entity_id}` | 替换某个团队文件或文件夹的完整标签集合 |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | 给单个团队文件或文件夹附加一个标签 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/{entity_type}/{entity_id}` | 从单个团队文件或文件夹移除一个标签 |
| `PUT` | `/teams/{team_id}/tags/{tag_id}/batch` | 给多个团队文件和文件夹批量附加一个标签 |
| `DELETE` | `/teams/{team_id}/tags/{tag_id}/batch` | 从多个团队文件和文件夹批量移除一个标签 |

`entity_type` 当前支持 `file` 和 `folder`。

## 标签库

`GET /tags` 和 `GET /teams/{team_id}/tags` 使用 offset 分页：

- `limit`：默认 `50`，受全局 API 最大页大小限制
- `offset`：默认 `0`
- `q`：可选，按标签名大小写不敏感搜索，最长 64 字符

返回结构是 `OffsetPage<TagInfo>`：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "items": [
      {
        "id": 7,
        "scope_type": "personal",
        "owner_user_id": 1,
        "team_id": null,
        "name": "Invoice",
        "normalized_name": "invoice",
        "color": "#3b82f6",
        "sort_order": 0,
        "usage_count": 12,
        "created_at": "2026-06-10T12:00:00Z",
        "updated_at": "2026-06-10T12:00:00Z"
      }
    ],
    "total": 1,
    "limit": 50,
    "offset": 0
  }
}
```

创建请求：

```json
{
  "name": "Invoice",
  "color": "#3b82f6"
}
```

更新请求：

```json
{
  "name": "Receipts",
  "color": "#22c55e"
}
```

规则：

- 标签名会 trim，不能为空，最长 64 个 Unicode 标量字符
- 名称按小写规范化后在同一工作空间内唯一
- `color` 必须是 7 字符十六进制颜色，例如 `#3b82f6`
- 删除标签会删除对应 `system.tags` 实体属性绑定，但不会删除文件或文件夹本体

## 实体绑定

实体标签响应结构：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "tags": [
      {
        "id": 7,
        "name": "Invoice",
        "color": "#3b82f6"
      }
    ]
  }
}
```

替换请求：

```json
{
  "tag_ids": [7, 8]
}
```

批量附加 / 移除请求：

```json
{
  "file_ids": [10, 11],
  "folder_ids": [20]
}
```

绑定规则：

- `tag_ids` 最多 64 个，且必须全部属于当前工作空间
- 批量请求最多带 1024 个文件 ID 和 1024 个文件夹 ID
- 文件和文件夹 ID 会按当前个人或团队工作空间校验后再写入
- 附加、移除和替换操作对客户端来说可以按幂等语义处理
- 单实体附加、移除和替换会返回该实体当前标签列表
- 批量附加和移除只返回空成功响应

## 搜索与事件

搜索 API 支持按标签过滤：

- `tag_ids=1,2,3`
- `tag_match=any` 或 `tag_match=all`，默认 `any`

完整查询契约见 [搜索 API](./search.md)。

标签写操作还会向已认证订阅者发布存储变更事件：

- `tag.created`
- `tag.updated`
- `tag.deleted`
- `tag.assignment_changed`

这些事件通过 `GET /auth/events/storage` 推送，前提是当前用户可见对应工作空间。SSE 入口见 [认证 API](./auth.md)。
