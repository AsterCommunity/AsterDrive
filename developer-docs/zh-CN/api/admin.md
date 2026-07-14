# 管理 API

以下路径都相对于 `/api/v1`。除存储 OAuth provider 回调 `/admin/policies/storage-authorization/callback` 外，本页接口都需要管理员权限。

这页只保留管理端最值得记住的接口分组；更偏使用体验的内容见 [管理面板](../../../docs/guide/admin-console.md)。

当前大多数“列表类”管理员接口都已经是 offset 分页：

- `/admin/policies`
- `/admin/policy-groups`
- `/admin/remote-nodes`
- `/admin/users`
- `/admin/teams`
- `/admin/teams/{id}/members`
- `/admin/shares`
- `/admin/tasks`
- `/admin/files`
- `/admin/file-blobs`
- `/admin/config`
- `/admin/locks`
- `/admin/audit-logs`

这些分页接口的默认排序不完全一样，具体字段以 DTO 为准。常见默认值是：

- 用户、团队、存储策略、策略组、远端节点、分享、审计日志：按 `created_at desc`
- 后台任务：按 `updated_at desc`
- 锁：按 `id asc`
- 团队成员：按 `role asc`

## 总览面板

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/overview` | 读取管理后台总览所需的聚合数据 |

当前返回内容包含：

- 总用户数、启用中用户、禁用用户
- 总文件数、总文件字节数、总 blob 数、总 blob 字节数、总分享数
- 今日审计事件数、今日新增用户数、今日上传数、今日新分享数
- 最近 N 天日报（默认 7）
- 最近一批审计事件
- 最近一批后台任务 / 系统运行任务

支持这些查询参数：

- `days`：日报天数，默认 `7`，最大 `90`
- `timezone`：IANA 时区名，例如 `UTC`、`Asia/Shanghai`
- `event_limit`：最近活动返回数量，默认 `8`，最大 `50`

这个接口当前的日报和“最近活动”都基于审计日志统计，因此如果审计日志关闭，对应数据会偏少或为 0。总量类指标（用户 / 文件 / blob / 分享 / 字节数）不依赖审计日志。

## 存储策略

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/policies` | 列出全部存储策略 |
| `POST` | `/admin/policies` | 创建存储策略 |
| `GET` | `/admin/policies/{id}` | 读取策略详情 |
| `GET` | `/admin/policies/{id}/capacity` | 读取策略容量观测状态 |
| `PATCH` | `/admin/policies/{id}` | 更新策略 |
| `DELETE` | `/admin/policies/{id}` | 删除策略 |
| `GET` | `/admin/policies/storage-drivers` | 列出存储 connector descriptor |
| `GET` | `/admin/policies/storage-credential-providers` | 列出存储 OAuth 凭据 provider |
| `POST` | `/admin/policies/{id}/test` | 测试已保存策略 |
| `POST` | `/admin/policies/{id}/action` | 对已保存策略执行存储 action |
| `POST` | `/admin/policies/{id}/promote-s3-driver` | 把通用 S3-compatible 策略提升为受支持的专用驱动 |
| `POST` | `/admin/policies/{id}/storage-authorization/start` | 为策略启动存储 OAuth 授权 |
| `GET` | `/admin/policies/{id}/storage-credentials` | 列出策略已保存的存储 OAuth 凭据 |
| `POST` | `/admin/policies/{id}/storage-credentials/{provider}/validate` | 验证策略已保存的存储 OAuth 凭据 |
| `GET` | `/admin/policies/storage-authorization/callback` | 存储 OAuth provider 回调入口，不要求管理员 JWT，处理后重定向到管理页 |
| `POST` | `/admin/policies/test` | 用临时参数测试连接 |
| `POST` | `/admin/policies/action` | 用草稿策略参数执行存储 action |

### 创建策略示例

```json
{
  "name": "archive-s3",
  "driver_type": "s3",
  "endpoint": "https://s3.example.com",
  "bucket": "archive",
  "access_key": "AKIA...",
  "secret_key": "...",
  "base_path": "asterdrive/",
  "max_file_size": 10737418240,
  "chunk_size": 10485760,
  "is_default": false
}
```

当前实现注意点：

- `driver_type` 当前支持 `local`、`s3`、`sftp`、`azure_blob`、`tencent_cos`、`remote` 和 `one_drive`
- `GET /admin/policies/storage-drivers` 返回 `StorageConnectorDescriptor` 列表，前端应以 descriptor 的 `capabilities`、`fields`、`upload_workflows`、`actions` 和 `credential_mode` 决定表单、连接测试、上传/下载策略和操作入口，不要在前端维护一份 driver-type 能力矩阵
- 创建和更新都会采用请求里的 `chunk_size`
- `options` 当前承载策略级行为：
  - S3-compatible / Azure Blob / Tencent COS 这类对象存储 connector 使用 `object_storage_upload_strategy` / `object_storage_download_strategy` 表达传输策略，例如 `{"object_storage_upload_strategy":"presigned","object_storage_download_strategy":"presigned"}`；旧 `s3_upload_strategy` / `s3_download_strategy` JSON 字段仍作为兼容 alias 接受
  - Remote 上传下载策略，例如 `{"remote_upload_strategy":"presigned","remote_download_strategy":"presigned"}`
  - 本地策略的内容去重开关 `content_dedup`
  - 通用 S3 path-style 访问开关：`s3_path_style`，默认 `true`
  - S3 连接 / 读取 / 操作超时：`s3_connect_timeout_secs`、`s3_read_timeout_secs`、`s3_operation_timeout_secs`
  - 存储原生缩略图 / 图片预览：`storage_native_processing_enabled`、`thumbnail_processor`、`thumbnail_extensions`
  - 存储原生媒体元数据：`storage_native_media_metadata_enabled`、`media_metadata_extensions`
  - OneDrive 位置选项：`onedrive_account_mode`、`onedrive_tenant`、`onedrive_site_id`、`onedrive_drive_id`、`onedrive_group_id`、`onedrive_root_item_id`
  - SFTP 主机密钥固定：`sftp_host_key_fingerprint`
- `application_config.microsoft_graph` 用于保存 OneDrive / Microsoft Graph 应用配置；client secret 只写入加密存储，API 响应只暴露 `client_secret_configured`
- `driver_type = "azure_blob"` 使用 Azure Blob Block Blob 能力，预签名上传使用 SAS URL，前端直传时需要带 `x-ms-blob-type: BlockBlob`
- `driver_type = "one_drive"` 使用 Microsoft Graph OAuth 凭据，授权前需要先保存策略和 `application_config.microsoft_graph`
- `driver_type = "sftp"` 使用 SSH 用户名 / 密码连接 SFTP 服务器；Endpoint 支持 `sftp://host:port`、裸 `host` 和 `host:port`，远程根目录放在 `base_path`。未知或不匹配的 SSH 主机密钥会以 `StorageErrorKind::Precondition` 拒绝，并通过诊断提示 actual / expected 指纹；确认后的指纹保存在 `options.sftp_host_key_fingerprint`。
- `driver_type = "tencent_cos"` 普通读写复用 S3-compatible 对象存储路径，会校验 Tencent COS endpoint 形态；策略启用后可通过 COS CI 暴露原生缩略图、图片预览和媒体元数据能力
- 内置 Local、S3-compatible、SFTP、Azure Blob、OneDrive 和 Remote 驱动不暴露存储原生缩略图、图片预览或媒体元数据能力
- 旧配置 `{"presigned_upload":true}` 仍兼容，等价于 S3 预签名上传策略
- `POST /admin/policies/{id}/promote-s3-driver` 当前支持把通用 `s3` 策略提升为 `tencent_cos`。请求体必须包含目标驱动和当前 endpoint / bucket，例如 `{ "target_driver_type": "tencent_cos", "endpoint": "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com", "bucket": "bucket-1250000000" }`。提升时不允许改变 bucket；若该策略还有活动上传 session，或目标驱动不能接受当前 endpoint / bucket 组合，会直接拒绝。
- REST 已经可以通过 `allowed_types` 管理策略允许的 MIME / 类型列表；不传时创建会使用空列表，更新会保持原值
- `driver_type = "remote"` 时需要绑定 `remote_node_id`，远端节点本身通过 `/admin/remote-nodes` 管理
- 当前 `PATCH` 不能修改 `driver_type`
- `GET /admin/policies` 支持 `limit`、`offset`、`sort_by`、`sort_order`
- `GET /admin/policies/{id}/capacity` 返回 `StoragePolicyCapacityInfo`，其中 `capacity.status` 为 `supported` / `unsupported` / `unavailable`：
  - Local 驱动通过文件系统容量接口返回 `total_bytes`、`available_bytes`、`used_bytes`
  - S3-compatible 和 Azure Blob 驱动明确返回 `StorageErrorKind::Unsupported`，服务层转换成 `unsupported` 状态，不伪造 bucket / account 容量
  - OneDrive 驱动通过 Microsoft Graph drive quota 返回容量信息
  - Remote 驱动通过 follower 内部协议 `/internal/storage/capacity` 转发实际接收落点的容量能力
- `DELETE /admin/policies/{id}` 支持 `?force=true`；这只会强制清理仍引用该策略的上传 session，仍有 blob 或策略组项引用时照样拒绝删除。若清理后还有临时对象或 multipart upload 需要延后处理，会创建 `storage_policy_temp_cleanup` 后台任务

### 存储连接测试

`POST /admin/policies/{id}/test` 和 `POST /admin/policies/test` 成功时返回普通空成功响应：

```json
{
  "code": "success",
  "msg": "",
  "data": {}
}
```

连接失败时不再返回 `StoragePolicyProbeResult` 这类成功 payload，而是走标准错误响应，并通过 `error.diagnostic` 暴露脱敏诊断：

```json
{
  "code": "storage.auth_failed",
  "msg": "storage authentication failed",
  "error": {
    "retryable": false,
    "diagnostic": {
      "kind": "auth",
      "message": "credentials were rejected by the storage provider"
    }
  }
}
```

草稿测试请求支持可选 `policy_id`。编辑已保存策略时，如果 `access_key`、`secret_key` 等敏感字段为空，S3-compatible、SFTP、Azure Blob 和 Tencent COS connector 会从该策略已保存凭据补齐空白字段；新建未保存策略时仍必须传完整凭据。

### 存储 OAuth 凭据

这组接口当前主要服务 OneDrive / Microsoft Graph connector：

- `GET /admin/policies/storage-credential-providers` 会列出 provider。当前 `microsoft_graph` 可用，`google_drive` 只是预留且 `supported = false`
- 创建或更新 OneDrive 策略时，先通过 `application_config.microsoft_graph` 保存 Microsoft Graph app 配置；client secret 会加密落库
- `POST /admin/policies/{id}/storage-authorization/start` 只需要发送 provider，后端会复用已保存的 application config 发起授权
- 授权成功后，回调入口会写入 `storage_policy_credentials` 并重定向到 `/admin/policies?storage_authorization=success&policy_id=...`；这条 callback 不要求管理员 JWT，因为它由 Microsoft Graph 等 provider 从浏览器跳回
- `GET /admin/policies/{id}/storage-credentials` 返回凭据状态、租户、账号标签、scope、过期和刷新时间，不返回 access token / refresh token
- `POST /admin/policies/{id}/storage-credentials/{provider}/validate` 会用已保存凭据验证实际可用性，并按结果更新 `authorized`、`reauth_required`、`permission_denied` 或 `invalid`

启动授权请求示例：

```http
POST /api/v1/admin/policies/12/storage-authorization/start
```

```json
{
  "provider": "microsoft_graph"
}
```

成功响应里的 `authorization_url` 交给浏览器跳转：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "authorization_url": "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?...",
    "expires_in": 300,
    "provider": "microsoft_graph",
    "microsoft_graph": {
      "cloud": "global",
      "tenant": "common",
      "client_id": "00000000-0000-0000-0000-000000000000",
      "client_secret_configured": true,
      "scopes": ["offline_access", "Files.ReadWrite.All", "Sites.ReadWrite.All"]
    }
  }
}
```

### 存储策略 action

存储策略 action 是给存储驱动暴露可选管理能力的统一入口。新增默认能力时优先扩展 `StoragePolicyActionType` 枚举，不再为每个云厂商动作单独挂专用 HTTP 路由。后续插件化能力如果需要专有 schema，可以在插件自己的 schema 路由里描述参数，但执行入口仍应尽量保持 action 化。

当前支持的 action：

| action | 支持驱动 | 是否改远端状态 | 说明 |
| --- | --- | --- | --- |
| `configure_tencent_cos_cors` | `tencent_cos` | 是 | 按 `public_site_url` 自动配置腾讯云 COS bucket CORS |

已保存策略请求：

```http
POST /api/v1/admin/policies/12/action
```

```json
{
  "action": "configure_tencent_cos_cors"
}
```

草稿策略请求：

```http
POST /api/v1/admin/policies/action
```

```json
{
  "action": "configure_tencent_cos_cors",
  "policy_id": 12,
  "driver_type": "tencent_cos",
  "endpoint": "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
  "bucket": "bucket-1250000000",
  "access_key": "AKID...",
  "secret_key": "...",
  "base_path": "prod/"
}
```

`configure_tencent_cos_cors` 的参数来源和行为：

- 请求体不接受 `allowed_origin` 或 `allowed_origins`
- 草稿请求的连接字段是平铺字段，不包在 `policy` 对象里
- `policy_id` 可选，只用于编辑已保存策略时的草稿 action；如果 `access_key` 或 `secret_key` 为空，后端会从该已保存策略补齐空白密钥字段
- 不传 `policy_id` 时，草稿 action 必须自己携带完整凭证；这用于新建未保存策略或纯临时参数测试
- 后端从运行时配置 `public_site_url` 读取全部公开站点来源，并写成同一条 COS CORS rule 的多个 `AllowedOrigin`
- 如果 `public_site_url` 为空，返回 `policy.action_parameter_required`
- 如果策略不是 `tencent_cos`，返回 `policy.action_unsupported`
- AsterDrive 使用固定 rule id `asterdrive-presigned-access`
- 腾讯云 COS 没有原子追加 CORS rule 的接口；实现是 `GET Bucket cors` 后保留其他 rule、替换同 ID rule，再 `PUT Bucket cors` 写回完整配置
- `PUT Bucket cors` 必须带 `Content-MD5`，服务端会对 XML body 计算 MD5 并参与 COS 签名
- 成功执行会写管理员审计日志，action 字符串统一是 `admin_trigger_storage_action`，details 里包含 `action`、`driver_type`、`used_draft_values` 和 `mutates_remote_state`

成功响应示例：

```json
{
  "code": "success",
  "msg": "",
  "data": {
    "action": "configure_tencent_cos_cors",
    "tencent_cos_cors": {
      "rule_id": "asterdrive-presigned-access",
      "allowed_origins": [
        "https://drive.example.com",
        "https://panel.example.com"
      ],
      "request_id": "NmEy...",
      "preserved_rule_count": 1,
      "replaced_existing_rule": true,
      "response_vary": true
    }
  }
}
```

常见错误码：

| 错误码 | 含义 |
| --- | --- |
| `policy.action_unsupported` | 当前 action 不支持该策略或驱动类型 |
| `policy.action_parameter_required` | action 需要的后端配置缺失，例如 `public_site_url` 为空 |
| `policy.action_parameter_invalid` | action 参数或后端派生参数非法 |
| `storage.auth_failed` | COS 凭证错误或签名失败 |
| `storage.permission_denied` / `storage.permission` | COS CAM 权限不足，例如缺少 `name/cos:PutBucketCORS` |
| `storage.misconfigured` | COS 返回配置类错误，例如 bucket、endpoint、请求头或 XML 不符合要求 |
| `storage.transient_failure` / `storage.transient` | COS 或网络临时失败，可提示稍后重试 |

## 存储迁移

管理员可以通过这组接口创建和恢复跨存储策略的 blob 迁移任务。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `POST` | `/admin/storage-migrations` | 创建存储策略迁移任务 |
| `POST` | `/admin/storage-migrations/dry-run` | 预检查迁移计划，不创建任务 |
| `POST` | `/admin/storage-migrations/{task_id}/resume` | 继续执行已有迁移任务 |

创建请求体：

```json
{
  "source_policy_id": 1,
  "target_policy_id": 2,
  "delete_source_after_success": false
}
```

当前实现注意点：

- `source_policy_id` 和 `target_policy_id` 都必须大于 0，而且不能相同
- `delete_source_after_success` 目前只保留在请求体里，第一版实现会直接拒绝 `true`
- `dry-run` 会检查目标策略是否支持 stream upload，并尝试对目标存储做一次写删探测
- `dry-run` 会返回 `target_capacity_check` 和 `target_capacity`。容量判断使用预计仍需复制的 blob 总字节数，不使用源策略总字节数；目标已有的 content SHA-256 blob 不计入预计复制量。
- `target_capacity_check = "insufficient"` 会阻止创建任务；`unsupported` 和 `unavailable` 只会进入 warnings，调用方需要提示管理员自行确认目标容量。
- `dry-run` 会返回 `opaque_key_conflict_count`，表示源策略 opaque blob key 在目标策略已经存在，执行时需要为这些源 blob 生成新的迁移 key。
- 任务本身会落到 `BackgroundTaskKind::StoragePolicyMigration`
- 这类任务有独立 checkpoint，恢复入口只接受该 kind
- 迁移任务完成后，结果里会包含扫描、迁移、合并、跳过、失败、迁移字节数和 `renamed_opaque_blobs`
- 跨策略匹配只允许 content SHA-256 blob 参与：`hash` 必须是 64 位十六进制，且目标 blob 的 `hash` 和 `size` 都匹配。Opaque blob 永不跨策略 merge；如果目标策略已有同 opaque key，执行阶段会把源 blob 改成新的 `migration-...` key 并复制到新路径。

## 远端节点

远端节点是 primary 管理的 follower 存储节点，主要给 `driver_type = "remote"` 的存储策略使用。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/remote-nodes` | 分页列出受管 follower 节点 |
| `POST` | `/admin/remote-nodes` | 创建远端节点记录 |
| `GET` | `/admin/remote-nodes/{id}` | 读取远端节点详情 |
| `PATCH` | `/admin/remote-nodes/{id}` | 更新名称、地址、传输模式或启用状态 |
| `DELETE` | `/admin/remote-nodes/{id}` | 删除远端节点；仍被策略引用时会拒绝 |
| `POST` | `/admin/remote-nodes/{id}/test` | 测试已保存远端节点连接 |
| `POST` | `/admin/remote-nodes/test` | 用临时参数测试远端节点连接 |
| `POST` | `/admin/remote-nodes/{id}/enrollment-token` | 生成 follower enrollment 命令 |
| `GET` | `/admin/remote-nodes/{id}/storage-target-drivers` | 列出 follower 侧远程存储目标可用 driver descriptor |
| `GET` | `/admin/remote-nodes/{id}/storage-targets` | 列出 follower 侧远程存储目标 |
| `POST` | `/admin/remote-nodes/{id}/storage-targets` | 创建 follower 侧远程存储目标 |
| `PATCH` | `/admin/remote-nodes/{id}/storage-targets/{target_key}` | 更新 follower 侧远程存储目标 |
| `DELETE` | `/admin/remote-nodes/{id}/storage-targets/{target_key}` | 删除 follower 侧远程存储目标 |

`0.4.0` 已移除旧 `/ingress-profile-drivers` 和 `/ingress-profiles` 兼容路径；客户端必须使用 `/storage-target-drivers` 和 `/storage-targets`。DTO 字段名使用 `target_key`。

创建远端节点示例：

```json
{
  "name": "edge-sh-01",
  "base_url": "",
  "transport_mode": "auto",
  "is_enabled": true
}
```

当前实现注意点：

- `transport_mode` 支持 `direct`、`reverse_tunnel`、`auto`；创建时不传默认 `direct`，更新时可通过 `PATCH /admin/remote-nodes/{id}` 修改
- `direct` 要求 `base_url` 可由 primary 直连；如果远端策略引用这个节点，`base_url` 不能为空
- `reverse_tunnel` 不要求 follower 被 primary 直连，但 follower 必须能主动连到 primary 的 `/api/v1/internal/remote-tunnel/*`
- `auto` 在 `base_url` 非空时按 direct 连接，`base_url` 为空时走 reverse tunnel
- `base_url` 为空时通常走 enrollment 流程，由 follower 兑换绑定信息后再完成实际接入
- `/enrollment-token` 返回给 CLI 使用的命令信息；follower 会再调用公开 enrollment 接口完成 redeem / ack
- `GET /admin/remote-nodes` 支持 `limit`、`offset`、`sort_by`、`sort_order`
- 远端节点详情会返回 `transport_mode`、`enrollment_status`、`last_error`、`capabilities`、`last_checked_at` 和 `tunnel`
- `tunnel` 当前包含 `status`、`last_error`、`last_seen_at`，用于管理端展示 reverse tunnel 在线状态
- reverse tunnel 模式不能配合 remote 浏览器预签名上传 / 下载策略使用。创建或更新远端策略、切换远端节点传输模式时，如果引用该节点的策略使用 `remote_upload_strategy = "presigned"` 或 `remote_download_strategy = "presigned"`，服务端会拒绝这个组合
- 远程存储目标的请求体和 follower 内部协议一致，见 [内部存储协议](./internal-storage.md)

## 外部认证提供商

外部认证 provider 由管理员配置，匿名登录入口只读取启用后的公开摘要。当前支持的 provider kind 是 `oidc`、`generic_oauth2`、`github`、`qq`、`google` 和 `microsoft`。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/external-auth/provider-kinds` | 列出服务端支持的外部认证类型 |
| `GET` | `/admin/external-auth/providers` | 分页列出外部认证提供商 |
| `POST` | `/admin/external-auth/providers` | 创建外部认证提供商 |
| `POST` | `/admin/external-auth/providers/test` | 用草稿参数测试 provider 配置 |
| `GET` | `/admin/external-auth/providers/{id}` | 读取 provider 详情 |
| `PATCH` | `/admin/external-auth/providers/{id}` | 更新 provider |
| `DELETE` | `/admin/external-auth/providers/{id}` | 删除 provider |
| `POST` | `/admin/external-auth/providers/{id}/test` | 测试已保存 provider |

创建 OIDC provider 示例：

```json
{
  "provider_kind": "oidc",
  "display_name": "Corp SSO",
  "icon_url": "/static/external-auth/corp.svg",
  "issuer_url": "https://idp.example.com",
  "client_id": "asterdrive",
  "client_secret": "secret",
  "scopes": "openid email profile",
  "enabled": true,
  "auto_provision_enabled": true,
  "auto_link_verified_email_enabled": true,
  "require_email_verified": true,
  "allowed_domains": ["example.com"]
}
```

创建 Generic OAuth2 provider 示例：

```json
{
  "provider_kind": "generic_oauth2",
  "display_name": "Logto",
  "icon_url": "/static/external-auth/oauth-logo.svg",
  "issuer_url": "https://id.example.com",
  "authorization_url": "https://id.example.com/oidc/auth",
  "token_url": "https://id.example.com/oidc/token",
  "userinfo_url": "https://id.example.com/oidc/me",
  "client_id": "asterdrive",
  "client_secret": "secret",
  "scopes": "openid email profile",
  "enabled": true,
  "auto_provision_enabled": false,
  "auto_link_verified_email_enabled": false,
  "require_email_verified": true
}
```

创建 Microsoft provider 示例：

```json
{
  "provider_kind": "microsoft",
  "display_name": "Microsoft",
  "icon_url": "/static/external-auth/microsoft-logo.svg",
  "options": {
    "microsoft": {
      "tenant": "organizations"
    }
  },
  "client_id": "00000000-0000-0000-0000-000000000000",
  "client_secret": "secret-value",
  "scopes": "openid profile email",
  "enabled": true,
  "auto_provision_enabled": false,
  "auto_link_verified_email_enabled": false,
  "require_email_verified": false
}
```

当前实现注意点：

- provider `key` 由服务端生成，登录路径使用 `/auth/external-auth/{kind}/{provider}/start`
- `issuer_url`、`authorization_url`、`token_url`、`userinfo_url` 必须是 HTTPS，localhost 例外；fragment 不允许
- `oidc` 支持 discovery；`generic_oauth2` 要手动配置 authorization、token 和 userinfo endpoint
- `github`、`qq`、`google` 和 `microsoft` 是专用 provider kind，端点和默认 claim 语义由后端 driver 固定；不要在请求里传这些 provider 不支持的手动 endpoint
- `microsoft` 的租户配置走 `options.microsoft.tenant`，支持 `common`、`organizations`、`consumers` 或具体 tenant UUID；读取详情时会返回规范化后的 `options`
- `options` 当前主要用于 provider 专用配置；非 Microsoft provider 传 `options.microsoft` 会被拒绝
- provider kind 的能力、默认 scope 和字段要求来自 `GET /admin/external-auth/provider-kinds`
- `client_secret` 在读取详情时会脱敏为 `***REDACTED***`，同时返回 `client_secret_configured`
- `auto_provision_enabled` 允许外部身份自动创建本地用户；`allowed_domains` 可限制邮箱域名
- `auto_link_verified_email_enabled` 允许用已验证邮箱自动绑定已有本地用户
- `require_email_verified` 打开后，未验证邮箱的外部身份需要走 `/auth/external-auth/email-verification/*`
- Generic OAuth2 有 `client_secret` 时使用 `client_secret_post` 发起一次 token exchange；不会为了探测认证方式重放 authorization code
- 专用 provider 的行为细节见 [外部认证模块](../external-auth.md)
- 创建、更新、删除和测试都会写管理员审计日志

## 文件与 Blob 管理

这组接口是管理员侧的文件 / blob 观测与维护入口，不是业务 API。

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/files` | 查看文件记录、所属 blob 和版本摘要 |
| `GET` | `/admin/files/{id}` | 查看单个文件和该文件的版本摘要 |
| `GET` | `/admin/file-blobs` | 查看 blob 记录、hash 类型和引用计数 |
| `GET` | `/admin/file-blobs/{id}` | 查看单个 blob 的文件与版本引用 |
| `POST` | `/admin/file-blobs/maintenance` | 为指定 blob 创建维护任务 |

当前实现注意点：

- `GET /admin/files` 支持 `name`、`blob_id`、`policy_id`、`owner_user_id`、`team_id`、`deleted`、`limit`、`offset`、`sort_by`、`sort_order`
- `GET /admin/file-blobs` 支持 `hash`、`policy_id`、`storage_path`、`ref_count_min`、`ref_count_max`、`size_min`、`size_max`、`limit`、`offset`、`sort_by`、`sort_order`
- `hash_kind` 只是观测派生字段：64 位十六进制 SHA-256 记为 `content_sha256`，其他值记为 `opaque`
- `POST /admin/file-blobs/maintenance` 请求体为 `{ "action": "...", "blob_ids": [...] }`，`blob_ids` 可省略；提供时必须非空且当前最多 1000 个
- `action` 支持 `integrity_check`、`ref_count_reconcile`、`orphan_cleanup`
- `integrity_check` 只检查对象是否存在以及对象大小是否和 blob 记录一致，不修改 blob
- `ref_count_reconcile` 会按当前文件和文件版本引用重新计算并修正 `ref_count`
- `orphan_cleanup` 会先重新计算引用，再只清理实际引用数和 `ref_count` 都为 0 的孤儿 blob

## 策略组

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/policy-groups` | 列出全部存储策略组 |
| `POST` | `/admin/policy-groups` | 创建策略组 |
| `GET` | `/admin/policy-groups/{id}` | 读取策略组详情 |
| `PATCH` | `/admin/policy-groups/{id}` | 更新策略组 |
| `DELETE` | `/admin/policy-groups/{id}` | 删除策略组 |
| `POST` | `/admin/policy-groups/{id}/migrate-assignments` | 迁移用户与团队的策略组绑定（仅更新 `policy_group_id`），响应统计分别返回受影响用户数、团队数与总绑定数 |

创建示例：

```json
{
  "name": "default-hot-cold",
  "description": "小文件走本地，大文件走对象存储",
  "is_enabled": true,
  "is_default": false,
  "items": [
    {
      "policy_id": 1,
      "priority": 10,
      "min_file_size": 0,
      "max_file_size": 10485760
    },
    {
      "policy_id": 2,
      "priority": 20,
      "min_file_size": 10485761,
      "max_file_size": 0
    }
  ]
}
```

当前实现注意点：

- 策略组至少要包含一个策略项
- 同一组里 `policy_id` 和 `priority` 都不能重复
- `is_default = true` 的组必须保持启用
- 已被用户或团队绑定的策略组不能直接删掉；被绑定时也不能随便禁用
- `/migrate-assignments` 会同时迁移用户和团队的策略组绑定，仅更新各自的 `policy_group_id`；响应中的 `affected_users`、`affected_teams`、`migrated_assignments` 分别表示受影响用户数、团队数与总绑定数
- `GET /admin/policy-groups` 支持 `limit`、`offset`、`sort_by`、`sort_order`

迁移请求体很简单：

```json
{
  "target_group_id": 9
}
```

## 用户

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/users` | 列出用户 |
| `POST` | `/admin/users` | 管理员直接创建用户 |
| `GET` | `/admin/users/{id}` | 获取用户详情 |
| `PATCH` | `/admin/users/{id}` | 更新角色、状态、总配额、策略组绑定和强制改密标记 |
| `PUT` | `/admin/users/{id}/password` | 管理员直接重置用户密码 |
| `DELETE` | `/admin/users/{id}/mfa` | 清空用户 MFA 配置并吊销会话 |
| `POST` | `/admin/users/{id}/sessions/revoke` | 吊销该用户所有现有会话 |
| `DELETE` | `/admin/users/{id}` | 永久删除用户及其全部数据 |
| `GET` | `/admin/users/{id}/avatar/{size}` | 读取指定用户已上传头像 |

`GET /admin/users` 现在支持：

- `limit`
- `offset`
- `keyword`
- `role`
- `status`
- `sort_by`
- `sort_order`

`POST /admin/users` 的请求体与普通注册类似：

```json
{
  "username": "alice",
  "email": "alice@example.com",
  "password": "password",
  "must_change_password": false
}
```

`password` 可省略或留空。此时服务端会生成 24 字符临时密码，自动设置 `must_change_password = true`，并且只在创建响应里返回一次 `generated_password`。如果请求里提供了密码，`must_change_password` 默认是 `false`，除非显式传 `true`。

### 更新用户示例

```json
{
  "role": "user",
  "status": "active",
  "storage_quota": 107374182400,
  "policy_group_id": 3
}
```

注意：

- `storage_quota = 0` 表示不限
- `policy_group_id` 不传表示保持不变；当前实现明确拒绝 `null`
- 当前实现禁止禁用初始管理员 `id = 1`
- 当前实现也禁止把初始管理员 `id = 1` 降级为非管理员
- `PATCH /admin/users/{id}` 支持 `must_change_password: true | false`。设为 `true` 会要求用户下次成功登录后先修改密码；设为 `false` 可在用户完成改密前清除这个要求。这个字段变化时会递增 `session_version`、删除该用户现有 refresh session、刷新认证快照缓存，并在 `admin_update_user` 审计 details 中记录新的 `must_change_password` 值。
- `must_change_password = true` 时，密码登录、MFA 完成、Passkey 登录完成和外部认证登录完成都会返回 `status = "password_change_required"`，并签发只能改密的受限 access token。这个 token 只能调用 `GET /auth/me`、`PUT /auth/password` 和 `POST /auth/logout`；普通认证接口会返回 `403` 和 `auth.password_change_required`。`POST /auth/refresh` 在强制改密状态或改密 token scope 下也会被拒绝。`PUT /auth/password` 仍然必须提供当前临时密码，成功后自动清除 `must_change_password`。
- `PUT /admin/users/{id}/password` 使用 `{ "password": "new-secret" }`
- `DELETE /admin/users/{id}/mfa` 会删除该用户全部 MFA factor、恢复码、待处理 MFA 登录 flow、邮箱验证码和 TOTP setup flow，并递增 `session_version`、删除该用户现有 refresh session；用户需要重新登录并重新配置 MFA
- `POST /admin/users/{id}/sessions/revoke` 会让这个用户现有 JWT / Cookie 会话全部失效
- `GET /admin/users/{id}/avatar/{size}` 只会返回“已上传头像”的二进制资源；Gravatar 应看用户详情里的 `profile.avatar.url_*`
- `DELETE /admin/users/{id}` 是物理删除，不是软删除；当前也不允许删除管理员用户

## 团队

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/teams` | 分页查看全部团队 |
| `POST` | `/admin/teams` | 创建团队并指定初始团队管理员 |
| `GET` | `/admin/teams/{id}` | 读取团队详情 |
| `PATCH` | `/admin/teams/{id}` | 更新团队名称、描述、策略组 |
| `DELETE` | `/admin/teams/{id}` |归档团队 |
| `POST` | `/admin/teams/{id}/restore` | 恢复已归档团队 |
| `GET` | `/admin/teams/{id}/audit-logs` | 查看团队审计记录 |
| `GET` | `/admin/teams/{id}/members` | 分页查看团队成员 |
| `POST` | `/admin/teams/{id}/members` | 添加团队成员 |
| `PATCH` | `/admin/teams/{id}/members/{member_user_id}` | 调整成员角色 |
| `DELETE` | `/admin/teams/{id}/members/{member_user_id}` | 移除团队成员 |

`GET /admin/teams` 支持：

- `limit`
- `offset`
- `keyword`
- `archived`
- `sort_by`
- `sort_order`

创建示例：

```json
{
  "name": "Operations",
  "description": "跨职能运营空间",
  "admin_identifier": "lead@example.com",
  "policy_group_id": 4
}
```

当前实现注意点：

- `admin_user_id` 和 `admin_identifier` 二选一，不能同时传，也不能都不传
- 创建团队时如果没传 `policy_group_id`，会退回系统默认策略组；如果系统没有默认组，创建会失败
- 团队更新接口也支持 `policy_group_id`，但和用户一样，当前实现拒绝显式传 `null`
- 团队成员列表支持 `keyword`、`role`、`status`、`limit`、`offset`、`sort_by`、`sort_order`
- 团队审计接口支持 `user_id`、`action`、`entity_type`、`after`、`before`、`limit`、`offset`

## 后台任务

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/tasks` | 分页查看全站后台任务和系统运行任务 |
| `POST` | `/admin/tasks/cleanup` | 按条件清理已结束任务记录 |

`GET /admin/tasks` 支持：

- `limit`
- `offset`
- `kind`
- `status`
- `sort_by`
- `sort_order`

清理请求体：

```json
{
  "finished_before": "2026-03-31T12:00:00Z",
  "kind": "archive_extract",
  "status": "succeeded"
}
```

当前实现注意点：

- `finished_before` 必填
- `kind` 和 `status` 不传时表示不按该字段过滤
- `status` 只能清理终态值：`succeeded`、`failed`、`canceled`
- 清理接口只删除终态任务，响应返回 `{ "removed": 3 }`

`GET /admin/tasks` 现在能看到两类记录：

- 用户后台任务，例如归档、缩略图、媒体元数据、链接导入、回收站清空
- 系统运行时留痕任务，例如 `mail-outbox-dispatch`、`background-task-dispatch`、`upload-cleanup`、`completed-upload-cleanup`、`blob-reconcile`、`system-health-check`、`trash-cleanup`、`team-archive-cleanup`、`lock-cleanup`、`auth-session-cleanup`、`external-auth-flow-cleanup`、`mfa-flow-cleanup`、`audit-cleanup`、`task-cleanup`、`wopi-session-cleanup`

系统运行时任务有几个特殊点：

- `kind = system_runtime`
- 空轮询不会写表
- `system-health-check` 健康成功时会刷新最近一条成功记录，而不是每次新增一行

## 系统运行时配置

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/config` | 列出全部运行时配置 |
| `GET` | `/admin/config/schema` | 读取系统配置 schema |
| `GET` | `/admin/config/template-variables` | 读取模板变量清单 |
| `GET` | `/admin/config/{key}` | 获取单个配置项 |
| `PUT` | `/admin/config/{key}` | 设置配置项 |
| `DELETE` | `/admin/config/{key}` | 删除配置项 |
| `POST` | `/admin/config/{key}/action` | 对特定配置目标执行动作 |

### 当前常用 key

- 具体定义以 `/admin/config/schema` 和 `src/config/definitions.rs` 为准；下面只列一批当前高频项，不是完整清单。邮件 SMTP、邮件模板、头像上传限制、注册/找回 TTL、分页上限等键也都在 schema 里
- `default_storage_quota`
- `webdav_enabled`
- `webdav_block_system_files_enabled`
- `webdav_block_system_file_patterns`
- `trash_retention_days`
- `team_archive_retention_days`
- `max_versions_per_file`
- `auth_allow_user_registration`
- `auth_register_activation_enabled`
- `auth_local_email_allowlist`
- `auth_local_email_blocklist`
- `auth_passkey_login_enabled`
- `auth_email_code_login_enabled`
- `auth_email_code_login_allow_totp_fallback`
- `auth_email_code_login_ttl_secs`
- `auth_email_code_login_resend_cooldown_secs`
- `audit_log_enabled`
- `audit_log_recorded_actions`
- `audit_log_retention_days`
- `public_site_url`
- `auth_cookie_secure`
- `cors_enabled`
- `cors_allowed_origins`
- `cors_allow_credentials`
- `cors_max_age_secs`
- `gravatar_base_url`
- `mail_outbox_dispatch_interval_secs`
- `background_task_dispatch_interval_secs`
- `background_task_dispatch_idle_max_interval_secs`
- `background_task_max_concurrency`
- `background_task_max_attempts`
- `maintenance_cleanup_interval_secs`
- `blob_reconcile_interval_secs`
- `remote_node_health_test_interval_secs`
- `team_member_list_max_limit`
- `task_list_max_limit`
- `background_task_archive_max_concurrency`
- `background_task_thumbnail_max_concurrency`
- `background_task_storage_migration_max_concurrency`
- `share_download_rollback_queue_capacity`
- `share_stream_session_ttl_secs`
- `offline_download_engine`
- `offline_download_engine_registry_json`
- `offline_download_max_file_size_bytes`
- `offline_download_max_mb_per_sec`
- `offline_download_max_concurrency`
- `offline_download_request_timeout_secs`
- `offline_download_temp_dir`
- `offline_download_aria2_rpc_url`
- `offline_download_aria2_rpc_secret`
- `offline_download_aria2_request_timeout_secs`
- `offline_download_aria2_split`
- `offline_download_aria2_max_connection_per_server`
- `offline_download_aria2_lowest_speed_limit_bytes_per_sec`
- `archive_extract_max_source_bytes`
- `archive_extract_max_uncompressed_bytes`
- `archive_extract_max_entries`
- `archive_extract_max_files`
- `archive_extract_max_directories`
- `archive_extract_max_depth`
- `archive_extract_max_path_bytes`
- `archive_extract_max_compression_ratio`
- `archive_extract_max_entry_compression_ratio`
- `archive_extract_max_duration_secs`
- `archive_build_max_entries`
- `archive_build_max_total_source_bytes`
- `archive_build_max_temp_bytes`
- `archive_preview_enabled`
- `archive_preview_user_enabled`
- `archive_preview_share_enabled`
- `archive_preview_max_source_bytes`
- `archive_preview_max_entries`
- `archive_preview_max_manifest_bytes`
- `archive_preview_max_duration_secs`
- `task_retention_hours`
- `archive_extract_max_staging_bytes`
- `avatar_max_upload_size_bytes`
- `thumbnail_max_source_bytes`
- `thumbnail_max_dimension`
- `image_preview_max_dimension`
- `media_metadata_enabled`
- `media_metadata_max_source_bytes`
- `media_processing_registry_json`
- `frontend_image_preview_preference`
- `mail_template_login_email_code_subject`
- `mail_template_login_email_code_html`

`thumbnail_max_source_bytes` 控制哪些源文件允许进入缩略图生成；`thumbnail_max_dimension` 和 `image_preview_max_dimension` 分别控制列表缩略图和预览面板图片生成后的最长边。调整尺寸会进入带尺寸后缀的 derivative cache namespace，不会覆盖另一种配置尺寸下的缓存。

`media_processing_registry_json` 是统一媒体处理注册表，用来管理内置 `images`、内置 `lofty`、VIPS CLI、FFmpeg CLI、FFprobe CLI 的启用状态、能力用途、后缀绑定和命令路径。缩略图与媒体元数据都走这条注册表；`media_metadata_enabled` 只保留为媒体元数据总开关，单类媒体是否启用由对应处理器控制。

`POST /admin/config/media_processing_registry_json/action` 支持 `test_vips_cli`、`test_ffmpeg_cli` 和 `test_ffprobe_cli`，会用当前草稿注册表或已保存注册表里的命令执行探测，适用于二进制文件改名、不在 PATH 下，或安装在自定义路径的环境。

链接导入由 `offline_download_*` 键控制。当前主要入口是结构化的 `offline_download_engine_registry_json`：它按顺序保存 `builtin` 和 `aria2` 引擎条目，并用 `enabled` 控制是否参与执行。启用多个引擎时会按注册表顺序尝试；全部关闭时链接导入被禁用。旧的 `offline_download_engine` 单值配置仍作为注册表缺失或无效时的兼容兜底。`offline_download_temp_dir` 是链接导入的临时 staging 根目录，留空时使用服务默认临时目录；填写时必须是双方都能访问的同一个绝对路径。

文件大小、单任务速度上限、任务并发数和请求超时对所有引擎都生效。速度键 `offline_download_max_mb_per_sec` 的单位是 MB/s，值为 `0` 表示不限速；在 aria2 引擎下会映射为单任务的 `max-download-limit`，不是全局 daemon 限速。

`offline_download_aria2_rpc_url`、`offline_download_aria2_rpc_secret`、`offline_download_aria2_request_timeout_secs`、`offline_download_aria2_split`、`offline_download_aria2_max_connection_per_server` 和 `offline_download_aria2_lowest_speed_limit_bytes_per_sec` 只在 aria2 引擎启用时使用。RPC URL 只能是 HTTP(S) 且不能带账号密码；RPC secret 是敏感配置，读取时会脱敏。AsterDrive 不透传任意 aria2 参数，只暴露这些管理员控制的安全子集。管理端可以对 `offline_download_engine_registry_json` 执行 `test_aria2_rpc` action，服务端会调用 `aria2.getVersion` 探测 RPC 地址、密钥和连通性。配置 action 可以通过 `value` 和 `draft_values` 携带未保存的前端草稿，所以 aria2 探测会使用当前草稿里的注册表、RPC 地址、密钥和超时，而不是只能测试已保存值。RPC 密钥错误会返回 `code = "offline_download.aria2_rpc_auth_failed"`；其他探测失败会返回 `code = "offline_download.aria2_rpc_probe_failed"`，不会复用 storage driver 错误码。运维部署、临时目录语义和常见问题见 [离线下载](../../../docs/config/offline-download.md)。

内置引擎会把下载流式写入临时文件，再校验 SHA-256 并导入工作空间，不会把整文件先塞进内存。

邮箱验证码 MFA 由 `auth_email_code_login_*` 四个键控制。启用 `auth_email_code_login_enabled` 前，SMTP host、发件人地址必须完整，SMTP 用户名和密码也必须成对配置；如果后续邮件关键配置被改到不可投递状态，服务端会自动把 `auth_email_code_login_enabled` 写回 `false`。邮件正文和主题使用 `mail_template_login_email_code_subject` / `mail_template_login_email_code_html`。

- `wopi_access_token_ttl_secs`
- `wopi_lock_ttl_secs`
- `wopi_discovery_cache_ttl_secs`
- `frontend_preview_apps_json`

### `public_site_url`

`public_site_url` 的数据库 key 保持单数，但值语义是“公开站点来源列表”：

```json
{
  "key": "public_site_url",
  "value": ["https://drive.example.com", "https://panel.example.com"]
}
```

实现约束：

- `value_type` 是 `string_array`，管理 API 写入时必须传字符串数组；数据库中保存为规范化后的 JSON 数组字符串
- 每一项必须是精确 HTTP(S) origin，只包含协议、host 和可选端口
- 不接受路径、查询、片段、通配符、`*` 或非 HTTP(S) scheme
- 第一项是无请求上下文时的默认回退来源
- 有请求上下文时，服务端会用当前请求的 scheme/Host 在列表里做精确匹配，命中后用对应来源生成 WebDAV、分享、预览和 WOPI URL
- 这个列表不是 CORS 白名单；浏览器跨域访问仍然由 `cors_allowed_origins` 控制
- 这个列表会参与 Cookie 认证写操作的 same-site CSRF 来源信任判断

### 自定义配置可见度

`system_config` 里的自定义配置支持 `visibility` 字段，用来控制消费侧是否能通过公开接口读取：

| 可见度 | 行为 |
| --- | --- |
| `private` | 仅管理员可见，不会返回给 `/api/v1/public/custom-config` |
| `public` | 匿名即可读取 |
| `authenticated` | 需要有效访问 token 才会返回 |

`visibility` 只对 `source = "custom"` 的条目生效。系统内置配置不会使用这字段，也不允许通过它改成公开条目。省略该字段时，新建自定义配置默认是 `private`。

`GET /admin/config` 当前也支持：

- `limit`
- `offset`

### 读取 schema

这个接口会返回：

- `value_type`
- `label_i18n_key`
- `description_i18n_key`
- `category`
- `description`
- `requires_restart`
- `is_sensitive`

`GET /admin/config` 返回的是实际配置项分页，字段还会包含 `id`、`key`、`value`、`source`、`visibility`、`namespace`、`updated_at` 和 `updated_by`。敏感配置项的 `value` 会被脱敏成 `***REDACTED***`。

前端管理后台就是靠它动态渲染设置页，而不是写死每个配置项。

### 配置分区

`category` 来自 `src/config/definitions.rs` 的允许列表，管理后台按一级分区展示，二级分区折叠成分组。当前系统分区如下：

- `site` / `site.preview`：站点公开入口、品牌和预览应用
- `user.registration_and_login` / `user.avatar`：注册登录和头像
- `auth`：认证 Cookie 和 token TTL
- `mail.config` / `mail.template`：发信配置和邮件模板
- `network`：CORS 等网络访问规则
- `runtime.mail` / `runtime.background_task` / `runtime.maintenance` / `runtime.limits` / `runtime.share_stream`：运行时派发、维护和限制
- `storage`：版本、回收站、团队归档和默认配额等存储保留策略
- `file_processing.archive_extract` / `file_processing.archive_preview` / `file_processing.archive_build` / `file_processing.offline_download` / `file_processing.media`：压缩包、链接导入和媒体处理
- `webdav` / `audit`：WebDAV 和审计日志

和自定义前端相关的公开读取接口见 [公共接口](./public.md) 里的 `GET /public/custom-config`。那里只会返回当前身份可见的自定义配置条目，不会暴露管理端字段。

旧的前端设置路径 `/admin/settings/general` 和 `/admin/settings/operations` 只保留为跳转兼容，不应再作为 `system_config.category` 写入。新增系统配置时必须使用已登记分区；如果确实需要新增分区，要同时补允许列表、前端路由 / 图标以及 zh/en 标题和描述文案。分类完整性测试会拦住未登记分区和缺少二级分区文案的配置。

### 读取模板变量

`GET /admin/config/template-variables` 会返回按类别分组的模板变量清单，当前主要给管理后台在邮件、品牌文案等支持模板占位符的配置项旁边做提示，不必把变量表硬编码在前端里。

### 设置配置项示例

```json
{
  "value": "14",
  "visibility": "public"
}
```

`visibility` 只允许对自定义配置使用。系统内置配置仍然只能写 `value`。

### 执行配置动作

当前已经落地三类动作目标：

- `POST /admin/config/mail/action`
- `POST /admin/config/frontend_preview_apps_json/action`
- `POST /admin/config/media_processing_registry_json/action`（`test_vips_cli`、`test_ffmpeg_cli`、`test_ffprobe_cli`）

邮件测试示例：

```json
{
  "action": "send_test_email",
  "target_email": "ops@example.com"
}
```

当前语义：

- `target_email` 不传时，默认发给当前管理员自己的邮箱
- `action = send_test_email` 会立即走运行时邮件发送链路
- 成功响应里会返回一段可直接展示给前端的 `message`
- 这条调用也会写管理员审计日志

预览应用 WOPI discovery 导入示例：

```json
{
  "action": "build_wopi_discovery_preview_config",
  "discovery_url": "https://office.example.com/hosting/discovery"
}
```

这条动作的当前语义：

- 目标 key 必须是 `frontend_preview_apps_json`
- `discovery_url` 必填，用来拉取并解析远端 WOPI discovery XML
- `value` 可选；传了就把它当“预览应用草稿 JSON”来导入并返回结果，不直接落库
- `value` 不传时，会基于当前线上配置或默认配置生成并直接写回 `frontend_preview_apps_json`
- 成功响应除了 `message`，还可能带一份新的 `value`，也就是归一化后的预览应用 JSON 草稿

## 分享审计

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/shares` | 查看全站分享 |
| `DELETE` | `/admin/shares/{id}` | 管理员删除任意分享 |

`GET /admin/shares` 支持：

- `limit`
- `offset`
- `sort_by`
- `sort_order`

## 审计日志

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/audit-logs` | 分页查询审计日志 |

当前实现支持这些查询参数：

- `user_id`
- `action`
- `entity_type`
- `after`
- `before`
- `limit`
- `offset`
- `sort_by`
- `sort_order`

其中 `after` 和 `before` 使用 RFC3339 时间字符串。

返回结果包含分页信息与日志项，日志项里会带时间、用户、动作、实体、名称、IP 等字段。

日志项同时包含 `presentation` 字段，给前端做结构化展示：

- `presentation.summary`：操作摘要，`code` 通常对应 `AuditAction::as_str()`，`params` 携带展示参数
- `presentation.target`：目标对象摘要，优先描述实际展示对象；服务启动/关闭这类系统事件会使用 `server`
- `presentation.detail`：按动作类型补充的结构化详情；旧记录或无法解析的详情会安全降级为空

审计记录范围由运行时配置 `audit_log_recorded_actions` 控制，值是审计 action 字符串数组。空数组表示审计日志开关开启但不写入任何 action；非法值会在配置写入时被拒绝，运行时读取异常时会回退为记录全部 action。

主节点启动和关闭会分别写入 `server_start`、`server_shutdown`，归在 `system` action 分组。当前 follower 的审计和运行日志补全仍是后续工作。

## 锁管理

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/admin/locks` | 查看全部资源锁 |
| `DELETE` | `/admin/locks/{id}` | 强制解锁 |
| `DELETE` | `/admin/locks/expired` | 清理全部过期锁 |

`GET /admin/locks` 支持：

- `limit`
- `offset`
- `sort_by`
- `sort_order`

`DELETE /admin/locks/expired` 会返回：

```json
{
  "removed": 3
}
```
