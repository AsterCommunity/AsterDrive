---
title: "生产上线检查清单"
---

:::tip[这一篇适合什么时候看]
准备把 AsterDrive 对外提供给真实用户前，按这页做最后一轮检查。这里不重复每个配置页的细节，只把上线前最容易漏掉的项集中到一张清单里。
:::

## 先看结论

| 检查项 | 通过标准 | 继续看 |
| --- | --- | --- |
| 数据目录 | `config.toml`、数据库、本地存储目录和头像目录都已持久化 | [备份与恢复](/deployment/backup/) |
| 访问入口 | 用户访问的是 HTTPS，反向代理保留必要请求头和响应头 | [反向代理](/deployment/reverse-proxy/) |
| 公开站点地址 | `管理 -> 系统设置 -> 站点配置 -> 公开站点地址` 是真实 HTTPS 来源 | [系统设置](/config/runtime/#站点配置) |
| 登录安全 | `jwt_secret` 已固定，Cookie 仅 HTTPS 发送已开启 | [登录与会话](/config/auth/) |
| 存储路线 | 默认存储策略、默认策略组、用户/团队绑定都符合预期 | [存储策略](/config/storage/) |
| 邮件 | 注册激活、密码重置、邮箱改绑邮件能发通 | [邮件](/config/mail/) |
| 多实例配置同步 | 多实例共享数据库、Redis endpoint 和 topic，跨实例修改已验收 | [配置同步](/config/config-sync/) |
| 监控 | 如需 Prometheus，已按需编译 `metrics` feature，并限制 `/health/metrics` 访问来源 | [监控与 Grafana](/deployment/monitoring/) |
| 备份 | 至少完成一次备份，并知道恢复顺序 | [备份与恢复](/deployment/backup/) |
| 升级 | 知道当前版本来源，升级前已看更新日志 | [升级与版本迁移](/deployment/upgrade/) |

## 1. 数据必须能留下来

确认这些内容不会在容器重建、系统重启或升级时丢失：

- `data/config.toml`
- 数据库文件，或者外部数据库实例
- 默认本地存储目录
- 额外创建的本地 `local` 存储策略目录
- 头像目录，默认通常是 `data/avatar`
- 从节点的 `remote_storage_target_local_root`，如果你用了本地远程存储目标

如果使用 Docker bind mount，宿主机目录还要确认属主和权限符合容器运行用户。官方镜像默认 UID/GID 是 `10001:10001`。

:::caution[不要只备份数据库]
数据库只保存元数据。文件对象、头像、本地远程存储目标和对象存储状态也要纳入备份边界。只备份数据库，恢复后很可能出现文件记录还在、对象已经不在的情况。
:::

## 2. HTTPS 和公开入口

正式上线建议始终放在反向代理后面，由代理负责 TLS、HSTS、上传限制、WebDAV 方法透传和长连接。

反向代理至少要确认：

- 保留真实 `Host`
- 传递公网协议，例如 `X-Forwarded-Proto`
- 上传大小限制足够覆盖直传小文件和 WebDAV 写入
- WebDAV 方法没有被拦截
- 没有覆盖成不兼容的全站 CSP
- WOPI、分享页、预览和缩略图相关路径都能透传

上线前用真实域名访问一次，然后确认：

```text
管理 -> 系统设置 -> 站点配置 -> 公开站点地址
```

里面每一项都是用户真实访问的 HTTPS 来源，例如：

```text
https://drive.example.com
https://panel.example.com
```

如果你有多个公开域名，每个域名都登录一次，确认分享链接、WebDAV 地址和 WOPI 地址会使用当前域名或你预期的默认来源。

## 3. 登录与 Cookie

上线前确认：

- `auth.jwt_secret` 已经固定，不再使用临时随机值
- `管理 -> 系统设置 -> 认证与 Cookie -> 认证 Cookie 仅 HTTPS 发送` 已开启
- Access Token / Refresh Token 有效期符合你的安全策略
- 第一个管理员账号密码已经更换为正式密码
- 管理员账号不止一个，或者至少确认恢复流程里能重建管理员

如果你最初用纯 HTTP 引导过：

```toml
[auth]
bootstrap_insecure_cookies = true
```

切到 HTTPS 后，要在后台系统设置里把 Cookie 安全要求改回开启。这个 bootstrap 配置只影响系统设置第一次初始化，后续以后台系统设置为准。

## 4. 存储策略和策略组

至少确认这些事：

- 默认存储策略存在且测试通过
- 默认策略组存在且启用
- 新用户默认策略组符合预期
- 已有用户和团队绑定的策略组没有指向废弃策略
- 单文件大小上限、分片大小、用户配额和团队配额符合真实使用场景
- S3 / MinIO / R2 的 CORS、endpoint、bucket 和密钥已经测试过
- 远程节点已经接入、启用，并且有已应用的默认远程存储目标

如果你准备把生产流量切到新存储后端，不要直接改已有策略的 `base_path`、`bucket`、`endpoint` 或绑定的远程节点。更稳的做法是新建策略、迁移数据，再切换策略组。

## 5. 邮件和账号流程

如果你要开放公开注册、邮箱改绑或密码重置，邮件必须先发通。

上线前至少测试：

- SMTP 连接测试通过
- 测试邮件能收到
- 注册激活链接使用正确域名
- 密码重置链接使用正确域名
- 邮箱改绑链接使用正确域名
- 链接有效期和重发冷却时间符合预期

如果你不准备开放注册，可以关闭：

```text
管理 -> 系统设置 -> 用户管理 -> 允许公开注册新用户
```

## 6. WebDAV 和在线编辑

如果要给桌面客户端使用 WebDAV：

- 系统设置里的 WebDAV 总开关已开启
- WebDAV 前缀和反向代理路径一致
- 真实客户端能连接、列目录、上传、覆盖保存和删除
- 客户端使用的是 WebDAV 专用账号，不是普通网页登录密码
- 如果要访问团队空间，使用团队空间创建的 WebDAV 账号，并确认成员权限符合预期

如果要接在线预览或 WOPI：

- `公开站点地址` 能被外部 Office / WOPI 服务访问
- `预览应用` 已启用对应打开方式
- WOPI Discovery 地址可访问
- 真实 `docx` / `xlsx` / `pptx` 能打开
- 保存后能写回 AsterDrive，并生成历史版本
- 反向代理没有拦截 WOPI 回调路径

完整接入流程见 [在线预览与 WOPI](/guide/preview-and-wopi/)。

## 7. 备份和恢复演练

上线前至少做一次备份，并确认你知道恢复顺序。

最少要覆盖：

- `data/config.toml`
- 数据库
- 本地存储目录
- 头像目录
- 额外本地策略目录
- 对象存储的 bucket 版本化、复制或外部备份策略

建议做一次小规模恢复演练：找一个测试环境恢复备份，启动后跑一次 `aster_drive doctor`，再用真实账号上传、下载、分享、恢复回收站项目各测试一次。

## 8. 日志、审计和限流

正式环境建议确认：

- 运行日志能被 Docker 日志、journald 或文件采集系统收走
- `RUST_LOG` 没有误设成过于详细的级别
- 审计日志已按你的需求开启
- 审计日志保留时间符合容量和合规要求
- 公开入口的限流策略符合站点规模
- 反向代理层和 AsterDrive 的限流边界没有互相冲突

如果站点对公网开放，建议至少开启认证、公开分享和写入类路径的限流。

## 9. Prometheus 指标

Prometheus 指标是按需编译能力，默认构建不启用。需要上线采集时，使用：

```bash
cargo build --release --features metrics
```

或者使用包含全部可选能力的 `full` feature。

上线前确认：

- `/health/metrics` 能被采集端访问
- `/health/metrics` 没有直接暴露到公网
- 反向代理、防火墙或安全组只允许监控系统来源访问
- scrape interval、保留周期和采集目标已经明确
- 至少能看到 HTTP、DB、上传、下载、后台任务、存储驱动和进程 RSS / CPU / uptime 指标

AsterDrive 当前不对 `/health/metrics` 做应用层鉴权。这个选择是为了让 Prometheus scrape 保持简单稳定，访问控制交给反向代理或网络边界处理。

Grafana dashboard 和本地 Prometheus + Grafana 示例见 [监控与 Grafana](/deployment/monitoring/)。

## 10. 多实例配置同步

如果只运行一个 AsterDrive 进程，保持 `[config_sync].backend = "disabled"`。

如果运行多个实例，确认：

- 所有 primary 实例显式设置 `[deployment].profile = "cluster"`
- 所有实例连接同一份权威数据库
- 所有实例都能访问配置的 Redis endpoint
- 所有实例使用完全相同的 `aster_drive.config_reload` topic
- 所有 storage policy 使用共享存储，不使用彼此独立的 `local` 根目录
- remote node 使用 `direct` transport；当前 reverse tunnel 仅用于 single profile
- 在实例 A 修改系统设置后，实例 B 能及时看到变化
- Redis 中断和恢复后的处理流程已经演练

完整部署契约见 [部署模式](/config/deployment/)，配置通知和故障语义见 [配置同步](/config/config-sync/)。

## 11. 最后一轮验收

上线前用真实域名、真实账号、真实客户端跑一遍：

1. `/health` 返回 200
2. `/health/ready` 返回 200
3. 如果启用了 metrics，受限来源访问 `/health/metrics` 返回 Prometheus 文本
4. 登录、刷新页面、退出登录都正常
5. 上传一个小文件和一个大文件
6. 下载文件，预览图片或 PDF
7. 创建分享链接，用无登录浏览器访问
8. 删除文件，再从回收站恢复
9. 如果启用了 WebDAV，用真实客户端读写一次
10. 如果启用了 WOPI，用真实 Office 文件打开并保存一次
11. 到 `管理 -> 任务` 看最近有没有持续失败任务
12. 到 `管理 -> 审计日志` 看关键操作是否按预期记录
13. 跑一次 `aster_drive doctor`

## 上线后第一天重点看什么

- 错误日志里是否持续出现同类 4xx / 5xx
- 上传失败是否集中在某个策略组或存储后端
- 邮件队列是否有持续失败
- 缩略图和在线解压任务是否堆积
- 公开分享访问是否使用正确域名
- WebDAV 客户端是否频繁留下锁
- 数据目录和临时目录磁盘占用是否符合预期
