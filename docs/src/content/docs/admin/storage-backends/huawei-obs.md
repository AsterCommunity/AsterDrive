---
description: 华为云 OBS 存储策略教程，覆盖原生 OBS 签名、CNAME、Type A / SHA-256 CDN 下载、私有回源、CORS 和 multipart 验收。
title: "华为云 OBS 存储策略教程"
---

协议依据、版本、复核状态与真实验收边界见[provider 证据索引](https://drive.docs.astercosm.com/developer/reference/storage-provider-evidence/#huawei_obs)。源站 API、上传域、CDN 和自定义域的能力分别验收。

:::tip[这一篇覆盖什么]
这一篇讲如何把 AsterDrive 文件写入华为云 OBS：准备 bucket 和 OBS 凭证、创建 `asterdrive.storage.huawei_obs` connector、选择区域或自定义域名访问、配置策略组，并按 relay 或 presigned 路径完成验收。

华为云 OBS 在 AsterDrive 中使用原生 OBS 签名。它不是把 endpoint 填进普通 S3 策略后自动变成 OBS；如果你需要 AWS SigV4 或通用 S3 兼容服务，请看 [S3 / MinIO / R2 存储策略教程](/admin/storage-backends/s3/)。
:::

## 适合什么时候用

华为云 OBS 适合这些场景：

- 已经在华为云使用 OBS，希望 AsterDrive 直接写入指定 bucket
- 需要官方 OBS 签名，而不是 generic S3 的 AWS SigV4
- 需要 OBS 原生 multipart、Range、预签名或自定义域名访问
- 希望后台明确显示“华为云 OBS”，让 endpoint、region 和访问模式可审查

## 自定义域名（CNAME）

`obs_use_cname` 与 OSS 的 `oss_use_cname`、COS 的 `cos_use_cname` 含义一致：关闭时由驱动使用 bucket 构造官方访问地址；开启时直接使用已绑定 bucket 的域名。对象读写、multipart 和预签名 URL 使用同一开关，最终 URL 的 host 和路径都不会重复追加 bucket。各家签名协议由对应驱动处理。

| `obs_use_cname` | Endpoint 示例 | `obs_region` | 请求地址 | 适用情况 |
| --- | --- | --- | --- | --- |
| `false`（默认关闭） | `https://obs.cn-north-4.myhuaweicloud.com` | 必填，例如 `cn-north-4` | `https://BUCKET.obs.REGION.myhuaweicloud.com/OBJECT` | 官方区域 endpoint，推荐默认使用 |
| `true`（开启） | `https://files.example.com` | 可以留空 | `https://files.example.com/OBJECT` | bucket 已绑定自定义域名 |

区域 endpoint 可以填写官方根地址，也可以填写带 bucket 前缀的地址，例如：

```text
https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/
```

AsterDrive 会把它规范化保存为区域根 endpoint，并由 driver 按 bucket 生成 virtual-hosted 请求。普通 S3 endpoint、带路径前缀的 endpoint、带 query 或 fragment 的 endpoint 会在保存前被拒绝。

开启 **使用自定义域名（CNAME）** 后，直接使用绑定到 OBS 的域名。AsterDrive 不会把 bucket 再拼到 custom hostname 前面；签名规范资源也遵循 OBS 官方 SDK 的 CNAME 行为。官方 OBS endpoint 应保持 CNAME 开关关闭。这个开关对应官方 Go SDK 的 [`WithCustomDomainName(bool)`](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/master/obs/conf.go)，Python SDK 则称为 `is_cname`。

升级后，旧 `obs_addressing_mode=virtual_hosted` / `custom_domain` 会分别迁移为 `obs_use_cname=false` / `true`，已有对象路径和签名语义不变；策略、远端目标和已删除策略的清理快照都会转换。所有节点一起升级，旧 API 客户端应改用布尔字段；回退旧版本前需执行对应 down migration。升级前先完成正在执行的 OBS 策略迁移任务，升级后重新生成尚未执行的迁移预检计划。

OBS 支持 CDN 回源，但 CNAME 开关不提供 CDN 下载鉴权。需要加速下载时，使用下方独立的 CDN 下载交付配置；CNAME 可同时用于 OBS API 源站。

## 1. 准备 OBS bucket 和凭证

在华为云 OBS 控制台创建或选择专用 bucket，例如：

```text
archive-bucket
```

建议为每个 AsterDrive 实例规划独立 bucket 或 prefix，例如：

```text
prod/
```

不要让多个实例在没有规划的情况下共用同一个 prefix；AsterDrive 的删除、迁移和后台清理都会依赖数据库记录的对象路径。

为 AsterDrive 创建最小权限的 OBS 访问凭证。权限至少要覆盖当前使用的对象能力：

- 列出目标 bucket / prefix
- 读取对象和对象 metadata
- 写入对象
- 删除对象
- multipart 初始化、上传分片、列出分片、完成和终止

具体 IAM action 名称和控制台位置以华为云当前 OBS 文档为准，不要把管理整个账号的凭证直接放进 AsterDrive。

## 2. 配置上传和下载方式

第一次接入建议先使用服务端中继：

| 方向 | 建议初始值 | 原因 |
| --- | --- | --- |
| 上传 | `relay_stream` | 浏览器不需要直连 OBS，先验证签名、权限和对象路径 |
| 下载 | `relay_stream` | 先让 AsterDrive 承接响应，排查范围更小 |

确认基本读写、分享和 Range 请求稳定后，再切换到 `presigned`：

```text
浏览器 -> OBS
AsterDrive 只负责签发短时效 OBS URL
```

预签名模式要求浏览器可以访问 OBS endpoint 或 custom domain，并且 OBS CORS、HTTPS 证书和响应头都配置正确。

## 2.1 可选：华为云 CDN 下载交付

AsterDrive 支持华为云 CDN 的 **Type A URL 鉴权 + SHA-256**。默认选择 **OBS 源站直连（原生 OBS 签名）**。将 **下载交付方式**改为 **华为云 CDN 加速下载（Type A + SHA-256）**后，才展示 CDN 域名、有效时长、鉴权参数名和密钥；这些字段在 CDN 模式必填。

| 字段 | 配置 |
| --- | --- |
| 下载方式 | `presigned`；选择 `relay_stream` 时仍使用 AsterDrive 中继，并保留 CDN 设置 |
| 下载交付方式 `download_delivery` | 默认 `obs_origin`；CDN 选择 `huawei_cdn_type_a_sha256` |
| 下载交付地址 `download_base_url` | HTTPS 加速域名，例如 `https://cdn.example.com`；不包含凭证、非默认端口、路径、query 或 fragment |
| CDN 鉴权有效时长 `obs_cdn_auth_validity_seconds` | 默认 300 秒，范围 1–31536000 秒，必须与华为云 CDN 控制台一致 |
| CDN 鉴权参数名 `obs_cdn_auth_param` | 默认 `auth_key`，必须与控制台一致；1–100 位，以英文字母开头，仅含英文字母、数字和下划线 |
| CDN 鉴权密钥 `obs_cdn_auth_key` | Type A 的主 KEY 或备 KEY，6–32 位英文字母或数字；加密保存，与 OBS AK/SK 分开配置 |

对象路径继续使用策略的 **Base path**。例如 Base path 为 `prod`，下载 URL 的路径就是 `/prod/对象键`；CDN 域名中不再添加第二份路径前缀。上传、multipart、列表、删除、清理及服务端中继继续使用 OBS API endpoint。

### 云端准备与 AsterDrive 配置

1. 在华为云 CDN 控制台添加加速域名并启用 HTTPS，源站选择当前 OBS bucket。
2. 私有桶需要完成 CDN 的 OBS 私有回源授权，确认目标 bucket、回源 Host 和权限。不要为了 CDN 下载把 bucket 改成公共读。
3. 进入目标域名的 **访问控制 → URL 鉴权**，启用 **方式 A**、**SHA256**，鉴权范围选择 **所有文件**。AsterDrive 对象键可能没有后缀，仅按扩展名鉴权无法覆盖全部文件。
4. 记录当前主 KEY 或备 KEY、鉴权参数名和有效时间，在 AsterDrive 填写相同值。AsterDrive 仅支持正有效期，不使用控制台允许的 0 秒设置。
5. 在 CDN 端配置允许 AsterDrive 站点来源的 CORS；检查 GET、HEAD、Range，以及 `Content-Range`、`Content-Length`、`Content-Type`、`Content-Disposition`、`ETag` 等所需响应头。OBS bucket 的 CORS 不能代替 CDN 域名的 CORS。
6. 保持对象路径大小写及编码，不配置会改变对象身份的路径重写。CDN 验签应使用最终请求中的编码路径；中文、空格、`+`、`%`、`#`、`?` 的对象分别验证。
7. 按华为云文档配置 **忽略 URL 参数**等缓存规则，避免每个随机鉴权参数产生独立缓存。缓存 key 忽略参数、回源是否剥离参数、缓存命中是否仍鉴权是不同问题，必须分别验证。限制非预期业务 query，尤其是可能影响 OBS 响应的 `response-*` 参数。

**普通“测试连接”验证的是 OBS API endpoint，并检查本地 CDN 配置是否完整。** 它不会读取或修改 CDN 控制台，也不能证明 CDN 边缘鉴权、CORS、缓存和私有回源已经生效。

:::caution[用户鉴权与私有回源分别配置]
CDN 获得读取私有 OBS bucket 的权限，并不表示客户端需要鉴权。必须同时启用 Type A / SHA-256 的全路径鉴权。AsterDrive 只在原有文件或分享权限检查通过后签发链接；无法替代云端访问控制配置。
:::

### 链接有效期与缓存

Type A 使用 `auth_key=timestamp-rand-0-sha256`，参数名可按控制台配置修改。随机数为无中划线 UUID；摘要按华为文档的 `Filename-Timestamp-rand-uid-PrivateKey` 规则计算，Filename 为最终编码路径，不包含 query。AsterDrive 不向 CDN URL 追加 OBS 签名或未认证的响应头覆盖参数。

**实际有效期边界是 `timestamp + 控制台有效时长`**，不是 timestamp 本身。AsterDrive 将链接剩余时长限制为调用方请求时长与配置时长的较小值；请求较短时回拨签名时间，使资源句柄中的到期时间与之匹配。例如配置 300 秒、调用方只请求 60 秒，签名时间回拨 240 秒，而不是将当前时间加 60 秒填入 timestamp。

**鉴权期限不等于缓存保留时间。** CDN 对象可以继续驻留缓存，但过期或无效凭据仍应被拒绝。检查客户端响应的浏览器缓存策略；需要避免浏览器保留私有下载内容时，在 CDN 配置相应响应头，例如 `Cache-Control: private, no-store`，并分别验证边缘缓存策略。分享撤销或权限变更不会即时撤销已签发链接；有效期内链接仍可被持有者重复使用。紧急撤销需按云端配置轮换密钥，并注意主、备 KEY 的并行有效期。

### 哪些请求可以使用 CDN

- 已授权的浏览器 Blob 下载和安全预览可以使用 CDN，前端按当前文件名保存，并根据资源句柄重建 MIME。
- 必须精确控制附件文件名、响应头或使用应用同源隔离的请求继续使用中继；网页等需隔离的内联预览不会直接交付 CDN。
- 选择 OBS 源站交付时，即使保存着 CDN 地址，也不会自动启用 CDN；切换交付方式保留可恢复设置。
- 本轮不支持 B/C1/C2、MD5、上传加速或 M3U8 子资源鉴权继承的应用集成。

### 分层验收

先验收 OBS 源站或 CNAME 的普通请求、预签名 PUT/GET、multipart 和清理，再测试 CDN：

1. 使用新签发链接，验证 GET、HEAD 和单字节 Range，检查返回内容及 `Content-Range`。
2. 分别对冷缓存对象、用有效链接填充缓存后的对象测试：无签名、错误密钥、篡改路径/时间/摘要和过期链接都被 CDN 拒绝；到期边界按华为的“当前时间小于或等于 timestamp + 有效时长”核对。
3. 检查首次回源能读取私有桶，再检查重复请求的缓存命中与鉴权。确认请求确实来自预期的 CDN 域名、源站 bucket 和 Host。
4. 验证浏览器 Blob 下载、图片预览、当前文件名、Range、CORS 和暴露头；单独检查浏览器缓存与 CDN 缓存的保留行为。
5. 测试特殊对象键、无扩展名对象、鉴权参数缓存/回源规则及密钥轮换；核对非预期 query 不会改变共享缓存中的响应。

本地签名向量、mock 和浏览器表单测试不代表真实华为云 CDN 验收通过。真实环境的回源、边缘缓存、CORS、过期及密钥轮换结果需要单独记录，不保存完整签名 URL 或凭据。

参考华为云官方[配置鉴权方式 A](https://support.huaweicloud.com/usermanual-cdn/cdn_01_0040.html)和[CDN 域名全量配置](https://support.huaweicloud.com/api-cdn/UpdateDomainFullConfig.html)。

## 3. 配置 OBS CORS

管理员可在新建草稿、编辑草稿或已保存策略的动作区显式点击 **配置 CORS**，确认通用远端状态变更提示后执行。先在系统设置配置有效的 **公开站点地址**；后端严格解析为 Origin，不默认使用 `*`。编辑时留空的 AK/SK 可保留已保存凭据，但目标使用当前草稿，不会写到旧 bucket。普通保存和“测试连接”不会配置 CORS。

凭据除对象读写权限外，自动配置仅新增目标 bucket 的 `obs:bucket:GetBucketCORS` 与 `obs:bucket:PutBucketCORS` 权限，该动作不调用删除接口，但华为的 `PutBucketCORS` 授权项本身也允许删除 CORS；不需要公开读、bucket policy 或 CDN 管理权限。动作先读再合并，保留既有规则、ID 和顺序；已满足当前请求则不重复写入。若早期匹配规则缺少 ETag 等响应头，或有未知 XML 字段、超出自动合并器的 100 条规则／64 KiB 安全上限，则拒绝写回，提示手工处理，不清空重建。避免与控制台同时修改 CORS；完整文档替换不提供跨控制台原子合并。

结果显示 bucket（配置范围）、规范化来源、是否变更、保留规则数及可用的 OBS RequestId。官方模式访问 bucket 的区域 OBS endpoint；CNAME 模式访问已直接绑定 OBS 的 endpoint，不能使用 CDN 加速域名。独立的 CDN 下载地址不参与管理请求。协议依据、固定官方 SDK revision、签名向量与本地／真实云边界见 [OBS CORS 契约与证据](https://drive.docs.astercosm.com/developer/design/huawei-obs-cors-contract/)；配置源站 CORS 不会修改 CDN 响应头、缓存或鉴权。


只使用 `relay_stream` 时，浏览器不会直接请求 OBS，CORS 可以稍后处理。使用 `presigned` 上传或下载前，至少确认：

通用的 `presigned` CORS 原则见 [S3 / MinIO / R2 教程的 CORS 章节](/admin/storage-backends/s3/)；华为云 OBS 控制台字段对应关系见下面的 OBS 专用表格。

- `AllowedOrigin` 包含 AsterDrive 的公开站点来源，例如 `https://drive.example.com`
- 上传允许 `PUT`，并允许 AsterDrive 发出的请求头
- 下载允许 `GET`、`HEAD` 和 Range 请求所需的 header
- `ExposeHeader` 包含 `ETag`；multipart 直传完成时客户端需要读取分片 ETag
- 预签名 URL 的 hostname、证书和浏览器网络路径可用

在华为云 OBS 控制台中，单文件和 multipart 预签名上传可以先使用下面这条规则：

| OBS 字段 | 建议值 |
| --- | --- |
| 允许的来源 | AsterDrive 页面实际 Origin，例如 `https://drive.example.com` |
| 允许的方法 | `GET`、`HEAD`、`PUT` |
| 允许的头域 | `Content-Type`、`Range` |
| 补充头域 | `ETag`、`Content-Length`、`Content-Range`、`Accept-Ranges`、`Content-Disposition` |
| 缓存时间 | `600` |

`ETag` 是上传响应头，应放在“补充头域”，不是当前预检需要的请求头。不要把 AWS S3 的 `x-amz-*` 头照搬到 OBS 规则；当前浏览器预检最少需要允许 `Content-Type`。如果控制台显示 `OPTIONS` 预检 403，优先检查来源、`PUT` 和 `Content-Type` 是否同时匹配。

AsterDrive 的连接测试只从服务端验证 endpoint、凭证和基础对象请求。它不代替浏览器侧的 CORS 和网络验收。

## 4. 创建 Huawei Cloud OBS 存储策略

进入：

```text
管理 -> 存储策略 -> 新建策略
```

选择驱动类型：

```text
华为云 OBS
```

如果已有通用 `s3` 策略使用官方 OBS endpoint 和明确的 `s3_region`，管理端会提供 `promote_from_s3` connector upgrade。该操作原地切换 connector 和加密凭据，不复制对象；bucket、base path、region 和 object namespace 保持不变。源策略使用 bucket-prefixed OBS endpoint 时，promotion 会将其规范化为对应的区域根 endpoint；源策略已经使用区域根 endpoint 时，endpoint 保持不变。普通 S3 endpoint、`s3_region = auto` 或不匹配的 endpoint 不会获得该升级建议。

常见字段：

| 字段 | 官方域名示例 | 自定义域名示例 |
| --- | --- | --- |
| Endpoint | `https://obs.cn-north-4.myhuaweicloud.com` | `https://files.example.com` |
| Bucket | `archive-bucket` | `archive-bucket` |
| OBS region | `cn-north-4` | 可以为空 |
| 使用自定义域名（CNAME） | `false`（默认关闭） | `true`（开启） |
| Base path | `prod/` | `prod/` |
| Access Key ID | 华为云 AK | 华为云 AK |
| Secret Access Key | 华为云 SK | 华为云 SK |

签名由 connector driver 固定使用原生 OBS 协议，不作为管理员配置项。不要把 OBS endpoint 配置到普通 S3 策略中，也不要改用 AWS SigV4。

## 5. 测试连接并配置策略组

保存前或保存后点击 `测试连接`，确认：

1. AsterDrive 服务端可以访问 endpoint
2. `obs_use_cname=false` 时，核对 bucket 名、区域 endpoint 和 `obs_region` 是否匹配
3. AK/SK 有目标 prefix 的读、写、删和 multipart 权限
4. `obs_use_cname=true` 时，核对自定义域名是否已绑定目标 bucket；`obs_region` 可以留空
5. AsterDrive 服务器时间准确

编辑已保存策略时，如果凭据字段留空，草稿测试可以复用已保存的静态凭据；新建策略仍要填写完整凭据。

然后创建测试策略组，把一个测试用户或测试团队绑定到该组。不要一开始就改默认策略组或正在承载生产流量的策略。

## 6. 做一轮真实验收

使用测试账号完成：

- 小文件上传和下载
- 大文件 multipart 上传
- presigned 上传（如果启用）
- presigned 下载（如果启用）
- 图片或视频的 Range 读取
- 文件 metadata 读取
- 删除和回收站恢复
- 分享链接下载
- multipart 失败后的重试和清理

观察 OBS 控制台和 AsterDrive 日志时，不要记录 AK、SK、临时 token 或完整预签名 URL。确认对象最终落在预期 bucket/prefix 后，再把真实用户或团队迁移到该策略组。

## 常见问题

### 把 OBS endpoint 配进了 `s3`

普通 `s3` connector 使用 AWS SigV4，不能代表原生 OBS policy。请选择 **华为云 OBS**，由 connector driver 固定使用原生 OBS 签名。

### Endpoint 被拒绝

检查以下内容：

- CNAME 关闭时使用 `obs.<region>.myhuaweicloud.com` 或官方文档列出的区域后缀
- `obs_region` 与 host 中的 region 一致
- endpoint 没有路径前缀、query、fragment、用户名或密码
- CNAME 开启时填的是已绑定域名，不是 `bucket.obs.<region>...` 官方 endpoint

### 服务端测试通过，浏览器预签名失败

这是两条不同的网络路径。继续检查：

- 浏览器是否能解析和访问预签名 URL 的 hostname
- OBS CORS 的 origin、method、request header 和 exposed header
- HTTPS 证书是否覆盖实际 hostname
- 是否放行了 `GET`、`HEAD`、`PUT` 和 Range 请求

## 官方参考

- [华为云 OBS 使用 Authorization header](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0010.html)
- [华为云 OBS 使用预签名 URL](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0011.html)
- [华为云 OBS 列举 bucket 对象](https://support.huaweicloud.com/intl/en-us/api-obs/obs_04_0022.html)
- [华为云 OBS Go SDK](https://github.com/huaweicloud/huaweicloud-sdk-go-obs)
