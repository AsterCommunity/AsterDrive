# AsterDrive 服务层模块化重构历史方案

> 状态：历史规划快照。`63efbcb3` 已完成按领域目录重组，本文保留当时的分析和后续边界建议，但正文中的 `*_service` 名称不再是当前代码路径。当前实现以[后端服务所有权边界](../architecture/backend-service-ownership.md)和下面的目录映射为准。

本文记录服务层模块化重构形成阶段的设计判断。它不是一次性重写计划，也不是要求把现有模块全部拆成独立 crate；仍然有效的目标是逐步收窄服务边界、减少隐式耦合，让后续文件、上传、任务、分享、WebDAV / WOPI 等功能继续扩展时不把复杂度推给调用方。

## 当前目录映射

| 历史名称 | 当前落点 |
| --- | --- |
| `file_service` / `folder_service` / `upload_service` / `trash_service` / `batch_service` | `src/services/files/file/`、`src/services/files/folder/`、`src/services/files/upload/`、`src/services/files/trash/`、`src/services/files/batch/` |
| `workspace_scope_service` / `workspace_storage_service` / `workspace_storage_core` | `src/services/workspace/scope/`、`src/services/workspace/storage/`、`src/services/workspace/storage_core/` |
| `task_service` | `src/services/task/`；typed contract 在 `src/services/task/spec/`，集中注册在 `src/services/task/registry.rs` |
| `share_service` | `src/services/share/` |
| `webdav_service` | `src/services/webdav/` |
| `wopi_service` | `src/services/preview/wopi/` |
| `admin_service` / `integrity_service` / `config_service` | `src/services/ops/admin/`、`src/services/ops/integrity.rs`、`src/services/ops/config/` |

当前已经完成的部分包括：按领域目录收拢模块、拆分 `task` 的 `dispatch` / `spec` / `registry` / `presentation` / `retry` / `types`、以及把多数旧 `*_service` 调用路径迁到新命名。尚可继续执行的是可见性收窄、facade re-export 清理、mutation 副作用收口和高风险流程的 command / context / result 建模。

下文中带 `_service` 的名称仅用于解释历史设计和迁移背景；涉及实际改动的路径统一以本节目录映射和当前 `src/services/` 树为准。

## 1. 当前判断

当前代码已经具备明确的模块化雏形：

- `src/services/files/`、`src/services/share/`、`src/services/task/` 等业务目录已经按能力拆分。
- `src/services/workspace/scope/` 负责个人空间 / 团队空间的 scope 判断。
- `src/services/workspace/storage/` 作为统一工作空间文件链路 facade，收敛上传、落盘、store、scope 校验等入口。
- `src/services/workspace/storage_core/` 承担较稳定的底层存储语义，例如策略解析、配额、blob / 文件记录创建。
- 多个模块已经把审计包装放在聚合入口层，避免核心逻辑直接混入 route 级副作用。

因此现阶段的问题不是“没有模块化”，而是“模块边界还不够硬”。目录结构已经拆开，但部分 service 的可见性、re-export 和跨 service 调用仍然偏宽，导致依赖方向主要靠开发者自觉维护。

## 2. 主要问题

### 2.1 `services/mod.rs` 顶层暴露过宽

当前 `src/services/mod.rs` 中大部分 service 都以 `pub mod` 暴露。这样做虽然方便 route 层和其他模块引用，但也让任意业务模块都能随手依赖另一个 service 的公共入口。

风险：

- 依赖方向缺少编译期约束。
- 新功能容易绕过既有 facade，直接调用其他模块的内部能力。
- 后续拆 crate 或收窄边界时会发现调用链散得很开。
- 模块之间的“允许依赖关系”没有显式表达。

### 2.2 facade 同时承担入口和零件仓库职责

`src/services/files/mod.rs`、`src/services/workspace/storage/mod.rs` 等 facade 当前既提供面向 route / 上层业务的稳定入口，也 re-export 了不少 `pub(crate)` 内部函数。

这种模式短期很实用，但长期会让 facade 变成“所有人都能从这里拿零件”的仓库：

- 调用方很容易依赖低层 helper，而不是高层业务入口。
- 内部重构时需要兼容过多 crate-wide 调用点。
- `pub(crate)` 虽然不是外部 public API，但在当前 crate 内仍然太宽。

### 2.3 `src/services/task/mod.rs` 过大

历史上的 `task_service/mod.rs`（当前为 `src/services/task/mod.rs`）同时承载后台任务查询、重试、lease guard、execution context、typed create helper、presentation、scope 校验和部分测试。它已经有清晰的架构注释，但根模块仍然过重。

风险：

- 新开发者打开 `task_service` 时很难快速分清哪些是稳定入口、哪些是运行时内部机制。
- lease / execution 这种核心概念与列表查询、重试等应用功能混在一起。
- 新增任务类型时，虽然有 spec / registry 约束，但根模块仍然容易继续膨胀。

### 2.4 横切逻辑需要继续收口

审计、权限 scope、storage change、cache invalidation 都属于横切逻辑。当前审计包装整体控制得还可以，但 storage change / cache invalidation 在文件、文件夹、批量、回收站、版本、WebDAV 等路径中都有调用点。

风险：

- 某条 mutation 路径忘记 publish storage change。
- 某条删除 / 归档 / 恢复路径漏掉 share cache 或 folder path cache invalidation。
- 相同事件在不同模块中手写，语义逐渐漂移。

### 2.5 数据库 entity 容易变成跨 service 合同

`entities` 层作为 SeaORM entity 暴露是合理的，但如果 service 之间长期裸传 `file::Model`、`folder::Model`、`upload_session::Model` 再配合大量参数，数据库结构会逐渐成为事实上的领域接口。

风险：

- 数据库字段变动会向上穿透多个 service。
- 业务流程缺少显式 command / context / result 类型。
- 参数列表越来越长，调用方需要知道过多内部语义。

## 3. 重构目标

本次模块化重构的目标不是减少文件数量，而是建立更清晰的边界：

1. 顶层 service 暴露面可控。
2. 每个业务模块有小而稳定的 public facade。
3. 内部 helper 优先限制在当前模块或子模块树内。
4. 横切逻辑尽量通过统一 helper / event / wrapper 表达。
5. 高风险核心流程逐步引入 command / context / result 类型。
6. 不做大规模搬家，不引入无收益的 crate 拆分。

## 4. 设计原则

### 4.1 按业务能力拆，不按技术名词拆

模块应该表达业务能力，例如：

- 文件元数据
- 文件内容更新
- 下载构建
- 删除 / 清理
- 上传初始化
- 上传完成
- 后台任务调度
- 后台任务执行上下文

避免出现没有语义边界的目录：

- `utils`
- `helpers`
- `common2`
- `misc`

如果必须有 `common`，里面只能放同一业务模块内部共享的稳定小工具，不能成为跨模块垃圾桶。

### 4.2 facade 只暴露调用方真正需要的入口

每个 service 的 `mod.rs` 应该优先承担两件事：

1. 声明子模块。
2. 导出稳定入口。

不应该把所有内部函数都通过 `pub(crate) use` 统一抛出去。更好的做法是：

```rust
pub use public_api::{download, upload, update};

pub(crate) use internal_api::{
    only_cross_module_helpers_that_are_intentionally_shared,
};
```

如果某个函数只被当前 service 的子模块使用，应优先使用：

```rust
pub(super)
```

或：

```rust
pub(in crate::services::some_service)
```

而不是默认 `pub(crate)`。

### 4.3 route 层依赖 service facade，service 内部避免随意互调

推荐依赖方向：

```text
api/routes -> services::<domain facade> -> db/repository / storage / config
```

允许的 service 间依赖应该有清晰理由。例如：

- `src/services/files/` 使用 `src/services/workspace/storage/` 做统一存储动作。
- `src/services/share/` 使用 `src/services/files/file/` 构建公开下载。
- `src/services/task/` 使用 `src/services/workspace/storage/` 校验任务 scope。

不推荐因为“函数刚好在那里”就跨 service 调用内部 helper。

### 4.4 横切逻辑应该可枚举

对于审计、storage change、cache invalidation 这类副作用，应尽量形成固定模式：

```text
核心 mutation -> mutation outcome -> 审计 / 事件 / cache invalidation wrapper
```

这样做的目的不是追求抽象，而是让每条写路径都能被检查：

- 是否记录审计？
- 是否发布 storage change？
- 是否失效 folder path cache？
- 是否失效 share target cache？
- 是否需要清理 blob / thumbnail / preview cache？

## 5. 分阶段计划

### 阶段一：建立边界清单

目标：先把“谁应该被谁调用”写清楚，不急着改代码。

产出：

- 服务层依赖矩阵。
- 每个 service 的 public API 清单。
- 每个 service 的 crate-internal API 清单。
- 高风险横切副作用清单。

建议先覆盖这些模块：

- `src/services/files/file/`
- `src/services/files/folder/`
- `src/services/files/upload/`
- `src/services/workspace/storage/`
- `src/services/workspace/storage_core/`
- `src/services/task/`
- `src/services/share/`
- `src/services/files/trash/`
- `src/services/webdav/`
- `src/services/preview/wopi/`

验收标准：

- 每个模块明确“对 route 暴露什么”。
- 每个模块明确“允许其他 service 调用什么”。
- `pub`、`pub(crate)`、`pub(super)` 的使用规则形成文档。

### 阶段二：收窄 `services/mod.rs`

目标：顶层 `services` 不再默认把所有模块作为公共 API 暴露。

建议动作：

1. 统计哪些 service 被 `src/api/routes/**` 直接引用。
2. 统计哪些 service 只在 `services/**` 内部引用。
3. 将纯内部服务从 `pub mod` 调整为 `pub(crate) mod`。
4. 对已经是内部实现细节的模块，继续收窄为私有子模块。

候选方向：

```rust
pub mod auth;
pub mod content;
pub mod events;
pub mod files;
pub mod mail;
pub mod media;
pub mod ops;
pub mod preview;
pub mod remote;
pub mod share;
pub mod storage_policy;
pub mod task;
pub mod user;
pub mod webdav;
pub mod workspace;
```

具体哪些顶层模块或子模块可以收窄，必须以实际调用点为准，不要一次性硬改；上面的名称与当前 `src/services/mod.rs` 保持一致。

验收标准：

- `cargo check` 通过。
- route 层仍能正常使用稳定 service 入口。
- 没有为了编译通过而把内部 helper 重新扩大成 `pub`。

### 阶段三：拆轻 `src/services/task/mod.rs`

目标：让 `src/services/task/mod.rs` 回到 facade 和稳定入口职责。历史名称 `task_service` 仅作为迁移说明保留。

建议拆分：

```text
src/services/task/
  mod.rs
  presentation.rs
  registry.rs
  retry.rs
  spec/
  steps.rs
  types.rs
  dispatch/
  offline_download/
```

职责建议：

- `dispatch/`：任务 claim、执行、lane 和维护循环。
- `runtime.rs`：系统任务运行时注册和执行记录。
- `presentation.rs`：任务查询结果和展示 DTO 构建。
- `registry.rs` / `spec/`：任务类型契约、编解码和集中注册。
- `retry.rs` / `steps.rs`：重试策略与步骤序列化。
- `mod.rs`：只保留模块说明、re-export 和少量稳定 facade；新增职责先落到对应子模块。

迁移策略：

1. 先机械移动类型和函数，不改行为。
2. 每移动一批运行 `cargo check`。
3. 移动后再收窄可见性。
4. 最后整理测试位置。

验收标准：

- `src/services/task/mod.rs` 显著变薄。
- 外部调用路径基本不变。
- 后台任务相关测试通过。
- 新增任务类型仍按 `types -> spec -> registry -> create helper` 流程落地。

### 阶段四：收窄 facade re-export

目标：减少 `pub(crate) use` 把内部零件暴露给整个 crate 的情况。

优先模块：

- `src/services/files/`
- `src/services/workspace/storage/`
- `src/services/share/`

建议动作：

1. 对每个 `pub(crate) use` 查调用点。
2. 如果只在当前 service 子模块内使用，改成 `pub(super)` 或直接模块内引用。
3. 如果只被一个外部 service 调用，评估是否应该提供更高层 facade。
4. 如果确实是跨模块稳定 helper，保留并加注释说明使用场景。

示例规则：

```text
download range parser 可以跨下载入口复用，但不应该被无关业务模块直接依赖。
blob cleanup 可以作为维护 / 版本恢复路径共用能力，但不应该挂在 `src/services/files/` public API 上。
`src/services/workspace/storage_core/` 的底层动作不应该被 route 层直接调用。
```

验收标准：

- crate-wide internal API 数量下降。
- 跨 service 调用更集中。
- 没有新增循环依赖。

### 阶段五：统一 mutation 副作用模式

目标：减少 storage change / cache invalidation / audit 漏调用风险。

建议引入轻量 outcome 类型，而不是一上来做复杂事件总线。

示例：

```rust
pub(crate) struct FileMutationOutcome<T> {
    pub value: T,
    pub storage_change: Option<StorageChangeEvent>,
    pub affected_share_scope: Option<WorkspaceResourceScope>,
    pub invalidate_folder_path_cache: bool,
}
```

也可以按业务拆小一点：

```rust
pub(crate) struct StorageMutationSideEffects {
    pub storage_change: Option<StorageChangeEvent>,
    pub invalidate_folder_paths: bool,
    pub invalidate_share_targets: Option<WorkspaceResourceScope>,
}
```

调用模式：

```text
核心 mutation 返回 outcome
聚合入口执行 outcome.side_effects()
审计 wrapper 再记录 audit
```

优先覆盖：

- 文件移动 / 删除 / purge
- 文件夹移动 / 删除 / copy
- 批量移动 / 删除 / copy
- 回收站恢复 / 清空
- WebDAV copy / move / delete / put
- 版本恢复 / 删除

验收标准：

- 写路径是否发 storage change 可以从 outcome 看出来。
- cache invalidation 不再散落成大量手写调用。
- 现有 SSE / WebDAV path cache / share cache 行为不变。

### 阶段六：为高风险流程引入 command / context / result

目标：减少长参数列表和裸 entity 跨层传递。

优先流程：

- 上传初始化
- 上传完成
- 下载构建
- 文件内容更新
- WOPI put / rename / put relative
- 后台任务创建
- 批量操作

示例：

```rust
pub(crate) struct CompleteUploadCommand {
    pub scope: WorkspaceStorageScope,
    pub upload_id: String,
    pub expected_parts: Option<Vec<CompletedPart>>,
    pub audit: Option<AuditContext>,
}

pub(crate) struct CompleteUploadResult {
    pub file: FileInfo,
    pub storage_delta: i64,
    pub transport: UploadTransport,
}
```

原则：

- command 表达业务输入，不直接等同于 route DTO。
- result 表达业务输出和后续副作用所需信息。
- context 承载运行时依赖，例如 actor、scope、request origin、audit context。
- 不为了包装而包装，优先处理参数多、规则复杂、调用方多的流程。

验收标准：

- 关键流程参数列表变短。
- 测试可以直接构造 command 验证业务分支。
- route DTO 与 service 输入解耦。

## 6. 不建议做的事

### 6.1 不建议一次性拆 workspace crate

当前代码仍然有大量服务间协作，提前拆 crate 容易遇到：

- 循环依赖。
- 大量临时 `pub`。
- feature flag 复杂化。
- 编译边界收益不明显。

只有当某个模块已经具备稳定 API、低反向依赖、可独立测试时，才考虑拆 crate。

### 6.2 不建议按文件长度机械拆分

超过 800 行不自动等于坏设计。判断标准应该是职责是否混杂、变化原因是否不同、测试是否困难。

例如：

- `src/services/task/mod.rs` 同时承担多类职责，适合拆；旧文档中的 `task_service/mod.rs` 仅是历史路径别名。
- 单个复杂协议适配文件如果职责单一，可以暂缓。

### 6.3 不建议引入泛化事件总线

storage change、cache invalidation 和 audit 当前更适合先用明确 outcome / helper 收口。过早引入泛化事件总线会让调用链更隐蔽，排查反而困难。

## 7. 优先级建议

### P0：先做约束，不大搬家

- 写服务层依赖矩阵。
- 明确 `pub` / `pub(crate)` / `pub(super)` 规则。
- 对新增代码执行规则，不让问题继续扩大。

### P1：拆轻 `src/services/task/`

- 移动 lease / execution / query / mutation。
- 保持外部 API 兼容。
- 每批移动后跑编译和相关测试。

### P2：收窄 facade

- 从 `src/services/files/`、`src/services/workspace/storage/` 开始。
- 查调用点后逐个降低可见性。
- 对确实跨模块使用的 helper 加说明。

### P3：统一 mutation side effects

- 先覆盖文件 / 文件夹 / 回收站 / 批量操作。
- 再覆盖 WebDAV / WOPI / 版本恢复。

### P4：引入 command / result

- 优先上传完成、下载构建、WOPI 操作、后台任务创建。
- 不追求全项目统一风格，先处理最复杂流程。

## 8. 代码评审检查清单

后续涉及服务层的 PR，可以按这份清单看：

- 新增函数是否真的需要 `pub` 或 `pub(crate)`？
- 能否限制为 `pub(super)` 或 `pub(in crate::services::xxx_service)`？
- 是否绕过了现有 facade 直接调用内部 helper？
- 是否新增了 service 间反向依赖？
- 写路径是否包含必要的 audit？
- 写路径是否发布必要的 storage change？
- 是否遗漏 folder path cache、share cache、auth snapshot cache 等失效逻辑？
- 是否直接把 route DTO 传入深层 service？
- 是否让数据库 entity 成为不必要的跨 service 合同？
- 是否能用 command / result 让流程更清楚？
- 是否有针对核心规则的纯单元测试？

## 9. 推荐落地顺序

实际执行时建议按小 PR 推进：

1. 新增服务层边界规则文档和依赖矩阵。
2. 收窄明显内部的 `services/mod.rs` 导出。
3. 在 `src/services/task/` 内拆分 lease / execution 相关职责，保持行为不变。
4. 在 `src/services/task/` 内继续收拢 query / mutation 相关职责。
5. 清理 `src/services/files/` 中只被内部子模块使用的 `pub(crate)` re-export。
6. 清理 `src/services/workspace/storage/` 中底层 `storage_core` re-export。
7. 设计并引入 mutation side effects helper。
8. 在文件 / 文件夹 / 回收站写路径试点 side effects。
9. 将成功模式推广到批量、WebDAV、WOPI、版本恢复。

每个 PR 都应该能独立编译、独立回滚，不要把“移动文件、改可见性、改行为”混在同一个提交里。

## 10. 最终目标状态

理想状态不是文件数量更多，而是开发者能快速回答这些问题：

- 这个功能应该从哪个 service facade 进入？
- 这个 helper 是否允许跨 service 调用？
- 这个 mutation 会触发哪些副作用？
- 这个流程的业务输入输出在哪里定义？
- 这个模块的内部实现能不能在不影响调用方的情况下重排？

如果这些问题能靠模块结构和类型系统回答，而不是靠口头约定回答，服务层模块化才算真正完成。
