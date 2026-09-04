# 内部存储协议（Follower）

这组接口是主节点和 follower 节点之间的内部对象存储协议，不是给浏览器前端或第三方普通客户端用的公开 API。

这页描述的是 follower 侧实际执行对象读写的 `/api/v1/internal/storage/*`。primary 侧另外提供独立的 binding 控制面 `/api/v1/internal/remote-node-control/*`，以及让不能被 primary 直连的 follower 主动连回来的 reverse tunnel 传输入口 `/api/v1/internal/remote-tunnel/*`。

以下路径都相对于：

```text
/api/v1/internal/storage
```

并且只会在 `follower` 节点注册。

## Direct 与 Reverse Tunnel

远端节点协议分成三层，别混在一起看：

- `/api/v1/internal/storage/*` 只在 follower 注册，是实际对象读写、绑定同步、远程存储目标管理的协议。
- `/api/v1/internal/remote-node-control/*` 只在 primary 注册，是 follower 主动拉取 binding desired state 的独立控制面。
- `/api/v1/internal/remote-tunnel/*` 只在 primary 注册，是 reverse tunnel 的对象请求传输入口，不承担 binding 状态收敛。

`direct` 模式下，primary 直接请求 follower 的 `/api/v1/internal/storage/*`。`reverse_tunnel` 模式下，primary 把同样的内部存储请求登记到 tunnel registry，follower 主动向 primary 轮询或建立 WebSocket 连接取走请求，再在本地调用内部存储处理逻辑并回传响应。

`auto` 由 primary 按当前 `base_url` 解析成明确的 `resolved_transport`：非空 `base_url` 使用 `direct`，空 `base_url` 使用 `reverse_tunnel`。follower 会为所有 binding 周期性请求独立控制面，包括 disabled 和解析为 direct 的 binding；因此 transport 切换不依赖切换前或切换后的对象数据路径。

binding 状态按 revision 收敛：

1. primary 持有 `name`、`is_enabled`、`resolved_transport` 和单调递增的 `desired_revision`；只有这些 follower 可观察状态变化时才递增 revision。
2. follower 通过 `GET /api/v1/internal/remote-node-control/binding-state?applied_revision=N` 拉取 primary 的 desired state，并以 primary 返回值为权威持久化本地 binding。
3. follower 刷新运行时 binding registry，再启动、停止或替换 reverse tunnel worker；只有 registry 和 worker topology 都完成后，才把本地 `applied_revision` 标记为当前 `desired_revision`。
4. 下一轮 pull 携带新的 `applied_revision`，作为对 primary 的隐式 ACK。primary 只接受不高于当前 desired revision 的 ACK；即使 follower 本地 revision 更高，primary 当前状态仍会覆盖本地状态并重新收敛。

当前 binding 控制面入口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/api/v1/internal/remote-node-control/binding-state` | follower 拉取 desired state，并通过 `applied_revision` query 回报已应用 revision |

primary 侧 reverse tunnel 当前入口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/api/v1/internal/remote-tunnel/poll` | follower 长轮询待处理请求 |
| `POST` | `/api/v1/internal/remote-tunnel/complete` | follower 回传轮询请求的处理结果 |
| `GET` | `/api/v1/internal/remote-tunnel/connect` | follower 建立 WebSocket 流式 tunnel |

binding 控制面和 reverse tunnel 接口都使用远端节点签名鉴权，不是浏览器或第三方客户端 API。binding 控制面允许 direct、reverse tunnel 和 disabled 节点访问；reverse tunnel 数据面还会校验节点已启用且当前 `resolved_transport` 为 `reverse_tunnel`，因此解析为 direct 的节点即使签名有效，也不能访问 `poll`、`complete` 或 `connect`。

这次 binding 控制面扩展没有改变 tunnel frame wire format：WebSocket frame version 仍为 `1`。它也没有抬高 internal storage 协议版本，当前仍是 `v5`、最低兼容 `v4`；滚动升级通过 optional capability 和 JSON 字段默认值处理。

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
| `GET` | `/capacity` | 读取 follower 当前远端存储目标的容量观测状态 |
| `PUT` | `/binding` | 向未声明 binding control pull capability 的 legacy follower 推送绑定信息 |
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

当前协议版本和最低兼容版本都是 `v6`。内部存储 JSON 包装里的顶层 `code` 使用稳定字符串 `ApiErrorCode`，不再使用旧数字码。绑定 remote 策略前，primary 和 follower 必须运行同一代协议。

当前 follower 在 `features.binding_state_pull` 显式声明支持独立 binding control pull。该 capability 缺省为 `false`，所以 primary 可以区分滚动升级边界：

- capability 为 `true`：primary 不再主动 push binding state，完全由 follower pull 收敛。
- capability 缺失或为 `false`：primary 保留 legacy `PUT /binding` push。
- 新 follower 对旧 primary 请求 binding-state 得到 `404` 时，保留本地 legacy push 状态并继续运行。

`v6` 在能力响应中通过 `remote_storage_target.connector_ids` 声明远程存储目标 connector。primary 只展示 follower 声明且当前版本已注册 descriptor 的 connector；未知的未来 connector id 会被保留为协议数据，但不会自动变成可配置项。

主节点在加载远端策略或刷新绑定时会做能力协商：

- `protocol_version` / `min_supported_protocol_version` 必须和本地支持区间有交集，当前本地区间是 `v6-v6`
- 基础远端策略要求 `object_get`、`object_head`、`object_put`、`object_delete`、`metadata`、`range_get`、`accept_ranges_header`、`list`、`compose`
- 如果远端策略启用浏览器预签名下载，`browser_cors` 必须声明允许 `range` 请求头，并暴露 `Accept-Ranges`、`Content-Range`、`Content-Length`
- 如果远端策略启用浏览器预签名上传，`browser_cors` 必须声明允许 `content-type` 请求头，并暴露 `ETag`

当前 follower 返回的 `browser_cors.allowed_headers` 至少包含 `content-type`、`range`；`browser_cors.exposed_headers` 会覆盖 GET/PUT 预签名所需的缓存、Range、长度、类型和 ETag 响应头。

## `GET /capacity`

返回 follower 当前远端存储目标 driver 的 `StorageCapacityInfo`：

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

- follower 直接调用当前 target driver 的 `capacity_info()`
- local target 通常返回真实文件系统容量
- S3 target 明确返回 `StorageErrorKind::Unsupported`，primary 侧会把它转换成用户可见的 `unsupported` 容量状态
- 这个接口只用于管理端容量观测和迁移 preflight，不在上传 / 下载热路径里调用

## `PUT /binding`

这条接口只服务于没有声明 `features.binding_state_pull` 的 legacy follower。新 primary 会用它把 binding desired state push 到旧 follower，请求体字段包括：

- `name`
- `is_enabled`
- `resolved_transport`：`direct` 或 `reverse_tunnel`；字段缺省时按 `reverse_tunnel` 处理
- `desired_revision`：primary desired state revision；字段缺省时按 `1` 处理

这条接口只更新绑定元信息，不直接搬运对象数据。对象命名空间来自 follower 本地保存的 master binding，不由这条请求体传入。

legacy push 只在当前或切换前确实存在可用数据路径时尝试；没有可用路径时跳过，支持新控制面的 follower 仍会自行 pull 收敛。兼容 push 的删除条件是最低支持 follower 版本都显式声明 `binding_state_pull`。

## 远程存储目标管理

这组接口用于 primary 管理 follower 侧的远程存储目标，控制后续对象写入实际落到 follower 本地还是 follower 管理的 S3。当前请求 / 响应 DTO 使用 `target_key` 字段名。

创建本地目标的请求体形态：

```json
{
  "name": "local-default",
  "connection": {
    "connector_config": {
      "format_version": 1,
      "connector_id": "asterdrive.storage.local",
      "schema_version": 1,
      "values": { "base_path": "data/storage" }
    },
    "credential": { "mode": "none" }
  },
  "is_default": true
}
```

创建 S3 目标的请求体形态：

```json
{
  "name": "edge-s3",
  "connection": {
    "connector_config": {
      "format_version": 1,
      "connector_id": "asterdrive.storage.s3",
      "schema_version": 1,
      "values": {
        "endpoint": "https://s3.example.com",
        "bucket": "aster-edge",
        "base_path": "objects/"
      }
    },
    "credential": {
      "mode": "static",
      "values": { "access_key": "AKIA...", "secret_key": "..." }
    }
  },
  "is_default": false
}
```

创建和更新接口使用共享的 `StorageConnectionInput` envelope，在 `name`、`is_default` 之外承载 connector 自己的配置和凭据；实际可选项受到 follower 的 `remote_storage_target.connector_ids` 能力声明约束。这些控制面接口只接受主节点签名头，不使用预签名 query。

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

支持以下 query：

- `prefix`：只返回匹配前缀的对象 key。
- `cursor`：从相对位置继续列举；通常使用上一页返回的 `next_cursor`。
- `limit`：请求页大小，必须大于 `0`；服务端会把它钳制到内部页大小上限。

新客户端应始终发送 `limit`。响应形态如下：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "items": ["files/part-001", "files/part-002"],
    "next_cursor": 2
  }
}
```

只有后面仍有数据时才会返回 `next_cursor`。不传 `limit` 时，follower 保留旧客户端使用的无分页响应，并一次返回全部匹配项。

当前返回体里的 `items` 是 follower 绑定命名空间下的相对 key，不会把 provider 内部前缀原样暴露回去。

## 什么时候看这页

下面这些情况，不要再去普通 `files` / `upload` / `shares` 路由里瞎找：

- 主节点写远端存储节点失败
- 受管 follower 拼 part 失败
- 远端节点健康正常，但对象列举 / 读取 / 删除异常
- 远端节点 enrollment 成功后，后续对象同步行为不对
