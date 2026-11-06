# Workspace 与资源操作授权契约

本契约对应 #745。AsterDrive 产品授权由 `src/services/workspace/scope/` 拥有，REST、WebDAV、WOPI、分享、后台任务和管理员受控查看复用这里的规则。认证协议、资源归属、锁凭据和缓存命中分别表达不同事实，不能互相替代。

## 输入、授权结果与提交

- `WorkspaceStorageScope` 是 actor 与请求 workspace 的未验证输入，不能当作授权证明。
- `WorkspaceAccessConstraints` 保留委托 root、WebDAV 账号/verifier 绑定或 WOPI session/派生目标约束。它表达限制，不表达已经验证过的权限。
- `WorkspaceOperation` 显式声明读取、创建、修改、复制、移动、删除、恢复、锁和任务控制操作。当前使用既有用户/团队规则；#663 的 RBAC 应在这个产品决策点接入，协议 adapter 不自行解释角色。
- `authorize_workspace_on` 从权威数据库验证用户状态、团队生命周期和成员资格，以及实际携带的 credential/session 限制。
- `AuthorizedWorkspaceAccess` 借用本次操作所用的数据库连接。mutation 调用方必须传实际 writer transaction；重试事务重新获取，不能把结果序列化、缓存或保存进任务。
- `AuthorizedFileCreation` 绑定同一 transaction、actor、workspace 和最终父集合。底层文件记录创建不再接收第二套可替换的 scope/folder 参数。
- `AuthorizedResourceLock` 绑定 namespace 和 canonical lock root。token lifecycle 消费该证明，在同一事务中检查并更新锁，不能按 token 重新全局选择目标。
- `AuthorizedResourceRead` 区分用户、分享和受控管理员读取。公共详情、版本和回收站 projection 接收授权结果，管理员不冒充资源所有者，公开分享不伪造 actor。

Repository 仍只提供事实查询、锁定读取和原子 SQL。类型和模块可见性防止常见的内部误用，测试同时覆盖直接 service 调用；任意原始 SQL 本身不因此获得自动权限保护。

## 操作矩阵

| 操作 | 需要验证的范围 | 最终执行边界 |
| --- | --- | --- |
| 文件/目录读取、版本、回收站 | 当前产品资格、workspace、实体归属、公开能力或委托范围 | 授权事实从 writer 读取；允许滞后的内容 projection 可继续使用 reader |
| icon、元数据、属性 | actor 的当前访问资格与目标实体 | writer transaction 中授权和资源锁检查 |
| 创建空文件、目录、相对上传路径 | 目标 workspace、最终父集合、每个实际新建的路径段 | 授权先于路径/blob/session/provider grant 等副作用；重放也验证当前结果可见性 |
| 上传初始化/完成 | 当前 actor、session 归属与目标；凭据允许的 root | 准入先验证，接收 body/provider staging 后在发布 transaction 重新验证；新建检查集合锁，覆盖检查当前文件 |
| 复制 | 源读取范围和目标新增范围 | 每个目标集合检查；不能因为能读取源就认为能修改目标 |
| 重命名、移动 | 当前目标、移出集合、移入集合 | 同父目录重命名也改变成员 URL；目录 Infinity 操作检查 canonical 后代锁 |
| 删除、恢复 | 当前实体及实际移出/移入集合 | 父目录不可用而回退 root 时校验最终目标；后台任务同样受约束 |
| LOCK、UNLOCK、refresh、discovery | 当前 workspace、canonical lock root、委托挂载范围 | token 不是访问其他 workspace/root 的授权；不可见祖先锁仍限制 mutation，但不暴露它的 token |
| 任务创建、手动 retry/cancel | 当前主体、目标、task kind | creator/owner 不代表仍有管理权限；管理任务控制读取当前管理员资格 |
| 管理员查看和下载 | 当前 admin/session、受控记录、目标、时效 | 保留原因、真实 actor、workspace、版本范围及必要审计；没有普通文件写入授权 |

`allow_locked` 等裸 boolean 绕过已从文件/目录删除入口移除。协议 adapter 解析正向 token 条件并传递真实凭据，产品锁模块验证实际受影响资源。

## 并发与数据库一致性

写入统一先取得 workspace lock namespace，再锁 owner/team、成员/actor/credential 及实际资源。具体 use case 仍需遵守现有配额、revision、task lease 和有序资源锁约束。成员移除/团队归档持有同一 team 行；发布持 guard 后再作 current membership read，不能在 MySQL REPEATABLE READ 下依赖较早建立的普通快照。

授权资格、撤权和资源范围来自 writer；reader 仅服务允许滞后的 projection。WebDAV 认证缓存只保留昂贵密码验证对应的 verifier 指纹，每次请求核对当前账号、verifier、owner 和团队资格。删除缓存只是优化，撤销后并发回填旧值仍必须失败。

接收 body、下载和 provider staging 不长期持有数据库事务。发布时重新授权并验证当前实体 ancestry；挂载内目标在接收期间被移出 root 后拒绝发布，临时资源按既有失败清理机制处理。幂等命中也不绕过当前权限与结果范围检查。

撤权先完成，后续授权/发布必须拒绝；合法发布先持有并完成保护事务，则保留已提交结果。撤权不追溯删除合法写入，也不能追回已交付的字节。

## Canonical 锁与协议语义

锁的身份是 workspace namespace、root kind 和稳定实体 ID；`lockroot_path` 仅用于历史呈现。锁覆盖、Infinity 后代冲突、发现、删除和覆盖重绑定使用当前实体/ancestry。两个受限挂载都可以有 `/same.txt`，路径相等不意味着同一资源。

批量发现一次加载锁根及祖先 frontier，避免逐锁重建相同目录链。协议输出将当前 canonical root 投影成 mount-relative URI，目录保留尾斜杠。tagged/untagged `If`、共享锁的同根 token、真实后端错误和 423/412 等状态仍按 WebDAV 契约处理。

## 任务、WOPI 与已签发能力

用户委托的文件任务在执行和最终发布时检查当前资格。已接受的系统维护作业按系统生命周期恢复，保留原发起者归属；手动重启或取消特权任务是新的管理操作，必须检查当前权限。

WOPI 保留自己的 session、file、app、proof 和锁规则，发布时核对当前 session 与用户版本。PUT_RELATIVE 只能派生到原文件当时的父集合，原文件被移动后不把旧请求扩大成 workspace 任意写。

普通请求的认证时效由认证 adapter 验证；最终 mutation 复核当前 actor/成员/资源和显式携带的协议约束。既有读取流不引入逐字节数据库检查。已签发到外部对象存储的 presigned URL 仍有供应商侧 TTL，应用拒绝发布不等于立即收回已签发 URL。

## 前端异步操作

上传任务在入队时冻结 `accountId + generation + workspace + target`。所有传输模式、轮询、完成、重试和取消使用这个目标的静态 service，页面导航不改变目的地。目录扫描在第一次 await 前捕获目标。

generation 随登录身份生命周期变化，正常 token refresh 保留 generation。登出开始即取消旧请求；HTTP config 保留 signal，避免 401 refresh 后用新 cookie 重放旧请求。XHR 在实际 send 前再检查取消；await 后的迟到结果不能启动下一阶段或修改新账号状态。

登出同步清理文件列表、团队、音乐队列和下载；下载 controller 与重试闭包一起释放。团队和目录树的异步回填验证请求生命周期，旧请求的 finally 不能清除新请求的去重状态。refresh/logout 的响应排序及单会话立即撤销见 [认证状态机契约](./auth-flow-state-machine.md)。

恢复任务绑定 upload host 生命周期，单次发现 effect 的取消与已恢复任务生命周期分开。持久化记录必须有账号和 workspace，未知账号的历史记录不能自动执行；当前账号可以从服务端重新发现属于自己的可恢复 session。前端绑定不替代服务端授权。

## 维护与验证

新增入口必须在操作矩阵中明确主体、动作、源/目标范围、权威数据库、决策时点和审计。修改共享接口时同步迁移全部消费者，删除被替代的转发入口和授权实现。

回归测试至少包括非成员/已移除成员、管理员降权、撤销后缓存回填、跨 workspace/root token、body 期间移动目录、集合 Resource-depth 锁、canonical 后代锁、幂等重放、长任务发布，以及前端导航/换号/卸载后的迟到结果。数据库竞争验证覆盖 SQLite、PostgreSQL 和 MySQL，协议验证同时覆盖允许与拒绝路径。

#586 拥有新连接账号和 credential 生命周期产品；本契约提供其执行边界。#663 拥有 RBAC 角色/权限目录与管理界面。密码/MFA、外部身份绑定、XSS 和 SSRF 的独立漏洞不因 #745 完成而自动视为修复。
