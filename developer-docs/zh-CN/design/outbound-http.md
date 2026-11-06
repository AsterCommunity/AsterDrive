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

`aster_drive_outbound::ClientConfig::new(profile)` 必须明确用途，默认使用
`aster_drive_outbound::OUTBOUND_HTTP_USER_AGENT`（`AsterDrive/<workspace version>`）。
该常量从主程序 config 移到 outbound crate 统一定义，避免反向依赖或重复定义；
调用方无需传入身份，有专门用途时通过 `.user_agent("custom identity")` 覆盖。
PluginTransport 同样默认使用产品 UA，只有 host 可以通过 `.user_agent(...)` 覆盖。
使用 Driver/Runtime 实例持有 client，clone 复用连接池；管理动作在显式执行时装配，
不要为了 CORS 动作把独立管理生命周期塞入普通 driver 构造。
WOPI 和 direct remote 的既有缓存池保留；本次不创建新全局 registry。

控制请求使用 bounded total deadline；下载限制 idle read，不强加整文件总期限；
OneDrive 分别持有控制 client（60 秒总期限）与下载 client（30 秒读取空闲期限、无总期限），
直接 200 内容与显式重定向目标都使用下载策略；只有 Graph 请求携带凭据，Range 只发给下载目标。
OneDrive 分片另用 ProviderUpload：以 HTTP transport 实际消费 body 的进度刷新 30 秒空闲期限，
发送完成后限制等待响应头的空闲时间，确认 JSON／错误 body 解码也最多 30 秒。
小体积 PUT 和 metadata 仍走控制 client；relay 分片调用方保留 90 秒总期限与 claim heartbeat，
取消调用会丢弃 HTTP exchange 和输入 reader。
Google Drive 同样复用独立 control/download/upload 连接池：control 总期限 120 秒，
download 保留 120 秒读取空闲期限且无总期限，上传用 120 秒进度／确认期限。
Google OAuth／refresh 明确使用 ExternalAuth，不再借用 storage download pool。
OneDrive、Google Drive 和 Kodo 共用 `aster_drive_outbound::send_upload` 的 transport
进度与确认响应生命周期；provider 保留 token、签名、256/320 KiB 对齐、308/Range 收据、
响应解析及错误分类。共用的是传输机制，不把三家协议做成一个状态判断。
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

授权是精确 HTTPS origin（含端口），拒绝 wildcard、userinfo、fragment 和跨 origin 请求。
仅允许 GET、HEAD、POST、PUT、PATCH、DELETE，拒绝其余方法（包括 OPTIONS、CONNECT、TRACE）
以及控制连接/目标/UA 的 header。每次执行验证全部 DNS 答案并固定地址，
拒绝私网、loopback、link-local、metadata、特殊 IPv4/IPv6 地址；强制直接连接，禁止重定向；条件请求的 `304 Not Modified` 保留原始状态与缓存 header。
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
