# 七牛云 Kodo Connector 契约

## 目标与边界

七牛云 Kodo 作为 S3 兼容对象存储接入。Qiniu connector 负责配置、静态凭据、
descriptor、本地化、配置规范化、连接测试和运行时 driver 构造；共享
`S3CompatibleDriver` / `S3Driver` 负责对象 I/O、AWS SigV4、Range、流式上传、
multipart、分页、presigned GET/PUT 和 S3 错误映射。

该 connector 不使用 Qiniu SDK 或七牛原生数据面协议。QBox、UpToken、
UploadToken、原生表单上传、原生 multipart REST 和硬编码 z0/z1/z2 endpoint 表
不属于实现边界。上传服务和前端只消费通用 descriptor/capability，不按 Qiniu
connector ID 分支。

## 配置与版本

这是未发布 connector 的第一个稳定 schema，配置版本为 **V1**，不保留分支内
曾出现的原生协议或 V2 schema 兼容路径。策略必须提供：

- `endpoint`：必填 HTTP(S) Kodo S3 兼容 endpoint；可使用区域、内网或经验证的
  CNAME endpoint，但 bucket 必须单独填写。
- `bucket`：必填 bucket 名称。
- `base_path`：可选对象 prefix；空值表示 bucket 根。
- `s3_region`：必填 SigV4 签名区域，必须为 1–128 个可打印 ASCII 字符，且不含
  空白或 `/`；`cn-east-1` 仅为界面示例，不是代码中的区域表。
- `s3_path_style`：默认 `true`，使用 `/bucket/key`；仅在实际 endpoint 支持
  virtual-hosted-style bucket URL 时关闭。
- 通用 object-storage 上传和下载策略，以及静态 Qiniu AccessKey / SecretKey。

SecretKey 是 secret descriptor 字段，不能进入日志、错误、审计记录或 presigned
响应 payload。编辑策略时空的 secret 字段沿用已保存凭据；新建策略必须提供完整
凭据。

## 协议与运行时行为

`QiniuDriver` 将配置映射为共享 `S3DriverConfig`，并使用选定 endpoint、bucket、
base path、region、静态凭据、超时和寻址方式构建 AWS SDK client。为兼容
S3-compatible endpoint，请求 checksum 计算和响应 checksum 校验均限制为
`WhenRequired`。

浏览器直传仅使用标准 presigned PUT；multipart 使用标准 S3 `uploadId`、
`partNumber` 和 ETag 语义。启用 `presigned` 前，管理员必须验证浏览器可访问
endpoint、TLS 和 Kodo CORS；普通 relay 请求与 presigned 请求分别验收。

## 验证要求

- 本地 S3-compatible mock 或 RustFS 集成测试必须证明 PUT、GET、Range、HEAD、
  DELETE、list、流式上传、multipart 和 path-style / virtual-hosted-style 签名请求。
- 认证、403、404、429、5xx、网络中断和超时必须经共享 S3 错误映射表现为稳定
  `StorageErrorKind`；草稿和已保存策略连接测试覆盖成功与失败分支。
- 文档、connector manifest、OpenAPI 和前端 SDK 必须从最终 descriptor/schema
  重新生成，不得保留 `PresignedFormUploadRequest` 或其他原生 form-upload DTO。
- 正式支持和合并前，使用隔离真实 Kodo bucket 记录连接测试、上传、下载、Range、
  multipart、list、删除、presigned 与 CORS smoke 证据。不得用本地 mock 替代该证据。
