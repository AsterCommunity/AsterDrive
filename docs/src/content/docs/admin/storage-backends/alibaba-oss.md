---
description: 阿里云 OSS 存储策略教程，覆盖原生 OSS V4 签名、公网与服务端 endpoint、CNAME、上传下载方式、CORS 和上线验收。
title: "阿里云 OSS 存储策略教程"
---

:::tip[这一篇覆盖什么]
这页讲怎样把 AsterDrive 文件写入阿里云对象存储 OSS。AsterDrive 使用原生 `OSS4-HMAC-SHA256` 签名，不把 Cloudreve 或 OSS 原生策略伪装成普通 S3-compatible 配置。
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

## 3. 创建 AsterDrive 存储策略

在 `管理 -> 存储策略` 新建 **阿里云 OSS**，填写：

- 公网 endpoint
- 可选服务端 endpoint
- OSS region
- bucket
- 可选基础路径
- 是否使用 CNAME
- AccessKey ID / AccessKey Secret
- 上传方式和下载方式

先使用 `relay_stream` 完成连接测试和端到端验收。确认服务端读写、Range、删除、复制和 multipart 稳定后，再切 `presigned`。

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
