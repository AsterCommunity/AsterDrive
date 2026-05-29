# 内部存储协议（Follower）

这组接口是主节点和 follower 节点之间的内部对象存储协议，不是给浏览器前端或第三方普通客户端用的公开 API。

以下路径都相对于：

```text
/api/v1/internal/storage
```

并且只会在 `follower` 节点注册。

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
| `GET` | `/ingress-profiles` | 列出当前绑定可用的受管 ingress profile |
| `POST` | `/ingress-profiles` | 创建受管 ingress profile |
| `PATCH` | `/ingress-profiles/{profile_key}` | 更新受管 ingress profile |
| `DELETE` | `/ingress-profiles/{profile_key}` | 删除受管 ingress profile |
| `POST` | `/compose` | 把多个 part 对象拼成目标对象 |
| `GET` | `/objects` | 按前缀列举对象 key |
| `GET` | `/objects/{tail}/metadata` | 读取对象元信息 |
| `PUT` | `/objects/{tail}` | 上传对象内容 |
| `GET` | `/objects/{tail}` | 读取对象内容 |
| `HEAD` | `/objects/{tail}` | 探测对象是否存在并返回头信息 |
| `DELETE` | `/objects/{tail}` | 删除对象 |

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

当前协议版本是 `v3`，向后兼容 `v2`，最小支持版本仍是 `v2`。`v3` 在能力协商中声明 `supports_capacity`；兼容窗口内主节点仍可连接不支持容量观测的 `v2` follower。主节点在加载远端策略或刷新绑定时会做能力协商：

- `protocol_version` / `min_supported_protocol_version` 必须和本地支持区间有交集
- 基础远端策略要求 `object_get`、`object_head`、`object_put`、`object_delete`、`metadata`、`range_get`、`accept_ranges_header`、`list`、`compose`
- 如果远端策略启用浏览器预签名下载，`browser_cors` 必须声明允许 `range` 请求头，并暴露 `Accept-Ranges`、`Content-Range`、`Content-Length`
- 如果远端策略启用浏览器预签名上传，`browser_cors` 必须声明允许 `content-type` 请求头，并暴露 `ETag`

当前 follower 返回的 `browser_cors.allowed_headers` 至少包含 `content-type`、`range`；`browser_cors.exposed_headers` 会覆盖 GET/PUT 预签名所需的缓存、Range、长度、类型和 ETag 响应头。

## `GET /capacity`

返回 follower 当前 ingress driver 的 `StorageCapacityInfo`：

```json
{
  "code": 0,
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

## Ingress Profile 管理

这组接口用于 primary 管理 follower 侧的受管 ingress profile，控制后续对象写入实际落到 follower 本地还是 follower 管理的 S3。

创建本地 profile 的请求体形态：

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
