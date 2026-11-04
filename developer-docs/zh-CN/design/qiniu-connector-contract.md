# Kodo 原生 connector 契约

## 所有权和协议

Qiniu connector 的 schema V2 完整替换 V1 S3 wrapper，不使用七牛 Rust SDK，也不在运行时探测/回退到 S3。协议依据为 `qiniu/go-sdk@f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e` 的 `auth/credentials.go`、`storagev2/region/query.go` 与 `storagev2/internal/api-specs/`。通用 S3 connector 的 SigV4 合同保持独立。

- connector：配置/凭据规范化、descriptor、条件字段、连接探测、运行时与清理 driver 构造。
- driver：HMAC-SHA1、padded URL-safe Base64、UpToken、管理凭证、区域发现、原生表单和 multipart v2、管理 API、私有读取/交付及结构化错误。
- 上传 service/repository：用户/团队鉴权、策略协商、会话、part-size 验证、Blob/version、配额、审计、任务和完成事务。
- 前端：执行后端给出的有限请求合同，不识别七牛 connector ID、URL 或 JSON 格式。

## Provider resumable 与编号分片

`upload_transport` 返回 `ProviderResumable`，策略为 `server_relay` 或 `frontend_direct`。Kodo 原生 v2 与 OneDrive/Google Drive 的顺序 range 协议不同：它支持独立编号分片与显式合并。`ProviderResumableUploadDriver::numbered_parts()` 明确协商这种协议，并复用已有 multipart planner、并发调度、完成事务和取消清理；不通过顺序执行器制造隐式完成。

所有编号分片统一返回已有 `BrowserUploadRequest`：S3/OSS/COS/OBS 在后端将签名 PUT 转成通用请求，Azure 声明固定 block token，Kodo 声明受限 UpToken 与后端查询回执。`completion` 的有限规则为响应头、固定 token、供应商权威查询；前端调度器只有一条执行路径，不按 presigned/native 或供应商分支。原生 ETag/JSON 不由前端解析；完成时后端查询真实 `listParts` 并验证 partNumber、ETag 和实际大小。已有 wire mode `presigned_multipart` 是历史浏览器编号分片调度标识，不表示 native 请求使用 SigV4。外部客户端必须按新 schema 更新分片请求消费；单次 presigned PUT 合同保持不变。

换浏览器恢复时，原生 provider 的已上传分片以供应商查询为依据，不要求本地存储存在 ETag。旧 S3 uploadId 不可解释为原生会话。原生会话身份保存 uploadId、bucket、完整 key 和供应商截止时间，并在每个请求上核对所有权与过期时间。

`complete_upload_session` 为非隐式 provider 提供显式完成合同；默认隐式 provider 无额外请求。Kodo 正式编号分片路径由 multipart completion 执行原生合并，普通 provider 完成事务继续负责元数据发布。

## 数据面和资源

- 表单：`POST` multipart/form-data，key/token/file；官方单次上限 1 GiB。
- multipart v2：init、streamed part PUT、listParts、complete JSON 和 abort；编号 1..10000，非末片至少 1 MiB，单片最多 1 GiB，供应商会话 7 天。
- 管理：原生 stat、list（有界分页/visitor）、copy、delete；不存在和错误明确映射。
- 私有源站：原生 `e/token` GET、HEAD、Range。Range 被忽略或 Content-Range 不一致时拒绝返回假成功。
- 下载：最终 Host/编码路径/截止时间一起签名；token 最后，`expires_at` 与 e 一致；TTL 拒绝零和超限。源站与 CDN 统一接受供应商原生 MIME/缓存头，附件通过签名内 attname 保持当前逻辑文件名。普通 GET 和 Range 均可直连，不因响应头覆盖要求回退中继；不提供此类开关。HTML/SVG inline 沙箱与公共条件验证合同保持。

流式上传使用固定 64 KiB ReaderStream 缓冲，逻辑分片不会分配同等大小的 Vec，也不先落临时文件。ExactSizeReader 在最后一个声明字节前探测额外数据，避免静默截断。流式 multipart 的失败/取消尝试 abort；取消通过 per-upload guard 安排清理并记录失败。进程终止或初始化响应丢失无法保证立即清理，保留供应商过期保障。不得将其描述为同步必达清理。

下载签名有效期由 descriptor 数字字段 `download_auth_validity_seconds` 提供，范围 1..3600 秒，默认 180；缺字段的旧 V2 使用 serde 默认。有效 TTL 为 caller/config/provider 三者最小值，默认 Kodo 源站 `*.kodo-*.qiniucs.com` 另有 300 秒上限。服务端流式 GET、HEAD、Range、缩略图读取也消费此配置；直链的 e 和 expires_at 使用同一个真实截止时间。精确匹配供应商“expires 与 server time 相差过大”的 403 时映射为 Misconfigured 并提供固定脱敏原因，不把它当成账号 Permission，也不透传任意 provider body。

区域发现按 AK/bucket 调用官方 v4/query，并在 driver 实例内按 TTL 缓存上传/RS/RSF 服务；无全局产品状态。下载源站、CDN、上传与管理服务不混用。HTTP client 禁止重定向，避免将凭据带到意外目标。

## 配置和升级

配置字段为 native bucket、base_path、download_origin、download_auth_validity_seconds、download_delivery、download_base_url、provider_resumable_upload_strategy 和 object_storage_download_strategy。静态凭据仍为 V1，加密存储与留空保留规则不变。CDN 域仅在 qiniu_cdn 时显示/必填；relay 策略可保存休眠的交付设置。

升级由 `src/storage/connectors/qiniu/upgrade.rs` 在启动加载凭据/监听端口前编排，driver 的 migration-only resolver 核对外部协议。已有实例加密主密钥用于解密 policy、remote target 和对象清理快照的凭据；`/buckets?shared=rd` 列举候选，`/v4/query` 的 `s3.region_alias` 缩小区域，`io_src.domains` 提供默认 HTTPS 源站。旧 S3 空间的随机 16 字节探测对象必须从唯一原生空间读到相同 nonce，不能按名字或已有文件大小猜测。探测对象有唯一 key、位于原 prefix，始终尝试清理；用户内容不读写。

网络验证在数据库事务外完成，所有结果准备好后以原配置/凭据的 compare-and-swap 一次提交。无默认源站的候选空间不参与匹配；区域查询的网络、鉴权及解析错误仍传播。任何连接失败、歧义、探测清理失败或并发修改均保留原始配置并中止启动；5 分钟单连接时限。成功后直接丢弃旧 S3 字段，不创建备份表，也不保留运行时双协议。schema migration 只接受已验证 V2，schema-only 工具不得清空 V1 字段。

保留策略 ID、对象 key、base_path、行为和加密 credential V1。原生空间与 download_origin 联网自动填入；可查询默认源站，不强制管理员新绑自定义域。CDN 交付是独立配置选择，不能从任意域名列表自动猜测。后续 V2 启动无升级网络请求。

对象清理快照使用其原 policy ID/AAD 和加密凭据一起转换；缺少凭据快照时明确停止升级，不查询其他 owner 的凭据表，也不提交此前准备的配置转换，须在旧版本完成/取消对应任务后重试；保留 temp key 和任务身份。不可转换的旧 S3 multipart 会话须先完成/取消，已完成/取消快照保留历史。迁移成功后不可逆，不提供旧配置备份或自动恢复路径；无 Kodo 数据时 down 无配置可逆转。不得修改已发布 migration。

## 安全与验证

AK/SK、token、完整 URL 和 provider 原始错误体不得进入日志/Debug/审计。错误保留 HTTP provider code、经过筛选的 X-Reqid 和稳定 StorageErrorKind；reqwest error 的 URL 被丢弃。浏览器仅获得本次对象的短期上传能力。原生 CDN 使用私有 bucket 的 e/token 鉴权；不叠加时间戳防盗链，真实缓存 HIT/MISS、过期与密钥轮换仍需供应商验收。

验证覆盖签名独立向量、原生请求、stat/list/copy/delete、GET/HEAD/Range、大小不符、重试/恢复、取消/过期、失败清理、有界流、schema/凭据、SQLite migration 原子失败与不可逆边界、远端配置确认、前端执行器与既有 provider 回归。OpenAPI、前端 SDK 和 storage docs 通过正式流程生成。真实云测试缺少输入时明确跳过，不能将 mock 或编译通过当成供应商验收。

原生附件参数依据为 [七牛下载设置](https://developer.qiniu.com/kodo/1659/download-setting)。源站和当前 CDN 实测 attname 在完整 GET/206 中生效；早期探测确认 response-* 的行为因入口和状态码不同，因此正式交付不使用这些覆盖参数或相关回退分支。签名向量独立核对最终编码 URL。
