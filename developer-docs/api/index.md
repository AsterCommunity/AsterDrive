# API 概览

这页按功能分组带你找接口，不打算把每个接口都重写成 OpenAPI 导出物。

绝大多数面向用户的 JSON / REST 接口都挂在：

```text
/api/v1
```

## 先分清节点模式

当前仓库有两类 HTTP 暴露面：

- `primary` 节点：
  - 普通用户 REST API
  - 公开分享 API
  - WebDAV
  - 前端页面
  - 健康检查
- `follower` 节点：
  - 健康检查
  - 内部对象存储协议 `/api/v1/internal/storage/*`

这意味着：

- 你在浏览器、前端 SDK、公开分享页会碰到的，基本都在本目录这些页面里
- `/api/v1/internal/storage/*` 是主从节点之间的内部协议，不是给浏览器客户端用的普通公开 API

## 不在 `/api/v1` 下的入口

当前明确不在 `/api/v1` 前缀下的能力有：

- 健康检查：`/health*`
- 直接下载链接：`/d/{token}/{filename}`
- 预览下载链接：`/pv/{token}/{filename}`
- WebDAV：默认 `/webdav`
- 前端页面、公开分享页面和静态资源兜底：由 primary 最后注册

## 统一响应格式

大多数 JSON 接口都使用统一包装：

```json
{
  "code": 0,
  "msg": "",
  "data": {}
}
```

字段含义：

- `code`：数字错误码，`0` 表示成功
- `msg`：错误消息；成功时通常为空
- `data`：响应体；部分成功接口会省略

错误响应还会带 `error` 对象：

```json
{
  "code": 2003,
  "msg": "untrusted request origin for cookie-authenticated action",
  "error": {
    "internal_code": "E013",
    "subcode": "auth.request_origin_untrusted",
    "retryable": false
  }
}
```

错误字段约定：

- `code` 是稳定数字大类，用来区分认证、上传、存储、分享等分域。
- `error.internal_code` 是后端内部错误枚举码，给日志和排障用，不作为前端业务分支依据。
- `msg` 是诊断性 fallback 文本，不能作为 i18n key，也不要在客户端用字符串匹配判断业务原因。
- `error.subcode` 是稳定机器可读细分原因，前端 i18n 和交互分支应该优先用它；没有 subcode 时再回退到 `code`。
- 新增用户可见错误时，如果同一个 `code` / `internal_code` 会覆盖多个业务原因，必须补 `ApiSubcode` 并同步前端 `ApiSubcode` 常量、`useApiError` 映射和中英文 locale。
- 辅助函数不要靠错误消息或 label 字符串反推 subcode；调用点应显式传入对应 `ApiSubcode`。

## 不走统一 JSON 包装的接口

以下能力返回原始内容而不是 `ApiResponse`：

- 文件下载
- 直接下载链接
- 预览下载链接
- 分享 stream session 流式播放
- 文件缩略图
- 批量打包下载 ticket 对应的 ZIP 流
- 分享文件下载
- 分享缩略图
- 当前用户已上传头像
- 管理员读取用户已上传头像
- 分享拥有者已上传头像
- 当前用户存储变更事件流
- WOPI `CheckFileInfo` 与内容回调
- WebDAV 协议响应
- Prometheus 指标
- follower 内部对象读取流 `/api/v1/internal/storage/objects/{tail:.*}`

公开品牌配置、公开预览应用配置、公开缩略图能力和公开 enrollment 虽然不需要登录，但仍然是普通 `/api/v1/public/*` JSON 接口。

## 错误码分域

| 范围 | 含义 |
| --- | --- |
| `0` | 成功 |
| `1000-1099` | 通用、数据库、配置、限流、邮件、冲突错误 |
| `2000-2099` | 认证、授权、激活、联系方式验证错误 |
| `3000-3099` | 文件、上传 session、分片、锁、缩略图、条件请求错误 |
| `4000-4099` | 存储策略、配额、驱动、对象存储和存储后端细分错误 |
| `5000-5099` | 文件夹错误 |
| `6000-6099` | 分享错误 |

## 当前支持的认证方式

### REST / 前端

- HttpOnly Cookie
- `Authorization: Bearer <jwt>`

### WebDAV

- `Authorization: Basic ...`
- `Authorization: Bearer <jwt>`

### Follower 内部存储协议

- 主节点签名头：
  - `x-aster-access-key`
  - `x-aster-timestamp`
  - `x-aster-nonce`
  - `x-aster-signature`
- 某些对象 GET / PUT 也支持预签名 query 参数

## 工作空间作用域

当前有两类受保护工作空间：

- 个人空间：接口直接挂在 `/files`、`/folders`、`/batch`、`/search`、`/shares`、`/trash`
- 团队空间：复用同一套语义，但统一加前缀 `/teams/{team_id}`

常见团队路径长这样：

```text
/api/v1/teams/{team_id}/folders
/api/v1/teams/{team_id}/files/{id}
/api/v1/teams/{team_id}/batch/move
/api/v1/teams/{team_id}/search
/api/v1/teams/{team_id}/shares
/api/v1/teams/{team_id}/trash
/api/v1/teams/{team_id}/tasks
```

也就是说，团队空间不是另一套业务模型，而是把同一套文件 / 文件夹 / 搜索 / 回收站语义切到团队作用域下执行。

## 模块索引

- [认证](./auth.md)
- [文件](./files.md)
- [文件夹](./folders.md)
- [团队与团队空间](./teams.md)
- [批量操作](./batch.md)
- [分享](./shares.md)
- [回收站](./trash.md)
- [搜索](./search.md)
- [后台任务](./tasks.md)
- [WOPI](./wopi.md)
- [WebDAV](./webdav.md)
- [属性](./properties.md)
- [公共接口](./public.md)
- [管理](./admin.md)
- [健康检查](./health.md)
- [内部存储协议（follower）](./internal-storage.md)

其中比较值得优先看的几组能力是：

- 上传与版本：见 [文件](./files.md)
- Passkey、外部认证和登录会话：见 [认证](./auth.md)
- ZIP 归档只读预览：见 [文件](./files.md)、[分享](./shares.md) 和 [后台任务](./tasks.md)
- 批量删除 / 移动 / 复制 / 打包：见 [批量操作](./batch.md)
- 回收站恢复与清理：见 [回收站](./trash.md)
- 搜索、文件分类和扩展名筛选：见 [搜索](./search.md)
- 后台任务列表与重试：见 [后台任务](./tasks.md)
- 团队管理与团队工作空间：见 [团队与团队空间](./teams.md)
- 公开分享、预览链接和流式播放 session：见 [分享](./shares.md)
- Office / WOPI 预览与回调：见 [WOPI](./wopi.md)
- WebDAV 协议、账号与 DeltaV：见 [WebDAV](./webdav.md)
- 登录页、匿名页、缩略图能力与远端节点注册握手：见 [公共接口](./public.md)
- 主从节点内部对象协议：见 [内部存储协议（follower）](./internal-storage.md)
- 后台策略、远端节点、外部认证 provider、锁、运行时配置与审计：见 [管理](./admin.md)

## OpenAPI 与 Swagger

如果你就是想要机器可读规范，也还是有两条路：

- `debug_assertions + openapi feature` 构建：访问 `/swagger-ui` 与 `/api-docs/openapi.json`
- 任意构建：运行 `cargo test --features openapi --test generate_openapi` 导出静态规范到 `frontend-panel/generated/openapi.json`

OpenAPI 注册列表维护在 `src/api/openapi.rs`，真实 HTTP 注册入口仍然以 `src/api/primary.rs`、`src/api/follower.rs` 和 `src/api/routes/**` 为准。新增 route 时如果忘了补 `openapi.rs`，运行时接口仍可能存在，但 Swagger、静态规范和生成的 SDK 会漏掉它；如果两者冲突，先按 route 源码确认实际行为，再修 OpenAPI 装配。

## 继续阅读

- [认证](./auth.md)
- [文件](./files.md)
- [团队与团队空间](./teams.md)
- [搜索](./search.md)
- [后台任务](./tasks.md)
- [WOPI](./wopi.md)
- [分享](./shares.md)
- [公共接口](./public.md)
- [管理](./admin.md)
