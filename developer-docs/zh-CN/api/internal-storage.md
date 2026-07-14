# 内部存储协议（Follower）

这组接口是主节点和 follower 节点之间的内部对象存储协议，不是给浏览器前端或第三方普通客户端用的公开 API。

这页描述的是 follower 侧实际执行对象读写的 `/api/v1/internal/storage/*`。primary 侧还有一组 reverse tunnel 内部入口 `/api/v1/internal/remote-tunnel/*`，用于让不能被 primary 直连的 follower 主动连回 primary。

以下路径都相对于：

```text
/api/v1/internal/storage
```

并且只会在 `follower` 节点注册。

## Direct 与 Reverse Tunnel

远端节点的对象协议有两层，别混在一起看：

- `/api/v1/internal/storage/*` 只在 follower 注册，是实际对象读写、绑定同步、远程存储目标管理的协议。
- `/api/v1/internal/remote-tunnel/*` 只在 primary 注册，是 reverse tunnel 的控制面和传输入口。

`direct` 模式下，primary 直接请求 follower 的 `/api/v1/internal/storage/*`。`reverse_tunnel` 模式下，primary 把同样的内部存储请求登记到 tunnel registry，follower 主动向 primary 轮询或建立 WebSocket 连接取走请求，再在本地调用内部存储处理逻辑并回传响应。

primary 侧 reverse tunnel 当前入口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/api/v1/internal/remote-tunnel/poll` | follower 长轮询待处理请求 |
| `POST` | `/api/v1/internal/remote-tunnel/complete` | follower 回传轮询请求的处理结果 |
| `GET` | `/api/v1/internal/remote-tunnel/connect` | follower 建立 WebSocket 流式 tunnel |

这组 reverse tunnel 接口同样使用远端节点签名鉴权，不是浏览器或第三方客户端 API。

## 认证方式

当前有两种访问方式：

- 主节点签名请求
  - `x-aster-access-key`
  - `x-aster-timestamp`
  - `x-aster-nonce`
  - `x-aster-signature`
- 预签名 query
  - `aster_access_key`
  - `aster_expires`
  - `aster_signature`

常规控制面接口都要求签名头；对象 GET / PUT 会按场景支持预签名 URL。

## 接口列表

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/capabilities` | 读取 follower 声明的协议能力 |
| `GET` | `/capacity` | 读取 follower 当前接收落点的容量观测状态 |
| `PUT` | `/binding` | 同步主节点维护的远端节点绑定信息 |
| `GET` | `/targets` | 列出当前绑定可用的远程存储目标 |
| `POST` | `/targets` | 创建远程存储目标 |
| `PATCH` | `/targets/{target_key}` | 更新远程存储目标 |
| `DELETE` | `/targets/{target_key}` | 删除远程存储目标 |
| `POST` | `/compose` | 把多个 part 对象拼成目标对象 |
| `GET` | `/objects` | 按前缀列举对象 key |
| `GET` | `/objects/{tail}/metadata` | 读取对象元信息 |
| `PUT` | `/objects/{tail}` | 上传对象内容 |
| `GET` | `/objects/{tail}` | 读取对象内容 |
| `HEAD` | `/objects/{tail}` | 探测对象是否存在并返回头信息 |
| `DELETE` | `/objects/{tail}` | 删除对象 |

`0.4.0` 已移除旧 `/ingress-profiles` 和 `/ingress-profiles/{target_key}` 兼容路径；primary 与 follower 必须统一使用 `/targets`。

## `GET /capabilities`

返回仍然走统一 JSON 包装，典型字段包括：

- `protocol_version`
- `min_supported_protocol_version`
- `server_version`
- `features`
- `browser_cors`
- `limits`
- `supports_list`
- `supports_range_read`
- `supports_stream_upload`
- `supports_capacity`

当前协议版本是 `v4`，最小支持版本也是 `v4`。`v4` 和 `v2` / `v3` 不再 wire-compatible：内部存储 JSON 包装里的顶层 `code` 已经从旧数字码改成稳定字符串 `ApiErrorCode`。跨过这个边界时，先同时升级 primary 和 follower，再绑定 remote 策略。

主节点在加载远端策略或刷新绑定时会做能力协商：

- `protocol_version` / `min_supported_protocol_version` 必须和本地支持区间有交集，当前就是 `v4-v4`
- 基础远端策略要求 `object_get`、`object_head`、`object_put`、`object_delete`、`metadata`、`range_get`、`accept_ranges_header`、`list`、`compose`
- 如果远端策略启用浏览器预签名下载，`browser_cors` 必须声明允许 `range` 请求头，并暴露 `Accept-Ranges`、`Content-Range`、`Content-Length`
- 如果远端策略启用浏览器预签名上传，`browser_cors` 必须声明允许 `content-type` 请求头，并暴露 `ETag`

当前 follower 返回的 `browser_cors.allowed_headers` 至少包含 `content-type`、`range`；`browser_cors.exposed_headers` 会覆盖 GET/PUT 预签名所需的缓存、Range、长度、类型和 ETag 响应头。

## `GET /capacity`

返回 follower 当前 ingress driver 的 `StorageCapacityInfo`：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "capacity": {
      "status": "supported",
      "total_bytes": 1099511627776,
      "available_bytes": 549755813888,
      "used_bytes": 549755813888,
      "source": "local_filesystem",
      "observed_at": "2026-05-28T12:00:00Z"
    }
  }
}
```

实现约定：

- follower 直接调用当前 ingress driver 的 `capacity_info()`
- local ingress 通常返回真实文件系统容量
- S3 ingress 明确返回 `StorageErrorKind::Unsupported`，primary 侧会把它转换成用户可见的 `unsupported` 容量状态
- 这个接口只用于管理端容量观测和迁移 preflight，不在上传 / 下载热路径里调用

## `PUT /binding`

主节点会用这条接口把 follower 绑定信息同步过去，请求体字段包括：

- `name`
- `is_enabled`

这条接口只更新绑定元信息，不直接搬运对象数据。对象命名空间来自 follower 本地保存的 master binding，不由这条请求体传入。

## 远程存储目标管理

这组接口用于 primary 管理 follower 侧的远程存储目标，控制后续对象写入实际落到 follower 本地还是 follower 管理的 S3。当前请求 / 响应 DTO 使用 `target_key` 字段名。

创建本地目标的请求体形态：

```json
{
  "driver_type": "local",
  "name": "local-default",
  "base_path": "data/storage",
  "max_file_size": 0,
  "is_default": true
}
```

创建 S3 profile 的请求体形态：

```json
{
  "driver_type": "s3",
  "name": "edge-s3",
  "endpoint": "https://s3.example.com",
  "bucket": "aster-edge",
  "access_key": "AKIA...",
  "secret_key": "...",
  "base_path": "objects/",
  "max_file_size": 0,
  "is_default": false
}
```

更新接口使用扁平字段，支持修改 `name`、`driver_type`、连接参数、`base_path`、`max_file_size` 和 `is_default`。这些控制面接口只接受主节点签名头，不使用预签名 query。

## `POST /compose`

这条接口用于把多个上传 part 合成为最终对象，请求体包括：

- `target_key`
- `part_keys`
- `expected_size`

成功后返回 `bytes_written`。实现上会在拼接成功后清理被消费的 part 对象。

## 对象读写

### `PUT /objects/{tail}`

写入一个对象。请求必须带 `Content-Length`，follower 会按 ingress 策略检查对象大小上限。

### `GET /objects/{tail}`

返回原始对象字节流，不走 JSON 包装。

可选 query：

- `offset`
- `length`
- `response-cache-control`
- `response-content-disposition`
- `response-content-type`

也就是说，这条接口既支持整对象读取，也支持范围读取和响应头覆写。范围读取也可以通过标准 `Range: bytes=...` 请求头触发；返回部分内容时使用 `206 Partial Content`。

### `HEAD /objects/{tail}`

返回对象是否存在以及基础响应头，常用于轻量探测。

### `GET /objects/{tail}/metadata`

返回统一 JSON 包装，`data` 里当前主要有：

- `size`
- `content_type`

### `DELETE /objects/{tail}`

删除对象，成功时返回空的统一成功响应。

## 列举

### `GET /objects`

支持 `prefix` query，返回匹配前缀下的对象 key 列表。

当前返回体里的 `items` 是 follower 绑定命名空间下的相对 key，不会把 provider 内部前缀原样暴露回去。

## 什么时候看这页

下面这些情况，不要再去普通 `files` / `upload` / `shares` 路由里瞎找：

- 主节点写远端存储节点失败
- 受管 follower 拼 part 失败
- 远端节点健康正常，但对象列举 / 读取 / 删除异常
- 远端节点 enrollment 成功后，后续对象同步行为不对
