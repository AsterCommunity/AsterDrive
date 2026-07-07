# 后端服务所有权边界

本文记录 AsterDrive 后端在 0.3 系列继续拆分 service 时应遵守的所有权边界。

这里描述的是当前仓库的工程边界，不是抽象分层理论。判断一个改动应该放在哪里时，先从产品语义出发：文件、工作空间、上传、存储策略、远端节点、remote storage target、WebDAV/WOPI 分别是谁的职责。

## 快速规则

后端主链路仍然是：

```text
src/api/routes/*
  -> src/services/*
  -> src/services/<domain>/*
  -> src/db/repository/* / src/storage/* / src/webdav/*
```

如果一个函数同时做了协议解析、业务校验、数据库写入、driver 构造、远端协议调用、UI descriptor 拼装和 audit/registry reload，那就不是“功能完整”，是边界已经混了。继续加代码前先拆出当前改动真正需要的职责。

## 层级所有权

### Route 层：协议适配

对应目录：

- `src/api/routes/*`
- `src/api/primary.rs`
- `src/api/follower.rs`

route 层只负责接入形态：

- HTTP path/query/body/header 提取
- JWT、admin、team、internal storage、WOPI token 等 guard
- DTO 到 service input 的轻量转换
- 调用 service
- REST envelope、文件流、SSE、Prometheus、WebDAV/WOPI 兼容响应映射

route 层不应该拥有：

- 存储策略选择规则
- 上传完成、配额、版本、blob 引用计数一致性
- driver capability 判断
- remote node / remote storage target target 选择
- UI 表单字段矩阵
- 数据库事务编排

例外是协议格式本身必须在 route 层保持可见，例如 WOPI 的 header 映射、internal storage 的 HTTP status 兼容、文件下载的 Range / conditional request 响应头。但这些协议细节不能反过来决定产品行为。

### Service 层：use case 编排

对应目录：

- `src/services/*`

service 的公共入口负责一个完整的产品用例，例如：

- 初始化上传、上传分片、完成上传、取消上传
- 创建文件、移动文件、下载文件、生成预览资源
- 创建存储策略、测试连接、执行策略 action
- 创建远端节点、测试远端节点、同步绑定
- 创建 WOPI 启动会话、处理 WOPI 文件写回

service 可以做：

- 加载 use case 所需上下文
- 调用 domain helper 做 normalize / validate / resolve
- 调用 repo 做读写
- 调用 storage driver 或 remote protocol 完成必要副作用
- 在明确位置触发 audit、storage change、cache invalidation、policy snapshot reload、driver registry reload、task 创建
- 把领域结果整理成对 route 稳定的返回类型

service 不应该变成：

- driver registry 的替代实现
- remote protocol 的 wire parser
- repo 层 SQL 的堆放处
- 前端 descriptor 规则的隐藏来源
- 多个产品域互相调用内部 helper 的零件仓库

### Domain helper：可测试的业务规则

对应形态：

- `src/services/<domain>/normalization.rs`
- `src/services/<domain>/driver.rs`
- `src/services/<domain>/paths.rs`
- `src/services/<domain>/complete/plan.rs`
- `src/services/<domain>/scope.rs`
- `src/services/<domain>/targets.rs`

domain helper 承载可复用、可测试、尽量少依赖 `AppState` 的规则：

- 输入 normalization
- 路径解析与防 traversal
- upload completion plan
- capability resolver
- descriptor builder
- target selection
- finalization contract
- protocol-independent conflict / lock / rename 规则

如果规则可以写成纯函数，就不要让它依赖 `PrimaryAppState` 或 `FollowerAppState`。如果它必须读数据库，也要让读取边界明确，例如传入 repo 查出的 model 或显式传入 `ConnectionTrait`。

### Repository 层：数据访问和原子 SQL

对应目录：

- `src/db/repository/*`

repo 只负责数据库事实：

- 按 ID / scope / token / binding 查询
- 分页、排序、聚合计数
- 事务内 create / update / delete
- 跨数据库兼容 SQL
- 原子计数、锁行、唯一约束冲突处理

repo 不应该知道：

- HTTP、WebDAV、WOPI 或 internal storage 协议
- storage driver 类型如何构造
- remote storage target 应该显示哪些字段
- 上传模式如何协商
- 用户界面如何展示 diagnostic
- policy target 应该由哪个页面创建

如果 repo 函数需要知道 `DriverType::S3` 才能决定流程，先停下。这个判断通常属于 storage connector、policy service、upload service 或 domain helper。

### Storage connector / driver 层：对象内容能力

对应目录：

- `src/storage/connectors/*`
- `src/storage/drivers/*`
- `src/storage/traits/*`

storage 层拥有：

- driver / connector descriptor
- 连接测试 action
- credential / application config 处理
- upload transport capability
- presigned、multipart、streaming、range read 等对象内容能力
- storage capacity / metadata / delete / compose 能力

storage 层不拥有：

- 用户、团队、文件夹、分享权限
- 工作空间配额归属
- storage policy group 优先级
- remote node 页面或 policy 页面如何组织 target
- audit 文案
- REST envelope

### Remote protocol 层：wire protocol

对应目录：

- `src/storage/remote_protocol/*`
- `src/storage/remote_protocol/tunnel/*`

remote protocol 只拥有主从节点之间的 wire contract：

- internal auth 签名和预签名 query/header 常量
- HTTP / reverse tunnel transport
- remote storage request / response model
- capability wire model
- path encoding
- response parsing
- protocol version fallback

remote protocol 不应该决定：

- 某个 remote node 是否应该作为默认 storage target
- policy 创建时选择哪个 follower-side target
- remote storage target descriptor 的 UI 呈现
- 产品级错误是否应该阻止策略变更

这些决定属于 `remote::remote_node`、`remote::storage_target`、`policy_service` 或后续抽出的 capability / target resolver。

### WebDAV / WOPI 协议接入

对应目录：

- `src/webdav/*`
- `src/api/routes/wopi.rs`
- `src/services/webdav_service.rs`
- `src/services/wopi_service/*`

WebDAV 和 WOPI 是协议入口，不是 REST 文件接口的普通变体。

它们应该：

- 保持协议要求的状态码、header、lock、ETag、Range、PUT_RELATIVE、rename、proof/token 行为
- 复用 AsterDrive 的文件、文件夹、workspace scope、storage、quota、audit、storage change 语义
- 在协议边界做兼容映射，不污染全局 REST envelope

它们不应该：

- 绕过 `workspace::storage` / `workspace::storage_core` 自己写一套文件落账
- 绕过 `file` / `folder` 自己维护 blob 引用、版本、回收站语义
- 把 WOPI/WebDAV 专用错误格式塞回通用 service 错误模型

## 什么时候必须拆模块

遇到下面信号时，先拆 domain helper 或子模块，不要继续往当前函数里堆：

- 一个 service 函数同时出现 `AppState`、repo 写入、remote protocol client、driver 构造、descriptor 拼装、audit、registry reload
- `match driver_type` 在业务 service 中反复出现，而不是位于 connector、driver registry、capability resolver 或 remote-storage-target 专用 registration
- 同一个函数既做输入 normalize，又做远端 capability 判断，又做 DB side effect
- 为了前端表单展示在 use case 函数里拼字段矩阵
- 上传完成路径散在多个入口里，导致 quota、blob、file version、cleanup、audit 不能被一次性审查
- 函数超过约 80-120 行，但没有清晰的“加载上下文 -> 校验 -> 写入 -> 副作用 -> 返回”结构

推荐形状：

```rust
pub async fn create_xxx(state, input) -> Result<Output> {
    let input = normalize(input)?;
    let context = load_context(state, &input).await?;
    validate(&context, &input)?;
    let result = repo::create(...).await?;
    run_required_side_effects(state, &result).await?;
    Ok(present(result))
}
```

## 责任清单

本清单用于 review 时快速判断“这段逻辑该不该留在当前 service”。它不是要求一次性重写。

### `upload_service`

当前职责：

- 上传入口 facade：direct multipart body、init、chunk、complete、cancel、progress、recoverable sessions、presign parts
- 把个人空间和团队空间请求映射到 `WorkspaceStorageScope`
- 根据策略和 driver capability 协商上传模式：direct、chunked、presigned single、presigned multipart、relay multipart、remote presigned
- 在 `complete/*` 下选择 completion plan，并把临时上传状态转为正式文件
- 处理上传级 metrics 和 route 级 audit wrapper

应该保留：

- 上传生命周期 use case 编排
- upload session 状态流转
- completion plan 选择
- upload mode 的对外响应模型
- cancel / cleanup / recoverable session 的统一入口

应该下沉或继续收口：

- finalization contract：trusted size、actual size、hash、blob、file version、quota charge、cleanup 必须形成可审查的统一契约
- 不同 completion path 对 `workspace::storage` 的调用差异，后续应尽量收敛到同一 finalization shape
- remote / object multipart 细节应留在 `init/remote.rs`、`complete/object_multipart.rs` 等子模块，不回流到 facade

必须显式的副作用：

- upload session 状态更新
- 临时对象、multipart upload、chunk 文件清理
- quota 落账
- metrics
- audit
- storage change / cache invalidation，如果某条上传路径触发文件变更

### `workspace::storage`

当前职责：

- 统一工作空间文件链路 facade
- 重新导出 scope helper、storage core、multipart/store/blob upload 能力
- 处理 REST direct upload、WebDAV flush、预上传 blob、multipart/staged/streaming direct 等入口
- 为文件创建、内容写入、临时文件持久化提供统一入口

应该保留：

- `WorkspaceStorageScope` 入口和 scope-aware 文件写入流程
- 不同上传入口到统一 storage core 的编排
- storage operation cancellation / cleanup 的边界
- 对 route、WebDAV、file service 暴露稳定 facade

应该下沉或继续收口：

- `mod.rs` 当前 re-export 面较宽，后续收窄时应先统计调用方，再把纯内部 helper 收回子模块
- connector upload transport 判断应优先保持在 `src/storage/connectors/*`，service 只消费结果
- WebDAV 专用写入路径不应在这里产生协议判断；只接收已经解析好的文件写入意图

必须显式的副作用：

- 存储对象写入和失败清理
- 事务外不可回滚副作用的补偿
- storage change
- audit 调用方传入的 actor attribution

### `workspace::storage_core`

当前职责：

- 工作空间文件主链路的稳定核心动作
- 策略解析、目录路径补齐、blob / file record 创建、upload session 最终落账、配额读写
- 上传方式无关、HTTP 接入无关的文件一致性底座

应该保留：

- `resolve_policy_for_size*`
- `check_quota` / `update_storage_used*`
- `create_*_file_from_blob*`
- `finalize_upload_session_*`
- `ensure_upload_parent_path`

不应该承担：

- HTTP multipart 解析
- WOPI/WebDAV 协议状态码
- remote protocol transport
- 前端 descriptor
- route 级 audit

必须显式的副作用：

- 数据库事务内的文件、blob、version、quota 一致性
- upload session 完成态和正式文件之间的绑定

### `policy_service`

当前职责：

- 存储策略和策略组的管理用例
- policy connection normalize / prepare / validate
- policy group 默认项和 assignment 迁移
- capacity info、连接测试、draft/saved action、S3-compatible driver promote
- 策略变更后的 policy snapshot reload、公开缩略图/媒体能力 cache invalidation
- 管理 audit wrapper

应该保留：

- storage policy / policy group 的产品语义
- default policy / default group 的一致性
- policy 是否可删除、是否被 blob/group/upload session 引用的校验
- 与 storage connector descriptor/action 的编排

应该下沉或继续收口：

- connector 字段 normalization 和 application config 持久化继续属于 `src/storage/connectors/*`
- remote target / remote storage target target 的产品选择规则不应塞进 remote node service；最终应由 policy 流程消费 capability / target resolver
- policy action diagnostic 的展示 shape 可以保留在 service model，但 driver-specific 细节应来自 connector/action

必须显式的副作用：

- policy snapshot reload
- driver registry 或 public capability cache invalidation
- upload session cleanup task 创建
- audit

### `remote::remote_node`

当前职责：

- primary 侧远端节点 CRUD、分页、enrollment status presentation
- base URL / transport mode normalization
- connection test、health test、capability probe
- remote binding sync
- remote protocol client 获取
- 远端节点变更后的 driver registry / policy snapshot reload 或 invalidation

应该保留：

- remote node 作为连接对象的生命周期
- enrollment / health / transport / capability probe
- direct / reverse tunnel 连接入口选择
- 判断节点是否已完成 enrollment

不应该承担：

- follower-side storage target 的产品归属
- storage policy UI 应该怎样创建 target
- remote storage target driver descriptor 的最终字段规则
- 远端对象存储路径和 blob/file finalization

应该下沉或继续收口：

- capability parsing 可以先保留，但 capability 到产品能力的解释应抽成 resolver，供 policy / remote storage target / UI descriptor 共同使用
- remote node presentation 不应继续扩大成 policy target presentation

必须显式的副作用：

- remote binding sync failure 的 warn
- registry reload / invalidate
- policy snapshot reload
- health test 写回 last capabilities / last error

### `remote::storage_target`

这是旧 issue 文案里 “managed ingress profile service” 在当前代码里的落点。少量 wire field 和错误码仍保留 `managed_ingress.*` 名称是兼容层；新的产品所有权应继续使用 remote storage target 语义。

当前职责：

- follower 侧 remote storage target CRUD
- primary 侧转发远端 target CRUD
- remote storage target driver descriptor / field descriptor
- local/S3 field normalization、path normalization、driver build / validation
- 默认 target 选择、revision apply 状态、effective target resolution

应该保留：

- remote storage target 作为 follower-side ingress target 的生命周期
- 目标 driver 的 remote-storage-target-specific registration
- target normalization / driver validation / effective target resolution
- primary 到 follower 的 remote forwarding facade

不应该承担：

- primary storage policy 的完整产品流程
- remote node 的连接生命周期
- 通用 storage connector registry 的替代品
- UI 自己推断出来的 driver capability 表

应该下沉或继续收口：

- `driver.rs` 继续作为 remote-storage-target-specific registration 层，不要和通用 connector registry 盲目合并
- `remote.rs` 当前同时做 capability filter 和 remote forwarding，后续 capability resolver 成熟后应只消费 resolver 结果
- 命名迁移已收敛到 remote storage target；后续只保留明确标注的旧 route、wire field 和 config alias 兼容层

必须显式的副作用：

- desired/applied revision 更新
- default target 替换约束
- driver registry reload / target validation
- remote forwarding failure 的协议错误映射

### `remote::master_binding`

当前职责：

- follower 侧主节点绑定 upsert / sync
- internal storage 请求鉴权：header signature、nonce、timestamp、content-length 参与签名
- presigned PUT/GET query 鉴权
- master binding 到 ingress target 的授权结果解析
- provider storage namespace / object key prefix
- follower ready 检查

应该保留：

- follower 对 primary 的信任关系
- internal storage auth 和 presigned auth 的产品级授权
- binding storage namespace 的路径隔离
- 调用 `storage_target::resolve_effective_target` 取得授权后的 ingress driver

不应该承担：

- 具体对象 PUT/GET/compose/list 的 HTTP 响应实现
- remote storage target CRUD
- storage policy 选择
- remote node 管理页面语义

应该下沉或继续收口：

- wire-level 签名常量和签名算法继续由 `remote_protocol` 暴露；service 只做授权 use case
- provider path 规则必须继续复用 object key normalization，不能在 route 中裸拼路径

必须显式的副作用：

- nonce cache 写入
- driver registry reload
- disabled binding / missing ingress target 的 precondition 错误

### `file`

当前职责：

- 文件元数据、内容更新、删除/永久删除、下载、缩略图、resource handle、lock、复制等 facade
- 个人空间和团队空间的 file-level use case
- audit wrapper
- 下载 outcome 构建：stream、range、conditional request、presigned redirect、inline sandbox
- 与 `workspace::storage` 共享文件写入和 scope 校验

应该保留：

- 文件资源的产品语义：读写、移动、锁定、删除、复制、下载、预览
- `DownloadOutcome` 和文件访问 outcome 到 route 响应的稳定桥接
- 文件级 audit details
- range / ETag / sandbox / disposition 的文件访问规则

不应该承担：

- 上传 session 生命周期
- storage policy 管理
- WebDAV/WOPI 的协议状态机
- remote internal storage wire protocol

应该下沉或继续收口：

- download 子模块应继续拥有 streaming / range / response 分离，主下载链路不能回退到全量缓冲
- resource handle 只解析文件资源可访问形态，不决定前端页面流程
- audit wrapper 留在 facade，核心文件 mutation 保持可被 WebDAV/WOPI 复用

必须显式的副作用：

- blob 引用计数和清理
- storage change
- share/cache invalidation
- audit
- download metrics / share download result 计数由调用链明确触发

### WebDAV integration

当前职责：

- `src/webdav/*` 处理 WebDAV / DeltaV 协议、Basic Auth、path resolver、lock、property、transfer
- `src/services/webdav_service.rs` 承接协议层需要复用的 folder tree soft delete、purge、copy 等产品动作
- WebDAV 文件写入通过 `workspace::storage` 的统一链路落账

应该保留：

- WebDAV 专用鉴权、路径解析、锁、属性、Depth、Range、DeltaV 行为
- 协议到 AsterDrive workspace/file/folder 语义的适配
- 对 `file` / `folder` / `workspace::storage` 的复用

不应该承担：

- 独立文件模型
- 独立上传完成 / quota / blob cleanup 规则
- REST envelope 或普通 API DTO

必须显式的副作用：

- WebDAV mutation 对 storage change、audit、share/cache invalidation 的触发
- folder tree purge 对 property、share、folder path cache 的清理

### WOPI integration

当前职责：

- `src/api/routes/wopi.rs` 处理 WOPI HTTP method/header/status 兼容
- `src/services/wopi_service/*` 处理 discovery、session、proof、lock、target、operations
- WOPI 文件读写复用 `file` / `workspace::storage`

应该保留：

- WOPI token/session、proof validation、discovery cache
- CheckFileInfo、GetFile、PutFile、PutRelative、Rename、Lock/Unlock/RefreshLock 的协议语义
- WOPI conflict 和 invalid-name 结果模型

不应该承担：

- AsterDrive 通用文件权限模型的替代实现
- storage driver 直接分支
- REST 文件接口的响应格式

必须显式的副作用：

- WOPI 写回后的 file version / quota / storage change / audit
- WOPI lock 与文件锁状态的一致性
- discovery cache refresh / cleanup expired session

## Review 检查清单

改后端 service 时，review 至少问这几件事：

- route 是否只做协议适配和 response mapping？
- service 是否只是 use case 编排，还是混入了 driver/protocol/descriptor/repo 细节？
- 可测试规则是否已经放进 domain helper？
- repo 是否只做数据库访问，没有产品流程判断？
- storage connector / driver capability 是否来自 storage 层，而不是业务层硬编码矩阵？
- remote protocol 是否只处理 wire contract？
- 上传、文件写入、WebDAV/WOPI 写回是否复用同一套 quota/blob/version/finalization 语义？
- policy、remote node、remote storage target 的产品所有权是否清楚：remote node 管连接，policy 管存储策略，remote storage target 管 follower-side target？
- 所有不可回滚副作用是否在函数名、参数或结果类型上足够显式？

## 非目标

这份文档不是要求一次性重写服务层。后续 PR 应该：

- 一次移动一个清晰职责
- 保持 public API 和 DB schema，除非对应 issue 明确要求
- 给被移动的规则补 focused tests
- 避免为了“架构纯净”引入框架式新层
- 避免把清楚的 AsterDrive 业务名改成空泛的 manager/helper/object
