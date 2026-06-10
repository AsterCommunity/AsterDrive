# 错误码处理

这一页帮你看懂 AsterDrive 返回的错误：先看哪个字段、用户能不能自己处理、管理员应该去哪里查。

如果你只是普通用户，不需要记住所有错误码。把界面上看到的错误、操作时间和你刚才做了什么发给管理员，通常就够了。
如果你在写脚本、接 API、排查 WebDAV / WOPI / 远程节点问题，请优先看响应里的顶层 `code`。

::: tip 0.3.0 错误码契约
issue 211 已把公开错误契约收敛成一套：顶层 `code: ApiErrorCode`。

旧的数字错误码、`error.code`、`error.subcode` 和 `error.internal_code` 不再作为公开 API 响应字段暴露。客户端文案和业务判断都应该直接使用顶层 `code`。
:::

## 先看哪个字段

AsterDrive 的失败响应一般长这样：

```json
{
  "code": "auth.credentials_failed",
  "msg": "Invalid Credentials",
  "error": {
    "retryable": false
  }
}
```

字段含义如下：

| 字段 | 应该怎么用 |
| --- | --- |
| `code` | **稳定字符串错误码**。前端、SDK、脚本和第三方客户端都应该用它做业务判断。成功时是 `success`。 |
| `error.retryable` | 是否建议自动重试。`true` 不代表一定成功，只表示这个错误更像临时失败。 |
| `msg` | 给人看的诊断说明。不要用它做代码分支，也不要当翻译 key。 |

报 issue 或向管理员反馈时，优先贴：

1. `code`
2. `msg`
3. 出错时间和你正在做的操作

如果只贴一句英文报错，定位会慢很多。

## 字符串码看领域

`code` 使用稳定的 snake/dot 风格。点号前通常是领域，点号后是具体原因：

| 领域 | 常见错误码 |
| --- | --- |
| 通用 | `bad_request`、`not_found`、`internal_server_error`、`conflict` |
| 登录、会话、权限、MFA | `auth.credentials_failed`、`auth.token_expired`、`forbidden`、`auth.mfa_failed` |
| 文件、上传、缩略图、锁 | `file.not_found`、`upload.session_expired`、`thumbnail.failed`、`resource.locked` |
| 存储策略、驱动、配额、远程存储 | `storage.quota_exceeded`、`storage.auth_failed`、`storage.transient_failure` |
| 文件夹 | `folder.not_found` |
| 分享 | `share.expired`、`share.password_required` |

看到一个新错误时，先用 `code` 的领域判断方向，再按完整字符串决定具体处理。

## 常见处理入口

### 登录、会话和账号

如果登录失败，先看这些码：

- `auth.credentials_failed` / `auth.failed`：用户名、邮箱、密码或登录凭据不对。重新输入；如果确认无误，让管理员检查账号状态。
- `auth.pending_activation`：账号还没激活。去邮箱完成验证，或让管理员手动激活。
- `auth.contact_verification_invalid` / `auth.contact_verification_expired`：邮箱验证链接无效或过期。重新发送验证邮件。
- `auth.token_missing` / `auth.token_invalid` / `auth.token_expired`：登录态缺失、无效或过期。刷新页面；如果反复出现，清 Cookie 后重新登录。
- `auth.refresh_token_stale`：刷新 token 太旧，通常是多端登录、旧页面或旧会话导致。重新登录。
- `auth.refresh_token_reuse_detected`：检测到 refresh token 重放。系统会拒绝这条会话，建议撤销其他会话并重新登录。
- `auth.password_change_required`：管理员要求当前账号先修改密码。完成登录后会进入强制改密页面；在改密成功之前，普通 API 会被拒绝。
- `auth.account_disabled`：账号被禁用。普通用户只能联系管理员。
- `auth.registration_disabled`：站点关闭公开注册。让管理员创建账号，或开启注册。

如果普通登录、Passkey、外部认证和 MFA 混在一起失败，仍然优先看 `code`，不要只看页面上的一句“登录失败”。

### MFA 和 Passkey

MFA 相关错误一般发生在二次验证、启用认证器、邮箱验证码或恢复码流程里：

- `auth.mfa_failed`：MFA 总体失败，看同一响应里是否有更具体的码。
- `auth.mfa_flow_invalid` / `auth.mfa_flow_expired`：验证流程无效或过期，回登录页重新开始。
- `auth.mfa_code_invalid`：验证码或恢复码不正确。检查认证器时间，或换一个未使用的恢复码。
- `auth.mfa_attempts_exceeded`：尝试次数太多。重新开始登录流程。
- `auth.mfa_factor_required`：账号要求 MFA，但当前因子状态不完整。联系管理员重置 MFA。
- `auth.mfa_factor_already_exists`：已经有同类 MFA 因子，不能重复添加。
- `auth.mfa_recovery_code_used`：恢复码已用过。登录后重新生成恢复码。
- `auth.mfa_email_code_required` / `auth.mfa_email_code_expired`：需要先发送邮箱验证码，或验证码已过期。

Passkey 相关错误：

- `passkey.name_invalid` / `passkey.name_too_long`：Passkey 名称不合法或太长，改名后重试。
- `passkey.not_discoverable`：浏览器或安全密钥没有创建可发现凭据。换支持 Passkey 的设备 / 浏览器，或重新添加。

生产部署建议使用 HTTPS。很多 WebAuthn / Passkey 行为在不安全来源下不会按预期工作。

### 权限、团队和工作空间

`code = "forbidden"` 只说明“不能做”。如果同一类操作需要区分具体原因，响应会返回更细的字符串码：

- `auth.admin_required`：需要管理员权限。
- `team.not_member`：当前账号不是团队成员。
- `team.owner_required`：需要团队所有者权限。
- `team.admin_or_owner_required`：需要团队管理员或所有者权限。
- `workspace.scope_denied`：资源不属于当前工作空间。
- `share.scope_denied`：分享范围不允许访问这个资源。
- `lock.not_owner`：当前用户不是锁定者或资源所有者。
- `external_auth.provider_disabled` / `external_auth.policy_denied`：外部认证提供方被禁用，或策略不允许当前操作。

普通用户：确认自己是否在正确团队、正确工作空间里。
管理员：检查团队成员、角色、分享范围和外部认证策略。

### CSRF、来源校验和反向代理

如果 Cookie 登录后调用管理接口、WOPI 或 WebDAV 相关接口时失败，常见码是：

- `auth.request_source_missing`：请求缺少 `Origin` / `Referer` 等来源信息。
- `auth.request_source_untrusted`：请求来源不可信。
- `auth.request_origin_untrusted` / `auth.request_referer_untrusted`：`Origin` 或 `Referer` 不在允许范围内。
- `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid`：CSRF Cookie、请求头或 token 不正确。
- `validation.request_origin_invalid` / `validation.request_referer_invalid`：来源 header 格式无效。
- `validation.request_host_invalid` / `validation.request_scheme_invalid`：反向代理传来的 Host 或 scheme 不对。
- `validation.request_header_value_invalid`：来源相关 header 太长或无法解析。

处理顺序：

1. 刷新页面，重新登录一次。
2. 确认后台系统设置里的公开站点地址和当前访问地址一致。
3. 检查反向代理是否正确传递 `Host`、`X-Forwarded-Host`、`X-Forwarded-Proto`。
4. 如果是跨域直连 follower、WOPI 或自写脚本，确认来源和 CORS 配置匹配。

### 文件、文件夹和冲突

文件 / 文件夹相关：

- `file.not_found` / `folder.not_found`：文件或文件夹不存在、被删除、被移入回收站，或已永久清理。
- `file.name_conflict` / `folder.name_conflict`：同一目录下已有同名项目。换名，或刷新目录后重试。
- `file.etag_mismatch` / `file.modified_during_write` / `precondition_failed`：文件在你提交前被别的客户端改过。刷新后重新编辑。
- `resource.locked`：资源被 WebDAV LOCK 占用。等锁过期，或管理员在锁管理里解除。
- `file.too_large`：超过策略允许大小。联系管理员调整策略或换策略组。
- `file.type_not_allowed`：策略不允许这种文件类型。

如果数据库里有记录但存储后端找不到实际对象，会出现 `storage.object_not_found` 或 `storage.not_found`。这不是普通用户能修的 404，管理员应检查底层存储和备份恢复记录。

### 上传和大文件

上传失败时先看是哪一段：

- `upload.session_not_found` / `upload.session_expired`：上传会话不存在或过期。重新发起上传。
- `upload.assembling`：文件还在服务端合并，不是失败。等几秒再查状态。
- `upload.chunk_failed`：某个分片上传失败，通常和网络、磁盘空间或临时目录有关。
- `upload.assembly_failed`：服务端合并失败。管理员看任务和服务端日志。
- `upload.status_conflict` / `upload.previous_failure`：上传会话状态已经变化，通常不要继续复用旧会话。
- `upload.incomplete_chunks` / `upload.incomplete_parts` / `upload.missing_part`：还有分片或 S3 part 没传完整。
- `upload.chunk_number_out_of_range` / `upload.part_number_out_of_range` / `upload.part_numbers_too_many`：客户端提交的分片编号不符合规则。
- `upload.chunk_size_mismatch` / `upload.request_size_mismatch` / `upload.final_object_size_mismatch`：声明大小和实际大小不一致。
- `upload.temp_dir_create_failed`、`upload.temp_file_write_failed`、`upload.local_staging_write_failed`、`upload.assembly_io_failed`：服务器临时目录、暂存区或磁盘写入失败。

用户可以先重试一次。反复失败时，管理员重点查：

- `data/.uploads`、`data/.tmp` 或自定义临时目录所在分区是否满了
- 当前用户 / 团队 / 策略配额是否满了
- 对象存储 multipart 或 remote presigned 上传的浏览器直连地址是否可访问
- 远程 follower 是否健康，默认接收落点是否已经应用

### 存储策略、S3 和远程节点

存储类错误通常不是让普通用户多点几次就能解决的。管理员按 `code` 处理：

- `storage.policy_not_found`：用户、团队或策略组引用了不存在的策略。
- `storage.quota_exceeded`：用户、团队或系统配额已满。
- `storage.unsupported_driver` / `storage.operation_unsupported` / `storage.unsupported`：当前驱动或远程节点不支持这个操作。
- `storage.auth_failed` / `storage.auth`：S3 / remote 凭据或绑定认证失败。
- `storage.permission_denied` / `storage.permission`：凭据有效，但没有读写当前对象或前缀的权限。
- `storage.misconfigured`：策略配置不完整、不一致，或 remote follower 没准备好。
- `storage.rate_limited`：对象存储、网关或 follower 限流。
- `storage.transient_failure` / `storage.transient`：网络、对象存储或 follower 临时失败，可以稍后重试。
- `storage.precondition_failed` / `storage.precondition`：条件写入、远程接收状态或并发操作冲突。
- `storage.driver_error` / `storage.unknown`：驱动返回了无法归类的错误，查服务端日志。

远程节点相关：

- `remote_node.disabled`：远程节点被禁用。
- `remote_node.enrollment_required`：follower 还没有完成接入。
- `remote_node.unique_conflict`：远程节点绑定或唯一字段冲突。
- `managed_ingress.required`、`managed_ingress.default_missing`、`managed_ingress.default_not_applied`：follower 缺少可用的默认接收落点。
- `managed_ingress.local_path_invalid`：follower 本地接收路径不合法，常见于路径逃出允许根目录。
- `managed_ingress.driver_unsupported`：当前接收落点驱动不支持。
- `managed_ingress.single_primary_required`：这台 follower 需要只绑定一个 primary。
- `master_binding.disabled`：主从绑定被禁用。

如果 remote 策略使用浏览器直传，还要确认浏览器能访问 follower 的 `base_url`，并且 follower CORS 允许上传请求需要的 header。

### 分享

分享错误：

- `share.not_found`：分享不存在、token 错了、分享被删除，或源文件已删除。
- `share.expired`：分享已过期。让创建者重新生成。
- `share.password_required`：需要先输入分享密码，或密码验证 cookie 已丢失 / 过期。
- `share.download_limit_reached`：下载次数达到上限。
- `share.scope_denied`：分享范围不允许访问目标文件或文件夹。

注意：分享密码验证有缓存。改完分享密码后，已验证的访问者可能在缓存过期前仍能访问一段时间。

### 缩略图、头像和压缩包预览

这些错误通常不代表原文件丢了，而是处理链路失败。

缩略图：

- `thumbnail.failed`：缩略图通用失败。
- `thumbnail.source_too_large`：源文件超过缩略图处理上限。
- `thumbnail.processor_unavailable`：媒体处理器不可用，检查 `vips` / `ffmpeg` 或对应配置。
- `thumbnail.format_guess_failed` / `thumbnail.decode_failed` / `thumbnail.encode_failed`：格式识别、解码或编码失败。
- `thumbnail.source_open_failed` / `thumbnail.source_stream_failed`：读取源文件失败，可能牵涉底层存储。
- `thumbnail.task_panicked`：缩略图任务 panic，管理员看任务和日志。

头像：

- `avatar.file_required`：没有提交头像文件。
- `avatar.upload_read_failed`：读取上传头像失败。
- `avatar.processor_unavailable`：头像处理器不可用。
- `avatar.empty_image`：图片为空或无法得到有效尺寸。
- `avatar.render_failed` / `avatar.output_invalid`：渲染或输出失败。

压缩包预览：

- `archive_preview.disabled`：压缩包预览总开关关闭。
- `archive_preview.user_disabled` / `archive_preview.share_disabled`：用户空间或分享页未启用。
- `archive_preview.unsupported_type`：不是支持的压缩包类型。
- `archive_preview.source_too_large`：源压缩包太大。
- `archive_preview.invalid_archive`：压缩包损坏或格式不合法。
- `archive_preview.manifest_too_large`：生成的文件清单超过上限。
- `archive_preview.source_size_mismatch`：扫描时发现源文件大小和记录不一致。
- `archive_preview.rejected`：后台任务拒绝执行，通常是权限、文件状态或运行条件变化。

第一次打开压缩包只显示“生成中”时，等任务中心里的压缩包预览任务完成后再打开。

### 后台任务

后台任务本身也可能返回稳定错误码：

- `task.lease_lost`：任务租约丢失，通常说明别的 worker 接管了或任务已经被回收。
- `task.lease_renewal_timed_out`：任务续租超时，可能是数据库、worker 或系统负载问题。
- `task.worker_shutdown_requested`：worker 收到关闭请求，常见于服务重启或任务调度停止。

管理员应去 `管理 -> 任务` 看任务状态、失败原因、重试次数和同一时间段日志。

### 邮件、外部认证和离线下载

邮件：

- `mail.not_configured`：还没配置 SMTP。管理员去 `管理 -> 系统设置 -> 邮件投递`。
- `mail.delivery_failed`：SMTP 投递失败。先发测试邮件，再查 SMTP 返回和 mail outbox 日志。

外部认证：

- `external_auth.provider_disabled`：提供方被禁用。
- `external_auth.policy_denied`：策略不允许当前外部登录、绑定或创建账号。

离线下载：

- `offline_download.aria2_rpc_auth_failed`：aria2 RPC secret 不对。
- `offline_download.aria2_rpc_probe_failed`：aria2 探测失败，检查 RPC URL、网络、超时和 aria2 运行状态。

### WOPI 和 WebDAV

WOPI：

- `wopi.public_site_url_required`：没有配置公开站点地址。
- `wopi.app_disabled`：目标 WOPI 应用被禁用。
- `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted`：WOPI 请求来源不可信。
- `wopi.max_expected_size_exceeded`：文件超过 WOPI 应用声明或 AsterDrive 允许的最大编辑大小。

WebDAV：

- `webdav.username_exists`：WebDAV 用户名已存在。
- `resource.locked`：WebDAV LOCK 占用，对应 HTTP `423`。
- `precondition_failed`：条件请求失败，对应 HTTP `412`。

很多 WebDAV 客户端只显示 HTTP 状态码，不显示 JSON 错误：

| HTTP 状态 | 常见含义 |
| --- | --- |
| `401` | 鉴权失败。使用 WebDAV 专用账号，不是普通登录密码。 |
| `403` | 账号有效但无权限；也可能是系统文件拦截挡住了 `.DS_Store`、`Thumbs.db`、`desktop.ini`。 |
| `404` | 路径不存在。 |
| `412` | 前置条件失败，通常对应 `precondition_failed`。 |
| `423` | 资源被锁，对应 `resource.locked`。 |
| `503` | WebDAV 总开关关闭。 |

## 稳定字符串错误码速查

下面按处理方式整理当前公开的 `ApiErrorCode`。表里的 `*` 只是为了把同类错误合并展示；客户端仍然应该匹配完整字符串，不要用前缀猜行为。

### 通用和运行时

| 错误码 | 含义 |
| --- | --- |
| `success` | 成功响应，不是错误。 |
| `bad_request` | 请求参数或请求体不合法。 |
| `not_found` | 资源不存在。 |
| `endpoint.not_found` | API 路由不存在。 |
| `internal_server_error` | 服务端内部错误。 |
| `database.error` | 数据库连接或操作失败。 |
| `config.error` | 静态配置或运行时配置错误。 |
| `rate_limited` | 请求被限流。 |
| `conflict` | 资源冲突；通常还有更具体的冲突码。 |
| `mail.not_configured` / `mail.delivery_failed` | 邮件未配置或投递失败。 |

### 认证、安全和账号

| 错误码 | 含义 |
| --- | --- |
| `auth.failed` / `auth.credentials_failed` | 鉴权失败或凭据错误。 |
| `auth.token_missing` / `auth.token_invalid` / `auth.token_expired` | token 缺失、无效或过期。 |
| `auth.refresh_token_stale` / `auth.refresh_token_reuse_detected` | refresh token 过旧或疑似重放。 |
| `auth.password_change_required` | 当前账号必须先修改密码。 |
| `auth.pending_activation` | 账号待激活。 |
| `auth.contact_verification_invalid` / `auth.contact_verification_expired` | 联系方式验证无效或过期。 |
| `forbidden` | 无权限，通常会有更具体的权限码。 |
| `auth.admin_required` / `auth.account_disabled` | 需要管理员，或账号被禁用。 |
| `auth.username_exists` / `auth.email_exists` / `auth.identifier_exists` | 登录标识冲突。 |
| `auth.registration_disabled` | 公开注册关闭。 |
| `auth.session_user_mismatch` | 会话和账号不一致。 |
| `auth.request_source_missing` / `auth.request_source_untrusted` | 请求来源缺失或不可信。 |
| `auth.request_origin_untrusted` / `auth.request_referer_untrusted` | Origin 或 Referer 不可信。 |
| `auth.csrf_cookie_missing` / `auth.csrf_header_missing` / `auth.csrf_token_invalid` | CSRF 校验失败。 |
| `auth.mfa_failed`、`auth.mfa_*` | MFA 流程、验证码、恢复码或因子状态错误。 |
| `passkey.name_invalid` / `passkey.name_too_long` / `passkey.not_discoverable` | Passkey 名称或凭据能力问题。 |
| `external_auth.provider_disabled` / `external_auth.policy_denied` | 外部认证提供方或策略拒绝。 |

### 文件、上传、文件夹和锁

| 错误码 | 含义 |
| --- | --- |
| `file.not_found` / `folder.not_found` | 文件或文件夹不存在。 |
| `file.too_large` / `file.type_not_allowed` | 文件超过大小限制或类型被禁止。 |
| `file.upload_failed` | 文件上传通用失败。 |
| `file.name_conflict` / `folder.name_conflict` | 文件或文件夹同名冲突。 |
| `file.etag_mismatch` / `file.modified_during_write` | 文件版本变化，提交前置条件失败。 |
| `resource.locked` / `lock.not_owner` | 资源被锁，或当前用户不是锁所有者。 |
| `precondition_failed` | 条件请求失败。 |
| `upload.session_not_found` / `upload.session_expired` | 上传会话不存在或过期。 |
| `upload.chunk_failed` / `upload.assembly_failed` | 分片上传或合并失败。 |
| `upload.assembling` | 文件仍在合并中。 |
| `upload.temp_*` / `upload.local_staging_*` / `upload.assembly_io_failed` | 服务器临时目录、暂存或合并 I/O 失败。 |
| `upload.request_*` / `upload.body_size_overflow` / `upload.declared_size_invalid` | 请求体读取、大小或声明值不合法。 |
| `upload.chunk_*` / `upload.part_*` | 分片编号、大小、数量或传输模式不合法。 |
| `upload.incomplete_*` / `upload.missing_part` | 分片或 part 没传完整。 |
| `upload.temp_object_*` / `upload.final_object_size_mismatch` | 临时对象或最终对象大小不一致。 |
| `upload.status_conflict` / `upload.previous_failure` / `upload.session_corrupted` | 上传会话状态冲突、已有失败或会话损坏。 |

### 存储、远程节点和接收落点

| 错误码 | 含义 |
| --- | --- |
| `storage.policy_not_found` | 存储策略不存在。 |
| `storage.driver_error` / `storage.unknown` | 存储驱动返回未知错误。 |
| `storage.quota_exceeded` | 配额不足。 |
| `storage.unsupported_driver` / `storage.unsupported` | 驱动或能力不支持。 |
| `storage.auth_failed` / `storage.auth` | 存储认证失败。 |
| `storage.permission_denied` / `storage.permission` | 存储权限不足。 |
| `storage.misconfigured` | 存储配置错误。 |
| `storage.object_not_found` / `storage.not_found` | 存储对象不存在。 |
| `storage.rate_limited` | 存储后端限流。 |
| `storage.transient_failure` / `storage.transient` | 临时存储失败。 |
| `storage.precondition_failed` / `storage.precondition` | 存储前置条件失败。 |
| `storage.operation_unsupported` | 当前存储操作不支持。 |
| `remote_node.disabled` / `remote_node.enrollment_required` / `remote_node.unique_conflict` | 远程节点禁用、未接入或唯一性冲突。 |
| `managed_ingress.*` | follower 接收落点缺失、未应用、路径不合法、驱动不支持或绑定状态不一致。 |
| `master_binding.disabled` | 主从绑定被禁用。 |

### 分享、团队和工作空间

| 错误码 | 含义 |
| --- | --- |
| `share.not_found` / `share.expired` | 分享不存在或已过期。 |
| `share.password_required` | 分享需要密码验证。 |
| `share.download_limit_reached` | 分享下载次数到达上限。 |
| `share.scope_denied` | 分享范围不允许访问目标资源。 |
| `team.not_member` | 当前账号不是团队成员。 |
| `team.owner_required` / `team.admin_or_owner_required` | 需要团队所有者或管理员权限。 |
| `team.member_exists` | 团队成员已存在。 |
| `workspace.scope_denied` | 当前工作空间不能访问目标资源。 |
| `policy.*` | 存储策略前置条件、配置、远程节点绑定或驱动转换校验失败。 |

### 预览、媒体处理、WOPI、WebDAV 和其他

| 错误码 | 含义 |
| --- | --- |
| `thumbnail.failed` / `thumbnail.*` | 缩略图生成、读取、解码、编码、临时文件或处理器失败。 |
| `avatar.*` | 头像上传、读取、处理、渲染或输出失败。 |
| `archive_preview.*` | 压缩包预览开关、格式、大小、清单或任务拒绝问题。 |
| `wopi.public_site_url_required` | WOPI 需要公开站点地址。 |
| `wopi.app_disabled` | WOPI 应用被禁用。 |
| `wopi.request_origin_untrusted` / `wopi.request_referer_untrusted` | WOPI 请求来源不可信。 |
| `wopi.max_expected_size_exceeded` | 文件超过 WOPI 允许大小。 |
| `webdav.username_exists` | WebDAV 用户名已存在。 |
| `offline_download.aria2_rpc_auth_failed` / `offline_download.aria2_rpc_probe_failed` | aria2 RPC 认证或探测失败。 |
| `task.lease_lost` / `task.lease_renewal_timed_out` / `task.worker_shutdown_requested` | 后台任务租约、续租或 worker 关闭。 |
| `validation.*` | 请求来源、Host、scheme、header 或初始化状态校验失败。 |

## 还是没解决

普通用户按这个格式反馈给管理员：

```text
我在 2026-06-03 21:10 上传 test.zip 时失败。
code: upload.assembly_failed
msg: ...
```

管理员继续排查：

1. 在同一时间段查服务端日志。
2. 如果牵涉上传、存储、缩略图、压缩包预览或离线下载，去 `管理 -> 任务` 看任务失败原因。
3. 如果怀疑存储元数据和底层对象不一致，运行 [运维 CLI](/deployment/ops-cli) 里的 `doctor`。
4. 如果确认是 AsterDrive 问题，在 [GitHub Issues](https://github.com/AptS-1547/AsterDrive/issues) 提交复现步骤和错误响应。

错误码是定位问题最快的入口。优先贴 `code`，别只贴一句界面文案。
