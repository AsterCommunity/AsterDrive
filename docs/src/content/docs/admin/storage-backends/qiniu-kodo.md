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

## 上传、恢复和资源占用

七牛与 OneDrive/Google Drive 同属 provider resumable 入口，但协议不同：Kodo v2 是**编号分片、可并发、显式完成**；后两者是顺序 range、末片隐式提交。AD 为 Kodo 复用编号分片的调度、实际大小验证和清理，不将 UpToken 伪装成 SigV4 presigned PUT。

服务端小对象使用原生表单 POST，超过表单上限的流使用 multipart v2。代理请求直接消费 `AsyncRead`，使用固定 64 KiB 读取缓冲；逻辑分片大小不等于内存缓冲大小，不先组装整片或整个文件到内存/临时文件。失败和取消尝试中止 multipart；进程退出或创建响应丢失时，供应商会话过期仍是最终清理保障。

浏览器使用原生分片 PUT 和仅限当前空间/key 的短期 UpToken。密钥不会下发浏览器。分片可重试、并发和查询恢复；后端在合并前查询真实分片、大小和 ETag。非末片下限 1 MiB、单片上限 1 GiB、最多 10,000 片；原生会话有效期为 7 天，AD 上传会话可更早过期。完成后才执行 AD 的文件/Blob/version、配额和审计事务。

浏览器网络必须能访问七牛上传域名。配置与 AD 站点 origin 匹配的 CORS，允许 PUT、Authorization、Content-Type；下载侧按使用场景允许 GET、HEAD、Range，并暴露所需响应头。不要把服务端连接测试通过当作 CORS 验收。

## 私有源站和 CDN 下载

原生私有链接对最终 URL 计算签名，格式为 `?e=<deadline>&token=<credential>`，token 位于最后。签名后不能替换 Host、路径或追加参数。路径中的中文、空格和字面百分号分别编码；链接的截止时间与响应 `expires_at` 一致。

下载签名有效期可像 COS/OBS 的 CDN 有效期一样在策略里设置。实际有效期取调用方要求、策略设置和入口限制的最小值；默认 `*.kodo-*.qiniucs.com` 源站最多 300 秒，默认设置 180 秒。自定义源站/CDN 使用策略上限，不能延长调用方请求的期限。服务端流读取、HEAD、Range 和缩略图共用同一规则；签名 TTL 不用于截断已建立的流。旧 V2 配置缺少此字段时默认 180 秒，无需重新保存策略。

使用七牛 CDN 时，在七牛控制台开启面向私有 bucket 的回源鉴权。AD 使用原生 `e/token`，不额外实现 CDN 时间戳防盗链；七牛文档列有两种鉴权同时配置的互斥限制。分别验证缓存 MISS/HIT 的无凭证、错误凭证、改路径、改时间和过期请求，区分边缘内容缓存与鉴权结果缓存。

下载策略优先：选择 `relay_stream` 时可以保存休眠的 CDN 配置，但文件仍经 AD 中继。源站直连无法满足精确响应要求时回退中继；CDN 附件下载接受原生 MIME/缓存头。HTML/SVG 等资源仍遵守 AD 的安全隔离。临时 URL 是 bearer 凭据，撤销 AD 权限不会立即撤回已签发 URL；不要公开完整签名链接。

普通源站 GET 支持七牛原生 `response-content-disposition`、`response-content-type` 和 `response-cache-control` 参数；AD 将当前逻辑文件名、MIME 与缓存要求编码后放在 `e/token` 之前一起签名。因此启用 presigned 后，浏览器默认下载可从 AD 鉴权入口收到 302，再访问源站签名 URL。七牛仅承诺成功的完整 GET 应用这些覆盖；要求精确响应头的 Range 继续中继，可能自行发起 Range 的非 Blob 资源消费也保持保守回退。

CDN 附件下载统一使用原生响应：AD 将当前文件名放入 `attname` 并与 `e/token` 一起签名，MIME 和 Cache-Control 由 CDN 返回，不提供额外的响应模式开关。改名不需要重写远端对象。浏览器默认下载先由 AD 鉴权，随后 302 到 CDN；授权 Blob 下载/预览也可直连。inline HTML/SVG 的同源隔离与其他精确预览要求仍生效。请在七牛控制台管理 CDN 的 MIME/缓存策略，并验收 Content-Disposition（含中文名）和 Range；`attname` 不能在签名后追加或更改。签名截止时间控制联网访问鉴权，不会清除浏览器已经缓存的内容。

## 从旧 S3 connector 升级

1. 停止/排空旧版本写入，完成或取消 Kodo 上传；旧 S3 uploadId 无法转换为原生会话。Primary/Follower 在同一维护窗口升级。
2. 使用原实例的凭据加密主密钥启动新版本。启动升级先读取已有 AK/SK，查询可见原生空间及区域的默认源站 `io_src`，再通过旧 S3 空间下独立随机小对象的内容核对原生空间映射。探测对象仅位于原 `base_path`，完成后删除；不读写用户已有对象。
3. 确认只有一个原生空间与旧命名空间匹配、默认 HTTPS 源站可读取且探测清理成功后，一次事务替换全部 V1 配置：保留策略 ID、key、`base_path`、行为和加密凭据，填入已验证的原生空间与源站。原生上传/下载运行时不再使用旧 S3 协议。
4. 不创建备份表。旧 S3 endpoint、region 和空间别名只在全部验证成功后丢弃；网络/权限失败、映射不唯一、清理失败或并发编辑都中止升级并保留原配置，可修复条件后重启重试，不会先清空字段让管理员重填。
5. 新写入前检查原有文件下载、预览和 Range；CDN 自定义交付仍由管理员明确选择，不能从默认源站查询猜测使用哪条 CDN 域名。

联网升级需要列举原生空间、查询区域默认源站、旧空间写入/读取/删除探测对象及原生私有读取权限；这些是升级期权限，受限子账号不足时须临时补足。单个连接验证最多等待 5 分钟，失败信息只含记录身份与脱敏原因。升级仅在仍有 V1 配置时联网，之后重启不重复探测。

现存临时清理快照若只涉及对象删除，可使用快照中的加密凭据一并联网确认转换，保持 temp key；仍含旧 S3 multipart 会话的可执行清理任务须先完成或取消。已完成/取消任务保留历史原文。schema-only migration 不会猜测或丢弃 V1 数据：须先通过携带实例加密主密钥的应用启动完成联网升级。已转换的配置不可逆，migration down 不恢复旧 S3 配置。

“接管已有对象”指 AD 数据库已有的文件记录继续指向相同内容；仅从七牛控制台上传的任意新对象不会自动获得 AD 文件、权限和配额记录。本次不新增自动导入功能。

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
