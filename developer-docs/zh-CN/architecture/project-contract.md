# AsterDrive 项目契约

本文定义 AsterDrive 开发中长期成立的产品边界、工程不变量和完成标准。它不枚举当前所有模块，也不替代具体设计文档；代码结构以[架构概览](./index.md)为准，分层细节以[后端服务所有权边界](./backend-service-ownership.md)为准。

## 如何使用这份契约

开始实现前，用它确认功能归属、受影响层和必须保持的不变量。出现以下冲突时，不要自行选择一边继续写：

- 当前任务与本契约的长期边界冲突
- 当前代码与本契约表达的目标结构明显冲突
- 设计文档、测试和运行行为互相矛盾
- 实现需要破坏数据、协议兼容或跨层一致性

事实来源按用途区分：

| 来源 | 负责回答 |
| --- | --- |
| 当前用户任务和对应 issue | 本次范围、目标、明确排除项 |
| 当前 branch、HEAD、代码和测试 | 当前 checkout 已经实现了什么 |
| 本项目契约 | 哪些长期边界和不变量必须保持 |
| `developer-docs/` 的架构、设计和测试文档 | 子系统为何这样设计、如何验证 |
| `records/` | 历史背景和未落地草稿，不作为当前实现依据 |

## 产品身份

AsterDrive 是面向小团队的 Rust 自托管文件基础设施产品，核心领域是文件、目录、工作空间、分享、上传、存储策略、远端节点、WebDAV、WOPI、后台任务和审计。

- 代码和类型应直接表达 AsterDrive 领域语义。
- 不把其他产品或早期通用模板的领域概念带进来。
- 不为了表面通用化，把明确的业务名改成含糊的 `manager`、`helper`、`data` 或 `object`。
- 新抽象必须消除真实复杂度、建立明确边界，或匹配已有架构；不能只为减少几行重复代码而增加间接层。

## 最终形态优先

在需求和产品边界已经确认时，默认直接实现当前可知的最优完整形态，而不是人为拆成多个长期共存的半成品阶段。

- 一个需求涉及数据、后端、协议、前端、测试或文档多个边界时，按完整垂直链路交付。
- 编译检查点和分批验证是内部风险控制，不是保留临时架构的理由。
- 不为了缩小当前 diff 保留已经失去边界价值的旧路径、旧名称或双轨实现。
- 不用“先能跑、以后再重构”替代当前已经能判断清楚的结构。
- 最终形态优先不等于扩大产品范围；不添加与已确认需求无关的功能或抽象。

### 兼容层和转发函数

内部代码默认不写只做改名、转发或 re-export 的兼容函数和 facade。重命名、trait 调整或 Forge 接入时，应在同一变更中更新声明、调用方、import、测试和文档。

兼容层只有在以下情况才有边界价值：

- 已发布的公共 API 或 crate API 需要明确的弃用周期
- 线上协议需要跨版本互操作
- 数据 migration 或滚动部署需要新旧版本短期共存
- adapter 确实负责产品错误映射、配置注入、指标、审计、权限或类型隔离

保留兼容层时必须说明兼容对象、测试覆盖、弃用或删除条件。没有这些内容的薄转发函数视为应删除的历史包袱。

## AsterDrive 与 AsterForge 的边界

AsterForge 拥有产品无关的共享机制，AsterDrive 拥有文件产品语义及其数据一致性。

Forge 可以拥有：

- 生命周期、组件注册、启动和关闭机制
- 数据库、缓存、配置、任务、邮件、审计等产品无关的运行机制
- 产品无关的协议模型、解析、规划、错误分类和适配接口
- 通用验证、分页、排序、加密、指标和工具能力

Drive 必须保留：

- 用户、团队、工作空间、权限和产品认证流程
- 文件、目录、分享、回收站、版本、配额和存储策略语义
- 产品数据库实体、历史 migration、repository 查询和事务编排
- 存储 connector 配置、driver 注册、远端节点和上传策略
- WebDAV/WOPI 的产品适配、持久化、审计和集成测试
- 产品错误码、API 文案、运行时配置项和管理界面

“多个项目可能用到”不是迁入 Forge 的充分条件。接入或抽取前必须写清：

```text
旧函数或旧模块 -> Forge API -> Drive 保留职责 -> 必测行为
```

## 后端分层契约

后端默认链路是：

```text
route / protocol adapter
  -> service use case
  -> domain rule
  -> repository / storage / protocol port
```

- `src/api/routes/*` 负责 transport、guard、参数提取和响应映射，不承载业务规则或事务编排。
- `src/services/*` 负责编排完整 use case，不替代 repository、driver registry 或 wire protocol parser。
- `src/services/<domain>/*` 承载 normalization、validation、capability resolution、target selection 和 finalization 等可测试规则。
- `src/db/repository/*` 只表达数据库事实和原子 SQL，不决定 UI、协议或存储驱动行为。
- `crates/aster_drive_storage/*` 表达共享存储 trait、descriptor、能力和结构化错误。
- `src/storage/*` 实现产品 connector、driver、registry、策略快照和远端协议运行时。
- `src/webdav/*`、WOPI 和 internal storage adapter 必须保持各自协议边界。

一个改动跨越 route、service、domain、repository、storage 或 protocol 多层时，实施前应列出每层的职责。边界说不清时先继续调查，不能把所有逻辑堆进一个 service 函数。

## 数据与事务不变量

- writer database 是事务性写入、读后写、配额判断、token rotation、上传完成和权威状态判断的入口。
- reader database 只用于允许短暂滞后的纯读场景；通用 helper 不得暗中把所有调用切到 reader。
- migration、SeaORM entity、repository、OpenAPI 类型和业务状态必须一起审查。
- 涉及跨表一致性的操作必须在 service/repository 边界明确表达事务范围和副作用顺序。
- 缓存是可重建投影，不是权限、锁、配额或数据一致性的分布式权威来源。
- 多数据库 SQL 必须考虑 SQLite、PostgreSQL 和 MySQL 的语义差异。
- 跨层数值转换使用 checked conversion；领域状态优先使用枚举或强类型 wrapper，不传播魔法字符串。
- 结构化数据只有在确实需要数据库侧 JSON 查询、索引或约束时才使用 JSON 列。

## 存储与上传不变量

- 具体后端能力通过 connector、driver、descriptor、capability 和 registry 表达，业务 service 不直接依赖具体 SDK。
- 新增或修改存储后端时，必须同时检查配置、凭据、连接测试、运行时构造、上传下载、前端表单、OpenAPI 和测试。
- direct、chunked、presigned、multipart、remote relay 和 remote presigned 必须遵守同一策略协商结果。
- 上传完成必须保持 metadata、blob/object、file version、quota、audit、task/progress 和 session/temporary object 的一致性。
- 失败、取消、超时和重试路径必须定义清理或可恢复状态，不能只实现成功路径。
- blob 去重、引用计数、孤儿清理、公开资源缓存和对象 key 都属于文件安全链路，修改时必须补边界测试。

## API 与协议不变量

- 普通 REST API 使用 AsterDrive envelope 和稳定字符串错误码。
- 文件流、SSE、Prometheus、WebDAV、WOPI 和 internal storage 等端点按自身协议返回，不能为了统一 envelope 破坏兼容性。
- 协议专用错误通过独立映射层进入产品错误边界，不污染全局错误模型。
- WebDAV 规范性结论以 RFC Editor 文本为准；库实现、Litmus 和真实客户端是兼容性证据。
- 协议能力声明、允许的方法、状态码、header、锁、Range、ETag 和 token 行为必须由测试证明。

## 前端与生成代码边界

- 前端具体约束以 [`frontend-panel/AGENTS.md`](https://github.com/AsterCommunity/AsterDrive/blob/master/frontend-panel/AGENTS.md) 为准。
- API schema 和生成 SDK 是后端与前端的正式契约；修改 schema 后必须重新导出并检查生成差异。
- `frontend-panel/src/services/api.generated.ts` 等生成文件不手动编辑。
- 存储 UI 使用后端 descriptor、capability、field 和 action，不维护平行的前端 driver 能力矩阵。
- 管理界面服务于扫描、比较和重复操作，不使用营销落地页式信息架构。

## 安全与可观测性不变量

- token、密码、MFA secret、外部认证凭据、存储 secret 和远端节点 secret 不进入日志、错误消息或审计明文字段。
- 认证、分享、上传、WebDAV、WOPI 和 internal storage 必须保留对应权限、限流和协议安全边界。
- 路径、URL、MIME、文件名、大小、分页和外部输入在 DTO/service 边界校验。
- fire-and-forget 任务必须记录失败，不能用静默 `let _ =` 吞掉错误。
- 用户可见的长任务优先复用现有 task record、dispatch、retry 和 presentation 结构。

## 测试与完成标准

验证范围跟随风险和影响面，至少从以下维度选择：

- 正常路径
- 非法输入和资源上限
- 失败与错误映射
- 事务回滚和副作用顺序
- 重试、取消、超时和关闭
- 并发、竞争和幂等
- 协议和真实客户端兼容
- SQLite、PostgreSQL、MySQL 差异
- OpenAPI、生成 SDK 和前端行为

编译通过或一个 focused test 通过不代表任务完成。最终说明必须列出实际运行的验证和未运行部分，不能把历史绿色结果投射到当前 checkout。

## 契约变更规则

本文件只在产品边界或长期工程不变量发生变化时修改。具体模块路径、临时兼容层、单个 issue 方案和历史讨论应写入对应架构、设计或 `records/` 文档。

如果实现确实需要打破本契约，应先更新设计依据和验收标准，再修改代码；不要先让代码悄悄形成新的事实。

开发过程中确认了可重复的工程问题时，应同步修正最接近事实源的文档、路由、测试支持或自动化。只沉淀已验证、可复用的规则，不记录一次性偶发现象。
