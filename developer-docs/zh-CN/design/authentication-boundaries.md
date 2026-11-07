# 认证预算与跨边界验证

## MFA

账号级预算由 writer 数据库保存，不依赖可选的 HTTP/IP 限流。登录与敏感操作分别计数：15 分钟固定窗口内第 5 次失败进入 15 分钟冷却；新 flow、换会话、换敏感操作不会刷新同一用途的预算。冷却期间拒绝请求不写入、不延长截止时间。成功验证只清除对应用途预算；管理员重置 MFA 可解除全部 MFA 预算。

删除因子与重建恢复码共用敏感操作预算。失败计数必须提交后再向调用方返回验证码错误，不能随业务失败一同回滚。恢复码的昂贵哈希准备留在事务外；账号锁内再次检查预算和当前因子／恢复码状态，成功消费与业务修改同事务提交。

邮件验证码发送独立保留账号级发送资格。读取冷却、预留发送资格、替换验证码在同一账号锁内完成；发送资格提交后再执行邮件投递。验证码被消费或投递失败都不刷新冷却。每个账号最多三个固定用途的预算记录，不为每次猜测创建历史记录。

## 一次性认证状态

Passkey 注册／登录 challenge 和内部签名请求 nonce 使用 `auth_once_states` 的唯一键与消费 CAS。key 以 SHA-256 存储，状态带绝对到期时间；写入或消费失败向上返回错误，不能退回独立内存副本。Passkey challenge 在凭据验证前消费，失败也不能重放。缓存恢复或残留旧 challenge 不重新获得认证权。

每次写入最多按到期索引回收 128 行，payload 最大 256 KiB、key 最大 1024 字节，避免全表 materialization。新增表需同时升级 Primary 与 Follower；切换后升级前仅在缓存中的 Passkey 流程需要重新发起。普通内容缓存继续保留原来的可用性降级语义。

## 最终授权

- 强制改密会话只能访问其允许的流程；optional identity 不能把有效 token 自动升级为完整登录身份。
- 改密带入已验证 hash 和签名 claims，在 writer 事务内锁定账号／session 后确认密码状态、账号代、session 撤销及到期，再修改密码。并发恢复或撤销优先时，旧请求不能覆盖新状态。
- 邮箱确认提交前检查当前 blocklist；allowlist 的既有 grandfathering 语义保留。
- 存储 SSE 保留 token 的绝对到期时间，在到期时终止并在发送前复核，不能仅凭账号状态延长 token 生命周期。
- HTTP worker 的认证／账号 API limiter 由进程在 worker factory 外创建并共享；账号 MFA 预算独立于 HTTP 限流是否启用。

## 远端节点与凭据

HTTP 或 HTTPS 的显式部署选择保留，不因 Finding 6、34、33 强制修改协议。签名正文摘要、防重放、目的范围和重定向策略仍由应用保证。

Multipart 清理先验证所有 listing 结果都是本次上传的规范 part key，再按已验证 part number 重建删除路径；不向返回的任意路径签发 DELETE。target key 在引用检查之前拒绝非规范拼写。隧道 polling/completion 在反序列化和派发之前核对有界原始正文与已签名摘要。内部 owner proxy 保持不跟随重定向。

生成的 POSIX 注册命令使用已转义参数和带引号的 heredoc 将 token 交给 `--token-stdin`，不把 token 放进进程 argv。aria2 连接测试只有在目的 endpoint 与保存配置一致时才复用隐藏密钥；其他 endpoint 必须提供显式草稿密钥。

Follower 语言包的 namespace 由前端按 node／connector 身份生成，服务端返回的 namespace 不得覆盖应用资源。刷新替换完整 bundle 并清理消失的资源，输入有总量预算。

## Provider 发布与配置

COS 的长期读写身份预签名 PUT 不能限制额外的 copy-source header，因此浏览器上传使用已有服务端流式中转；COS 存储、下载、原生处理与服务端 multipart 保留，不引入新的 STS 产品功能。

Azure 的客户端 multipart 写能力只对应临时 key，最终完成复制到未向客户端授予写权限的新 key，并校验大小后发布 metadata。Azure 临时回退文件保持独占创建／0600，下载 SAS 对响应类型和附件字段签名。

Provider session 在外部创建之前，先在 policy 锁内预留数据库上传记录；目的配置变更必须看到该记录。外部创建失败执行补偿，安装结果还需 session 状态 CAS。OneDrive 仅修改 tenant／cloud 时也同步应用身份并失效不兼容授权。driver cache 绑定 policy 快照和凭据代，旧快照不得安装成当前缓存，凭据代发布与缓存失效共享同一同步边界。

这些修复不引入 vendor/fork。真实 provider IAM、TLS 部署与生产验收仍需分别验证；本地协议 fixture 不能代表云端授权验收。

COS 升级前已签发的旧上传 URL 仍按原到期时间有效；需要立即撤销时由部署者轮换对应凭据。新版本不再签发该类浏览器写委托。
