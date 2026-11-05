# 华为 OBS CORS 动作契约与证据

对应 #732 / #730。代码入口为 `src/storage/connectors/huawei_obs.rs`、`src/storage/drivers/huawei_obs/cors.rs` 和 `signing.rs`。本页区分外部协议依据、本地防护和真实云验收，避免把 bucket 写入成功当作浏览器或 CDN 验收。

## 官方协议依据

本次核对时间：2026-11-05。沿用现有 OBS signer 固定的官方 Go SDK v3.26.6 revision **`fd2b44881f0cd9bd41ffff2fabeb94c783ccc321`**，以下链接均固定到该 revision：

| 事实 | 官方源码证据 |
| --- | --- |
| GET / PUT 桶根路径的 `cors` 子资源；读取、设置是独立管理操作 | [`obs/client_bucket.go`：GetBucketCors / SetBucketCors](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/client_bucket.go#L396) |
| PUT XML 完整文档，默认 Content-MD5 为 Base64(MD5(body)) | [`obs/trait_bucket.go`：SetBucketCorsInput.trans](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/trait_bucket.go#L188) |
| `CORSConfiguration/CORSRule`，可选 ID，Origin、Method、Header、MaxAgeSeconds、ExposeHeader | [`obs/model_base.go`：CorsRule / BucketCors](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/model_base.go#L159) |
| 原生 `OBS AK:Base64(HMAC-SHA1(SK, StringToSign))`，签入 MD5、Content-Type、Date、规范资源 | [`obs/authV2.go`](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/authV2.go)、[`obs/auth.go`](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/auth.go) |
| 普通模式以 bucket 构建 Host；CNAME 使用实际自定义 Host 构建规范资源 | [`obs/conf.go`](https://github.com/huaweicloud/huaweicloud-sdk-go-obs/blob/fd2b44881f0cd9bd41ffff2fabeb94c783ccc321/obs/conf.go) |

官方 OBS REST 文档镜像 revision **`b5515cf03462beac5ab60af7bc3525ba6e8e667c`**：[`获取桶的 CORS 配置`](https://github.com/huaweicloudDocs/obs/blob/b5515cf03462beac5ab60af7bc3525ba6e8e667c/cn.zh-cn/API%E5%8F%82%E8%80%83/%E8%8E%B7%E5%8F%96%E6%A1%B6%E7%9A%84CORS%E9%85%8D%E7%BD%AE.md) 给出 GET 响应的区域 OBS 默认 XML namespace、可为负数的 MaxAgeSeconds、64 KiB / 100 条上限和 `NoSuchCORSConfiguration` 404；[`设置桶的 CORS 配置`](https://github.com/huaweicloudDocs/obs/blob/b5515cf03462beac5ab60af7bc3525ba6e8e667c/cn.zh-cn/API%E5%8F%82%E8%80%83/%E8%AE%BE%E7%BD%AE%E6%A1%B6%E7%9A%84CORS%E9%85%8D%E7%BD%AE.md) 给出完整 XML PUT、Content-MD5 与单个星号匹配符；[`桶相关授权项`](https://github.com/huaweicloudDocs/obs/blob/b5515cf03462beac5ab60af7bc3525ba6e8e667c/cn.zh-cn/API%E5%8F%82%E8%80%83/%E6%A1%B6%E7%9B%B8%E5%85%B3%E6%8E%88%E6%9D%83%E9%A1%B9.md) 明确 Get / Put action，其中 **PutBucketCORS 同时授权设置和删除桶 CORS**。

补充核对官方 [Python SDK getBucketCors / setBucketCors](https://github.com/huaweicloud/huaweicloud-sdk-python-obs/blob/5558ee6d8b038e7d1d6296ad57531bb7522e027c/src/obs/client.py#L1577)，同样使用独立 `cors` 子资源。华为 support 文档站本次返回验证码页，未把该页算作已阅读的 REST 证据。不能由 XML 相似性推导 COS / OSS 签名通用；区域差异仍需真实云验证。

## 所有权与管理目标

- 现有管理员 action route / service 负责权限、策略归属与凭据解密；不增加 provider 专用 API。
- Connector 声明 `configure_huawei_obs_cors`、中英文本地化与输出；使用当前 draft / saved 配置，严格解析 `public_site_url`，编辑留空凭据通过现有 `prepare_draft_credential` 保留并校验 connector 归属。
- Driver 负责 HTTP、OBS 原生签名、XML、规则合并和脱敏错误。复用 OBS signer 的规范资源和 HMAC 实现，不调用 COS 或 OSS signer。
- 普通模式请求 `https://BUCKET.obs.REGION.myhuaweicloud.com/?cors`；已规范化的 bucket-prefixed endpoint 不重复追加 bucket。CNAME 模式请求 `https://BOUND-OBS-DOMAIN/?cors`，规范资源为 `/BOUND-OBS-DOMAIN/?cors`，不是 `/BUCKET/?cors`。
- `download_base_url` / CDN domain 永不作为管理目标。CNAME endpoint 必须直接绑定 OBS，不能把 CDN 加速域名当作 OBS 管理入口。
- 使用项目 `OUTBOUND_HTTP_USER_AGENT`，沿用连接／读取／操作超时，禁用重定向，防止签名被转发到另一 Host。

执行沿用通用“此操作可能修改远端存储状态，请确认后继续”确认框。不增加目标／Origin 专用确认 schema。创建、普通保存、连接测试不隐式调用 CORS。

## 合并、限制与失败边界

先 GET，只有 HTTP 404 且 provider code 为 `NoSuchCORSConfiguration` 才视为空配置。不存在 bucket、权限拒绝、网络错误、重定向、超限或无法解析的读取均不发 PUT。

保留现有规则、可选 ID、规则顺序和全部已识别字段。仅追加覆盖缺失请求的规则，不因包含本站 Origin 或某个 ID 就认领、修改或删除规则。当前请求已满足则返回 `changed=false`，不写入；再次执行不追加。保守按首个匹配规则检查：较早规则若匹配当前请求但缺少必要暴露头，则拒绝写入并提示在 OBS 控制台处理，不把追加一个更晚的规则当作修复。

新增规则来自当前实际浏览器路径：PUT 单文件／UploadPart（无自定义头及 Content-Type）、GET（含 Range）、HEAD；multipart 初始化、完成、取消仍由后端执行，不要求浏览器 POST / DELETE。允许 Content-Type、Range，暴露 ETag、Content-Length、Content-Range、Accept-Ranges、Content-Disposition，预检缓存 600 秒；不默认开放 `*`。PUT 至少需 ETag，GET / HEAD 需下载和预览使用的非 safelisted 头。支持既有单星号 Origin / header 模式，保守地按大小写不敏感检测潜在匹配；未知正则元字符拒绝自动写回。

官方文档规定至多 **100 条规则、64 KiB 完整 XML**；本地另限定 3000 XML 元素、深度 12，错误体最多 8 KiB。自动合并器超出限制时拒写，不推断所有区域配额。达到规则上限且已有规则可满足请求时仍可无写返回；需要追加或文档超过本地上限时失败，绝不截断再写。接受官方区域 OBS 默认 namespace 与无 namespace 文档；对未知 namespace、未知字段（包括本次 Go 模型之外的 ResponseVary）、属性、重复单值字段等拒绝写回；交给管理员保留原配置并手工处理。不要为兼容某个未核实扩展而静默丢字段。

最小新增管理权限为目标 bucket 上的 `obs:bucket:GetBucketCORS`、`obs:bucket:PutBucketCORS`，不调用删除 CORS、bucket policy 或 CDN 管理 API；但华为的 PutBucketCORS 授权项本身也允许删除，须认清权限范围；IAM 授权及 bucket policy 的显式拒绝均需云端核对。403 AccessDenied 按 GET / PUT 提示核对对应 action、bucket/user policy 和日期头，不断言根因就是缺少权限；其他 code（如 SignatureDoesNotMatch）不误报为缺少管理权限。结果输出 bucket、规范化 Origin、changed、保留规则数及 x-obs-request-id；错误仅输出有限长度的 provider code、状态、RequestId 和静态建议，不暴露响应 Message、凭据或签名 URL。

GET 后 PUT 替换文档并非原子 patch；该管理协议没有在本次依据中证明可用的条件写入。管理员执行期间应避免其他控制台或进程并发修改 bucket CORS。本地合并测试不能保证跨控制台并发配置不被覆盖。

## 签名与验证证据

`native_signatures_match_independent_python_hmac_vectors` 的向量用 Python 标准库独立计算，未调用 Rust signer 生成期望值：

```python
base64.b64encode(hmac.new(b'secret-key', string_to_sign.encode(), hashlib.sha1).digest())
```

时间固定 `Tue, 14 Nov 2023 22:13:20 GMT`。GET 使用空 MD5 / Content-Type；PUT 使用测试值 `abc=` / `application/xml`。普通模式资源 `/archive-bucket/?cors` 的 GET / PUT 摘要分别为 `pWHwARJ6hz+RjvkkGBtMIQ+5JO8=` / `1NBzQ9tTr6ngyq7fFAuSNeSO8YQ=`；CNAME `/files.example.test/?cors` 分别为 `wYewf0Go1hFOOW2a2LG9UNbWf9E=` / `JEt/LdLWl5Yqcp30pUKAeelpSEY=`。实际 HTTP PUT 测试另检查 XML、Content-MD5、原生 Authorization、Host／路径／query 和 User-Agent。

复核入口：

```bash
cargo nextest run --profile ci --lib storage::drivers::huawei_obs
cargo nextest run --profile ci --test storage obs_cors
make storage-docs-check
cd frontend-panel
bun run test -- src/components/admin/storage-policy-dialog/StorageConnectorActionsPanel.test.tsx src/components/admin/storage-policy-dialog/policyActionSelection.test.ts
```

本地覆盖请求目标、签名、XML／ID／顺序、空配置、幂等、规则遮挡、限制、权限及错误脱敏；管理员 route 覆盖草稿、已保存策略、留空凭据保留、目标切换、普通保存／连接测试隔离和非管理员拒绝。通用前端组件覆盖 action 显示、确认及草稿／保存路由，并保留 COS / OSS 回归。

**真实云状态：未验证。** 本次没有使用真实 OBS 凭据写入 bucket，没有证明真实浏览器单文件／分片直传、GET／HEAD／Range、ETag／Content-Disposition 可读、CNAME 网络路径或 CDN 生效。云端验收需保留脱敏前后配置、第二次不变更结果和 RequestId，按浏览器路径验证；#716 的 CDN 回源、鉴权与缓存验收继续独立保留。
