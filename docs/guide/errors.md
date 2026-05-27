# 错误码处理

这一篇覆盖普通用户和管理员在前端、API 和 WebDAV 客户端可能遇到的错误码：什么意思、自己能做什么、什么时候该找管理员。  
按你看到的错误码或界面提示往下翻即可，不必通读。

如果是 5xx 级别的服务端错误（`internal_server_error` / `database_error` / `config_error`），按 [故障排查](/deployment/troubleshooting) 处理。

## 错误码分段

后端错误码按千位分域：

| 段     | 用途                |
| ------ | ------------------- |
| `0`    | 成功                |
| `1xxx` | 通用 / 服务端       |
| `2xxx` | 认证与会话          |
| `3xxx` | 文件 / 上传 / 下载  |
| `4xxx` | 存储策略            |
| `5xxx` | 文件夹              |
| `6xxx` | 分享                |

知道分段后，看到一个新错误码时大致能猜到是哪类问题。

很多错误还会带更细的 `error.code`。这是新的稳定字符串错误码，前端和第三方客户端都应该优先判断它，不要解析英文 `msg`。`msg` 只适合作为兜底说明或排障线索。

在 0.3.0 之前，部分响应还会同时带 `error.subcode`。它只是兼容旧客户端的过渡字段；新脚本、新集成和新前端逻辑都应该使用 `error.code`。如果你在排障或提交 issue，优先记录 `error.code`，再附上数字 `code`。

---

## 通用 (1xxx)

### `bad_request` (1000)

请求参数不合法。

普通用户碰到通常是表单字段没填对（比如名字含非法字符、日期格式不对）。检查表单提示，按要求修正。

如果你确认参数没问题，可能是前后端版本不一致，刷新页面强制更新。

请求来源校验相关的格式问题会带更具体的子错误：

- `validation.request_origin_invalid`：`Origin` 请求头格式无效
- `validation.request_referer_invalid`：`Referer` 请求头格式无效
- `validation.request_host_invalid`：请求 Host 无效，常见于反向代理转发头配置错误
- `validation.request_scheme_invalid`：请求 scheme 无效，常见于 HTTPS 反代没有正确传递 `X-Forwarded-Proto`
- `validation.request_header_value_invalid`：请求来源相关 header 过长或无法处理

### `not_found` (1001) / `endpoint_not_found` (1005)

请求的资源或接口不存在。

- `not_found`：你访问的具体对象（用户、配置项等）不存在
- `endpoint_not_found`：URL 路由本身不存在；通常是你手动改了 URL 或前端版本不匹配

### `internal_server_error` (1002) / `database_error` (1003) / `config_error` (1004)

服务端异常。

普通用户：稍后重试一次；如果反复失败，把错误码和大致时间反馈给管理员。  
管理员：去看 [故障排查](/deployment/troubleshooting)。

### `rate_limited` (1006)

请求过于频繁，被限流挡了。

- 普通用户：等几秒再操作
- 管理员：如果合法用户经常碰到，去 [访问限流](/config/rate-limit) 调 `[rate_limit]` 或前面反向代理的限流规则

### `mail_not_configured` (1007)

邮件系统没配置，无法发激活邮件、密码重置邮件等。

如果你是普通用户，找管理员配 SMTP；如果你是管理员，去 `管理 -> 系统设置 -> 邮件投递` 填 SMTP 信息并发测试邮件验证。

### `mail_delivery_failed` (1008)

邮件投递失败。

先去 `管理 -> 系统设置 -> 邮件投递` 发一封测试邮件，再看服务端日志里 SMTP / mail outbox 相关报错。常见原因：

- SMTP 配置不对（端口、TLS 模式、认证）
- 收件人域名拒收
- 服务端被收件人邮件服务商拉黑

### `conflict` (1009)

资源冲突。最常见的是你要创建或修改的东西已经存在：

- 用户名、邮箱或登录标识重复
- 同一目录下已有同名文件 / 文件夹
- 团队成员已经存在
- WebDAV 用户名已经被占用
- 远程节点绑定碰到唯一性冲突

普通用户按页面提示换一个名字或刷新后重试。
管理员如果是在批量导入、脚本调用或远程节点接入时遇到，优先看响应里的 `error.code`，它会比 `conflict` 本身更具体。旧版本响应里可能还有 `error.subcode`，仅作为兼容参考。

---

## 认证 (2xxx)

### `auth_failed` (2000)

用户名或密码错误。

如果你确认密码对，可能是被锁定（账号被管理员禁用）；这种情况错误码会是 `forbidden`。

### `token_expired` (2001) / `token_invalid` (2002)

登录已过期或无效。

正常情况前端会自动 refresh，无感继续。如果反复出现，原因通常是：

- 浏览器禁用了 Cookie 或第三方 Cookie
- 你或管理员手动撤销了会话（`管理 -> 用户` 里"撤销所有会话"）
- 服务端时钟严重偏差

清浏览器 Cookie 重新登录。

### `forbidden` (2003)

没有权限执行此操作。

- 普通用户操作管理功能：换有权限的账号
- 管理员被禁用：联系其他管理员
- 操作另一个用户的资源：检查分享权限或团队成员权限

同样是 `forbidden`，`error.code` 会说明具体原因。0.3.0 之前，同一个值也可能出现在兼容字段 `error.subcode`：

- `auth.admin_required`：需要管理员权限
- `auth.account_disabled`：账号已被禁用
- `auth.request_source_untrusted` / `auth.request_origin_untrusted` / `auth.request_referer_untrusted`：Cookie 认证请求来源不可信，通常和跨站请求、反向代理站点地址配置或浏览器来源有关
- `auth.request_source_missing`：要求来源校验的请求缺少 `Origin` / `Referer` 等来源信息
- `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid`：CSRF Cookie、`X-CSRF-Token` 请求头缺失或 token 校验失败，刷新页面后重试
- `auth.registration_disabled`：系统关闭了公开注册
- `auth.session_user_mismatch`：当前会话和当前账号不一致，重新登录
- `team.not_member`：当前账号不是该团队成员
- `team.owner_required`：需要团队所有者权限
- `team.admin_or_owner_required`：需要团队管理员或所有者权限
- `workspace.scope_denied`：资源不属于当前工作空间
- `share.scope_denied`：分享范围不允许访问该资源
- `lock.not_owner`：当前用户不是锁定者或资源所有者
- `external_auth.provider_disabled` / `external_auth.policy_denied`：外部认证提供方被禁用，或策略不允许当前操作
- `wopi.app_disabled` / `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted`：WOPI 应用禁用或 WOPI 请求来源不可信

### `pending_activation` (2004)

账号还没激活，需要先完成邮箱验证。

去看注册时收到的激活邮件；如果没收到：

- 检查垃圾邮件
- 用 `重新发送验证邮件` 入口（在登录页或 `设置 -> 安全`）
- 如果系统邮件未配置，找管理员手动激活

### `contact_verification_invalid` (2005) / `contact_verification_expired` (2006)

邮箱验证链接无效或已过期。

链接有时效（默认 24 小时）。重新申请一封即可。如果反复 `invalid`，检查链接是否被邮件客户端截断（特别是企业邮箱常见）。

### Passkey 相关子错误

Passkey 相关问题通常仍归在 `bad_request`、`auth_failed` 或 `token_invalid` 这类主错误码下，看响应里的 `error.code` 更准：

- `passkey.name_invalid`：Passkey 名称含控制字符，换一个普通名称
- `passkey.name_too_long`：Passkey 名称太长，缩短后重试
- `passkey.not_discoverable`：浏览器或安全密钥没有创建可发现凭据，换支持 Passkey 的设备/浏览器，或重新添加

如果登录页提示当前浏览器不支持 Passkey，通常是浏览器、系统或当前访问来源不满足 WebAuthn 要求。正式部署建议使用 HTTPS。

### MFA 相关子错误

MFA 相关问题通常发生在登录二次验证、发送邮箱验证码、启用认证器、禁用 MFA 或重新生成恢复码时：

- `auth.mfa_flow_invalid`：MFA 登录或设置流程无效，返回登录页或重新开始设置
- `auth.mfa_flow_expired`：流程已过期，默认大约 5 分钟，重新开始即可
- `auth.mfa_code_invalid`：验证码或恢复码不正确，检查认证器时间是否同步，或换一个未使用的恢复码
- `auth.mfa_attempts_exceeded`：尝试次数过多，重新开始登录流程
- `auth.mfa_factor_required`：账号需要已启用的 MFA 因子，但当前状态不完整，联系管理员重置 MFA
- `auth.mfa_factor_already_exists`：当前账号已经启用 TOTP，不能重复添加同类因子
- `auth.mfa_recovery_code_used`：恢复码已经用过，登录后重新生成恢复码
- `auth.mfa_email_code_required`：选择了邮箱验证码方式，但还没有先发送验证码，先点发送
- `auth.mfa_email_code_expired`：邮箱验证码已过期，重新发送一封

邮箱验证码 MFA 还依赖邮件投递配置和已验证邮箱。如果登录页没有邮箱验证码入口，通常是管理员没有开启、当前账号邮箱未验证，或者站点不允许 TOTP 用户用邮箱兜底。

如果认证器和恢复码都丢了，且当前没有可用的邮箱验证码方式，普通用户无法自行绕过 MFA。联系管理员到 `管理 -> 用户 -> 用户详情 -> 安全操作` 重置 MFA。

### 外部认证相关问题

外部认证失败通常会在登录页展示“外部登录失败”，后端主错误码可能是 `auth_failed`、`forbidden`、`bad_request` 或 `mail_delivery_failed`。

管理员按这个顺序查：

1. `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 是否正确
2. `管理 -> 外部认证` 里提供商是否启用
3. 重定向 URI 是否已经登记到身份提供商侧
4. Issuer URL、Client ID、Client Secret、scope 和 claim 映射是否正确
5. 如果走邮箱验证，`管理 -> 系统设置 -> 邮件投递` 是否能发外部登录邮箱验证邮件

---

## 文件 / 上传 (3xxx)

### `file_not_found` (3000)

文件不存在或已被删除。

- 你或别人刚删了这个文件
- 文件被移到了回收站 → 左侧 `回收站` 找
- 文件被永久清理了 → 没救了，看 [备份与恢复](../deployment/backup)

### `file_too_large` (3001)

文件超过策略允许的最大大小。

策略层面的上限由管理员在 `管理 -> 存储策略` 里设定。如果你是合法用户，找管理员调整或换策略组。

### `file_type_not_allowed` (3002)

文件类型被策略禁止上传。

同上，由策略管控。常见限制扩展名包括可执行文件等，出于安全考虑。

### `file_upload_failed` (3003)

通用上传失败。

按这个顺序看：

1. 网络是否中断
2. 浏览器控制台是否有更具体的报错
3. 服务端日志里这个时间点是否有 `error_code` 更精确的报错

### `upload_session_not_found` (3004) / `upload_session_expired` (3005)

分块上传会话不存在或已过期。

大多数可恢复上传会话是 24 小时；S3 / 远程节点的单次 Presigned 直传短会话通常是 1 小时。服务重启后旧 session 仍然有效（持久化在数据库），具体以服务端返回的 `expires_at` 为准。如果出现：

- 你的上传超过会话有效期 → 重新发起
- 你 reload 了页面 → 前端会从 localStorage 恢复 session；如果 session 已被服务端清理，会报这个错
- 跨设备恢复上传 → 不支持，必须在同一浏览器同一 localStorage 下继续

重新发起上传即可。

### `chunk_upload_failed` (3006)

分块上传单个 chunk 失败。

最常见原因：

- 磁盘满（管理员检查 `data/.uploads` 所在分区）
- 用户配额已满（看个人配额）
- 默认策略或绑定的策略组被禁用

### `upload_assembly_failed` (3007)

服务端合并 chunk 失败。

通常意味着上传的 chunk 数据不完整或哈希校验失败。重新上传即可；如果反复失败，可能是网络中间有传输错误。

### `thumbnail_failed` (3008)

缩略图生成失败。

不影响文件本身使用。常见原因：

- 文件损坏
- 文件类型不在缩略图支持列表
- 缩略图 worker 出错（管理员看 `管理 -> 任务`）
- 媒体处理器没有启用，或者 `vips` / `ffmpeg` 命令不可用

### `resource_locked` (3009)

资源被 WebDAV LOCK 占住。

- 等锁过期（默认锁有 timeout）
- 在 `管理 -> 锁` 里手动解锁（管理员权限）
- 让占用的客户端正常退出（理想情况下客户端会发 UNLOCK）

### `precondition_failed` (3010)

前置条件不满足，多见于多端同时编辑同一个文件。

刷新页面拿到最新版本，再重新提交。

如果错误消息里带 `managed_ingress.*`，通常是远程节点的接收落点还没准备好。管理员去 `管理 -> 远程节点` 检查这台 follower：

- 有没有默认接收落点
- 默认接收落点是否已经应用
- 本地接收路径有没有逃出 `server.follower.managed_ingress_local_root`
- 这台 follower 是否只绑定了一个 primary

### `upload_assembling` (3011)

文件还在服务端合并 chunk，**不是错误**。

等几秒重试 complete 即可。大文件合并时间会长一些（合并 + 算 SHA256），不要立即多次重试。

### ZIP 压缩包预览相关子错误

ZIP 预览错误通常会挂在 `bad_request` 或 `forbidden` 下，具体看 `error.code`：

- `archive_preview.disabled`：ZIP 预览总开关未开启
- `archive_preview.user_disabled`：登录用户侧 ZIP 预览未开启
- `archive_preview.share_disabled`：分享页 ZIP 预览未开启
- `archive_preview.unsupported_type`：当前文件不是支持的 ZIP
- `archive_preview.source_too_large`：源 ZIP 超过预览大小上限
- `archive_preview.invalid_zip`：ZIP 损坏或格式不合法
- `archive_preview.manifest_too_large`：生成的清单超过 manifest 大小上限
- `archive_preview.source_size_mismatch`：扫描时发现源文件大小和记录不一致，通常要重新上传或检查底层存储
- `archive_preview.rejected`：后台任务拒绝执行，多半是文件已变化、权限变化或运行时限制不再满足

第一次打开 ZIP 时如果只是“生成中”，那不是错误。等 `管理 -> 任务` / `任务中心` 里的 `压缩包预览生成` 完成后再打开。  
如果界面提示当前文件名编码无法解析这个 ZIP，切换 ZIP 预览工具栏里的 `文件名编码` 后重试；这类提示不一定有单独的后端错误码。

---

## 存储策略 (4xxx)

### `storage_policy_not_found` (4000)

策略不存在。

通常是用户绑定的策略被删了，或策略组里某条规则引用的策略被删了。管理员去 `管理 -> 存储策略` / `策略组` 检查。

### `storage_driver_error` (4001)

存储后端报错。

按驱动类型看：

- `local`：检查目录权限、磁盘空间
- `s3`：检查 endpoint、credentials、bucket 是否存在；如果 S3 端慢或宕机，会触发我们配置的 timeout
- `remote`：检查绑定的远程节点是否启用、`base_url` 是否可达、从节点是否已经完成接入并处于健康状态；还要确认 follower 有已应用的默认接收落点
- 其他：看具体报错

如果 remote 策略使用 `presigned`，还要检查远程节点能力摘要是否支持内部协议 `v2` 和 `browser_presigned_cors`。浏览器直连 follower 时，CORS 需要允许 `content-type` / `range`，并暴露 `ETag`、`Accept-Ranges`、`Content-Range`、`Content-Length` 等响应头。

### `storage_quota_exceeded` (4002)

存储空间不足。

- 用户配额满 → 清理回收站、删大文件、找管理员加配额
- 团队空间满 → 同上，团队管理员处理
- 系统总配额满 → 管理员加策略容量

### `unsupported_driver` (4003)

策略配的驱动类型当前版本不支持。

通常是从更高版本配置降级回来时碰到，或手动改 DB 配错了。`管理 -> 存储策略` 重新选支持的驱动。

### `storage_auth_failed` (4004)

存储后端认证失败。

- S3 / MinIO：检查 Access Key、Secret Key、session token、签名版本和 endpoint
- remote：检查远程节点绑定是否仍然有效，主控和 follower 的绑定信息有没有被删
- 本地驱动通常不会报这个，除非上层把存储类型配错了

这类错误不是让用户重试能解决的，管理员要先修凭据。

### `storage_permission_denied` (4005)

凭据有效，但没有权限做当前操作。

常见原因：

- S3 凭据只能读不能写
- bucket policy 不允许访问当前前缀
- 本地目录权限不允许当前进程写入
- remote follower 的接收路径或内部接口权限不对

建议先修复存储后端权限，再重试上传。

### `storage_misconfigured` (4006)

存储策略配置本身不完整或不一致。

重点检查：

- endpoint、bucket、region、base path
- 本地存储根目录是否存在、是否在预期位置
- remote follower 是否完成 enroll、是否有已应用的默认接收落点
- remote follower 内部协议版本和能力摘要是否兼容当前主控；当前要求 `v2`
- 远程接收路径有没有逃出 follower 允许的根目录

这种错误通常是部署或策略配置问题，不是浏览器问题。

### `storage_object_not_found` (4007)

数据库里还引用着对象，但存储后端找不到实际内容。

这比普通 404 更麻烦：说明元数据和真实存储已经不一致。管理员先跑：

```bash
./aster_drive doctor \
  --database-url "sqlite:///var/lib/asterdrive/data/asterdrive.db?mode=rwc" \
  --deep \
  --scope storage-objects
```

如果最近做过手工迁移、恢复备份、清理 bucket 或移动本地目录，从这些操作开始查。

### `storage_rate_limited` (4008)

存储后端限流了 AsterDrive。

常见于对象存储请求太密、S3 兼容服务压力过高、远程 follower 后面还有一层网关限速。等一会儿可能会恢复；如果反复出现，管理员要看存储服务商或 follower 日志。

### `storage_transient_failure` (4009)

存储后端临时失败。

典型原因是网络抖动、对象存储短暂不可用、远程 follower 连接中断。用户可以稍后重试；管理员看同一时间段的网络、S3 / MinIO 或 follower 日志。

### `storage_precondition_failed` (4010)

存储后端前置条件不满足。

常见于对象存储条件写入冲突、远程接收状态变化、同一对象被并发操作。先刷新页面重试；如果一直出现，管理员检查对应存储策略和远程节点状态。

### `storage_operation_unsupported` (4011)

当前存储驱动不支持这个操作。

比如某些后端不能生成预签名 URL、不能走某种流式读写路径，或者 remote follower 版本和主控期望能力不一致。管理员需要换策略配置、升级节点，或者关闭依赖该能力的上传 / 下载模式。

---

## 文件夹 (5xxx)

### `folder_not_found` (5000)

文件夹不存在或已被删除。

跟 `file_not_found` 同理：被删、被移到回收站、被永久清理三种情况。

---

## 分享 (6xxx)

### `share_not_found` (6000)

分享链接不存在或已失效。

按概率排序：

1. token 拼错或被聊天软件截断（特别是从微信、企业 IM 复制时）
2. 分享被创建者删了
3. 分享对应的源文件被删了

### `share_expired` (6001)

分享已过 `expires_at`。

让分享创建者重新生成一份；新链接的 token 会变。

### `share_password_required` (6002)

分享需要密码，但当前请求没有有效的分享密码验证 cookie。

它通常表示还没验证、cookie 丢了或验证已过期。真正提交密码时如果密码不对，服务端会按认证失败处理，常见错误码是 `auth_failed`。

注意：服务端有 1 小时密码验证缓存，**改完密码后对方在 1 小时内可能仍然用旧密码访问**。这是有意设计，不是 bug。

### `share_download_limit_reached` (6003)

分享的下载次数已达上限。

- 创建者可以在左侧 `我的分享` 里调高下载次数限制
- 或者直接重新创建一份分享

---

## 不在以上分段的提示

### 前端 `unexpected_error`

前端兜底文案，不是后端错误码。表示前端拿到了无法识别的响应。

通常是：

- 后端返回了前端没映射的新错误码（升级版本不一致）
- 网络层就出错了，前端连 `error_code` 都没拿到

刷新页面强制更新一次前端；如果还有，看浏览器控制台具体报错。

### WebDAV 客户端报奇怪的 HTTP 码

WebDAV 客户端通常不会显示 `error_code`，只会显示 HTTP 状态码：

- `401`：鉴权失败；用 WebDAV 专用账号，不是普通登录账号
- `403`：账号有效但没权限访问该路径；也可能是管理员开启了 WebDAV 系统文件拦截，客户端正在创建 `.DS_Store`、`Thumbs.db`、`desktop.ini` 这类系统元数据文件
- `404`：路径不存在
- `423`：资源被锁；对应 `resource_locked`
- `412`：前置条件失败；对应 `precondition_failed`
- `503`：WebDAV 总开关被关；管理员去 `管理 -> 系统设置 -> WebDAV` 打开

---

## 还是没解决

按这个顺序提交问题：

1. 把错误码（数字 + 字符串名）记下来
2. 把出现错误时的操作步骤记下来
3. 跑一次 `aster_drive doctor`（管理员）
4. 在 [GitHub Issues](https://github.com/AptS-1547/AsterDrive/issues) 开 issue

`error.code` 和数字 `code` 是定位问题最快的线索。报 issue 时优先贴错误码，比贴英文报错有用得多。
