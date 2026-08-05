# AsterDrive

AsterDrive 是面向小团队的 Rust 自托管文件基础设施项目。代码围绕文件、工作空间、上传、分享、存储策略、远端节点、WebDAV、WOPI、后台任务和审计组织，不引入其他产品或早期模板的领域概念。

## 开始工作

涉及代码或工程设计的任务按以下顺序建立上下文：

1. 读 [`developer-docs/zh-CN/architecture/project-contract.md`](developer-docs/zh-CN/architecture/project-contract.md)，确认产品边界和长期不变量。
2. 读 [`developer-docs/zh-CN/contributing/task-routing.md`](developer-docs/zh-CN/contributing/task-routing.md)，选择本次真正需要的代码、文档和验证入口。
3. 检查当前 branch、HEAD 和 worktree；issue、PR、review comment 或历史测试结果必须与当前 checkout 对得上。
4. 沿现有调用链阅读相邻代码和测试，模式明确后再编辑。
5. 前端任务额外读取 [`frontend-panel/AGENTS.md`](frontend-panel/AGENTS.md)。

默认工程流程见 [`developer-docs/zh-CN/contributing/engineering-workflow.md`](developer-docs/zh-CN/contributing/engineering-workflow.md)。小型只读问题不需要机械地读取所有文档，但涉及实现、架构、协议、数据或跨层行为时必须先确认契约和当前代码。

任务开始后默认持续执行到完成标准。中间进度更新只是状态通知，不是审批点；不要在侦察、实现、focused test、修复或扩大验证之间等待用户说“继续”。只有本文件“事实和冲突”及工程工作流列出的高代价歧义才暂停确认。

## 事实和冲突

- 当前用户任务和对应 issue 决定本次范围。
- 当前 branch、HEAD、代码和测试决定当前实现事实。
- 项目契约决定长期边界；架构、design 和 testing 文档提供子系统依据。
- `developer-docs/**/records/` 是草稿或历史背景，不代表当前实现。
- 当前任务、代码、契约或权威规范存在实质冲突时，继续调查并向 1547 确认，不凭感觉选一个方向。
- 能从当前 issue、代码、测试和文档确定的路径、命名、实现方式与验证入口自行决定，不把低风险工程选择交还给用户。

## 硬约束

- 仓库可能有大量未提交改动。不要回滚、覆盖或格式化掉用户的无关改动。
- 只修改任务相关文件；同文件存在交叉修改时先读完整 diff，再做兼容编辑。
- 不主动扩大 issue 范围，不把未来事项顺手实现。
- 不手动编辑生成文件。OpenAPI schema 变化后按仓库生成流程更新并审查生成 diff。
- 优先复用现有 helper、trait、registry、error mapping 和测试支持，不建立平行抽象。
- 新抽象必须消除真实复杂度或建立明确边界，不能只为少写几行代码增加间接层。
- 需求和边界明确时直接实现最终合理形态，不把完整需求人为拆成长期共存的半成品阶段。
- 内部 API 重命名、trait 调整或 Forge 接入默认一次迁移全部调用方，不增加只做转发、改名或 re-export 的兼容函数。
- 只有公开 API、线上协议、滚动部署或真实产品 adapter 需要时才保留兼容层，并写明测试与删除条件。
- 不引入全局可变单例、隐藏注册表或无法隔离测试的静态产品状态。
- 需要破坏数据、修改公开协议、执行不可逆 migration 或改变产品长期边界时，先确认设计和验收标准。

## 所有权速记

AsterForge 拥有产品无关的共享机制；AsterDrive 保留文件产品语义和数据一致性。

- Forge：运行时生命周期、产品无关的数据库/缓存/配置/任务机制、通用协议模型与解析、验证和基础工具。
- Drive：认证流程、用户/团队/workspace、权限、文件/目录、分享、版本、配额、存储策略、远端节点、产品实体和 migration、事务、审计及产品集成测试。
- “多个项目可能使用”不是迁入 Forge 的充分条件。抽取前写清 `旧模块 -> Forge API -> Drive 保留职责 -> 必测行为`。

完整边界以[项目契约](developer-docs/zh-CN/architecture/project-contract.md)为准。

## 后端实现速记

默认链路：

```text
src/api/routes/*
  -> src/services/*
  -> src/services/<domain>/*
  -> src/db/repository/* / src/storage/* / src/webdav/*
```

- Route 只做 transport、guard、参数提取、调用 service 和响应映射。
- Service 编排完整 use case，不堆 SQL、wire parser、driver registry 或 UI descriptor 矩阵。
- Domain helper 承载 normalization、validation、capability、target selection 和 finalization 等可测试规则。
- Repository 只做数据访问和原子 SQL。
- Storage connector/driver 表达对象内容能力，业务层不直接分支到具体 SDK。
- WebDAV、WOPI、internal storage 等协议端点遵守各自格式，不套普通 REST envelope。
- 跨 route/service/domain/repo/storage/protocol 的改动，实施前列出每层各自职责。

更完整说明见 [`developer-docs/zh-CN/architecture/backend-service-ownership.md`](developer-docs/zh-CN/architecture/backend-service-ownership.md)。

## 数据、存储和安全

- writer database 用于事务写入、读后写、配额、token rotation、上传完成和权威状态判断；reader 只用于允许短暂滞后的纯读。
- migration、entity、repository 和业务状态一起审查；跨数据库逻辑考虑 SQLite、PostgreSQL 和 MySQL。
- 缓存是可重建投影，不是权限、锁、配额或一致性的权威来源。
- 存储能力通过 connector、driver、descriptor、capability 和 registry 表达。
- 上传完成必须保持 metadata、blob/object、version、quota、audit、task/progress 和 session cleanup 的一致性。
- token、密码、MFA secret、外部认证凭据、存储 secret 和远端节点 secret 不进入日志、错误或审计明文字段。
- fire-and-forget 操作必须记录失败，不使用静默 `let _ =` 吞错。

## 测试和验证

新增或修改行为必须有测试。验证范围跟随风险，不能用编译通过或一个 focused test 代替完整验收。

Rust 测试优先缩小 target：

```bash
cargo test --lib <filter>
cargo test --test <target> <module_or_test_filter>
```

- API/schema 改动：导出 OpenAPI，重新生成前端 API，并检查生成 diff。
- migration/repository/SQL：至少跑 SQLite；涉及跨库语义时补 PostgreSQL/MySQL。
- 上传、配额、版本、锁、引用计数、认证和公开访问：覆盖成功、失败/回滚和边界。
- connector/driver：覆盖 validation、descriptor/payload、连接测试和实际请求契约；普通请求与 presigned 行为分别证明。
- WebDAV/WOPI/internal storage：覆盖协议状态码、header、token/lock、资源边界和兼容性；规范性结论查权威文档。
- 前端 service 或关键交互：运行 focused Vitest；用户流程变化时补/跑 Playwright。
- 公共 trait、runtime state 或跨模块契约变化：先跑编译检查点，再扩大相关测试矩阵。

结束前运行 `git diff --check`，并在结果中准确列出实际运行和未运行的验证。被中断、仍在运行或来自旧 checkout 的命令不算当前通过证据。

实质性任务结束时检查是否存在可复用的工程摩擦。重复搜索、错误路由、无价值 facade、手工重复步骤或缺失 test support 能小范围验证时，直接改进对应文档、脚本或测试基础设施；一次性现象和未经证明的猜测不写入长期规则。

## Code Review Fixes

用户粘贴 Greptile、CodeRabbit、Gemini 或人工 review comments 时：

1. 对照当前代码和 revision 逐条判断真实问题或误报。
2. 只修仍成立的真实问题，保持修改最小。
3. 按相关性分批修复，每批完成后编译或测试。
4. 最终列出已修、误报、跳过原因和验证命令。

## 文档入口

- [项目契约](developer-docs/zh-CN/architecture/project-contract.md)
- [架构概览](developer-docs/zh-CN/architecture/index.md)
- [关键模块设计](developer-docs/zh-CN/architecture/module-designs.md)
- [后端服务所有权](developer-docs/zh-CN/architecture/backend-service-ownership.md)
- [工程工作流](developer-docs/zh-CN/contributing/engineering-workflow.md)
- [开发任务路由](developer-docs/zh-CN/contributing/task-routing.md)
- [测试与数据库后端](developer-docs/zh-CN/testing/index.md)
- [文档贡献指南](developer-docs/zh-CN/contributing/documentation.md)
