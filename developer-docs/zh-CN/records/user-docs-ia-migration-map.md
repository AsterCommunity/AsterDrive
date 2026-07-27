# 用户文档 IA 重构迁移地图

> 状态：执行中工作文档。每一批次 PR 完成后回来勾选对应页面行。新读者请先读 issue 的审计发现，再用本文执行迁移。

本文是 issue [#435](https://github.com/AsterCommunity/AsterDrive/issues/435) Phase 1 的产出：用户文档全量页面盘点与处置决策、概念到权威页面映射、新旧 URL 重定向规则和中英文同步策略，是 Phase 2-5 各批次迁移的执行依据，随批次落地持续更新。

## 1. 基线状态（Phase 0 结论）

- `bun run docs:build` 当前退出码 0，starlight-links-validator 通过，139 个 HTML 页面生成成功。issue 中记录的 88 个无效链接是审计环境缺少 workspace install 导致的内容条目缺失，不是内容问题。
- 链接验证、Pagefind、sitemap、llms.txt、双语路由均正常。后续每个迁移批次都必须保持这个基线为绿。
- 已知真实内容漂移（不是死链，但属于 Phase 各批次顺手修的范围）：
  - ~~`deployment/monitoring.md` 中英标题层级数不一致（zh 7 / en 6）~~ ✅ 2-c 已修（en 补"压测期间的指标口径"一节，现 233/233 行对齐）
  - ~~`deployment/performance-benchmarking.md` 中英差约 77 行（zh 277 / en 200）~~ ✅ 2-c 已修（en 补基准范围 8 项、环境变量 3 项、k6 命令、后台任务混合负载和对象存储两整节，现 277/277 行对齐）
  - ~~`deployment/capacity-planning.md` 中英差 21 行~~ ✅ 2-c 已修（en 补内存一节 k6 混合负载验证块，现 295/295 行对齐；zh 顺带清理一处误留的称呼语）
  - ~~中文 40 页、英文 40 页缺显式 `description`~~ ✅ Phase 5 已全部补齐（迁移页随批次补齐，剩余 12×2 页统一补）

## 2. 目标信息架构与 URL 前缀

| 分区 | 前缀 | 目标读者 | 职责 |
| --- | --- | --- | --- |
| 开始使用 | `/start/` | 新用户、新部署者 | 认识产品、5 分钟试用、选部署方案、首次管理员初始化 |
| 使用 AsterDrive | `/using/` | 普通用户 | 文件、上传下载、团队、分享、回收站版本、预览编辑、WebDAV、账号安全 |
| 管理实例 | `/admin/` | 实例管理员 | 用户团队、注册登录 SSO、邮件、存储与策略组、存储后端、远程节点、预览处理、审计 |
| 部署 | `/deploy/` | 部署者 | 六种场景主路径：单实例 Docker、单实例 systemd、多实例、Kubernetes、Follower 节点、反向代理 |
| 运维 | `/ops/` | 运维者 | 验收、监控日志、备份恢复、升级回滚、容量、排障、命令参考 |
| 参考 | `/reference/`（沿用） | 所有角色 | 配置字段、能力矩阵、协议兼容边界、错误码、术语、关于 |

规则：

- 场景指南只写"要完成什么、按什么顺序、如何验收"，字段细节链接到 `/reference/`。
- 配置参考只写字段、默认值、环境变量、是否需重启，不写完整部署流程。
- 每个核心概念只有一个权威页面（见第 4 节），其余页面用场景摘要加链接。
- 开发者内容（Rust 模块、service ownership、源码定位）不在用户站出现，由本开发者站承接。

## 3. 全量页面盘点与处置决策

处置标记：**保留**＝内容和职责基本不变；**改造**＝保留主体但调整职责或归属；**收缩**＝删重复内容改为入口/索引；**拆分**＝按读者或职责拆到多个新页；**合并**＝并入另一权威页；**迁移**＝移到开发者站；**删除**＝内容被新结构吸收后删页重定向。

### 3.1 `guide/`（14 页）

| 页面 | 行数 | 目标读者 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- | --- |
| `guide/index.md` ✅ | 68 | 全角色 | 使用指南分区索引 | 收缩 | 首页 `/` 承担分流，本页重定向到 `/start/` |
| `guide/getting-started.md` ✅ | 208 | 新部署者 | 首次跑通全流程 | 保留 | `/start/quick-trial/`；部署细节让位给 `/deploy/` 场景页 |
| `guide/installation.md` ✅ | 45 | 新部署者 | 部署方式选择 | 改造 | `/start/choose-deployment/`；扩写为按规模和环境的分流表 |
| `guide/user-guide.md` ✅ | 468 | 普通用户 | 全功能手册（第二套文档） | 拆分 | 收缩为 `/using/` 分区首页（心智模型+入口）；各节流向对应任务页 |
| `guide/core-workflows.md` ✅ | 154 | 全角色 | 常用流程合集 | 改造 | `/start/common-workflows/`；每条流程只留摘要+权威页链接，不复制步骤 |
| `guide/webdav.md` ✅ | 170 | 普通用户 | WebDAV 用法与协议边界 | 保留 | `/using/webdav/`；限制速查与 `/reference/webdav-compat/` 合并为一份权威 |
| `guide/admin-console.md` ✅ | 421 | 管理员 | 按后台菜单复述全部管理功能（第二套文档） | 收缩 | `/admin/` 分区首页（后台菜单地图）；重复说明删除并链接到各管理场景页 |
| `guide/remote-nodes.md` ✅ | 364 | 管理员 | 远程节点概念+接入全流程 | 拆分 | 概念与接入决策 → `/admin/follower-nodes/`；部署操作 → `/deploy/follower-node/`。✅ 3-b 已迁移到 `/admin/follower-nodes/`（zh+en） |
| `guide/custom-frontend.md` ✅ | 218 | 管理员 | 自定义前端机制与 API | 改造 | `/admin/custom-frontend/` |
| `guide/editing.md` ✅ | 139 | 普通用户 | 浏览器编辑与版本 | 合并 | 与 `guide/preview-and-wopi.md` 用户部分合为 `/using/preview-editing/` |
| `guide/preview-and-wopi.md` ✅ | 273 | 用户+管理员 | 预览方式、WOPI 接入 | 拆分 | 用户向 → `/using/preview-editing/`；管理员接入配置 → `/admin/preview-processing/` |
| `guide/sharing.md` ✅ | 132 | 普通用户 | 分享与公开访问 | 保留 | `/using/sharing/` |
| `guide/teams-and-permissions.md` ✅ | 168 | 用户+管理员 | 团队空间、角色、管理边界 | 拆分 | 用户向 → `/using/workspaces-teams/`；管理向 → `/admin/users-teams/` |
| `guide/upload-modes.md` ✅ | 142 | 用户+管理员 | 上传模式、大文件、按后端选择 | 拆分 | 用户向 → `/using/upload-download/`；部署准备 → 链接到 `/deploy/` 与 `/admin/storage-backends/` |

### 3.2 `config/`（15 页）

| 页面 | 行数 | 目标读者 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- | --- |
| `config/index.md` ✅ | 172 | 部署者+管理员 | 三层配置心智模型 | 保留 | `/reference/config/` 分区导览 |
| `config/deployment.md` ✅ | 57 | 部署者 | single/cluster 字段与前置 | 改造 | 字段 → `/reference/config/deployment/`；cluster 前置正文由 `/deploy/multi-instance/` 引用，不复制 |
| `config/server.md` ✅ | 162 | 部署者 | 服务器字段+常见写法 | 保留 | `/reference/config/server/` |
| `config/database.md` ✅ | 125 | 部署者 | 数据库字段与后端选择 | 保留 | `/reference/config/database/` |
| `config/cache.md` ✅ | 68 | 部署者 | 缓存字段 | 保留 | `/reference/config/cache/` |
| `config/config-sync.md` ✅ | 175 | 部署者 | 多实例配置同步 | 保留 | `/reference/config/config-sync/`；多实例场景引用 |
| `config/logging.md` ✅ | 74 | 运维 | 日志字段 | 保留 | `/reference/config/logging/` |
| `config/rate-limit.md` ✅ | 114 | 管理员 | 限流字段与边界 | 保留 | `/reference/config/rate-limit/` |
| `config/webdav.md` ✅ | 127 | 管理员+用户 | WebDAV 静态+运行时配置 | 拆分 | 字段 → `/reference/config/webdav/`；用户用法已在 `/using/webdav/`；协议边界 → `/reference/webdav-compat/`（3-c 补齐） |
| `config/runtime.md` ✅ | 483 | 管理员 | 系统设置全域参考（超长页） | 拆分 | 按域拆为 `/reference/config/runtime/` 下站点/用户/认证/邮件/网络/运行时/保留/文件处理/WebDAV/审计子页；操作教程流向 `/admin/` 各场景页 |
| `config/auth.md` ✅ | 341 | 管理员 | 密钥字段+登录+MFA+Passkey+注册 | 拆分 | 静态字段 → `/reference/config/auth/`；首次管理员 → `/start/first-admin/`；MFA/Passkey/注册开关 → `/admin/auth-sso/` |
| `config/external-auth.md` ✅ | 366 | 管理员 | SSO 场景+字段+FAQ | 拆分 | 场景教程 → `/admin/auth-sso/`；字段与 FAQ → `/reference/config/external-auth/` |
| `config/mail.md` ✅ | 133 | 管理员 | 邮件配置 | 改造 | 场景 → `/admin/mail/`；字段表 → `/reference/config/runtime/mail/`（偏差：不单设 `/reference/config/mail/`，字段并入 runtime 邮件投递子页） |
| `config/storage.md` ✅ | 237 | 管理员 | 存储策略与策略组概念 | 改造 | 概念权威 → `/admin/storage-policies/`；类型字段细节 → `/reference/storage-matrix/` |
| `config/offline-download.md` ✅ | 148 | 用户+管理员 | 离线下载行为与引擎配置 | 拆分 | 用户行为 → `/using/upload-download/`；引擎配置 → `/admin/offline-download/` |

### 3.3 `deployment/`（18 页）

| 页面 | 行数 | 目标读者 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- | --- |
| `deployment/index.md` ✅ | 142 | 部署者 | 部署概览 | 改造 | `/deploy/` 分区首页＝方案选择枢纽，与 `/start/choose-deployment/` 分工：start 负责"选"，deploy 负责"走" |
| `deployment/docker.md` ✅ | 200 | 部署者 | Docker 部署 | 改造 | `/deploy/docker/`；补验收清单与运维入口链接，成为完整场景页 |
| `deployment/systemd.md` ✅ | 147 | 部署者 | systemd 部署 | 改造 | `/deploy/systemd/`；同上 |
| `deployment/docker-follower.md` ✅ | 276 | 部署者 | Docker follower 部署 | 合并 | 并入 `/deploy/follower-node/` 场景页 |
| `deployment/follower-network-topologies.md` ✅ | 189 | 部署者 | follower 五种网络方式 | 合并 | `/deploy/follower-node/network/` 子页 |
| `deployment/kubernetes.md` ✅ | 119 | 部署者 | K8s manifest | 改造 | `/deploy/kubernetes/`；明确引用 `/deploy/multi-instance/` 契约，不复制 |
| `deployment/load-balancing.md` ✅ | 121 | 部署者 | 多实例契约 | 保留 | `/deploy/multi-instance/`；multi-primary 契约唯一权威页 |
| `deployment/reverse-proxy.md` ✅ | 371 | 部署者 | 反代配置+CSP+验收 | 改造 | `/deploy/reverse-proxy/`；排障内容流向 `/ops/troubleshooting/` |
| `deployment/runtime-behavior.md` ✅ | 110 | 运维 | 首次启动检查 | 迁移 | `/ops/first-check/` |
| `deployment/production-checklist.md` ✅ | 234 | 运维 | 上线检查清单 | 迁移 | `/ops/launch-checklist/` |
| `deployment/monitoring.md` ✅ | 233 | 运维 | Prometheus/Grafana | 迁移 | `/ops/monitoring/`；顺手修中英标题漂移 |
| `deployment/capacity-planning.md` ✅ | 295 | 运维 | 容量估算 | 迁移 | `/ops/capacity/` |
| `deployment/backup.md` ✅ | 176 | 运维 | 备份恢复 | 迁移 | `/ops/backup/` |
| `deployment/upgrade.md` ✅ | 161 | 运维 | 升级与版本迁移 | 迁移 | `/ops/upgrade/`；吸收 `frontend-assets.md` |
| `deployment/troubleshooting.md` ✅ | 258 | 运维 | 症状排障 | 迁移 | `/ops/troubleshooting/` |
| `deployment/ops-cli.md` ✅ | 370 | 运维 | doctor/离线配置/enroll/迁移四任务 | 拆分 | `/ops/cli/` 命令参考；各任务从对应场景页链入 |
| `deployment/frontend-assets.md` ✅ | 52 | 运维 | 浏览器缓存与升级 | 合并 | 并入 `/ops/upgrade/` |
| `deployment/performance-benchmarking.md` ✅ | 277 | 运维 | 压测方法 | 迁移 | `/ops/capacity/benchmarking/`；顺手修中英 77 行漂移 |

### 3.4 `storage/`（8 页）

| 页面 | 行数 | 目标读者 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- | --- |
| `storage/index.md` ✅ | 55 | 管理员 | 后端教程索引 | 改造 | `/admin/storage-backends/`；扩写为后端选择指南+存储能力矩阵（唯一权威） |
| `storage/local.md` ✅ | 194 | 管理员 | local 教程 | 保留 | `/admin/storage-backends/local/` |
| `storage/s3-minio-r2.md` ✅ | 459 | 管理员 | S3 族教程 | 保留 | `/admin/storage-backends/s3/`；公共概念抽出后只留差异 |
| `storage/azure-blob.md` ✅ | 349 | 管理员 | Azure 教程 | 保留 | `/admin/storage-backends/azure-blob/`；同上 |
| `storage/tencent-cos.md` ✅ | 380 | 管理员 | COS 教程 | 保留 | `/admin/storage-backends/tencent-cos/`；同上 |
| `storage/onedrive.md` ✅ | 344 | 管理员 | OneDrive 教程 | 保留 | `/admin/storage-backends/onedrive/`；同上 |
| `storage/sftp.md` ✅ | 166 | 管理员 | SFTP 教程 | 保留 | `/admin/storage-backends/sftp/`；同上 |
| `storage/remote-follower.md` ✅ | 378 | 管理员 | remote 策略教程 | 保留 | `/admin/storage-backends/remote-follower/`；与 `/deploy/follower-node/` 互相链接，不复制接入步骤 |

七篇后端教程共用结构（分层说明→准备→relay_stream/presigned→建策略→测试策略组→绑定→验收），重复部分抽到 `/admin/storage-backends/` 分区首页，教程只讲该后端差异。

### 3.5 `features/`（6 页）

| 页面 | 行数 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- |
| `features/index.md` ✅ | 45 | 功能地图索引+模块速查 | 删除 | 用户向分流由 `/using/`、`/admin/` 首页承担；模块速查已在本站 `architecture/` |
| `features/auth-access.md` ✅ | 48 | 身份访问功能地图 | 删除 | 重定向到 `/admin/auth-sso/` |
| `features/files-workspaces.md` ✅ | 44 | 文件工作空间功能地图 | 删除 | 重定向到 `/using/` |
| `features/upload-storage.md` ✅ | 49 | 上传存储功能地图 | 删除 | 重定向到 `/admin/storage-backends/` |
| `features/preview-processing.md` ✅ | 45 | 预览处理功能地图 | 删除 | 重定向到 `/admin/preview-processing/` |
| `features/runtime-operations.md` ✅ | 48 | 系统运维功能地图 | 删除 | 重定向到 `/ops/` |

执行前提：先确认每页的"后端模块/数据边界"内容在本站 `architecture/`、`design/` 已有对应，没有的先补到本站再删页。

### 3.6 `reference/`（7 页）与首页

| 页面 | 行数 | 当前职责 | 处置 | 目标位置 |
| --- | --- | --- | --- | --- |
| `reference/index.md` ✅ | 15 | 参考分区索引 | 保留 | `/reference/` |
| `reference/architecture.md` ✅ | 309 | 运行模型+内部实现混合 | 拆分 | 运行模型（primary/follower、数据流、配置边界）→ `/reference/runtime-architecture/`；源码模块部分链接到本站 |
| `reference/faq.md` ✅ | 54 | FAQ | 保留 | `/reference/faq/` |
| `reference/glossary.md` ✅ | 54 | 术语表 | 保留 | `/reference/glossary/` |
| `reference/errors.md` ✅ | 433 | 错误码 | 保留 | `/reference/errors/`；错误码唯一权威 |
| `reference/docs-contributing.md` ✅ | 194 | 文档贡献 | 迁移 | 本站 `contributing/documentation.md`（zh+en 已迁移并更新 IA 表述；跨站重定向为手动 redirects 条目） |
| `reference/about.md` ✅ | 139 | 关于与定位 | 保留 | `/reference/about/`；产品能力边界表述的唯一权威 |
| `index.mdx` ✅ | 74 | 首页 | 改造 | 按五大目标分流；~~删除"不是多主集群系统"表述~~ ✅ 已替换为与 `reference/about.md` 一致的多 Primary 边界表述 |

## 4. 概念 → 权威页面表

| 概念 | 唯一权威页 | 当前主要重复位置 | 其他页面的写法 |
| --- | --- | --- | --- |
| 公开站点地址 `public_site_url` | `/reference/config/` 字段页（语义） | 30+ 页（reverse-proxy、mail、preview-and-wopi、remote-nodes、getting-started、7 篇存储教程等） | 只写"本场景要求它可达/必须是 HTTPS"，链接权威页 |
| 认证密钥（`jwt_secret` 等 6 个 secret） | `/reference/config/auth/` | config/auth、config/index、docker、production-checklist、runtime-behavior、load-balancing | 只写"多实例必须固定同一值"等场景条件 |
| 存储策略 vs 策略组 | `/admin/storage-policies/` | config/storage、admin-console、7 篇存储教程 | 教程直接进入"创建策略"步骤，概念链接权威页 |
| 存储能力矩阵（7 后端 × 直传/去重/原生处理等） | `/reference/storage-matrix/` | storage/index、config/storage、各后端教程、README | 教程只讲"本后端支持 X" |
| `relay_stream` vs `presigned` | `/admin/storage-backends/` 分区首页 | 7 篇存储教程、upload-modes、follower-network-topologies、admin-console | 教程只讲本后端的开启条件和差异 |
| multi-primary / cluster 契约 | `/deploy/multi-instance/` | config/deployment、kubernetes、load-balancing、about、首页 | 只写场景前置并链接 |
| follower（概念、enroll、生命周期） | `/deploy/follower-node/` | remote-nodes、docker-follower、remote-follower、ops-cli、config/server | 各页只讲自己环节并链接 |
| WebDAV 协议兼容边界 | `/reference/webdav-compat/` | guide/webdav、config/webdav、reverse-proxy、errors | 用户向 `/using/webdav/` 只写用法并链接 |
| WOPI 接入边界 | `/admin/preview-processing/` | preview-and-wopi、editing、custom-frontend、reverse-proxy | 用户向只写"打开方式从哪来" |
| 上传模式（direct/chunked/presigned） | `/using/upload-download/` | upload-modes、user-guide、webdav、troubleshooting | 排障页只按症状链回 |
| 分享模型 | `/using/sharing/` | sharing、user-guide、core-workflows、admin-console | 管理页只写管理动作 |
| 版本与回收站 | `/using/trash-versions/` | user-guide、editing、teams-and-permissions | 编辑页只写"何时产生版本" |
| 错误码 | `/reference/errors/` | troubleshooting、faq | 排障页按症状链接错误码条目 |
| 产品能力边界（含 multi-primary 准确表述） | `/reference/about/` | 首页、README、about、load-balancing | 其他页面引用同一句边界描述 |

## 5. 新旧 URL 重定向设计

机制：Astro `redirects` 配置（静态输出时为每个旧路由生成 meta refresh + canonical 页面），本开发者站 `astro.developer.config.mts` 的 `movedRoutes` 是现成先例。用户站在 `astro.config.mts` 中以同样的 `movedRoutes` 映射表集中维护。

规则：

1. 所有现有公开 URL 要么保留，要么在 `movedRoutes` 中有明确条目；禁止静默 404。
2. 中英文成对：`/guide/user-guide/` → `/using/` 的同时必须配 `/en/guide/user-guide/` → `/en/using/`。
3. 重定向至少保留两个 minor 版本，确认搜索收录更新后再评估是否清理。
4. 映射表随各批次 PR 同步更新：该批次移动的页面，同 PR 内加好重定向，不允许"先移后补"。
5. 带 heading anchor 的外部链接无法被重定向保留，迁移时检查互链页面的 anchor 引用并同步更新。

映射表按第 3 节各表的"目标位置"列生成；每批次 PR 在本文对应表格勾选完成状态。

## 6. 中英文同步策略

1. **同一变更单元**：每个迁移批次 PR 必须同时包含对应中文和英文页面的改动，不允许"先改完中文再补英文"。
2. **顺手修已知漂移**：`deployment/monitoring.md`、`deployment/performance-benchmarking.md` 的中英结构漂移随 Phase 2 运维批次一并修复。
3. **description 补齐**：中文 40 页、英文 40 页缺显式 `description`。页面迁移或改造时同 PR 补齐；未迁移页面在 Phase 5 统一补。
4. **PR 检查清单**（每个迁移批次 PR 必须过）：
   - [ ] 中英文页面成对变更
   - [ ] 新页面和改造页面有显式 `description`
   - [ ] 旧 URL 重定向已加入 `movedRoutes`（双语成对）
   - [ ] 站内互链已指向新 URL（不依赖重定向兜底）
   - [ ] `bun run docs:build` 退出码 0
   - [ ] 本迁移地图对应行已勾选

## 7. 执行批次（对应 issue Phase 2-5）

| 批次 | 范围 | 依赖 |
| --- | --- | --- |
| Phase 2-a ✅ | `/deploy/` 分区骨架 + 单实例 Docker、systemd 场景页 | 本文第 2、5 节 |
| Phase 2-b ✅ | 多实例、Kubernetes、Follower 场景页 | 2-a 的契约页就位 |
| Phase 2-c ✅ | `/ops/` 分区（验收/监控/备份/升级/容量/排障/CLI） | 2-a |
| Phase 3-a ✅ | `/start/` + `/using/` 分区，拆 user-guide | 概念表就位 |
| Phase 3-b ✅ | `/admin/` 分区（含 storage-backends 迁入、auth/mail/offline-download 场景页、`/start/first-admin/`）；runtime/auth/external-auth 字段拆分留 3-c | 3-a |
| Phase 3-c ✅ | `/reference/config/` 字段参考重组（runtime 拆 10 子页）+ `/reference/storage-matrix/` + `/reference/webdav-compat/`（ops-cli 拆分已在 2-c 完成） | 3-b |
| Phase 4 ✅ | features/ 删除迁移、architecture 拆分、docs-contributing 迁本站 | 先核对本站覆盖度 |
| Phase 5 ✅ | 产品事实对齐（首页/README/about/能力矩阵）、description 补齐、双语终查 | 全部批次完成 |
