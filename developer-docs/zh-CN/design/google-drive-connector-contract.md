# Google Drive Connector 契约

Google Drive 是管理员托管的 blob 存储池，不是普通用户的外部目录浏览、导入或同步连接件。
文件、版本、配额、权限、事务和物理回收仍由 AsterDrive 拥有。

## 所有权与代码入口

| 入口 | 职责 |
| --- | --- |
| `src/storage/connectors/google_drive.rs` | 配置与凭据输入校验、加密持久化、runtime / cleanup driver 构造 |
| `connectors/google_drive/descriptor.rs`、`localization.rs` | 管理 UI、动作、能力和凭据状态展示；前端只消费 descriptor |
| `connectors/google_drive/oauth.rs` | policy / state / PKCE 绑定、授权码交换、scope 与离线授权检查 |
| `connectors/google_drive/provider.rs` | writer 权威凭据读取、单实例刷新合并、跨实例 revision CAS、净化后的状态与审计 |
| `src/storage/drivers/google_drive/client.rs` | Google HTTP、存储池验证、appProperties 查询、对象定位和错误映射 |
| `drivers/google_drive/paths.rs` | 相对基础路径的逐级目录定位、写入创建及重名保护 |
| `drivers/google_drive/upload.rs` | 有界上传、resumable 会话、Range 确认、取消与 attempt 清理 |
| `drivers/google_drive/capacity.rs` | 账号 / 组织池化配额解释、共享盘与无限容量的保守分类 |
| `drivers/google_drive/mod.rs` | `StorageDriver` 对象读写与扩展能力装配；不访问产品数据库 |

OAuth 是 connector 的管理协议。driver 通过 token provider 获取 access token，不读取凭据表，
不决定管理员状态，也不记录原始 Google 错误消息或会话 URL。

## 与 OneDrive 的配置对照

| 配置或能力 | OneDrive | Google Drive |
| --- | --- | --- |
| 管理员 OAuth、加密凭据、flow revision CAS | 支持 | 支持；共用最终写入前置条件 |
| 上传策略 | `server_relay` / `frontend_direct` | 同样两种策略，默认中继 |
| resumable 分片确认 | 202 + JSON `nextExpectedRanges` | 308 + HTTP `Range` |
| 容量 | Graph drive quota | about 账号 / 组织 quota；独立共享盘边界不可观测 |
| 容量探测预算 | 默认 10 秒，可配置 | 默认 10 秒，2–30 秒，可配置 |
| 下载策略 | 中继 / 预认证直链 | 当前中继；不下发管理员鉴权 header |
| 下载文件名 | provider_native / strict_current | 中继直接使用当前 AsterDrive 文件名，无需空的选择器 |
| 原生缩略图 / 媒体信息 | 以当前 descriptor 为准 | Google API 有字段，当前产品 adapter 尚未接入 |

Google `webContentLink` 是浏览器下载链接，但文档没有把它承诺为私有管理员对象的匿名预认证 URL。
新的 `files.download` 使用 LRO 返回 `downloadUri`；24 小时描述的是 operation 有效期，不能
自行推导为链接可匿名使用 24 小时。是否能用于 Drive 私有对象直链仍需独立研究和真实账户验收，
不通过公开共享文件或把 OAuth token 塞入 URL 补齐能力。

独立调查确认不能把所有 `downloadUri` 当作预认证链接：Google Workspace CLI 官方仓库的
[#907](https://github.com/googleworkspace/cli/pull/907) 贡献者复现报告指出，共享盘二进制文件
可返回普通 `files/{id}?alt=media&source=downloadUrl`，其补丁仍为后续 GET 附 Bearer。
该 PR 未合并，属于具体实现 / 复现证据，不提升为 Google 协议保证。
官方 [`files.download`](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/download)
允许的 scope 也不含 `drive.appdata`，不能直接覆盖当前 appDataFolder 专用授权。
因此本次保留中继，不增加无法统一兑现的 direct-download / provider-filename 配置。

Google `thumbnailLink` 是短期链接，私有文件读取需要凭据且官方建议通过 proxy，
`imageMediaMetadata` / `videoMediaMetadata` 是可选且可能延迟的字段。接入它们需要相应的服务端
读取、错误映射、MIME / capability 和异步结果测试；当前继续使用已有通用处理链路。

## 规范来源

- [Google OAuth Web Server](https://developers.google.com/identity/protocols/oauth2/web-server)：授权码、离线 refresh token、应用错误与用户授权失效。
- [Google Drive 上传](https://developers.google.com/workspace/drive/api/guides/manage-uploads)：POST / PATCH 发起、Location 会话、PUT 分片、256 KiB 对齐、308 与 Range 确认、过期与恢复。
- [files.list](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list)：分页、`incompleteSearch`、空间与共享盘查询范围。
- [appDataFolder](https://developers.google.com/workspace/drive/api/guides/appdata)：独立 scope、隐藏空间、限制与 `alt=media` 下载。
- [文件自定义属性](https://developers.google.com/workspace/drive/api/guides/properties)：`appProperties` 的应用可见性和 key + value 合计 124 UTF-8 字节上限。

实现对照参考 OpenList `fb692b54d74210fcdb770ac8e82848be06990e75` 的
[`drivers/google_drive`](https://github.com/OpenListTeam/OpenList/tree/fb692b54d74210fcdb770ac8e82848be06990e75/drivers/google_drive)。
OpenList 提供了 POST / PATCH、服务端分片和共享盘参数的实现参考；其用户目录浏览、重试和
日志策略不替代 AsterDrive 的不可变 blob、凭据并发、错误脱敏和上传完成契约。

## 基础路径与位置变更

`base_path` 默认空，规范化首尾斜杠和重复分隔符，作为配置 root / shared drive / appDataFolder
之下的相对真实目录。它不修改 blob 的逻辑 key，也不把 `files/...` 展开成目录树。
最多 32 个组件、总长 1024 UTF-8 字节、单组件 255 字节是项目请求预算；拒绝点路径、
反斜杠、控制字符和 URL。目录 API 依据 [folder guide](https://developers.google.com/workspace/drive/api/guides/folder)。

读取 / 下载 / metadata / 删除不创建目录；写入逐级查找并创建缺失目录，然后重新确认。
查找必须包含 parent / space / shared-drive 范围；同名文件、快捷方式和重复目录都拒绝，
不通过 mimeType 过滤把一个同名非目录偷偷忽略。跨 Primary 的 create-by-name 不原子，
创建后查到多个 ID 或无法确认新 ID 时保守失败，不递归删除可能属于他人的目录。
一次创建失败留下的空目录可以由后续请求复用；底层 Google 名称唯一性不由本地 mutex 保证。

`validate_policy_update` 是 connector-owned 事务校验入口。Google 在 policy 行锁下检查
root / shared drive / appDataFolder / base path 是否变化，有 blob 或未完成上传时拒绝。
失败和过期会话仍可能持有待清理对象，不能只统计未到期活跃会话。传输策略和探测 timeout
不改变位置，仍可更新。迁移到新位置必须创建新策略并迁移已有数据。

定位变更保护的实际触发：旧版本允许将普通目录策略切到 appDataFolder，此后清理旧 blob
会在新空间查询，收到“未找到”后正常丢弃 DB 行，旧空间对象却可能遗留。单纯重试 GC 不能
恢复已丢失的旧位置；禁止有数据时原地换池，不能靠放宽删除范围搜索整个管理员 Drive 补救。

## 七天有效期与 scope 切换

外部应用发布状态为 Testing、请求 Drive scope 时，Google 签发的 refresh token 七天后到期；
持续自动刷新 access token 不能延长该期限。正式应用使用相同 token endpoint 和
`grant_type=refresh_token`，不受这条特定七天期限约束，但用户撤销、六个月未使用、
活跃 token 数量限制、限时授权或管理员策略仍可能使其失效。
[官方失效条件](https://developers.google.com/identity/protocols/oauth2#expiration)。

当前只记录 access token 的 expires_at，不伪造 refresh token 第七天精确截止值，也没有
提前提醒任务。下一次需要刷新时，invalid_grant 进入 ReauthRequired，文件保留且必须管理员
重新授权。应用从 Testing 改为正式后应重新授权，不假设旧 token 自动延长。

appDataFolder 的 `drive.appdata` 与普通空间的 `drive` scope 不同。切换后保存配置不会给
旧 token 增加权限，需要重新授权；这类 403 与 refresh token 七天失效的 invalid_grant 不同。
字段 descriptor help 和中英文教程都必须提示这一点。

## 凭据与授权边界

- 创建应用必须提供完整 client ID / secret；编辑时省略或留空 secret 只沿用同一 client ID 的旧值。
  换 client ID 必须显式提供新 secret，应用身份变化后清除旧授权。该行为通过正式 registry 入口验证。
  这不是数据迁移：另一 Google 应用可能无法读取原应用的私有 `appProperties` 或 appDataFolder。
  已有池保留原应用；换应用先新建策略并迁移，迁移和清理完成前不撤销旧应用。
- typed application 的 `Debug` 必须净化 secret；加密前序列化仅用于存储和 token 请求。
- 只有识别到 `invalid_grant` 才设置 `ReauthRequired`；`invalid_client` / `unauthorized_client`
  设置 `Invalid` 并返回 `Misconfigured`。429、408、5xx、代理 HTML、未知错误和畸形成功响应不改写已保存凭据。
- refresh 返回新的 refresh token 时加密更新；省略时保留旧值。CAS 失败后重新读取 writer 行，
  不发布过期的响应、不覆盖管理员新应用 / 新授权；竞争重试有界。
- 授权开始捕获 credential revision；网络请求前后检查配置，最终写事务也执行同一 revision CAS。
  一次性 state 的消费由通用 callback service 负责，失败后不能重放同一个 flow。
- 最终写入先锁 policy 行再检查配置与 credential revision，锁顺序与策略编辑一致。
  不能用事务外或 READ COMMITTED 下的普通读取代替，否则仅修改存储位置仍可能穿过最终检查窗口。
- OneDrive 同样捕获 revision，拒绝授权途中应用替换、凭据删除和最终提交前的竞争。
  旧版本未捕获 revision 的短期 OneDrive flow 允许解析，但要求重新发起授权，不降级为无条件写入。
- runtime 拒绝不可用状态。管理员显式验证可以在修复目录权限或配置后重新探测；它不能绕过
  `Revoked` / `ReauthRequired`，也不能在远端 root 验证完成前重新启用 runtime。
- policy 删除后的回收使用绑定 policy ID 的加密快照；独立刷新保存在该 cleanup provider 内，不能重建已删除的产品凭据行。
- OAuth 回调将 `ValidationError` 映射为 `InvalidState`，交换过程的 `StorageDriverError`
  映射为 `TokenExchangeFailed`，数据库、解密等其他错误映射为 `ServerError`。
  不用通用 `storage_error_kind()` 推断阶段，避免把 precondition / unsupported 误认为交换失败。

## 对象与上传边界

- 逻辑 key 使用 `appProperties.asterdrive_storage_path`，定位必须同时限定该标记、parent 和 space。
  共享盘查询限定 `driveId` / `corpora=drive`；appDataFolder 不与普通 root / shared drive 混用。
  根目录验证要求配置的共享盘 ID 与 Google `File.driveId` 完全匹配；未配置共享盘时也拒绝
  返回 `driveId` 的目录，避免共享盘池误用账号容量或默认查询范围。
- `pageSize=2` 是上限，短页或空页仍需按 `nextPageToken` 继续；不同 file ID 才证明重复对象。
  `incompleteSearch`、重复 page token 或超过 16 页预算返回暂时失败，不得把不完整查询当作不存在。
- resumable 会话 URL 只接受上传 endpoint 的同 origin、合法路径和单一非空 `upload_id`。
  不跟随重定向；管理员 OAuth 凭据不传给浏览器。只有选择 `frontend_direct` 的授权上传会话
  会向 owner 返回临时 capability URL；中继会话隐藏该 URL。URL 不进入错误或日志。
- 每次最多缓冲一个 8 MiB 分片；非末片长度按 256 KiB 对齐。`ExactSizeReader` 在最后一个字节
  发布前确认 EOF，短读 / 超读属于 precondition，底层 I/O 错误保持 transient。
- 308 是未完成确认，按 `Range` 计算下一个 offset，不能假定已收到整片；拒绝畸形、溢出或超出已发字节的确认。
  流式 reader 不能任意 seek，整片未被确认时返回暂时失败并取消本次会话；可恢复分片入口把 provider offset 返回上层恢复。
- 浏览器直传遵循[后端协调契约](./browser-upload-coordination.md)，driver 解析 308 / Range 并生成下一条请求；前端只执行指令、回报响应并消费确认 offset，
  严格串行并按 provider 的实际位置恢复。Header 因 CORS 不可见、网络丢失或确认畸形时，经后端
  progress API 协调，不能盲目推进。后端 finalization 仍验证对象实际大小后才发布并扣配额。
- `abort_attempt` 只取消当前 attempt 的会话，不能删除最终 `storage_path`。未发布对象回收归调用方。
  取消失败或取消 future 被丢弃时保留共享 session handle；确认完成或取消成功后移除。
- session DELETE 对成功 / 已失效响应幂等处理，并兼容 Google 上传端点可能返回的 499。
  Drive 官方上传指南未单独承诺 499 取消行为，必须通过真实 provider 验收，不能用 mock 宣称官方兼容性已证明。

## 容量与浏览器响应声明

`capacity_info` 对普通目录和 appDataFolder 查询 `about?fields=storageQuota(limit,usage)`。
官方将 Workspace 池化存储的 limit / usage 定义为组织跨服务容量，因此不能用 `usageInDrive`
替代总 usage，也不能把该值声称为目录独享空间。缺失 limit 表示没有有限边界；共享盘可能属于
其他组织且有独立限额，当前返回 Unsupported，不让管理员个人配额错误拒绝共享盘写入。
字段缺失或请求暂时失败属于不可确定，畸形 / 负数 / 溢出值不变成“容量充足”。
探测复用网络缓存、并发合并和可配置 timeout 预算，不新增轮询任务或全局状态。

现有 capability 尚未描述浏览器如何解析确认响应。本次暂时在通用前端支持两种已知格式，
在 runtime capability、init/progress DTO 和解析处保留 `TODO(#656)`。
[#656](https://github.com/AsterCommunity/AsterDrive/issues/656) 单独设计后端声明及会话快照，
不在本次增加 provider 白名单、任意脚本或通用表达式解释器。

## 验证矩阵

本地测试覆盖正式 registry、加密 SQLite、实际 HTTP 请求和可控并发闸门，包含配置、secret
preserve / replace、PKCE / scope / callback、刷新错误矩阵、CAS 竞争、删除、cleanup 快照、
普通 / 共享 / appData 根、分页、重复 key、短读 / 超读 / 空文件、分片与 Range、取消和重试。
新增 connector 不能只跑自身测试，还要跑 connector registry、生成文档、Clippy、前端存储表单和初始化 E2E。

```bash
cargo nextest run --profile ci --lib google_drive
cargo nextest run --profile ci --lib storage::connectors
make storage-docs-check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cd frontend-panel && bun run test:e2e:provider-upload
```

真实 provider 测试位于 `connectors/google_drive/tests/acceptance.rs`，默认 ignored。必须显式设置
`ASTER_TEST_GOOGLE_DRIVE_ALLOW_WRITE=1`、`ASTER_TEST_GOOGLE_DRIVE_CLIENT_ID`、
`ASTER_TEST_GOOGLE_DRIVE_CLIENT_SECRET`、`ASTER_TEST_GOOGLE_DRIVE_REFRESH_TOKEN`，并通过
`ASTER_TEST_GOOGLE_DRIVE_ROOT_FOLDER_ID` / `ASTER_TEST_GOOGLE_DRIVE_SHARED_DRIVE_ID`
选择隔离存储池，或显式设置 `ASTER_TEST_GOOGLE_DRIVE_APP_DATA_FOLDER=1`。
测试只清理自身唯一 key 和会话，不接管其他文件。

```bash
cargo nextest run --profile external --lib google_drive_real_provider_acceptance -- --ignored
```

该测试证明已有离线授权的刷新、root 权限、空文件、分片、下载 / Range、失败覆盖保护和
取消 / 删除行为。管理员浏览器 OAuth 首次同意、共享盘角色限制及不同空间仍需分别验收。
本地 mock、真实 token refresh、浏览器授权和实际 Google Drive 数据面是不同证据，不能相互替代。
