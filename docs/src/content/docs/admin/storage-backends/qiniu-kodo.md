---
title: "七牛云 Kodo"
description: "配置 Kodo 原生可续传上传、私有源站读取和七牛 CDN 交付。"
---

Qiniu Kodo connector 使用七牛原生协议，替换早期的 S3 wrapper。AsterDrive 继续拥有文件、版本、权限、配额、Blob 和上传完成事务；Kodo 保存对象内容。通用 S3 connector 仍用于 S3-compatible 接入。

## 配置前准备

准备 Kodo **原生空间名称**、AccessKey / SecretKey，以及绑定该空间并启用 HTTPS 的**源站下载域名**。S3 空间名可能不同于原生空间名称；旧策略升级由联网验证自动确认。按账号权限模型限制空间和对象前缀权限；原生上传、stat、list、copy、delete 以及私有读取均需可用。不要将原生权限与旧 S3 Bucket Policy 示例混用。

上传服务和管理服务通过七牛区域查询发现。源站域名用于下载，不能当上传 endpoint；CDN 下载域名也不能用于上传。本版本不提供自定义上传加速域名配置。七牛传输加速是单独的服务，有开通条件及额外上行计费，不能从下载 CNAME 推导出上传加速能力。

## 创建策略

进入“管理 → 存储策略 → 新建策略”，选择 **Qiniu Kodo**。

| 字段 | 含义 |
| --- | --- |
| 原生空间名 | 控制台中的普通 Kodo 空间名称，不是 S3 别名 |
| 基础路径 `base_path` | 空间内对象 key 的前缀；留空为根目录。接管已有 AD 对象时必须保持原值 |
| 源站下载域名 `download_origin` | 绑定空间的 HTTPS 源站域名，不含路径、query 或 fragment |
| 下载签名有效期 `download_auth_validity_seconds` | 1–3600 秒，默认 180 秒。作用于服务端源站读取和私有下载；默认 Kodo 源站的实际有效期最多 300 秒 |
| 原生可续传上传 | `server_relay`：服务器流式代理；`frontend_direct`：浏览器直传原生分片 |
| 下载策略 | `relay_stream`：通过 AD；`presigned`：按交付模式签发原生私有链接 |
| 下载交付 | `origin`：源站；`qiniu_cdn`：配置了私有空间鉴权的七牛 CDN |
| CDN 域名 | 仅 CDN 模式必填，必须为 HTTPS origin；对象前缀统一配置在 `base_path` |
| AK / SK | 静态凭据加密保存；编辑时密码留空保留已保存值 |

源站和 CDN 域名、原生空间、`base_path` 必须指向同一对象命名空间。连接测试会写入独立临时对象，验证 stat、HEAD、GET、Range 和删除；它不代表 CDN 或浏览器 CORS 已通过。

## 上传与恢复

- **服务器代理**：浏览器将内容交给 AD，AD 流式上传七牛。按策略分片大小和单次上传限制，选择单次上传或可恢复的编号分片。
- **浏览器直传**：内容直接上传七牛原生分片接口，支持并发、重试和恢复。AK/SK 不下发浏览器。

单次表单上传上限为 1 GiB。编号分片的非末片至少 1 MiB、每片最多 1 GiB、最多 10,000 片；七牛分片会话有效期为 7 天，AD 会话可更早过期。

浏览器网络必须能访问七牛上传域名。上传 CORS 允许 AD 站点来源、PUT、Authorization 和 Content-Type；下载侧按使用场景允许 GET、HEAD、Range，并暴露所需响应头。服务端连接测试不代表浏览器 CORS 验收通过。

## 私有源站和 CDN 下载

| 下载策略与交付 | 实际行为 |
| --- | --- |
| 服务器中继 | AD 从配置的源站读取并转发；CDN 设置可以保留，但不参与这条路径 |
| 私有直链 + 源站 | AD 鉴权后返回 302 到源站私有链接，支持 Range |
| 私有直链 + CDN | AD 鉴权后返回 302 到 CDN 私有链接，支持 Range |

源站和 CDN 直连统一使用供应商原生 MIME/缓存头；附件使用当前 AD 文件名，改名不需要重写七牛对象。不会为了覆盖 MIME/Cache-Control 或 Range 自动改走中继。HTML/SVG 内联预览、条件验证和 AD 生成的缩略图等保留各自的同源处理。

下载签名有效期范围为 1–3600 秒、默认 180 秒，实际取策略设置、调用方要求和入口限制的最小值。默认 Kodo 源站域名最多 300 秒。服务端读取和缩略图也遵守这一规则；签名过期不截断已经建立的传输，也不会清除浏览器已经缓存的内容。

七牛 CDN 须开启私有空间回源鉴权；AD 使用七牛原生私有链接，不叠加独立的时间戳防盗链。分别验证缓存 HIT/MISS 下的有效、无凭证、篡改与过期请求，并按需要验收密钥轮换和浏览器 CORS。CDN MIME/缓存策略在七牛控制台管理。签名链接属于短期访问凭证，请勿公开或修改；AD 权限撤销不保证立即撤回已签发链接。

## 从旧 S3 connector 升级

1. 排空旧版本写入，完成或取消 Kodo 上传和旧分片清理任务；旧 S3 上传会话不能在原生接口继续。Primary/Follower 在同一维护窗口升级。
2. 保留原实例的凭据加密主密钥，启动新版本。应用联网确认原生空间及默认 HTTPS 源站，成功后统一转换配置；策略 ID、对象 key 和基础路径保持不变。
3. 确认失败、网络/权限不足、清理失败或并发编辑时，启动会停止并保留原配置。修复条件后重启重试。每个连接的验证最多等待 5 分钟。
4. 迁移成功不可逆，不创建旧配置备份，也不提供自动回滚。必须通过正式应用启动完成联网升级，不能只运行 schema migration。
5. 新写入前验收原有文件下载、预览和 Range。CDN 需要管理员单独配置；之后的正常重启不重复联网升级。

联网升级需要列举空间、查询区域和临时探测对象的读写删除权限。可执行清理任务缺少加密凭据快照时须先在旧版本完成或取消，再升级。

已有 AD 文件继续引用原对象；仅从七牛控制台上传的新对象不会自动导入 AD。

协议、流式资源限制和迁移校验细节见 [Kodo connector 开发契约](https://github.com/AsterCommunity/AsterDrive/blob/master/developer-docs/zh-CN/design/qiniu-connector-contract.md)。

## 本地与真实云验收

本地测试覆盖原生请求合同、签名向量、分页/恢复边界、流式长度和清理，但不构成供应商验收。真实云测试读取以下环境变量：

```text
ASTER_TEST_QINIU_KODO_BUCKET          原生空间名
ASTER_TEST_QINIU_KODO_ORIGIN          HTTPS 源站 origin
ASTER_TEST_QINIU_KODO_ACCESS_KEY      AK
ASTER_TEST_QINIU_KODO_SECRET_KEY      SK
ASTER_TEST_QINIU_KODO_BASE_PATH       可选，已有对象前缀
ASTER_TEST_QINIU_KODO_EXISTING_KEY    可选，相对于 base_path 的已有对象 key；仅 stat/Range，不修改
```

```bash
cargo nextest run --profile ci --test storage native_kodo_cloud_acceptance
```

凭据缺失时测试明确打印跳过，不表示云端通过。测试只清理自身随机前缀内的对象。浏览器直传、CORS、CDN HIT/MISS、过期/轮换与多端恢复需另外在真实环境验收；记录仅保留匿名请求类别、状态码和结果。

## 官方依据

- [下载设置与响应头覆盖](https://developer.qiniu.com/kodo/1659/download-setting)
- [Go SDK 固定版本](https://github.com/qiniu/go-sdk/tree/f278ca5c9cc5d4a85f304e4b73f2b0b36437c66e)：认证、区域发现、原生 API 定义
- [分片上传 v2](https://developer.qiniu.com/kodo/7458/multipartupload)
- [上传凭证](https://developer.qiniu.com/kodo/1208/upload-token)
- [私有下载](https://developer.qiniu.com/kodo/1656/download-private)
- [CDN 回源鉴权](https://developer.qiniu.com/fusion/3930/back-to-the-source-authentication)
- [CDN 鉴权组合限制](https://developer.qiniu.com/fusion/13361/fusion-api-error-codes)
