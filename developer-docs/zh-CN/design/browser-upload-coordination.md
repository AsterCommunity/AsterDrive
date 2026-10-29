# 浏览器上传协调契约

Provider resumable 浏览器直传使用后端协议状态机。扩展协议由 driver 实现，核心和前端无需为 provider 增加响应解析分支；本契约不包含插件安装、加载或沙箱。

## 所有权

- `aster_drive_storage::BrowserUploadDriver`：协议标识、版本支持、provider 响应解析、权威进度查询，以及生成下一条浏览器指令。
- `ProviderResumableUploadDriver::browser_upload()`：可选能力入口。中继和其他 connector 不需要空实现。
- Driver runtime capabilities：声明 `browser_upload_protocol`；connector descriptor 通过同一运行时定义转换，不维护第二份事实。
- Upload session service：owner / workspace 校验、过期和终态限制、加密私有状态、指令校验及数据库条件更新。
- 前端 `uploadService`：执行有限 HTTP 指令，回报观察结果；`browserUploadResource`：串行协调、取消和有界重试；runner 负责任务展示和最后调用产品 complete。
- 原有 complete 链路：校验实际对象大小，事务性发布文件、版本和配额。浏览器回报与 provider completion 都不能代替最终校验。

## 状态与指令

协议标识是 driver 拥有的不透明字符串，协议版本由 driver 判断是否支持；前端只识别 `executor_version = 1`。

新会话在加密的 `provider_session_ciphertext` 内保存协议快照、私有 JSON 状态、连续确认 offset、provider completion 和待执行指令。这里的 JSON 是不透明数据，不是表达式、模板或脚本。

进度与恢复列表沿用 reader 数据连接，对 provider direct resumable 会话只读取已保存快照，不访问 provider、不写库、不更换待执行指令的 request ID。开始或恢复传输时，前端先显式发送一次空 report，获取 provider 权威进度和新指令；不直接重发快照中的旧指令。未知协议或 provider 查询失败只影响该会话的协调，不会因为枚举它而阻止其他会话恢复。

`POST /files/upload/{upload_id}/provider-step`（团队路径具有相同后缀）接受：

- 无 request ID 和 observation：由 driver 查询 provider 权威状态，恢复到下一步。
- 成对的 request ID 和 observation：只在 ID 匹配当前待执行指令时解析；旧 ID / 重复回报执行权威协调，不能把旧响应解释为新请求的结果。

返回 `BrowserUploadStep`：协议快照、连续确认字节数、协议进度 revision、provider completion，以及可选的下一条指令。`confirmed_offset = N` 表示 `[0,N)` 连续确认，不能使用浏览器已发送字节数替代。

指令支持跨 origin 的 GET / PUT / POST / PATCH / DELETE、driver 生成的 capability URL、受限 header，以及文件的半开字节区间或空 body。元数据交换、需要管理员认证的请求、特殊编码和其他控制逻辑留在后端 driver。当前执行器不支持浏览器运行插件代码、任意 body 生成器或外部认证。Provider 不支持 CORS 时应选择中继；不能宣称所有协议都能浏览器直传。

指令具有 request ID、60 秒请求超时和 64 KiB 响应体预算。前端禁止携带 cookie、管理员 Authorization 和产品 CSRF header；同 origin 指令被拒绝，因为 XHR 的 withCredentials=false 无法省略同源 cookie，响应只回报指令声明的 header；不可见的 header 不伪造为空值。网络失败和超时回报 status 0。响应正文按 UTF-8 字节截断并标记 `body_truncated`，后端也独立检查输入大小。私有协议状态最多 32 KiB；完整加密会话最多 60 KiB，兼容现有 MySQL TEXT 列，不引入数据库 migration。

## 一致性、恢复和安全

私有协议状态、确认 offset 或完成状态变化时才增加进度 revision；仅更新请求 ID 不算进展。前端允许不推进文件 offset 的控制阶段，通过 revision 区分协议推进与重复停滞。

状态变更按旧 ciphertext 做 CAS，同时要求会话仍为 uploading、未过期且 transport 为 provider direct resumable。Provider I/O 期间不持有数据库事务锁。并发协调只有一个更新可以成功；取消、完成或过期后的请求不能发布新指令。CAS 失败返回可重试错误。

Driver 的 advance 可能被重复调用，必须可重试，不执行没有幂等保证的外部写入。私有状态和下一条指令一起提交。无 observation 的 advance 必须能根据已保存状态和 provider 权威查询恢复，包括浏览器请求已经成功但响应丢失的情况。

已确认 offset 不能倒退，不能超过文件长度。前端和后端均限制 JavaScript 安全整数。未完成状态必须提供下一条指令；provider completed 必须确认全部字节且没有下一条指令。完成或错误均须经过取消检查。

浏览器 observation 不可信。driver 应校验 offset 是否落在实际发出的范围内；模糊确认、CORS 隐藏 header、网络失败和重复请求应查询 provider。会话消失不能直接判定完成，只有实际对象存在且大小正确才能恢复为 provider completed。最终对象校验仍由 complete 独立执行。

Capability URL、私有协议状态和观察结果不得记录到日志、错误或审计。插件是生成网络指令的受信任代码；执行器的 header / URL 检查不等于恶意插件沙箱，也不能检测被插件故意放进普通字段的 secret。

## 内置协议

- Microsoft Graph：202 + 非空 JSON `nextExpectedRanges` 表示未完成，200 / 201 表示完成。取缺失 ranges 的最小起点，不能将 ranges 当作上传分片大小建议。
- Google Drive：308 + `Range: bytes=0-N` 表示连续收到 N+1 字节，200 / 201 表示完成。浏览器看不到 Range 时必须查询；只有服务端能按完整响应判断“没有 Range 表示零字节”。
- 其他 HTTP 成功码不自动代表完成。空 JSON、非法范围、溢出、确认未发送字节和提前完成均进入协调或失败路径。

来源：[Microsoft Graph upload session](https://learn.microsoft.com/en-us/graph/api/driveitem-createuploadsession)、[Google resumable upload](https://developers.google.com/workspace/drive/api/guides/manage-uploads)。

## 前端版本与已有会话

新版本要求使用配套最新前端，不兼容旧前端直传解析方式。REST 响应仅通过 `browser_upload` 提供协调状态，不暴露独立的 `upload_url` / `next_expected_ranges` 字段；实际请求 URL 随单条执行指令下发，provider session URL 继续由后端加密保存。缺失协调状态时拒绝直传，不按品牌猜测。

旧持久化会话没有协议快照时，仅后端可以将历史 `microsoft_graph` / `google_drive` 映射到各自 v1；未知 provider 拒绝恢复。新会话始终保存快照，策略或 driver 升级不能替换它。未知协议版本由 driver 保守拒绝，升级需要保留旧实现或明确要求重启会话。

已有持久化会话与旧前端兼容是不同边界。**v0.8.0 删除 `legacy_protocol` 和 `stored_step` 中的历史会话推断分支，之后只接受持久化协议快照。** 删除前应确保升级前会话已完成或超过最长 24 小时存活期，并覆盖滚动部署期间旧实例最后创建的会话。

## 验证

- Rust driver tests：两家解析规则、部分确认、畸形 / 隐藏 header、溢出、HTTP 状态、provider 权威查询和终态校验。
- Session / repository tests：自定义第三种协议、私有状态往返、刷新恢复、重复回报、权限和 workspace、协议版本、过期及取消竞争、CAS。
- Vitest：有限执行器、第三方协议、恢复、无进展 / 协调重试预算、取消和 finalization 失败。
- `bun run test:e2e:provider-upload`：真实浏览器与本地跨 origin HTTP peer，覆盖 preflight、暴露 / 隐藏响应头、无凭据、取消，以及自定义 PATCH 协议的完整控制回路。

本地 HTTP peer 和浏览器测试不等于真实 OneDrive / Google 账号验收；真实云 API 验收需要独立记录。
