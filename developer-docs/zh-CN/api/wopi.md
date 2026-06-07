# WOPI API

WOPI 相关能力分成两层：

- 启动层：登录用户先为某个文件创建 WOPI 启动会话
- 协议层：Office / WOPI 宿主随后回调 `/api/v1/wopi/files/{id}` 及其 `/contents`

## 启动接口

以下路径都相对于 `/api/v1`，且都需要认证。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/files/{id}/wopi/open` | 为个人空间文件创建 WOPI 启动会话 |
| `POST` | `/teams/{team_id}/files/{id}/wopi/open` | 为团队空间文件创建 WOPI 启动会话 |

请求体：

```json
{
  "app_key": "custom.onlyoffice"
}
```

返回体是统一 JSON 包装下的 `WopiLaunchSession`：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "access_token": "...",
    "access_token_ttl": 1775995200000,
    "action_url": "https://office.example.com/hosting/wopi/word/edit?WOPISrc=https%3A%2F%2Fdrive.example.com%2Fapi%2Fv1%2Fwopi%2Ffiles%2F1",
    "form_fields": {},
    "mode": "iframe"
  }
}
```

当前语义：

- `app_key` 必须命中 `/public/preview-apps` 里启用中的 `provider = "wopi"` 应用
- 系统必须配置 `public_site_url`，因为服务端要生成绝对的 `WOPISrc`。多来源配置下，服务端会优先使用当前请求 Host 在 `public_site_url` 列表里的精确命中项；没有命中时回退第一项
- 如果预览器配置了 `config.action_url`，会直接展开 / 追加 `WOPISrc`
- 如果没配 `action_url` 但配了 `config.discovery_url`，服务端会拉取 discovery XML，并按“扩展名 -> MIME -> 通配”顺序解析可用 action URL
- `access_token_ttl` 按 WOPI 规范返回“过期时间的 Unix 毫秒时间戳”，不是“TTL 秒数”
- 团队文件虽然走 `/teams/{team_id}/files/{id}/wopi/open` 启动，但后续回调仍统一打到 `/api/v1/wopi/files/{id}`；团队作用域保存在 access token 里

## 协议回调接口

以下路径也都相对于 `/api/v1`，但它们不是普通前端 JSON 接口，而是给 WOPI 宿主调用的协议入口。

成功时返回原始 WOPI JSON / 文件流；失败时仍复用统一的 `ApiResponse` JSON 错误格式。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/wopi/files/{id}?access_token=...` | `CheckFileInfo` |
| `POST` | `/wopi/files/{id}?access_token=...` | `LOCK` / `UNLOCK` / `REFRESH_LOCK` / `GET_LOCK` / `PUT_RELATIVE` / `RENAME_FILE` / `PUT_USER_INFO` |
| `GET` | `/wopi/files/{id}/contents?access_token=...` | 获取文件内容 |
| `POST` | `/wopi/files/{id}/contents?access_token=...` | 覆盖文件内容（`X-WOPI-Override: PUT`） |

## `GET /wopi/files/{id}`

返回原始 WOPI `CheckFileInfo` JSON，不走 `ApiResponse` 包装。

当前返回内容包含：

- `BaseFileName`
- `FileNameMaxLength`
- `OwnerId`
- `Size`
- `UserId`
- `UserCanNotWriteRelative`
- `UserCanRename`
- `UserInfo`
- `UserCanWrite`
- `ReadOnly`
- `SupportsGetLock`
- `SupportsLocks`
- `SupportsExtendedLockLength`
- `SupportsRename`
- `SupportsUserInfo`
- `SupportsUpdate`
- `Version`

当前实现里：

- `UserCanNotWriteRelative = false`
- `UserCanRename = true`
- `SupportsLocks = true`
- `SupportsExtendedLockLength = true`
- `SupportsUpdate = true`
- `SupportsGetLock = true`
- `SupportsRename = true`
- `SupportsUserInfo = true`
- `FileNameMaxLength = 255`
- 如果当前用户档案里已有 `wopi_user_info`，会透传到 `UserInfo`

## `POST /wopi/files/{id}`

这条协议入口当前按 `X-WOPI-Override` 分派多种操作：

- `LOCK`
- `UNLOCK`
- `REFRESH_LOCK`
- `GET_LOCK`
- `PUT_RELATIVE`
- `RENAME_FILE`
- `PUT_USER_INFO`

另外还有一个特殊变体：

- `LOCK` + `X-WOPI-OldLock`：按 WOPI 语义执行 `UnlockAndRelock`

如果 override 不在支持列表内，服务端当前会返回 `501 Not Implemented`。

### 锁相关操作

`LOCK` / `UNLOCK` / `REFRESH_LOCK` / `GET_LOCK` 依赖这些请求头：

- `X-WOPI-Lock`

`UnlockAndRelock` 额外需要：

- `X-WOPI-OldLock`

冲突时返回 `409`，并会带：

- `X-WOPI-LockFailureReason`
- `X-WOPI-Lock`（当前锁值已知时返回；某些场景会显式返回空字符串）

当前实现要点：

- 读取 WOPI 锁状态时会先清理该文件已经过期的锁记录，再判断当前请求
- 同一 app、同一锁值再次 `LOCK` 会被视为续期
- `LOCK + X-WOPI-OldLock` 会尝试原子替换锁值
- 文件如果已经被非 WOPI 锁住，或同一文件同时存在多条活动锁记录，会按 WOPI 外部冲突处理，通常返回空的 `X-WOPI-Lock`
- `GET_LOCK` 成功时返回 `200`，并在响应头里带当前 `X-WOPI-Lock`
- `UNLOCK` / `REFRESH_LOCK` 在没有活动锁时同样返回 `409`

### `PUT_RELATIVE`

这条操作用于在源文件旁边创建或覆写相邻文件。

请求头规则：

- 必须二选一传 `X-WOPI-SuggestedTarget` 或 `X-WOPI-RelativeTarget`
- 可选 `X-WOPI-OverwriteRelativeTarget`
- 可选 `X-WOPI-Size`

成功时返回原始 WOPI JSON，核心字段是：

- `Name`
- `Url`

冲突时返回 `409`，并可能额外带：

- `X-WOPI-Lock`
- `X-WOPI-LockFailureReason`
- `X-WOPI-ValidRelativeTarget`

也就是说，如果目标文件名冲突且当前请求不允许覆写，服务端会直接给出一个可用候选名。

### `RENAME_FILE`

这条操作通过下面的请求头驱动：

- `X-WOPI-RequestedName`
- 可选 `X-WOPI-Lock`

成功时返回原始 WOPI JSON，包含：

- `Name`

当前实现要点：

- 服务端会校验并规范化请求名
- 如果目标名称冲突，会自动避让出一个可用名称，而不是简单报 409
- 如果名称非法，会返回 `400`，并在响应头里带 `X-WOPI-InvalidFileNameError`

### `PUT_USER_INFO`

这条操作把 WOPI 宿主传来的用户状态字符串持久化到用户档案，后续会通过 `CheckFileInfo.UserInfo` 回传。

当前约束：

- body 必须是有效 UTF-8
- body 只允许 ASCII 字符
- 最大长度 `1024` 字节

## `GET /wopi/files/{id}/contents`

返回原始文件流，不走 JSON 包装。

行为上和普通文件下载类似：

- 默认按 inline 方式返回
- 支持 `If-None-Match`
- 支持可选请求头 `X-WOPI-MaxExpectedSize`；文件大于该值时返回 precondition failed

## `POST /wopi/files/{id}/contents`

当前只支持：

- `X-WOPI-Override: PUT`

成功时返回 `200`，并带：

- `X-WOPI-ItemVersion`

当前语义：

- 如果文件存在活动 WOPI 锁，调用方必须带匹配的 `X-WOPI-Lock`
- 锁不匹配时返回 `409`
- 底层仍然复用普通文件覆盖写入链路，会写历史版本、更新 ETag / 版本信息

## 安全边界

这组接口当前会做几层校验：

- access token 必须存在且未过期
- token 里的文件 ID、用户会话版本和团队作用域必须匹配
- 如果用户被禁用、会话被吊销、WOPI app 被禁用或移除，对应持久化 session 会立刻失效
- 如果 WOPI app 配置里能推导出可信来源（`allowed_origins`、`action_url`、`discovery_url`），服务端会校验请求的 `Origin` / `Referer`

关于来源校验的当前行为：

- 缺少 `Origin` 和 `Referer` 时仍允许
- 头格式非法时返回 `400`
- 来源不在可信列表内时，协议层会返回未授权响应

## 相关文档

- [文件 API](./files.md)
- [团队与团队空间 API](./teams.md)
- [公共接口](./public.md)
