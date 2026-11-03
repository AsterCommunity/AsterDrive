# 阿里云 OSS CDN 下载交付

Alibaba OSS 驱动使用原生 OSS V4 完成对象 API、CNAME 和浏览器上传，独立使用阿里云 CDN Type A 交付允许采用源站响应头的下载。CDN 不参与后端读写、multipart 或清理。

## 鉴权方式的选择

阿里云提供 A、B、C、F 四种 URL 鉴权。AsterDrive 只实现 Type A，不提供未签名 CDN 模式或多种 CDN 鉴权类型选择字段：

| 方式 | 官方签名结构 | 取舍 |
| --- | --- | --- |
| A | `MD5(URI-timestamp-rand-uid-key)`，查询参数 `auth_key`，Unix 秒 | 保留对象路径，采用固定格式，作为本项目唯一支持方式 |
| B | `MD5(key+timestamp+URI)`，时间戳和摘要位于路径，UTC+8 年月日时分 | 不维护额外的路径改写与时间格式 |
| C | `MD5(key+URI+timestamp)`，路径式或查询参数式，十六进制 Unix 秒 | 不维护多种输出格式 |
| F | `MD5(key+URI+timestamp)`，查询参数名和时间戳格式可配置 | 不维护额外参数配置；格式接近腾讯 CDN Type D，但算法不同 |

字母不是安全等级。四种标准方式均采用 MD5；现有腾讯 CDN Type D 使用 SHA-256，不能因 URL 结构类似而复用摘要算法。

## 配置与验证

Connector 配置（由 descriptor 驱动 UI）：

- `download_delivery`：`oss_origin`（默认）或 `alibaba_cdn_type_a`；选择 CDN 后才展示域名、有效时长及密钥，域名与密钥通过 `required_when` 标记为必填
- `download_base_url`：可选 HTTPS 域名，不含凭证、路径、query 或 fragment；保存前修剪首尾空白
- `oss_cdn_auth_validity_seconds`：CDN 控制台的“鉴权 URL 有效时长”，默认 300 秒，范围 1–31536000 秒
- 静态凭据 `oss_cdn_auth_key`：CDN 主 KEY 或备 KEY，6–128 位 ASCII 字母或数字，与 OSS AccessKey 独立并标记为敏感字段

选择 CDN 模式后必须提供域名与有效密钥，保存策略及构造驱动时均校验。域名、密钥或时长错误直接失败，不生成公开 CDN 链接，也不静默绕过配置错误。配置和凭据中的未知字段按现有 schema 契约拒绝；本功能尚未发布，不为被删除的鉴权类型字段保留别名或迁移层。

CDN 用户 URL 鉴权与私有 OSS 回源授权分别配置。只授权 CDN 回源不能保护边缘节点面向用户的访问。

## Type A 签名

```text
encoded_path = URL 实际发送的编码路径（包含基础路径）
nonce        = 不带连字符的 UUID
uid          = 0
signature    = MD5(encoded_path + "-" + timestamp + "-" + nonce + "-0-" + key)
URL          = HTTPS 域名 + encoded_path + "?auth_key=" + timestamp + "-" + nonce + "-0-" + signature
```

路径通过原生 URL API 按 segment 编码；对空对象键、`.` / `..` 段和控制字符报错，保留对象名称大小写以及字面量 `%2F`，避免解码或拼接改变签名目标。Type A 不签署任意业务查询参数，不能把追加 query 会失效概括为“签署整个 URL”。驱动只输出 `auth_key`，不附加未认证的 `response-*` 参数。

官方已知向量的输入为 `/video/standard/test.mp4-1444435200-0-0-aliyuncdnexp1234`，MD5 为 `23bf85053008f5c0e791667a313e28ce`。生产签名使用随机 nonce，测试注入确定的时间与 nonce 验证结果。

## 有效期

CDN 的真实失效时间是 `timestamp + 控制台鉴权有效时长`。驱动以两侧时长一致为前提计算：

```text
lifetime  = min(调用方请求时长, 配置有效时长)
deadline  = now + lifetime
timestamp = deadline - 配置有效时长
expires_at = deadline
```

请求较短时回拨 timestamp，不把未来失效时间误作生成时间。对零时长、无效时间和溢出直接报错。AsterDrive 无法自动读取控制台设置；两侧不一致会导致失效时间偏差，管理员必须同步修改。

**签名 URL 有效期不是 CDN 缓存保留时间。** 后者由 CDN 缓存规则及源站响应头决定，可能长于链接有效期；缓存命中仍须通过用户 URL 鉴权。持有已签发链接的人在失效前可以继续使用，应用撤销权限不自动作废已签发链接，紧急撤销需轮换 CDN 密钥。

## 下载分流与响应语义

文件服务在完成资源授权后，调用 `DirectDownloadStorageDriver::resolve_download_url`。调用方始终传入当前文件名、MIME 和期望的响应头，不由 driver 猜测 UI 行为。

- 浏览器 Blob 下载与安全预览明确设置 `allow_provider_response_headers`；前端下载器以当前文件名选择本地写入目标，预览以资源句柄 MIME 构造 Blob。两者可采用 CDN 源站响应
- `require_download_name_match` 或不能忽略的响应覆盖要求返回 `None`，使用同源中继；不追加未认证的响应覆盖参数，不丢弃附件或文件名语义
- Direct URL、外部查看器以及需要隔离的内联预览保留严格路径；HTML 等危险 MIME 的内联预览由现有 sandbox gate 拦截
- 选择源站交付时继续生成 OSS V4 presigned URL，隐藏的 CDN 字段不会隐式启用 CDN，保留现有受支持的响应覆盖。CNAME 仍保留 OSS bucket 的 canonical URI 与 wire path 区别
- 无论是否配置 CDN，presigned PUT 和 multipart part 请求始终由公网 OSS/CNAME client 使用 OSS V4 生成；服务端 I/O 可使用独立内网 endpoint

浏览器访问 CDN 使用 `credentials=omit`，不继承 AsterDrive cookie 或条件请求头。CDN 域名的 CORS 需独立验收。

## 验证边界

本地测试覆盖：

- 官方 MD5 向量、独立编码路径向量、随机 nonce 格式及真实过期时间
- 域名、密钥、有效时长、非法对象键及溢出校验
- Connector 配置与凭据组合校验、敏感字段标记和密钥 Debug 脱敏
- CNAME 与 CDN 同时配置时，上传与 multipart 仍使用 OSS V4
- 文件服务到 CDN 的 Blob 下载/预览、越权拒绝、HTML 预览隔离和严格回退
- 前端通过 CDN URL 下载且使用当前文件名写入本地文件

真实 OSS CNAME 可达性、CDN 边缘鉴权、有效/篡改/过期链接、Range、CORS 与缓存行为由具备阿里云资源的维护者验收，本地测试不代替实机验证。

## 实现位置

- `src/storage/connectors/alibaba_oss.rs`：配置、descriptor、凭据及保存前校验
- `src/storage/drivers/alibaba_oss/mod.rs`：OSS/CNAME client 与 CDN 下载分流
- `src/storage/drivers/alibaba_oss/cdn.rs`：Type A 签名、期限与 URL 校验
- `src/storage/drivers/alibaba_oss/signing.rs`：OSS V4 对象 API 签名
- `src/services/files/file/resource_handle.rs`：已授权资源的交付策略

## 官方资料

- [配置 URL 鉴权](https://help.aliyun.com/zh/cdn/user-guide/configure-url-signing)
- [鉴权方式 A](https://help.aliyun.com/zh/cdn/user-guide/type-a-signing)
- [鉴权方式 B](https://help.aliyun.com/zh/cdn/user-guide/type-b-signing)
- [鉴权方式 C](https://help.aliyun.com/zh/cdn/user-guide/type-c-signing)
- [鉴权方式 F](https://help.aliyun.com/zh/cdn/user-guide/authentication-method-f-description)
