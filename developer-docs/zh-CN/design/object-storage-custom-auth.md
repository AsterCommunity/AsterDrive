# 对象存储自定义认证与 AWS SDK 复用边界

本文记录 AsterDrive 为腾讯云 COS、华为云 OBS、阿里云 OSS 等对象存储复用
`aws-sdk-s3` operation/runtime，同时替换厂商原生签名协议时必须遵守的边界。

这里讨论的是 driver 内部实现，不改变 `StorageDriver`、上传策略或 connector
descriptor 的产品契约。

## 背景

`S3CompatibleDriver` 当前通过 `S3Driver` 复用 `aws-sdk-s3` 的对象读写、流式上传、
列举、multipart 和 presigned operation。只替换 endpoint 并不等于替换签名协议：
默认 client 仍然生成 AWS SigV4 header 和 `X-Amz-*` presigned query。

部分厂商提供与 S3 相近的 HTTP operation 和 XML 响应，但认证协议不同：

- 腾讯云 COS 使用 COS Q-Sign；
- 华为云 OBS 使用 `SignatureObs`，格式为 `Authorization: OBS AccessKeyID:Signature`，签名是 `Base64(HMAC-SHA1(SK, UTF-8(StringToSign)))`；
- 阿里云 OSS V4 使用 `OSS4-HMAC-SHA256`、`x-oss-*` 字段、
  `date/region/oss/aliyun_v4_request` scope 和 `aliyun_v4` 密钥派生前缀。

因此复用边界必须是“复用 operation/runtime，替换 auth scheme”，不能把
“能设置自定义 endpoint”误写成“原生协议天然兼容”。

## 已验证的 AWS SDK 扩展点

当前仓库锁定 `aws-sdk-s3 1.140.0`、`aws-runtime 1.9.1`、
`aws-smithy-runtime-api 1.14.0`。针对这些版本，已通过独立 mock spike 验证：

1. `Config::builder().interceptor(...)` 可以注册 `modify_before_signing` hook；
2. 单次 operation 的 `.customize().mutate_request(...)` 底层同样运行在
   `modify_before_signing` 阶段；
3. Smithy orchestrator 的顺序是 endpoint 解析、`modify_before_signing`、
   `Sign::sign_http_request`、发送；
4. `push_auth_scheme(...)` 可以替换已注册 scheme 的 identity resolver 和 signer；
5. 生成的 `.presigned()` operation 仍会调用替换后的 signer。

这意味着厂商 signer 可以在最终 endpoint 已确定后读取或修改 method、URI、query
和 headers，然后生成 header signature 或 query signature。AWS SDK 仍负责 input
序列化、HTTP body、timeout、transport、retry orchestration 和 response parsing。

## Auth scheme ID 约束

S3 endpoint resolver 只声明它认识的 auth scheme。使用任意新 ID（例如 `oss4`）
注册 signer，会在签名前触发 `MissingEndpointConfig`。

厂商专用 client 应使用现有 SigV4 scheme ID 注册自己的 `AuthScheme`：

```rust
const SCHEME_ID: AuthSchemeId = aws_runtime::auth::sigv4::SCHEME_ID;
```

`push_auth_scheme` 对相同 ID 的覆盖语义由 SDK 公开契约保证。该替换只能发生在
COS/OSS 专用 client 上；普通 S3 client 继续使用 AWS signer，禁止共享一份被替换
认证组件的 client。

## Presigned operation

生成的 S3 `.presigned()` 会安装 `SigV4PresigningRuntimePlugin`。这个插件会：

- 将 `SigV4OperationSigningConfig.signing_options.signature_type` 设为 query；
- 写入 `expires_in`；
- 为对应 operation 设置 unsigned payload；
- 在发送前停止 orchestrator，并返回已签名请求。

它不会重新选择或覆盖 `AuthScheme`。厂商 signer 可以读取
`SigV4OperationSigningConfig`，用 `signature_type` 区分普通 header signing 与
presigning，并读取有效期。正式使用这些类型时，应把当前传递依赖
`aws-runtime`、`aws-smithy-runtime-api` 和 `aws-smithy-types` 声明为直接依赖，
避免依赖 Cargo 的传递依赖实现细节。

## COS 迁移边界

COS 基础对象能力继续复用以下链路：

```text
TencentCosDriver -> S3CompatibleDriver -> S3Driver -> aws_sdk_s3::Client
```

区别是 `aws_sdk_s3::Client` 在构造时注册 COS `AuthScheme`，普通对象请求、multipart
和 presigned URL 都由 COS Q-Sign signer 认证，不再产生 AWS SigV4 签名。

COS CI 图片处理、媒体元数据和 bucket CORS 当前使用 `reqwest` 加 COS 原生签名。
这些能力不是本次底层 client 迁移的阻塞项，迁移时保留现有请求链路，避免同时改动
provider-native API 和基础对象 API。

COS signer 还必须处理 AWS operation serializer 与 COS 原生字段之间的窄差异，
例如 copy-source header、SDK 默认 checksum header 和 S3 endpoint 附加 query。
这些转换属于 COS driver/signing 模块，不进入 service、connector common 或共享
`aster_drive_storage` trait。

## Huawei OBS 复用边界

Huawei OBS 复用同一条 AWS SDK operation/runtime 链路，但使用独立的 OBS signer：

```text
HuaweiObsDriver -> S3CompatibleDriver -> S3Driver -> aws_sdk_s3::Client
```

driver 会在现有 SigV4 scheme ID 上注册 `SignatureObs` hook，让 AWS SDK继续负责请求序列化、body、timeout、重试和 XML response parsing；签名 hook 负责把 AWS header/query 残留转换成 OBS 原生字段并计算 `Authorization: OBS ...` 或 OBS presigned query。

OBS 的地址和列举协议不能按 generic S3 直接继承：

- virtual-hosted 模式要求区域 OBS endpoint 和匹配的 region；
- custom-domain 模式移除 AWS SDK 自动添加的 bucket host 前缀，并使用官方 OBS SDK 的 CNAME canonical resource；
- 官方 OBS SDK 和 API 使用 marker-based `ListObjects`，不发送 S3 `list-type=2` 或 continuation token；
- `x-amz-meta-*`、copy-source、storage-class、ACL、grant 和 security-token 等请求字段要转换成对应的 `x-obs-*` 字段；
- 普通 S3 client、COS client 和 OBS client 必须保持独立的 signer 配置。

实现固定对照华为官方 Go SDK `v3.26.6` commit `fd2b44881f0cd9bd41ffff2fabeb94c783ccc321`，重点文件是 `obs/auth.go`、`obs/authV2.go`、`obs/conf.go`、`obs/trait_object.go`、`obs/trait_part.go`、`obs/convert.go` 和 `obs/client_object.go`。

## OSS 后续实现边界

OSS 可以沿用 COS 验证后的结构，但 signer、endpoint/addressing 和字段转换必须独立：

- backend I/O 使用 server-side endpoint（未配置时回退 public endpoint）；
- 浏览器 presigned URL 使用 public endpoint；
- region 是 OSS V4 必填签名输入；
- CNAME 模式单独决定 Host 与 bucket addressing，不能复用前端或 service 的
  provider-specific 分支；
- header signing 和 query presigning 都使用 OSS V4，而不是 AWS SigV4。

建议为 backend operation 和 browser presign 构造两个共享 signer 的 client，避免在
单次请求里临时改 endpoint 后破坏 Host/canonical URI 一致性。

## 与 provider option 插件化的边界

Issue #458 现在就是 storage 重构 contract，不再是以后再做的兼容层。内建
connector 和动态加载的 plugin 使用同一个 namespaced
`ConnectorConfigEnvelope`：

- 持久化内容统一包含 `connector_id`、format version、schema version
  和 connector-owned values；
- descriptor 声明默认值、标量校验、secret 处理和 UI 元数据，具体 connector
  负责 normalize 与 runtime 解码；
- core service 不匹配 provider 字段名，也不维护 `DriverType` 到
  options 的矩阵；
- `StoragePolicyOptions` 及其中 provider-specific enum 只是过渡
  遗留，所有内建 connector 迁移完成后必须删除；
- 缩略图、媒体处理上限等跨 connector 的产品行为放入独立的 core policy
  behavior contract，不塞进 connector namespace；
- 未加载的 connector 对应 envelope 仍须作为 unavailable policy data 保留，
  不能静默转换成其他 connector 或丢弃。

### 旧 options 字段归属表

下面这张表是删除 StoragePolicyOptions 时的检查清单：

| 旧字段 | 新归属 |
| --- | --- |
| object_storage_upload_strategy、object_storage_download_strategy、s3_path_style、s3_region、s3_*_timeout_secs | S3 connector config |
| object_storage_upload_strategy、object_storage_download_strategy、storage_native_processing_enabled、storage_native_media_metadata_enabled | 由各 descriptor 声明的 object-storage connector config |
| remote_download_strategy、remote_upload_strategy | Remote connector config |
| provider_resumable_upload_strategy、provider_download_strategy、provider_download_filename_mode、onedrive_* | OneDrive connector config |
| sftp_host_key_fingerprint | SFTP connector config |
| content_dedup | Local connector config |
| thumbnail_processor、thumbnail_extensions、media_metadata_extensions | core storage policy behavior |

provider enum 要随具体 connector 一起移动，或者降为 connector 内部解析类型。
迁移完成后它们不能继续从 shared model facade 导出。

## 验证要求

每个厂商 signer 至少覆盖：

- 官方固定时间签名向量；
- 普通 GET/HEAD/PUT/DELETE/COPY 的捕获请求；
- Range、Content-Type 和 provider header 的 canonicalization；
- presigned GET、PUT、UploadPart；
- multipart initiate、upload、list、complete、abort；
- SDK 自动添加 header/query 的删除或转换；
- endpoint、virtual-hosted addressing、CNAME 和非默认端口；
- provider XML success/error response 的解析；
- 可选真实 provider 集成测试。

mock 测试只能证明 request contract 和 SDK orchestration；没有真实 provider 测试时，
不得宣称完整兼容。
