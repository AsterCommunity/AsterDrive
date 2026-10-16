# 认证 flow 状态机契约

本文定义登录、第二因子、账户恢复和 session 生命周期的共享边界。认证 payload 继续由 MFA、Passkey、external auth、contact verification、invitation 和 session 各自拥有；共享状态机只统一 identity、状态、过期、尝试次数、单次消费和并发转换规则。

## 所有权

```text
HTTP route
  -> typed auth command
  -> auth domain transition guard
  -> owning service transaction
  -> repository conditional update / cache atomic take
  -> commit
  -> cookie, redirect, mail and audit side effects
```

- `src/services/auth/flow/` 定义 `AuthFlowKind`、`AuthFlowState`、command、snapshot 和转换规则。
- MFA、external auth、local recovery、Passkey 和 session service 仍负责产品 guard、运行时策略和副作用顺序。
- repository 只执行原子条件更新。`rows_affected == 0` 或 cache `take == None` 是无结果 outcome，可能代表 conflict、replay、expiry 或 cache eviction；service 根据权威字段映射最终状态，repository 不选择 UI 行为。
- route 保持现有 API envelope、cookie 和 redirect 契约。

## Flow inventory

| Flow | Payload owner | Identity | 原子推进 | Terminal / cleanup |
| --- | --- | --- | --- | --- |
| Password primary | local auth service | request-local | password verify 后映射到 MFA/password-change/session | request 结束 |
| MFA login | `mfa_login_flows` | `mfa-login:<id>` | transaction + conditional attempt/consume | consumed/expired，runtime cleanup |
| Passkey login/registration | typed cache envelope | public flow UUID | cache atomic `take` | consumed/TTL eviction |
| External login | `external_auth_login_flows` | `external-login:<id>` | state + browser binding 条件消费 | consumed/expired，runtime cleanup |
| External email recovery | `external_auth_email_verification_flows` | `external-recovery:<id>` | conditional email request/consume | consumed/expired，runtime cleanup |
| Registration/password reset/email change | `contact_verification_tokens` | `contact-verification:<id>` | transaction + purpose-scoped single-use token | consumed/expired cleanup |
| Invitation | `user_invitations` | `invitation:<id>` | status-scoped conditional update | accepted/expired/revoked |
| Session | `auth_sessions` | session UUID | refresh JTI conditional rotation | revoked/expired cleanup |

Identity 只使用数据库主键或公开 flow UUID。Password primary 的 request-local identity 是明确例外：它只在当前请求内存在，不进入跨请求共享 snapshot、日志或错误。原始 token、state、browser binding、verification code、JTI 和它们的 hash 不进入共享 snapshot、日志或错误。

## Transition rules

- Password primary 可以进入 `SecondFactorPending`、`PasswordChangeRequired` 或 `Authenticated`。
- Passkey/external primary 必须先从 `FirstFactorPending` 进入 `Processing`，再进入 MFA 或 authenticated；本地密码策略不阻止这两种 first factor。
- MFA 只从 `SecondFactorPending` 进入 password-change 或 authenticated。
- Recovery 必须按 `RecoveryPending -> Processing -> Completed` 推进。
- `Failed`、`Expired`、`Cancelled`、`Consumed`、`Completed` 和 `Authenticated` 是 terminal，不接受后续 command。
- revision 不匹配先返回 conflict；terminal 检查其次；除显式 `Expire` 外，过期检查早于 cancel 和正常推进。
- failure command 原子增加 attempt；达到预算时进入 `Failed`。计数使用饱和运算，不能整数回绕。

## Policy and side effects

受保护请求读取 writer 上的当前账号授权，writer 连接池繁忙时鉴权会等待。通过鉴权后的文件、目录纯读查询仍使用 reader；缓存快照不能绕过尚未完成的 writer 撤销判定。

本地会话的 JWT 签发、严格校验、refresh 分类和有界进程内协调由 `aster_forge_auth` 提供；Drive 的 `local/tokens/refresh.rs` 实现存储接口，保留 writer 事务、账号策略、SQL、审计和错误映射。账号 generation 在事务内原子递增，达到上限时失败回滚。

JWT 使用必需的 `sub/sid/session_version/token_type/restriction/iat/exp/iss/aud`，refresh 必需 JTI，access 不携带 JTI。旧格式不兼容，部署后用户需要重新登录；Cookie 名称、CSRF 和 API envelope 不变。单会话撤销立即阻止 refresh，已有 access 保留短期到期窗口；账号级 generation 和禁用检查读取 writer，不接受缓存中的旧授权。

refresh 的 CAS、reuse 撤销及必要安全审计同事务提交，提交失败不返回新凭据。stale 只改变拒绝分类，绝不再次签发；UA/IP 不是设备身份。取得数据库锁后重新检查凭据截止时间，锁等待不延长有效期。缓存删除失败或旧缓存回填不能恢复 access 授权。HTTP 组件先关闭刷新协调器，再排空请求；已进入事务的请求允许完成。

- 每次跨请求推进都重新读取 runtime auth policy。创建 flow 时的 UI snapshot 不是授权依据。
- 密码 first factor 在 MFA 交换时重新检查 password login policy；external first factor 不受该开关影响。
- session row 必须先在 transaction 中持久化，再由 route 设置 cookie。事务失败不产生 session cookie。
- mail outbox、audit 和 cache invalidation 必须明确在 commit 前或后执行。失败不能用静默 fire-and-forget 吞掉。
- 前端 `AuthUiFlow` 是后端状态的单一 frontend/UI projection；URL adapter 只恢复有过期时间的 flow reference，限制 TTL、method 和本地 return path。
- auth check 与 provider list 由带 generation 的 coordinator 合并。旧 generation、卸载后的 promise 或较慢响应不更新当前页面。

## Test matrix

- Domain：全部允许转换、非法跨 context、terminal replay、revision conflict、attempt 边界、整数饱和、过期/cancel 优先级。
- Repository/integration：条件消费单次成功、并发 loser、失败不发 session、策略热更新、过期 cleanup。
- Passkey cache：flow 隔离、单次 `take`、TTL envelope、registration/login kind 隔离。
- Frontend：唯一顶层 flow、URL 恢复优先于 bootstrap、provider 部分失败、stale generation、恶意 return path、TTL 上限、未知/重复 method、query cleanup。

## Schema boundary

当前 typed 表已经保存各自需要的安全 payload，并通过 `consumed_at`、`expires_at`、attempt 或 status 提供权威生命周期。共享 snapshot 从这些字段派生，不复制一套 `state` 列形成双重事实源。只有某个 flow 确实需要跨请求多阶段 CAS，且现有条件字段不能表达 revision 时，才为该 typed 表增加 revision；不建立万能 auth JSON 表。
