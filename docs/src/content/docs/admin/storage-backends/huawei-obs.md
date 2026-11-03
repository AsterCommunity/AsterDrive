---
description: 华为云 OBS 存储策略教程，覆盖原生 OBS 签名、区域 endpoint、自定义域名、凭证、CORS、预签名和 multipart 验收。
title: "华为云 OBS 存储策略教程"
---

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

CNAME 只负责 AsterDrive 到 OBS 的访问；它不提供 CDN 下载鉴权。CDN 下载交付在下方单独配置。

## CDN 下载交付与 URL 鉴权

如果对象通过华为云 CDN 加速域名对外下载，可以在 connector 里配置独立的下载交付地址。**上传、清理和存储 API 仍走 OBS endpoint / custom domain，只有下载交付 URL 指向 CDN。** 这样 OBS 私密访问和 CDN 公共下载可以分开控制。

| 字段 | 说明 |
| --- | --- |
| Download base URL | CDN 加速域名，例如 `https://cdn.example.com`，可以带基础路径。只用于下载交付 |
| 鉴权方式 | `type_a` / `type_b` / `type_c1` / `type_c2` / `public`，默认 `type_a` |
| 有效期上限 | `cdn_expire_time`，0–31536000 秒。签发 URL 的有效期取请求有效期与该上限的较小值 |
| 鉴权参数名 | `cdn_sign_arg`，默认 `auth_key` |
| 时间参数名 | `cdn_time_arg`，默认 `timestamp` |
| 加密算法 | `md5`（默认）或 `sha256` |
| 主鉴权 KEY | `cdn_primary_key`，6–32 位字母和数字 |
| 备鉴权 KEY | `cdn_backup_key`，可选；主备任一通过即可访问 |

四种鉴权方式的 URL 形态不同，分别对应华为云 CDN 的四种 URL 鉴权：

- `type_a`：`/对象?auth_key=timestamp-rand-0-hash&timestamp=timestamp`，时间戳为十进制 Unix 秒；
- `type_b`：`/YYYYMMDDHHMM/hash/对象`，时间戳为 UTC+8 的分钟级时间；
- `type_c1`：`/hash/十六进制时间戳/对象`，时间戳为十六进制 Unix 秒；
- `type_c2`：`/对象?auth_key=hash&timestamp=timestamp`，时间戳为十进制 Unix 秒。

`public` 不生成任何签名，URL 就是 `download_base_url + 对象路径`。

### 私有桶回源授权不保护用户访问

如果 OBS 桶是私有的，CDN 需要额外的回源授权（例如 OBS 侧允许 `CDNAccessPrivateOBS`、配置回源鉴权），否则 CDN 节点无法读取对象。**这只解决“CDN → OBS”的读取授权，不会保护“用户 → CDN”的访问。** 要限制谁能下载，必须配置上面除 `public` 之外的 URL 鉴权方式，并妥善保管 CDN 鉴权 KEY。

### 缓存、忽略 URL 参数与已签发 URL

- 如果 CDN 没有配置“忽略 URL 参数”，鉴权 query（`auth_key`、`timestamp`）会成为缓存 key 的一部分：同一对象每次签发的 URL 不同，CDN 会当成不同缓存对象，增加回源概率。
- 建议在 CDN 域名配置“忽略指定参数”，把 `auth_key` / `timestamp`（或自定义的参数名）排除在缓存 key 之外，让同一对象复用缓存；CDN 仍会先校验鉴权，通过后才回源或命中缓存。
- 已签发的 CDN URL 在有效期内不会因为 AsterDrive 权限变化而立即失效。撤销分享只阻止后续签发；已经分发出去的 URL 在 CDN 侧仍可能有效，直到有效期结束。
- 边缘节点缓存命中期间，即使 URL 已过期或策略已变更，节点仍可能继续提供已缓存内容，直到缓存过期或手动刷新。

### Range、HEAD、CORS 与响应头

CDN 下载是浏览器直连第三方加速域名：

- 视频拖动和断点续传需要该 CDN 域名支持 Range 并返回 `Accept-Ranges` / `Content-Range`；
- 前端如需跨域读取（fetch 下载或播放），CORS 要配置在 CDN 加速域名上，而不是 OBS 桶的 CORS 规则；
- `Content-Disposition`、`Content-Type`、`ETag` 等响应头由 CDN 及回源配置决定，需要自定义时在 CDN 域名上配置。

### public 模式的风险

`public` 模式下不生成签名，等于把 `download_base_url` 下的对象公开给任何拿到 URL 的人。仅在内容明确公开、且 OBS 桶允许 CDN 回源时使用；否则请使用带 KEY 的鉴权方式。

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

## 3. 配置 OBS CORS

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
| 允许的来源 | AsterDrive 页面实际 origin；排查阶段可以使用 `*` |
| 允许的方法 | `GET`、`HEAD`、`PUT` |
| 允许的头域 | `Content-Type`；排查阶段可以临时使用 `*` |
| 补充头域 | `ETag`；Range 下载可再加入 `Content-Length`、`Content-Range`、`Accept-Ranges` |
| 缓存时间 | `3600` |

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
