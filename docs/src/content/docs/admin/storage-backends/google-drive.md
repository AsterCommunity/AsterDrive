---
title: Google Drive 存储策略教程
description: 从 Google Cloud 应用注册到管理员授权、普通目录或共享盘、浏览器直传、容量观测、下载行为和故障排查的完整教程。
---

:::tip[这一篇覆盖什么]
由管理员授权一个 Google 账号，把专用目录、共享盘或 appDataFolder 作为 AsterDrive 的 blob 存储池。普通用户继续使用 AsterDrive 的文件、权限、版本、配额和分享，无需绑定自己的 Google 账号。本功能不提供用户 Google Drive 目录浏览、导入或同步。
:::

## 先分清配置层次

完整使用路径是：Google Cloud OAuth 应用 → AsterDrive 存储策略 → 管理员授权与连接测试 → 策略组规则 → 用户或团队绑定。

| 你要做什么 | 后台入口 |
| --- | --- |
| 创建或编辑 Google Drive 策略 | `管理 → 存储策略` |
| 复制回调地址、授权、重新授权、验证凭据 | 目标策略的 Google Drive 授权面板 |
| 配置文件分流和目标优先级 | `管理 → 策略组` |
| 分配策略组 | 用户详情或团队详情 |
| 检查回收或迁移失败 | `管理 → 后台任务` |

Google Drive 需要先保存应用再打开浏览器授权，不能作为系统初始化时的首个存储策略。先完成 Local 或其他可立即验证的存储设置，再添加 Google Drive。

## 1. 准备 Google Cloud 项目

1. 在 Google Cloud Console 创建或选择专用项目。
2. 在 API 库中启用 **Google Drive API**。
3. 配置 Google Auth Platform / OAuth 同意屏幕，填写应用信息并选择适合账号的用户类型。
4. 在凭据页面创建 **Web application** 类型的 OAuth 客户端，保存客户端 ID 和客户端密钥。
5. 外部应用处于 **Testing** 状态时，将实际完成授权的管理员 Google 账号加入测试用户。

这里使用的是管理员在浏览器里同意的 delegated OAuth。不要把 service account JSON、普通 API key 或其他应用的 secret 填进客户端密钥。

### 测试应用的 token 有效期

Google 对外部应用 Testing 状态签发的 Drive refresh token 通常设置七天有效期。长期运行前应按 Google 的要求完成应用发布以及所需的验证；组织管理员还可能限制第三方应用和 Drive scope。

Testing 与正式发布应用使用相同的 token 刷新接口和 `grant_type=refresh_token`。区别是 Google 对 refresh token 的生命周期限制：外部 Testing 应用的 Drive 授权有七天期限，持续自动刷新 access token 也不能绕过；正式发布应用不受这条七天规则约束，但也不保证永久有效。

长期运行的 Testing 应用可能表现为“前几天正常，一周后上传、下载、缩略图或后台回收开始要求重新授权”。AsterDrive 在下一次需要刷新 token 时收到 `invalid_grant`，将该策略的凭据保存为“需要重新授权”；不删除文件，也不影响用户本地登录或其他存储策略。当前没有在第七天定时提醒，界面的 access token 到期时间也不是 refresh token 的七天期限。

正式应用的 refresh token 仍可能因用户撤销访问、六个月未使用、签发数量上限导致旧 token 失效、用户选择的限时授权或组织限制而失效。切换到正式发布后，建议重新授权取得新凭据，不要假设原先 Testing 签发的 token 会自动延长。

客户端 secret、管理员账号授权和 Google 应用发布状态是不同事项。更换 secret 不能修复被撤销的用户授权，重新授权也不能修复错误的客户端 secret。

### 授权范围

| 存储空间 | AsterDrive 请求的 scope |
| --- | --- |
| 普通目录、账号根目录、共享盘 | `https://www.googleapis.com/auth/drive` |
| appDataFolder | `https://www.googleapis.com/auth/drive.appdata` |

普通存储池使用完整 Drive scope，授权范围大于单个目录。AsterDrive 的读写和删除再限定到选定存储池与应用管理标记。请使用专用应用和专用目录，并确认 Google 或组织的授权要求。

## 2. 创建 Google Drive 策略并登记回调

进入 `管理 → 存储策略 → 新建策略`，选择 **Google Drive**。

| 字段 | 如何填写 |
| --- | --- |
| OAuth 客户端 ID | Google Web application 客户端的 ID |
| OAuth 客户端密钥 | 同一个客户端的 secret；首次创建必须提供 |
| 基础路径 | 根目录内的相对子目录，例如 `AsterDrive/blobs`；留空直接使用根目录 |
| 根目录 ID | 专用目录 URL 中的 ID；留空使用账号根目录或所选共享盘根目录 |
| 共享盘 ID | 使用 Google Workspace shared drive 时填写 |
| 使用 appDataFolder | 使用应用私有的隐藏数据空间，不能同时填写上面两个 ID |
| 上传传输策略 | 服务端中继或浏览器直传，默认服务端中继 |
| 容量探测超时（秒） | 默认 10 秒，允许 2–30 秒；只控制容量观测预算 |

保存策略后，从授权面板复制完整 OAuth 回调地址，在 Google OAuth 客户端的 **Authorized redirect URIs** 中添加它。例如：

```text
https://drive.example.com/api/v1/admin/policies/storage-authorization/callback
```

:::caution[回调地址必须完全匹配]
协议、域名、端口和路径都要一致。生产环境使用正确的 HTTPS 公开站点地址，代理部署也要让后台显示正确的外部地址。应登记界面实际显示的地址，而不是只照抄示例域名。
:::

编辑策略时，客户端密钥留空会保留**同一客户端 ID** 的已保存密钥。更换客户端 ID 时，需要同时提供新 secret 并重新授权。

## 3. 选择存储池

### 根目录与基础路径的关系

“根目录 ID”选择 Google 真实目录，“基础路径”选择它下面的相对子目录。例如根目录 ID 指向 `TeamStorage`，基础路径为 `AsterDrive/blobs`，对象就写入 `TeamStorage/AsterDrive/blobs`。留空保持旧策略的落点。

基础路径支持多级目录和中文名称。写入时逐层创建缺失目录；查询、下载和删除不创建目录。相同名字对应多个 Google 对象、路径组件是文件或快捷方式时会报错，不随意挑选。不要使用 `..`、反斜杠或完整 URL。

基础路径也可用于共享盘和 appDataFolder；内部 `files/...` 逻辑对象键不会继续展开为目录树。账号配额观测仍是整个账号或组织池，不是基础路径目录的独享容量。

### 普通目录

例如目录 URL 是 `https://drive.google.com/drive/folders/FOLDER_ID`，根目录 ID 填写 `FOLDER_ID`，不要填写完整 URL。

建议专门创建空目录。留空虽可使用账号根目录，但专用目录更便于区分 AsterDrive 对象和管理员的其他文件。

### Google Workspace 共享盘

共享盘 ID 与普通目录 ID 的作用不同：共享盘决定查询范围，根目录 ID 可以进一步把对象限定到该盘中的某个文件夹。根目录 ID 留空时使用共享盘根目录。

管理员账号需要访问目标盘、添加文件以及删除文件的权限。仅能浏览目录不足以支持存储后端。凭据验证会检查目标目录及其所属共享盘；连接测试还会实际写入并删除测试对象。

### appDataFolder

这个空间按应用隔离，使用 `drive.appdata` scope。它不在普通 Google Drive 文件界面中显示，也不能按普通 Drive 文件分享、跨空间移动或移入回收站。

启用时不要填写根目录 ID 和共享盘 ID。**每次切换 appDataFolder 开关后，都必须保存并重新授权**：普通空间需要 `drive` scope，隐藏空间需要 `drive.appdata`，只保存开关不会给旧 token 增加权限。没有重新授权时可能返回 Google HTTP 403；重新完成同意后再做凭据验证和上传测试。这个 scope 问题与 Testing 七天到期是不同原因。

删除 AsterDrive 文件仍由 AsterDrive 的回收流程管理，不需要通过 Google 网页回收站操作。

:::caution[已有文件后不要直接切换存储池或账号]
对象依赖原来的应用、Google 账号和存储位置。策略还有 blob 或未完成上传（包括等待清理的失败或过期会话）时，后端会拒绝修改根目录、共享盘、appDataFolder 和基础路径。搬迁数据应新建策略并使用 AsterDrive 存储迁移，不要通过重新定位旧策略来“搬家”。上传传输策略和容量超时仍可修改。

旧版本允许切换存储池时，切换后再永久删除可能只查到新空间中的“对象不存在”，旧空间对象因此遗留。单纯等待回收任务不会把它移动到正确空间；先核对原存储位置和对象身份，避免手动误删其他引用。
:::

## 4. 完成管理员授权和验证

1. 保存策略和 OAuth 应用配置。
2. 点击授权，用准备好的管理员 Google 账号完成同意。
3. 浏览器返回 AsterDrive 后检查授权状态。
4. 运行凭据验证，确认存储池可以访问。
5. 运行已保存策略的连接测试，确认临时对象能够写入并删除。

授权采用一次性 state 和 PKCE。客户端 secret、access token 和 refresh token 由后端加密保存，普通用户不会收到管理员 OAuth token。

授权使用已保存的应用配置。如果授权途中又修改应用、凭据或存储配置，旧回调会被拒绝，请重新发起授权。这能避免旧页面覆盖刚保存的新配置。

普通 access token 过期时后端会自动刷新。只有授权确实失效或被撤销时才需要管理员重新授权；限流、代理拦截和暂时网络故障不会自动清除已保存授权。

## 5. 选择上传传输策略

两种策略都使用 Google resumable upload。区别在于文件字节经过哪里。

| 策略 | 文件数据路径 | 适合场景 |
| --- | --- | --- |
| 服务端中继（`server_relay`，默认） | 浏览器 → AsterDrive → Google Drive | 用户设备只能访问 AsterDrive，或优先统一网络路径 |
| 浏览器直传（`frontend_direct`） | 浏览器 → Google Drive | 用户设备可以稳定访问 Google，希望减少服务器带宽 |

### 服务端中继

大文件按顺序传送，每次最多缓存一个 8 MiB 分片，不把完整文件读入内存。上传会话在后端加密保存，浏览器只调用 AsterDrive 的上传接口，不获取 Google 会话地址。

浏览器只需能访问 AsterDrive；Primary 需要能访问 Google OAuth 和 Drive API。容量探测超时与文件上传超时相互独立。

### 浏览器直传

AsterDrive 先做权限、配额和存储路由检查，再由后端创建 Google 上传会话。浏览器获得这次上传的临时会话地址，直接 PUT 文件分片；管理员 OAuth token 仍留在后端。

上传会话地址本身具有本次上传的访问能力，请勿复制到公开评论、日志或截图中。它不等于公开分享整个 Drive 目录。

Google 的正常中间片确认是 `308 Resume Incomplete`，通过 `Range` 表示已经收到的字节。AsterDrive 按确认位置续传；浏览器不可见响应头或遇到响应丢失时，会通过自己的进度接口让后端查询实际进度。

### 浏览器网络与 CORS

- 浏览器和 AsterDrive 后端都需要能访问相应的 Google 服务；只给服务器配置代理不能保证用户设备可以直传。
- 浏览器需要允许对 Google 会话地址的跨域 PUT，以及 `Content-Type`、`Content-Range` 请求头。
- Google 负责自身端点的 CORS 响应，AsterDrive 的反向代理不能替它补跨域头。
- `Range` 未暴露给浏览器时，客户端会请求后端协调进度；若预检本身被网络设备拦截，需修复浏览器到 Google 的连接或切回服务端中继。

AsterDrive 不会向 Google 上传请求附加自己的登录 cookie 或 CSRF header。直传完成后仍需后端检查最终对象和大小，才能创建文件、计入配额并完成上传。

### 恢复、取消与切换策略

刷新页面后，从 AsterDrive 的可恢复上传列表恢复任务，按界面提示重新选择原文件。恢复以 Google 已确认的位置为准。会话过期后需要重新开始上传。

取消通过 AsterDrive 处理后端会话和临时对象清理；中止浏览器请求与取消整个上传任务是两个步骤。清理失败应查看后台任务并重试。

修改上传策略影响后续新建的上传。已经创建的会话继续使用原会话类型；不要通过修改策略来强行转换正在进行的任务。

## 6. 容量观测的含义

普通账号目录和 appDataFolder 使用 Google `about.storageQuota` 的观测值。

| 情况 | AsterDrive 如何解释 |
| --- | --- |
| Google 返回有限 limit 和 usage | 可用容量为 `max(limit − usage, 0)` |
| Google Workspace 池化存储 | limit / usage 可能是组织池，不是目录独享容量 |
| 未返回 limit 的无限容量账号 | 不伪造零剩余，无法给出有限容量边界 |
| 配额响应暂时不可用或不完整 | 显示无法确定，并按现有容量准入规则处理 |
| 共享盘 | 当前不套用管理员个人账号配额，独立共享盘容量边界不可观测 |

`usage` 是相关账号或组织的跨服务总用量，可能包含 Gmail 和 Google Photos，不能只扣除 Drive 文件大小来计算剩余空间。

容量是共享缓存的观测值，不是空间预留。观测通过不保证其他写入不会随后用完空间；Google 的实际写入结果和组织限制仍然生效。

## 7. 下载方式和文件名

当前 Google Drive 下载通过 AsterDrive 服务端中继，支持流式下载和 Range 预览。AsterDrive 先检查访问权限，再以**当前 AsterDrive 文件名**返回文件，重命名后无需同步修改 Google 内部对象名。

因此这里没有 OneDrive 的“直接下载”和“优先使用 provider 文件名 / 严格使用当前文件名”配置。OneDrive 使用预认证下载地址；Google 的普通媒体下载需要授权，`webContentLink` 和异步 `files.download` 也不能未经验证就视为相同的私有对象分发机制。本实现不通过公开目录或下发管理员 token 换取直链。

`files.download` 也不接受 appDataFolder 使用的专用 scope，其 operation 的 24 小时有效期并不等于链接能够匿名下载 24 小时。当前下载中继可以统一保持普通目录、共享盘与 appDataFolder 的权限和文件名行为。

Google API 另外提供可选缩略图链接和图片 / 视频元数据，但本 connector 当前未接入存储原生处理。AsterDrive 继续使用现有缩略图与媒体处理链路；不要把 Google 提供字段等同于本项目已经支持相应能力。

## 8. 绑定策略组并做验收

将策略加入[策略组并绑定用户或团队](/admin/storage-policies/)。先在隔离目录验证：

1. 上传空文件、小文件和大于 8 MiB 的文件，确认命中预期策略。
2. 下载并检查内容，再验证图片或视频的 Range 预览。
3. 选择浏览器直传时，再验证跨域请求、刷新恢复和取消。
4. 重命名后下载，确认使用 AsterDrive 当前文件名。
5. 删除文件、清空回收站，等待 blob 引用释放后的物理回收。
6. 检查容量显示、后台任务和错误提示，不用一次连接测试代替整条文件流程。

本地协议测试不能替代真实 Google 账号、共享盘角色和组织策略验收。部署到实际网络后，应分别验证所选存储池和传输策略。

## 对象管理与日常维护

AsterDrive 在 Google 对象的应用属性中保存逻辑键，查询和删除限定在配置的存储池内。发现同一键对应多个对象时会拒绝操作，而不是随意挑一个删除。

不要手动移动、删除对象或更改应用属性。AsterDrive 文件删除后，版本、分享和其他引用可能仍保留 blob；只有满足引用和回收条件才物理删除。直接在 Google 网页删文件会绕过这些生命周期规则。

## 常见故障

| 现象 | 优先检查 |
| --- | --- |
| `redirect_uri_mismatch` | Google 中登记的回调是否与面板完全一致，代理是否生成了错误协议或域名 |
| 授权页拒绝访问 | 是否加入测试用户、应用发布状态、Google Workspace 第三方应用限制 |
| 需要重新授权 | refresh token 是否过期或撤销、Testing 七天有效期、是否取得离线授权 |
| OAuth 应用配置无效 | client ID 与 secret 是否属于同一 Web application；修正并保存后重新授权 |
| 开启 appDataFolder 后 403 | 保存后是否重新授权，是否获得 `drive.appdata`；不要先归因于 Testing 到期 |
| 目录或共享盘权限不足 | 目标 ID、共享盘归属、管理员角色；修复后运行凭据验证和连接测试 |
| 上传中的 308 | 正常的未完成确认；只有无法确认进度或网络请求失败时才需要排查 |
| 浏览器直传失败 | 用户设备到 Google 的网络、CORS 预检、扩展或企业代理；必要时选择中继 |
| 限流或暂时不可用 | Google 配额、代理和后端出站网络；稍后重试，无需先删除凭据 |
| 容量无法确定 | 是否无限容量、共享盘或暂时缺少配额响应，不要将其解释成剩余零字节 |
| 对象不存在或重复 | 是否被人工移动、删除、复制或修改应用标记；先恢复原应用和存储池配置 |

## 官方参考

- [Refresh token 有效期与失效原因](https://developers.google.com/identity/protocols/oauth2#expiration)
- [Google OAuth Web Server](https://developers.google.com/identity/protocols/oauth2/web-server)
- [Drive resumable upload](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
- [appDataFolder](https://developers.google.com/workspace/drive/api/guides/appdata)
- [账号或组织容量](https://developers.google.com/workspace/drive/api/reference/rest/v3/about)
- [下载与导出](https://developers.google.com/workspace/drive/api/guides/manage-downloads)
