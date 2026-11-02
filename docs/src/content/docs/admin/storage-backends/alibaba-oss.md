---
description: 阿里云 OSS 存储策略教程，覆盖原生 OSS V4 签名、公网与服务端 endpoint、CNAME、上传下载方式、CORS 和上线验收。
title: "阿里云 OSS 存储策略教程"
---

:::tip[这一篇覆盖什么]
这页讲怎样把 AsterDrive 文件写入阿里云对象存储 OSS。AsterDrive 使用原生 `OSS4-HMAC-SHA256` 签名，不把 OSS 原生策略伪装成普通 S3-compatible 配置。
:::

## 什么时候选阿里云 OSS

- 已经有阿里云 OSS bucket，希望保留 OSS 原生 endpoint 和 region 语义
- AsterDrive 服务端可以走 OSS 内网 endpoint，但浏览器直传仍要走公网 endpoint
- bucket 绑定了自定义域名，需要 CNAME 访问
- 需要 `relay_stream` 与 `presigned` 上传、下载和 multipart 上传

只接通一个“看起来像 S3”的 endpoint 不等于原生 OSS 支持。OSS V4 的算法、credential scope、canonical URI 和 query 参数都与 AWS SigV4 不同。

## 入口速查

| 任务 | 位置 |
| --- | --- |
| 创建 bucket、AccessKey、CORS | 阿里云 OSS 控制台 |
| 创建策略 | `管理 -> 存储策略 -> 新建策略 -> 阿里云 OSS` |
| 分配给用户或团队 | `管理 -> 策略组` |
| 对比 `relay_stream` / `presigned` | [存储能力矩阵](/reference/storage-matrix/) |

## 1. 准备 OSS bucket

1. 创建专用 bucket，记录 bucket 名称和 region，例如 `cn-hangzhou`
2. 记录公网 endpoint，例如 `https://oss-cn-hangzhou.aliyuncs.com`
3. 如果 AsterDrive 与 OSS 在同一云网络，记录可供服务端使用的内网 endpoint，例如 `https://oss-cn-hangzhou-internal.aliyuncs.com`
4. 创建只允许访问该 bucket 的 AccessKey，至少授予对象读、写、删除、列举和 multipart 所需权限
5. 如果使用 `presigned`，为 AsterDrive 站点配置 OSS CORS

:::caution[Alibaba Cloud 国际站账号]
[阿里云中国站文档](https://help.aliyun.com/zh/oss/user-guide/regions-and-endpoints)仍将 `oss-cn-<region>.aliyuncs.com` 列为中国内地地域的外网 endpoint，上面的默认配置适用于中国站账号。Alibaba Cloud 国际站则[另行规定](https://www.alibabacloud.com/zh/notice/oss_update_notice_policy_change_in_calling_data_api_operations_via_the_default_public_domain_name_45a)：在 2025 年 3 月 20 日 00:00:00（UTC+8）之后激活 OSS 的国际站用户，不能通过默认公网域名调用中国内地地域 bucket 的数据 API；此前已激活的国际站用户和内网域名不受影响。若请求返回 `PublicEndpointForbidden`（HTTP 400，EC `0048-00000401`），请给 bucket [绑定自定义域名](https://help.aliyun.com/zh/oss/user-guide/map-custom-domain-names-5)，启用 **使用 CNAME 自定义域名**，再把该域名配置为公网 endpoint。浏览器 presigned 上传和下载也需要使用该 CNAME 域名。
:::

不要把 AccessKey 写入日志、截图或 issue。AsterDrive 会把 connector credential 加密存储，备份和迁移时必须同时保留 `[auth].storage_credential_secret_key`。

## 2. 理解三个 endpoint 相关字段

| 字段 | 作用 |
| --- | --- |
| 公网 endpoint | 生成浏览器可见的 presigned URL；没有服务端 endpoint 时也用于后端 I/O |
| 服务端 endpoint | 可选，只用于 AsterDrive 后端请求；不会出现在浏览器 presigned URL 中 |
| 使用 CNAME 自定义域名 | 把公网 endpoint 视为已绑定当前 bucket 的自定义域名 |

普通模式的 endpoint 必须使用 `aliyuncs.com` OSS 域名。CNAME 模式的公网 endpoint 必须是自定义域名；bucket 仍参与 OSS V4 canonical URI，但不会重复出现在实际 URL path 中。

:::caution[CNAME 不会替代服务端 endpoint]
配置了服务端 endpoint 时，后端 I/O 使用服务端 endpoint，浏览器 presigned URL 仍使用公网 endpoint。先分别确认服务端和浏览器的 DNS、HTTPS 与网络可达性。
:::

## 2.1 可选：CDN 下载分发

如果希望下载走阿里云 CDN 加速，可以配置可选的 **CDN 下载域名**。启用后，面向用户的下载链接会使用 CDN 域名并按 CDN 鉴权规则签名；上传、删除、列举、multipart 和清理等后端操作仍使用 OSS API endpoint。

| 字段 | 作用 |
| --- | --- |
| CDN 下载域名 | 可选。面向用户的下载链接使用的 CDN 加速域名，例如 `https://cdn.example.com`。必须是干净的 scheme + host，不能带凭证、query、fragment 或路径 |
| CDN 鉴权方式 | `type_a`、`type_b`、`type_c` 或 `none`。对应阿里云 CDN 的 URL 鉴权方式 |
| CDN 鉴权密钥 | 鉴权方式对应的主 KEY，加密存储，不出现在日志和审计中 |

CDN 配置只影响下载交付：

- CNAME 自定义域名用于 OSS 对象 API 寻址，不能替代 CDN 下载鉴权；两者是不同的东西
- 后端 I/O 永远不会路由到 CDN 域名
- 下载链接带 TTL，到期即失效；撤销需要在阿里云 CDN 控制台旋转鉴权 KEY

准备步骤：

1. 在阿里云 CDN 控制台添加加速域名，源站指向当前 OSS bucket
2. 开启 URL 鉴权（A / B / C 方式之一），记录鉴权主 KEY
3. 确认 CDN 回源到 OSS 的授权正确，避免 CDN 缓存绕过访问控制
4. 在 AsterDrive 策略中填写 CDN 下载域名、鉴权方式和鉴权密钥

:::caution[CDN 鉴权方式必须与控制台一致]
AsterDrive 生成的签名 URL 按所选鉴权方式计算。鉴权方式或鉴权 KEY 与 CDN 控制台不一致时，CDN 会返回 403。修改任一侧后都要重新端到端验收。
:::

## 3. 创建 AsterDrive 存储策略

在 `管理 -> 存储策略` 新建 **阿里云 OSS**，填写：

- 公网 endpoint
- 可选服务端 endpoint
- OSS region
- bucket
- 可选基础路径
- 是否使用 CNAME
- 可选 CDN 下载域名、CDN 鉴权方式和鉴权密钥
- AccessKey ID / AccessKey Secret
- 上传方式和下载方式

先使用 `relay_stream` 完成连接测试和端到端验收。确认服务端读写、Range、删除、复制和 multipart 稳定后，再切 `presigned`。配置 CDN 下载域名时，单独验收 CDN 下载链路。

## 4. 为 presigned 配置 CORS

浏览器直传至少需要允许 AsterDrive 站点来源执行 `GET`、`HEAD`、`PUT`、`POST` 和 `DELETE`，并允许上传实际携带的请求头。单文件 presigned PUT 会由服务端在 complete 阶段校验对象 metadata 和大小，因此不要求浏览器读取 `ETag`；presigned multipart 的 part 仍需要 `ETag` 才能完成 multipart 对象。使用 multipart 时暴露 `ETag`，只有选定流程会读取时才暴露 `Content-Length` / `Content-Range` 等响应头。具体字段以 OSS 控制台当前 CORS 配置界面为准。

如果连接测试通过但浏览器直传失败，优先检查：

1. 浏览器拿到的 URL 是否使用公网 endpoint 或 CNAME，而不是内网 endpoint
2. CORS `AllowedOrigin` 是否与浏览器地址栏的 origin 完全一致
3. multipart part 响应的 `ETag` 是否对浏览器可见
4. 自定义域名 HTTPS 证书是否覆盖当前域名

## 5. 配置策略组并验收

新建测试策略组，把一个测试用户或团队绑定到 OSS 策略，然后依次验证：

- 小文件上传、下载、删除和恢复
- 大文件 multipart 上传、暂停后续传、取消
- Range 下载、PDF / 视频 seek、图片预览
- 文件复制、移动和覆盖冲突
- 公开分享下载
- `relay_stream` 与 `presigned` 两种上传和下载方式

验收完成前，不要直接修改已有策略的 bucket、endpoint、region、CNAME 或基础路径；这些字段共同决定旧对象的真实位置和签名方式。

## 常见问题

### `SignatureDoesNotMatch`

确认 region 与 bucket endpoint 匹配，AccessKey 没有多余空白，系统时间准确，并且没有把 OSS 原生策略填进通用 S3 connector。

### 服务端正常，浏览器 presigned URL 访问失败

服务端 endpoint 只证明 AsterDrive 后端可达。检查公网 endpoint / CNAME、HTTPS、DNS 和 CORS。

### CNAME URL 里又出现了 bucket

确认启用了 **使用 CNAME 自定义域名**，且公网 endpoint 填的是绑定到 bucket 的自定义域名，而不是 `aliyuncs.com` provider endpoint。

### CDN 下载返回 403

确认 CDN 鉴权方式与阿里云 CDN 控制台一致，鉴权密钥没有多余空白，并且下载链接尚未过期。CDN 侧会按鉴权方式用对象路径重算签名，方式或 KEY 不一致都会导致 403。
