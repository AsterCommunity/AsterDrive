# 出站 HTTP 装配与能力边界

AsterDrive #740 的所有权映射：

| 旧入口 | 统一入口 | Drive 保留职责 | 必测行为 |
| --- | --- | --- | --- |
| provider driver / 管理动作 builder | `ClientConfig` + Forge `HttpClient` | 签名、凭据、endpoint、解析、重试、错误映射 | UA、3xx、Range、body、控制/流式超时 |
| remote direct/tunnel/enrollment/proxy | InternalControl / InternalTransfer / InternalStreaming | 内部协议签名、owner 路由、取消、错误信封 | headers、流式、超时与 3xx 不转发 |
| WOPI discovery | Discovery | XML、缓存、失败回退 | UA、响应限制、3xx |
| builtin offline / aria2 RPC | OfflineDownload / InternalControl | 公网源验证、固定地址、任务取消、aria2 协议 | DNS pin 与 no_proxy、超时、UA |
| 外部认证 | ExternalAuth + Forge config client 注入 | 账号绑定、凭据、审计、provider 配置 | token/userinfo/OIDC 全链路继承 transport |
| plugin host | PluginTransport | manifest 授权、secrets、审计、调用生命周期 | origin、地址、大小、取消、超时拒绝 |

## Profile 与生命周期

`aster_drive_outbound::ClientConfig::new(profile, product_user_agent)` 必须明确用途。
使用 Driver/Runtime 实例持有 client，clone 复用连接池；管理动作在显式执行时装配，
不要为了 CORS 动作把独立管理生命周期塞入普通 driver 构造。
WOPI 和 direct remote 的既有缓存池保留；本次不创建新全局 registry。

控制请求使用 bounded total deadline；下载限制 idle read，不强加整文件总期限；
上传不安装 reqwest 的读取时钟，以免上传尚未结束就误判等待响应头超时。
七牛原有 progress watchdog 与 acknowledgement decode 期限继续归上传协议。
remote transfer 保留现有 1 小时总期限，tunnel long-poll 保留现有专用期限。
cluster proxy 使用 InternalStreaming，保留原先由 tunnel 持有的取消/期限，避免发送长 body
时触发 reqwest 等响应头的读取时钟。
离线下载保留按限速/大小计算的任务期限，固定 DNS 地址时禁用代理。

禁止自动重定向与自动重试。Google Drive resumable `308` 由 driver 解释；
不能把它当作普通跳转。签名后的 Request 通过 `execute` 原样发送，UA 由 client 默认头注入。
TLS 保留证书/hostname 验证，可信内置配置可增加根证书或指定代理；插件无此入口。

## 观测

每个 Drive client 都有 profile 标签和统一 tracing 事件。可通过 `.observer(...)` 注入
产品指标 sink，事件仅包含 method、status、outcome、duration；没有 URL、query、凭据或响应正文。
观测终点是响应头，取消单独记录；完整文件传输/EOF 指标继续使用现有 transfer 体系。
不把响应头计时宣称为完整请求体/响应体传输时长。

## Plugin host 契约

插件运行时尚由 #212 跟踪。本 crate 提供 host 可调用的网络边界，不注册插件或默认授权。
host 检查 `network:<origin>` capability 并记录审计后，创建 `NetworkGrant::approved_origin`，
再用 host 持有的 budget 与 CancellationToken 构造 `PluginTransport`。
插件只提交方法/URL/header/body，不能得到 raw client、builder、代理或 TLS 配置。

授权是精确 HTTPS origin（含端口），拒绝 wildcard、userinfo、fragment、跨 origin、
CONNECT/TRACE 和控制连接/目标/UA 的 header。每次执行验证全部 DNS 答案并固定地址，
拒绝私网、loopback、link-local、metadata、特殊 IPv4/IPv6 地址；强制直接连接，禁止重定向。
请求体/响应体默认各 1 MiB，host 上限 16 MiB，默认总期限 15 秒、上限 60 秒。
响应真实累计字节也受限；host 取消覆盖 DNS、响应头和读体。

凭据由 host 按授权 origin 注入，不能建立跨插件的默认认证头或共享 cookie store。
此接口不授予任意网络访问，也不开放 storage/auth provider 插件。

## Inventory 与有意边界

已迁移：OSS/COS/OBS/Azure CORS 管理、Kodo control/download/upload、Google Drive
及 OAuth、OneDrive Graph/OAuth、WOPI、remote direct/tunnel/cluster proxy/enrollment、
builtin offline、aria2 RPC、Forge 外部认证、OTLP blocking exporter、Mailpit 和 Rust fixture。

SDK 自带 transport 独立列账：AWS S3 SDK（含 OSS/COS/OBS 的 S3 数据面）使用 Smithy HTTP，
并非 reqwest；Azure Blob SDK 自己持有 pipeline，其内部 reqwest/重试/签名归 SDK，
本次没有把 SDK 偷换成裸 HTTP。显式原生管理请求已迁移；SDK 数据面要另验供应商协议，
不能用“没有直接 builder”证明 SDK 内部已采用 Drive profile。
浏览器 presigned URL 不经过服务器 HTTP client，也不属于该 factory 的出站路径。
aria2 下载源请求由外部 aria2 进程执行，RPC client 与下载 options 的 UA 分别保持。

运行 `python3 scripts/check-http-construction.py --self-test`。它扫描 src/crates/tests/examples/
templates，禁止 raw Client/ClientBuilder 构造和导入别名；Forge factory 是唯一底层例外。
CI 会执行同一检查。新增内置 provider 只使用 ClientConfig，测试只使用 Forge factory。
