# 存储 descriptor 与字段规范化契约

本文档记录 AsterDrive 当前 storage policy connector descriptor 与 remote storage target driver descriptor 的开发约定。它是给后端和前端贡献者看的契约，不是用户手册。

## 范围

当前有两类相近但不等价的 descriptor：

- `src/storage/connector_descriptor.rs` 与 `src/storage/connectors/`：描述 storage policy 管理表单、连接测试、授权、policy action、上传工作流和 connector 能力。
- `src/services/remote_storage_target_service/driver.rs`：描述 follower remote storage target 可用的 driver、字段和本地归一化规则。

这两类 descriptor 可以共享命名和字段语义，但不要强行合成一个万能 descriptor。storage policy 描述的是主控侧策略和上传/下载工作流；remote storage target 描述的是 follower 接收远端写入时的落点配置。

## Descriptor 规则

- 管理端字段、动作、能力和 UI 辅助元数据必须优先来自后端 descriptor。前端不能用本地 `driver_type` 白名单或矩阵推断连接测试、授权、上传策略、原生处理、远端绑定、字段可见性等能力。
- `label_key`、`help_key`、`placeholder`、`required_message_key` 和类似字段只表达稳定的本地化 key 或提示参数。具体文案由前端 i18n 决定，但字段是否存在、是否必填、是否敏感由后端 descriptor 决定。
- `secret: true` 或 secret kind 表示前端必须使用敏感输入控件，后端日志和 `Debug` 输出不得打印明文。创建流程按 descriptor 的 `required` 和后端校验执行；编辑流程中，省略 secret 字段表示保留已有值，显式提供新值才替换。
- 不支持的 driver 必须由后端返回稳定错误。remote storage target 只能暴露已注册且远端 capability 声明支持的 known driver；未知 driver id 可以在 wire model 中保留，但不能被前端当成本地可配置 driver。
- descriptor 缺失、远端 capability 缺失或解析失败时，前端只能做保守兜底：隐藏高风险动作或显示不可用状态。不能在兜底路径里恢复本地 capability 矩阵。
- action descriptor 用来声明入口是否需要 saved policy、授权 credential，以及是否会修改远端状态。路由和 service 仍要做最终校验，不能只靠前端隐藏按钮。

## 字段规范化规则

字段规范化属于 backend use case 或 connector/driver-specific pure helper，不能散落在 handler 或前端组件里。

- local remote storage target `base_path` 使用 `normalize_relative_local_path`：trim 空白，支持 `.` 当前目录归一化，拒绝空值、绝对路径、`..`、Windows prefix 和反斜杠逃逸，最终路径必须落在 `server.follower.remote_storage_target_local_root` 内。
- object-storage remote storage target `base_path` 当前按 prefix 处理：trim 空白并去掉首尾 `/`，允许空 prefix 表示 bucket/container 根。
- storage policy object-storage endpoint/bucket 使用 `normalize_s3_endpoint_and_bucket` 及 connector 错误码映射：endpoint 非空时必须是 `http://` 或 `https://` 且包含 hostname，bucket/container 不能为空。
- `max_file_size` 的 `0` 表示不声明额外限制；负数无效。上传链路仍要在使用 policy/target 时执行最终大小校验。
- 同 driver 编辑时，`access_key` / `secret_key` / `base_path` / endpoint / bucket 等可选字段省略表示保留已有值；显式提供字段则重新走对应 driver 的规范化和校验。
- 切换 remote storage target driver 时，旧 driver-specific 字段不能继承到新 driver：endpoint、bucket、access key、secret key 会重置为空；base path 回到新 driver 输入或默认根语义后再规范化。

## 实现边界

- route 层只做协议适配、鉴权、参数提取和响应映射，不拼 descriptor，不做 driver-specific normalization。
- service 层负责 use case 编排：加载上下文、调用 normalization、检查 capability、调用 repo、执行必要 side effects。
- `src/storage/connectors/` 负责 storage policy connector descriptor、连接字段规范化、连接测试、授权和 connector action。
- `src/services/remote_storage_target_service/driver.rs` 负责 remote storage target driver descriptor、driver-field normalization、target-to-policy materialization 和 driver build/validate。
- `src/storage/remote_protocol/` 只处理 wire model、签名、path encoding、transport 和 response parsing，不决定 UI 字段和 policy target 选择。

## 测试要求

修改 descriptor 或 normalization 时至少补对应的纯函数/单元测试：

- descriptor 必须覆盖每个内置 driver 的字段、secret 标记、action 和关键 capability。
- normalization 必须覆盖 trim、空值、逃逸路径、prefix 首尾斜杠、负数 `max_file_size`、同 driver secret preserve、显式 secret replace、driver 切换字段重置。
- storage policy descriptor 行为改变时，跑 `cargo test --lib storage::connectors` 或更小过滤；remote storage target 归一化改变时，跑 `cargo test --lib remote_storage_target_service::tests::<filter>`。
- 改 OpenAPI schema 后必须重新导出 OpenAPI 并生成前端 SDK。本契约切片不改变 API shape。
