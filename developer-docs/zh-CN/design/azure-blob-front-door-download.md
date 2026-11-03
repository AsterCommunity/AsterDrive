# Azure Blob Front Door 下载契约

Issue #689：支持 Azure Front Door Standard/Premium 客户端 service SAS 模式，下载 route 关闭缓存。此文描述当前实现，不涵盖历史 CDN、源站 SAS 注入或匿名公开缓存。

## 分层职责

- Connector：schema v1 增加可选下载入口，`download_delivery` 默认 `blob_origin`，可选 `front_door_client_sas`；`download_base_url` 仅在后者必填。descriptor 驱动显示和 payload；旧持久化配置通过 serde 默认值读取。不改变通用 OpenAPI 类型或数据库结构。
- Driver：只在 `DirectDownloadStorageDriver` 组装交付 URL；canonical resource 始终是 `/blob/account/container/full_key`。Blob API、上传、multipart、复制、清理、连接测试不使用下载域名。
- Service / resource handle：复用已有权限检查、策略选择、文件名/类型、安全内联回退和 300 秒 TTL；浏览器 omit credentials，禁止条件请求头，过期时间与 SAS `se` 一致。
- Frontend：复用通用 descriptor 表单，不增加 provider/domain 白名单。

## 官方依据

| 行为 | 官方依据 |
| --- | --- |
| Standard/Premium 透传客户端 SAS；Premium Private Link 隔离 Blob origin | [Storage scenario](https://learn.microsoft.com/en-us/azure/frontdoor/scenario-storage-blobs) |
| 独立 Front Door endpoint、Blob origin host、路由设置 | [Storage integration](https://learn.microsoft.com/en-us/azure/frontdoor/integrate-storage-account) |
| private/no-store 禁止缓存；query cache key 不等于每次验证 SAS | [Front Door caching](https://learn.microsoft.com/en-us/azure/frontdoor/front-door-caching) |
| SAS 字段顺序、解码后的 canonical resource、`rscc/rscd/rsct` 纳入 HMAC | [Create service SAS](https://learn.microsoft.com/en-us/rest/api/storageservices/create-service-sas) |

## 路由和安全

下载 URL 仅接受 HTTPS origin（无路径、userinfo、非默认端口、query、fragment、控制字符）。Blob origin 使用原 container/blob 路径，回源路径为空，不改写路径，完整 query 透传，禁止注入源站 SAS。拒绝静态网站 endpoint。对象 key 按 segment 编码，字面 `%2F` 编码为 `%252F`；拒绝 dot segment、反斜杠和控制字符，避免 URL normalization 改变签名对象。

只读 SAS 授权 GET、HEAD 和 Range，响应头覆盖值也参与签名。即使调用方请求 public 缓存，Front Door 模式仍强制签署 `private, no-store`。管理员必须关闭下载 route 的缓存并清除旧缓存。本功能不支持缓存命中交付：把 SAS 放进 cache key 不能保证每次请求都验证有效期。Private Link 不能替代用户 SAS。

服务端只组装下载地址，不探测或跟随下载域名。浏览器的 TLS、重定向和 CORS 取决于实际 Front Door 配置。真实 Azure SDK 的后端重定向拒绝行为由本地请求捕获测试证明。

已签发 URL 在过期前仍可用，修改分享权限不立即撤销。日志、错误和审计不得包含 account key 或完整 SAS URL。连接测试成功只证明 Blob API；不能证明 Front Door 域名、TLS、回源私网、CORS 或缓存配置正确。

## 验证

本次实现没有可用的 Azure 账号，未执行真实 Azure / Front Door 云端验收。下述本地验证与云端验收必须分别记录；本地测试通过不能视为云端配置已经验收。

本地层次：独立 HMAC 签名校验、特殊字符/路径/expiry/response header 测试；真实 SDK 请求捕获验证 Blob 操作路由和重定向；resource handle 权限及过期；策略创建/读取/编辑与旧配置；descriptor/form/payload 和浏览器创建/编辑。生成入口为 `make storage-docs`，OpenAPI 只有通用 DTO 变化时才需要生成差异。

Azurite 集成测试另行验证 Blob 请求、签名响应头、GET/HEAD/Range、缺失或篡改 SAS、只读权限和同一链接过期后的拒绝行为。没有 Docker 时可连接本机模拟器，运行方式见[测试文档](../testing/index.md#azure-blob-集成测试)。Azurite 不模拟 Front Door。

真实 Azure 验收单独执行：私有 container 与 Front Door 域名必须覆盖 GET/HEAD/Range、中文文件名和 CORS 预检；缺少、篡改、过期 SAS 都应拒绝；对已经成功下载过的同一 URL，在过期后再次请求也必须拒绝。检查 DNS、证书、回源 Host/path、可选 Private Link、独立 Blob API 连接以及禁用缓存的实际效果。本地模拟不能证明这些云端配置正确，具体操作见中英文 Azure Blob 用户指南。
