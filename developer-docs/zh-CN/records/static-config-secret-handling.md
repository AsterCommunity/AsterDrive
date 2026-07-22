# 静态配置密钥处理备忘

> 状态：草稿，待补充
> 背景：评估 `secrecy::SecretString` 是否适合用于 `config.toml` 读取后的敏感配置值。

## 先说结论

`secrecy::SecretString` 可以降低意外日志泄露和 `Debug` 输出泄露的风险，但它不是“防止 secret 驻留内存”的工具。

当前不建议为了“让 `config.toml` 获取的 secret 不驻留内存”而全面改用 `SecretString`。更直接的短期工作应该是：

- 给静态配置里的敏感字段做 `Debug` 脱敏，或移除相关类型的 `Debug` derive
- 确保 doctor、health、CLI、日志输出继续使用 redaction
- 后续再评估是否只对 `auth.jwt_secret` / `auth.mfa_secret_key` 这类长期密钥局部引入 `SecretString`

## 当前代码位置

静态配置 schema：

- `src/config/schema.rs`
  - `Config`
  - `AuthConfig`
  - `DatabaseConfig`
  - `Config.cache` 使用 `aster_forge_cache::CacheConfig`，Drive 不维护平行 cache schema

配置加载链路：

- `src/config/loader.rs`
  - 读取 `config.toml`
  - 合并 `ASTER__` 环境变量
  - `try_deserialize::<Config>()`
  - 解析相对路径和 SQLite URL

全局持有：

- `src/config/mod.rs`
  - `static CONFIG: OnceLock<Arc<Config>>`
- `src/runtime/mod.rs`
  - `PrimaryAppState.config: Arc<Config>`
  - `FollowerAppState.config: Arc<Config>`

这意味着静态配置通常会被持有到进程退出。即使使用 `SecretString`，其 drop-time zeroize 也主要发生在进程结束阶段，对“运行期间常驻内存”的改善有限。

## `SecretString` 能解决什么

`SecretString` 是 `secrecy::SecretBox<str>` 的类型别名，主要能力：

- `Debug` 输出自动脱敏
- 通过 `ExposeSecret` 显式访问 secret，方便审计使用点
- drop 时调用 `zeroize` 清理内部缓冲区
- 启用 `serde` feature 后支持反序列化；`Serialize` 不默认提供，只有内层类型实现 `SerializableSecret` 时才可用

它适合降低这些风险：

- 开发者误写 `tracing::debug!(?config)`
- 错误报告或测试失败输出把 secret 打出来
- secret 生命周期较短，drop 后希望减少释放内存中的残留

## `SecretString` 不能解决什么

它不能保证 secret 不出现在进程内存里，也不能阻止：

- `config` crate / TOML 解析过程中的中间字符串副本
- 环境变量读取产生的 `String`
- 传给 SeaORM、Redis、SMTP、JWT、S3 SDK 后产生的内部副本
- swap、core dump、debugger、ptrace、进程内存读取
- 长生命周期全局 `Arc<Config>` 持有

所以如果威胁模型是“攻击者能读取进程内存”，`SecretString` 不够，需要系统级防护或 secret manager 方案。

## 当前敏感字段清单

静态配置中需要重点关注：

- `auth.jwt_secret`
- `auth.share_cookie_secret`
- `auth.direct_link_secret`
- `auth.mfa_secret_key`
- `auth.storage_credential_secret_key`
- `database.url`
- `cache.redis_url`

其中 `database.url` 和 `cache.redis_url` 只有在 URL 中带账号密码时才包含密钥材料，但日志脱敏仍应按敏感字段处理。

### URL 脱敏约束

`src/cli/db_shared.rs::redact_database_url` 用于 doctor 和数据库迁移输出，必须同时覆盖：

- authority 中的用户名和密码，输出中不保留任一凭据；
- query 参数中名称为 `token`、`password`、`secret`、`api_key`、`credential` 及其常见前缀/后缀变体的值；
- 参数名和值经过 percent-encoding 的情况，先按 URL 语义解码匹配，再重新编码输出；
- SQLite 路径继续只保留文件名，同时对 query 凭据执行同样的替换；
- URL 解析失败时也要清理可识别的 authority 和 query，不能回退输出原始凭据。

测试至少覆盖带用户名/密码的 PostgreSQL 或 MySQL URL、无密码的用户名 URL、URL 编码后的 authority 凭据、query 中的 `token` / `password` 及编码值、SQLite query，以及 doctor 输出不包含原始 secret。

运行时配置中也存在敏感值，但不属于本文的 `config.toml` 静态配置范围：

- `mail_smtp_password`
- 外部认证 provider secret
- offline download / aria2 secret
- 其他系统配置表中的 token 或 password 类字段

## 待讨论

- 是否给 Drive 自有的 `Config` / `AuthConfig` / `DatabaseConfig` 手写 `Debug`；共享 `CacheConfig` 的 redaction 需要在 `aster_forge_cache` 边界评估
- 是否移除静态配置类型上的 `Serialize`
- 默认生成 `config.toml` 时是否设置 Unix `0600` 权限
- 是否引入统一的 redaction helper，覆盖 URL、token、password、secret key
- 是否局部引入 `SecretString`，以及接受哪些调用点改造成本

## 候选实施顺序

1. 手写静态配置类型的 `Debug`，敏感字段固定显示 `<redacted>`
2. 为 `database.url` 和 `cache.redis_url` 增加 URL 凭据脱敏测试
3. 审查 CLI / doctor / health / startup 日志是否可能输出完整配置
4. 再决定是否引入 `secrecy`
