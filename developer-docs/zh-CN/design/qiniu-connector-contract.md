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

- `endpoint`：必填 HTTPS Kodo S3 官方 endpoint，接受服务级
  `https://s3.<region>.qiniucs.com` 和空间级
  `https://<S3-space-name>.s3.<region>.qiniucs.com` 两种输入。空间级 host 中的名称
  必须与 `bucket` 一致；两种输入都规范化为不带末尾 `/` 的服务 endpoint 供 AWS SDK
  使用。明文 HTTP、自定义 CNAME、非标准端口、
  path、query 或 fragment 仍作为配置错误。
- `bucket`：必填**七牛 S3 空间名**。它遵循 S3 的全局唯一要求；普通 Kodo 空间
  名称全局唯一时两者相同，否则平台会生成另一个 S3 空间名。管理员从 Kodo 控制台
  的空间概览或 `Get Service` 获取该值，不能想当然地填普通空间名称。
- `base_path`：可选对象 prefix；空值表示 bucket 根。
- `s3_region`：必填 SigV4 签名区域，必须为 1–128 个可打印 ASCII 字符，且不含
  空白或 `/`；它同时必须等于 endpoint 中的 `<region>`。`cn-east-1` 仅为界面
  示例，不是代码中的区域 allow-list。
- 通用 object-storage 上传和下载策略，以及静态 Qiniu AccessKey / SecretKey。

SecretKey 是 secret descriptor 字段，不能进入日志、错误、审计记录或 presigned
响应 payload。编辑策略时空的 secret 字段沿用已保存凭据；新建策略必须提供完整
凭据。

## 协议与运行时行为

`QiniuDriver` 将配置映射为共享 `S3DriverConfig`，并使用选定 endpoint、bucket、
base path、region、静态凭据、超时和寻址方式构建 AWS SDK client。为兼容
S3-compatible endpoint，请求 checksum 计算和响应 checksum 校验均限制为
`WhenRequired`。

寻址方式是 connector-owned provider 策略，不作为管理员字段暴露。Qiniu connector
解除 AWS SDK 的强制 path-style，让 endpoint resolver 对普通 DNS-compatible S3
空间名使用 virtual-hosted-style，并在名称约束要求时选择兼容形式；管理员无需理解
或同步 endpoint 与 SDK 寻址开关。

首版独立 connector 的稳定价值是：强制官方 endpoint 与 region 一致、规范化服务级
与空间级寻址、明确七牛 S3 空间名而不是普通空间名称、设置 Kodo 兼容所需的 checksum 策略，并
提供七牛专属 descriptor、本地化、连接诊断、教程和真实服务验收入口。任意自建
S3-compatible endpoint 继续使用 generic S3 connector，不借 Qiniu 品牌入口绕过
供应商契约。

浏览器直传仅使用标准 presigned PUT；multipart 使用标准 S3 `uploadId`、
`partNumber` 和 ETag 语义。启用 `presigned` 前，管理员必须验证浏览器可访问
endpoint、TLS 和 Kodo CORS；普通 relay 请求与 presigned 请求分别验收。

## 验证要求

- 本地 S3-compatible mock 或 RustFS 集成测试只证明共享 S3 数据面的 PUT、GET、
  Range、HEAD、DELETE、list、流式上传、multipart 与 path-style /
  virtual-hosted-style 请求契约；它不构成 Kodo provider 验收。
- 认证、403、404、429、5xx、网络中断和超时必须经共享 S3 错误映射表现为稳定
  `StorageErrorKind`；草稿和已保存策略连接测试覆盖成功与失败分支。
- 文档、connector manifest、OpenAPI 和前端 SDK 必须从最终 descriptor/schema
  重新生成，不得保留 `PresignedFormUploadRequest` 或其他原生 form-upload DTO。
- 正式支持和合并前，通过 `tests/storage/qiniu.rs` 中受
  `ASTER_TEST_QINIU_KODO_*` 环境变量保护的测试，在隔离真实 Kodo S3 空间记录连接
  测试、上传、下载、Range、multipart complete/abort、list、删除、presigned 与
  CORS smoke 证据。不得用本地 mock 替代该证据，也不得在测试输出中打印凭据或
  完整签名 URL。

官方 endpoint、访问方式与 S3 空间名的事实源是七牛文档：
[AWS S3 协议兼容性说明](https://developer.qiniu.com/kodo/4088/s3-access-domainname)。
