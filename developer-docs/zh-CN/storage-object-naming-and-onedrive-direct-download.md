# 对象命名与 OneDrive 直接下载

本文记录 AsterDrive 当前对象命名能力、OneDrive Microsoft Graph 具名对象布局，以及直接下载文件名处理规则。本文面向后端、存储驱动和测试维护者，不是部署或用户使用说明。

## 设计目标

OneDrive 的 Graph 原生下载地址由 provider item 自己决定响应文件名。旧的扁平对象路径 `files/{upload_uuid}` 会让 Graph 把 UUID 当作文件名返回，因此 OneDrive 需要在 provider item 上保留原始文件名。

命名规则属于存储 connector 能力，不属于上传模式判断。上传走普通 relay、streaming-direct、chunk complete 还是 provider resumable，只影响数据传输方式；对象命名由 policy 对应 connector descriptor 的 `object_naming` 唯一决定。

## Connector 能力

能力定义在：

- `src/storage/connector_descriptor.rs`
- `src/storage/connectors/`

```rust
pub enum StorageConnectorObjectNamingMode {
    OpaqueUuid,
    OriginalFilename,
}
```

`StorageConnectorCapabilities.object_naming` 是必填字段，所有内置 connector 都必须显式声明：

| connector | `object_naming` | 对象布局 |
| --- | --- | --- |
| OneDrive | `original_filename` | `files/{upload_uuid}/{filename}` |
| Local | `opaque_uuid` | `files/{upload_uuid}` |
| S3-compatible | `opaque_uuid` | `files/{upload_uuid}` |
| Azure Blob | `opaque_uuid` | `files/{upload_uuid}` |
| Tencent COS | `opaque_uuid` | `files/{upload_uuid}` |
| Remote | `opaque_uuid` | `files/{upload_uuid}` |
| SFTP | `opaque_uuid` | `files/{upload_uuid}` |

descriptor 通过 OpenAPI 暴露给管理端和其他 API 消费方，wire 值固定为：

```json
"opaque_uuid"
"original_filename"
```

业务代码使用 `resolve_policy_object_naming(policy)` 读取能力。生产路径生成代码不应根据 `DriverType::OneDrive`、`ProviderResumable` 或其他上传模式自行推断命名方式。

## 路径生成

统一入口是：

```text
src/services/workspace/storage/blob_upload.rs
  prepare_non_dedup_blob_upload()
  nondedup_storage_path_for_policy()
```

调用顺序：

1. 根据 policy 解析 connector upload transport。
2. 为本次上传生成新的 UUID。
3. 根据 connector descriptor 解析 `object_naming`。
4. `opaque_uuid` 生成 `files/{uuid}`。
5. `original_filename` 先调用 `normalize_validate_name()`，再生成 `files/{uuid}/{filename}`。

以下入口都复用该函数：

- 普通非去重上传
- provider direct resumable 初始化
- streaming-direct
- chunk completion
- 空文件上传
- WebDAV 上传
- 临时预上传对象

### 同名文件

文件名不承担唯一性。每次上传使用独立 UUID 父目录，因此两个同名文件分别落在：

```text
files/550e8400-e29b-41d4-a716-446655440000/same-name.mp4
files/6ba7b810-9dad-11d1-80b4-00c04fd430c8/same-name.mp4
```

Graph 侧不会在同一个 UUID 目录中覆盖旧对象。OneDrive driver 在创建 UUID 目录时使用冲突失败策略；同一个 UUID 目录已经存在时返回 precondition 错误。

## OneDrive 目录生命周期

新布局只对具名对象生效：

```text
files/
└── {upload_uuid}/
    └── {filename}
```

OneDrive driver 写入具名对象前执行：

1. 创建或验证共享 `files` 目录。
2. 创建本次上传专属的 UUID 目录。
3. 验证两个对象都是 folder。
4. 创建 small upload 或 Graph upload session。

以下错误会清理 UUID 目录：

- Graph upload session 创建失败
- 大文件流式上传失败
- small upload 写入失败

删除具名对象时删除 `files/{upload_uuid}` 父目录，旧的 `files/{upload_uuid}` 扁平对象则按原路径删除。NotFound 删除保持幂等。

provider direct resumable 初始化还需要清理两种 provider 状态：

```text
abort Graph upload session
delete files/{upload_uuid}
```

数据库冲突、session 加密失败、session 落库失败、空 upload URL 等初始化错误都必须进入该清理流程；abort 和 delete 分别尝试，两个失败原因都要保留在错误中。

## 直接下载规则

`PresignedDownloadOptions.download_name` 表示当前逻辑文件名。下载 service 和 file resource handle 都把 `file.name` 传给 presigned driver。

OneDrive 策略还提供 `provider_download_filename_mode`：

| 模式 | 默认值 | 行为 |
| --- | --- | --- |
| `provider_native` | 是 | 优先使用 OneDrive 中保存的文件名，尽量保持 Graph 直接下载 |
| `strict_current` | 否 | 要求远端文件名与 AsterDrive 当前文件名一致，不一致时使用代理流式下载 |

在 `provider_native` 模式下，OneDrive driver 不检查当前文件名是否与远端名称一致。只要 Graph 能为对象返回合法 HTTP(S) 地址，就直接下载；文件重命名后可能仍然返回 OneDrive 中保存的旧文件名，旧的 `files/{uuid}` 对象也可能返回 UUID 文件名。

在 `strict_current` 模式下，OneDrive driver 只有同时满足下面条件时才返回 Graph 直接下载地址：

1. storage path 符合 `files/{uuid}/{filename}`。
2. path 中的 provider item 文件名等于 `download_name`，或调用方未提供 `download_name`。
3. Graph 返回合法的 HTTP(S) 下载地址。

以下情况使用 AsterDrive 代理流式下载：

- 严格模式下的旧对象 `files/{uuid}`，因为无法从路径确认远端文件名
- 严格模式下文件已在 AsterDrive 中重命名
- 严格模式下共享 blob 的逻辑文件名与 provider item 文件名不同
- driver 没有 presigned 能力
- 请求包含需要同源处理的条件请求或 inline sandbox 场景

严格模式回退的原因是 Graph 临时地址自己控制响应头。文件名策略由管理员显式选择，AsterDrive 不会在 `provider_native` 模式下暗中因为旧名称切换下载链路。

## 旧数据兼容

旧的 OneDrive 对象仍可能是：

```text
files/{upload_uuid}
```

路径解析器把这种路径识别为 legacy layout。旧对象仅保证读取和删除；metadata、stream 和 range 属于读取链路，继续按原路径工作。`provider_native` 模式可以继续请求 Graph 直接下载，`strict_current` 模式因为无法确认远端文件名而使用代理流式下载。新上传路径由 connector 的 `object_naming` 能力生成，不会复用旧的扁平路径。

新代码将旧路径继续视为 legacy layout，provider 类型也不承担历史 blob 路径批量改写职责。历史对象迁移需要单独的存储迁移任务和明确的数据库/对象一致性方案。

## 测试验收矩阵

改动命名能力或 OneDrive 直链时，至少运行以下测试：

```bash
cargo test --lib storage::connectors::tests
cargo test --lib services::workspace::storage
cargo test --lib storage::drivers::onedrive
cargo test --lib services::files::upload
cargo test --test test_upload
cargo test --test webdav
cargo test --features openapi --test generate_openapi
```

行为覆盖要求：

- 所有内置 connector 显式声明命名能力。
- `opaque_uuid` 和 `original_filename` 生成不同布局。
- 同名文件由不同 UUID 父目录隔离。
- Unicode 文件名规范化后进入对象路径。
- 空名、路径分隔符、反斜杠、parent traversal 和非法 UUID 被拒绝。
- OneDrive 具名对象成功创建共享目录和 UUID 目录。
- 共享目录已存在时验证目录类型并复用。
- UUID 目录冲突时不覆盖旧目录。
- session 创建、small upload、large upload 失败时清理 UUID 目录。
- 旧扁平路径仅保持读取和删除兼容；`provider_native` 模式继续使用 Graph 直链，
  `strict_current` 模式使用代理流式下载。新上传路径由 `object_naming` 生成。
- 当前文件名匹配时返回 Graph 直接下载地址。
- `provider_native` 模式下文件重命名仍保持 Graph 直接下载。
- `strict_current` 模式下文件重命名或逻辑文件名不一致时使用代理流式下载。
- provider session 清理的 abort/delete 四种成功与失败组合都保留结果。
- OpenAPI schema 与前端生成类型包含 `object_naming`。

## 新增 connector 时的要求

新增存储 connector 必须同时完成：

1. 在 descriptor 中明确填写 `object_naming`。
2. 说明对象布局和同名隔离方式。
3. 让所有上传入口复用 `nondedup_storage_path_for_policy()`。
4. 如果 provider 下载 URL固定文件名，扩展 `PresignedDownloadOptions.download_name` 的匹配规则。
5. 增加旧路径、非法输入、失败清理和直接下载回退测试。
6. 重新导出 OpenAPI 并生成前端 SDK。
