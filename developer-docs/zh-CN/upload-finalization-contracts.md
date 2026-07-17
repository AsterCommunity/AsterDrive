# 上传完成契约矩阵

本文档记录 AsterDrive 当前上传链路的完成契约。它是 issue #369 的开发者向基线，不改变公开 API，也不声明已经完成统一重构。

上传路径分成两组：

- 普通 HTTP multipart 上传：入口在 `storage::multipart`，直接在一次请求内落到正式文件。
- upload session 型上传：入口在 `upload::{init, chunk, complete}`，先持久化 `upload_session`，complete 阶段再把临时对象、分片或 assembled 文件收口成正式文件。

最终落文件时必须保持三个不变量：

- 正式文件、blob/version、配额和 upload session 状态不能互相脱节。
- 实际计费大小必须来自当前路径能信任的最终字节来源，而不是只信任客户端声明。
- 已写入但未完成 DB 收口的对象要有明确 cleanup 或孤儿回收归属。

## 当前 finalize 锚点

| 锚点 | 当前职责 | 备注 |
| --- | --- | --- |
| `storage::store_from_temp_with_hints` | 从服务端临时文件创建或覆盖文件；可走本地 dedup 或 non-dedup preuploaded blob | 普通 multipart server path、local direct 会落到这里 |
| `storage::store_preuploaded_nondedup` | 从已经写入 driver 的 non-dedup blob 创建或覆盖文件 | streaming direct 会落到这里 |
| `storage_core::finalize_upload_session_blob_with_actor_username` | 在一个 DB 边界里创建文件、更新配额、把 session 标记 completed | local chunked、stream relay chunked 直接使用 |
| `storage_core::finalize_upload_session_file` | 为 opaque object 找到或创建 blob，再调用 session finalize，并发布 storage change event | presigned single、presigned object multipart、relay object multipart 使用 |
| `upload::shared::run_upload_completion_stage` | complete 前把 session 从 expected status 切到 assembling；失败后按错误类型恢复或标 failed | 所有 upload session complete 路径共享 |

## 内部 verified blob 契约

`src/services/files/upload/complete/contract.rs` 定义 upload-session complete 阶段本地使用的 `VerifiedUploadedBlob`。所有 session 型 complete 路径在进入 DB finalization 前，必须先把当前 transport 已经验证过的最终对象表达成这个类型。

该类型显式携带：

- `size`：已验证的逻辑计费字节数。
- `policy_id`：最终 blob 所属的 storage policy。
- `storage_path`：已经写入或已经 complete 的对象路径。
- `source`：content-addressed dedup、opaque object 或 preuploaded non-dedup blob。
- `cleanup`：DB finalize 失败后要删除对象、清理 preuploaded blob、保留给 orphan GC，还是保留已完成 multipart object。

当前 `VerifiedUploadedBlob` 覆盖 presigned single、presigned object multipart、relay object multipart、local chunked 和 stream relay chunked。

`src/services/workspace/storage/store/contract.rs` 定义非 session 型 `store_from_temp` 路径使用的 `VerifiedTempStoreBlob`，覆盖普通 multipart/server path 和 local direct 最终进入 `store_from_temp_with_hints` 的落账契约。它把 content-addressed dedup、preuploaded non-dedup、staged dedup rollback、preuploaded cleanup 这些以前散在 `persist.rs` 里的约定集中起来。

`storage::store_preuploaded_nondedup` 使用本地 `VerifiedPreuploadedNondedupStoreBlob` 覆盖 streaming direct 的最终落账契约，校验 verified size、policy、storage path 和 prepared blob 一致后再进入 DB finalization。

## Local chunked 的 offset-staging 契约

新建的 server-managed chunked session 不再为每个 chunk 保存一份 payload，也不会在 Complete 阶段重新拼写一份完整文件。Init、Chunk PUT 和 Complete 共享以下目录契约：

```text
<upload_temp_dir>/<upload_id>/
├── .offset-staging-v1                 # 唯一内容载体，Init 时预分配到 total_size
├── .chunk_0.lock                      # 同一 chunk 的跨任务/进程排他锁
└── .chunk_1.lock                      # 其他 lock 文件按需创建
```

offset-staging 的本地 receipt 存在 `upload_session_parts`：

```text
part_number = chunk_number + 1
etag        = aster-drive-offset-staging-receipt-v1
size        = expected_chunk_size
```

`.offset-staging-v1` 是旧 session 的兼容格式线索，但新 session 的权威格式字段是 `upload_sessions.session_kind`。Complete、Chunk PUT、Progress 和 lifecycle 先使用显式 kind；只有 `session_kind IS NULL` 的迁移前 row 才通过统一 compatibility classifier 读取 policy transport、multipart 字段和 `.offset-staging-v1`。legacy compatibility path 创建的通用 `assembled` 文件不参与判断。这个边界很重要：legacy 首次拼装后如果 storage/DB 阶段出现可重试失败，`assembled` 可能保留到下一次 Complete；若拿它判断格式，就会把 payload-sized `chunk_N` 错当成 offset receipt。

### 显式 session kind

Init 根据 connector-owned `PolicyUploadTransport` 持久化执行计划，不根据 `DriverType` 猜路径。当前值包括：

| `session_kind` | 数据面 | 完成计划 |
| --- | --- | --- |
| `offset_staging` | 本地 `.offset-staging-v1` + DB receipt | 本地 staging finalize |
| `stream_staging` | staging file + connector stream relay | stream relay finalize |
| `provider_relay_multipart` / `remote_relay_multipart` | provider multipart parts + DB ETag | relay multipart complete |
| `provider_presigned_single` / `remote_presigned_single` | provider temp object | presigned single complete |
| `provider_presigned_multipart` / `remote_presigned_multipart` | provider multipart parts | presigned multipart complete |
| `legacy_chunk_files` | 迁移前 `chunk_N` payload | legacy assemble/relay compatibility |

`session_kind` 在 0.5.0 前保持 nullable，以便读取升级前的 session；新 Init 永远写入非空值。显式 kind 与 multipart 字段组合不一致时，接口返回 `upload.session_corrupted`，不会降级到另一条数据面。

### Chunk PUT 的 durable receipt 顺序

同一 chunk 先取得 `.chunk_N.lock`，不同 chunk 使用不同锁，因此可以并行写各自 offset。取得锁后按下面的顺序提交：

1. 在 `.offset-staging-v1` 的 `chunk_number * chunk_size` 位置完整写入 payload。
2. 对 staging file 执行 `sync_data`，先保证内容持久化。
3. 开启只包含数据库 SQL 的短 writer transaction。
4. 向 `upload_session_parts` insert-only 登记本地 chunk receipt。这里复用 `(upload_id, part_number)` 唯一键；本地 receipt 使用保留的 offset-staging 标识作为 `etag`，object multipart 仍保存 provider ETag。
5. 只有 receipt 首次插入时才增加 `upload_sessions.received_count`，然后提交 transaction。

收到重复 Chunk PUT 时仍会完整校验 payload 大小。若 receipt 已存在，服务端会 drain/忽略请求 body，校验 receipt 后直接返回当前进度，不覆盖已提交 range，也不重复计数。

### 崩溃与重试矩阵

| 中断位置 | 可见状态 | 重试行为 |
| --- | --- | --- |
| staging range 写入完成前 | receipt 缺失，range 可能部分写入 | 在同一 offset 完整覆盖，不计数 |
| staging `sync_data` 后、DB transaction 前 | durable range 存在，receipt 缺失 | 重新完整覆盖，然后登记 receipt |
| DB receipt transaction 提交后客户端未收到响应 | receipt 和内容都存在 | 重试校验请求大小，返回当前进度，不重写、不重复计数 |
| receipt row 缺失但 range 仍完整 | receipt 缺失，received_count 可能滞后 | 重试完整覆盖并补登记，只计一次 |
| receipt row 损坏 | receipt 存在但 sentinel/size 不匹配 | Chunk PUT 和 Complete 明确报损坏，不静默覆盖 |
| staging file 被截断 | receipt 可能完整但内容载体长度错误 | Complete 拒绝并保留失败状态，避免把短文件当成完整上传 |

Complete 必须同时校验：

- `upload_session_parts` 中恰好有 `total_chunks` 条本地 receipt，part 序号连续，sentinel 和 size 与每个 chunk 一致；
- `.offset-staging-v1` 是普通文件；
- staging file 长度等于 `session.total_size`。

Local completion 直接消费这份 staging file：开启 `content_dedup` 时会先流式计算 SHA-256，再按 content-addressed key promote；关闭 dedup 时把同一 staging file 写入预分配的独立 Blob。两种情况都不会再完整写一份 assembled 文件。需要 generic stream upload 的 connector 从 staging file 串流到目标 driver。S3-compatible、Azure Blob、Tencent COS 等已经协商到 provider relay multipart 的 session 不走这条本地 staging 路径。

### Legacy compatibility path

升级前创建的 session 仍可能采用 payload-per-chunk 目录：

```text
<upload_temp_dir>/<upload_id>/
├── chunk_0                            # payload
├── chunk_1                            # payload
└── assembled                          # Complete 拼装产物，失败/崩溃后可能保留
```

这条路径会在 Complete 时取得 `chunk_assembly_to_local_temp_file` limiter，然后拼装或串流 legacy chunk。它只保护 legacy assembly，不限制新 `.offset-staging-v1` session。兼容路径计划在 `0.5.0` 移除；移除前必须保留“已有 assembled 仍按 legacy 重试”的回归测试。

## 模式矩阵

| 上传模式 / transport | 初始状态和写入位置 | trusted size source | quota precheck / atomic charge | finalize function | cleanup / idempotency |
| --- | --- | --- | --- | --- | --- |
| regular multipart/server path | 不创建 upload session；`upload_with_hints` 读取 `actix_multipart::Multipart` 到 runtime temp file | 服务端读取 multipart body 时累计的 `size`；如有 `declared_size`，必须和累计值相等 | policy resolved by actual `size`；preuploaded non-dedup blob 会在对象写入前 precheck；DB 事务内再次 `check_quota`，再 `update_storage_used` | `store_from_temp_with_hints` -> `store::from_temp` / `persist_temp_store` / `write_file_record_from_temp` | 请求临时文件在 `store_from_temp_with_hints` 返回后删除；preuploaded 对象在 DB 失败时 cleanup；dedup staged 对象只有在确认没有 blob row 引用时回滚，否则交给 orphan GC |
| local direct | 不创建 upload session；local policy 且有 `declared_size` 时直接写入 local staging path | 写入 local staging file 时累计的 `size`，必须等于 `declared_size`；dedup 时同流计算 hash | 使用已解析 local policy；和 server path 一样通过 `store_from_temp_with_hints` 做 precheck / 事务内 atomic charge | `upload_local_direct` -> `store_from_temp_with_hints` | 写入、大小不匹配、空文件或 store 结束后删除 staging file；重复请求不会通过 session 幂等，只按普通创建语义处理 |
| streaming direct | 不创建 upload session；relay request body 到 driver 的 prepared non-dedup blob | driver `metadata(storage_path).size`，必须等于 `declared_size`，并再次检查 policy max file size | relay 前先用 `declared_size` 做 quota precheck；metadata 复验后再用 `actual_size` precheck；DB 事务内再次 `check_quota` 并 `update_storage_used` | `upload_streaming_direct` -> `store_preuploaded_nondedup` | storage upload、relay、metadata、size validation、quota validation 或 DB finalize 失败时 cleanup prepared blob；成功后按正式 blob 管理 |
| local chunked / offset staging | session status `uploading`；Init 预创建 `.offset-staging-v1`，Chunk PUT 按 offset 写 range 并登记 DB receipt | 每块必须等于 `expected_chunk_size_for_upload`；Complete 校验全部 receipt 和 staging file 的 `session.total_size`；dedup 时从 staging 流式计算 SHA-256 | chunk receipt 与 `received_count` 在只含 SQL 的短 writer transaction 内幂等登记；最终 quota 仍由 `finalize_upload_session_blob_with_actor_username` 原子落账 | `complete_chunked_upload_with_actor_username` -> `finalize_chunked_upload_session` -> `load_offset_staging_file` -> `stage_chunked_temp_file` -> `persist_chunked_upload` | staging range 先 `sync_data`；receipt 是唯一 completion index，唯一键避免重试重复计数；Complete 成功后删除 upload temp dir |
| legacy local chunk files | 升级前 session 的 `chunk_N` 是 payload；Complete 拼写 `assembled`，generic stream connector 则依次读取 chunk | 拼装路径按实际读取字节累计 size；legacy stream 使用 session total size 作为声明值并由下游 storage contract 校验 | 最终 quota 与当前 chunked path 相同；legacy assembly limiter 只限制本地拼装写，不影响 offset staging | `assemble_legacy_local_chunks_to_temp_file` 或 `stream_legacy_local_chunks_into_writer` -> 当前 chunked persist/finalize | `assembled` 可能在 retryable storage/DB 失败后保留，但不能触发 offset-staging 判断；进入兼容路径会 warn，计划在 `0.5.0` 移除 |
| presigned single | session status `presigned`；客户端 PUT 到 `object_temp_key` | complete 前读取 temp object metadata；copy 到 final key 后再次读取 final object metadata；两者都必须等于 `session.total_size` | complete 阶段没有独立 quota precheck；`finalize_upload_session_file` 在 DB 事务内创建 blob/file、atomic charge、标 completed | `complete_presigned_upload` -> `copy_presigned_object_to_final_key` -> `finalize_verified_opaque_upload_session` -> `finalize_upload_session_file` | temp object 缺失或大小不匹配会失败，大小不匹配会尝试删除 temp object；DB finalize 失败后删除 copied final object；成功后 best-effort 删除 temp object；completed retry 通过 `find_file_by_session` 返回已有文件 |
| presigned object multipart | session status `presigned`；客户端直传 object multipart parts，complete 时客户端回传 parts | provider `list_uploaded_part_details` 的 part size 求和，必须等于 `session.total_size`；multipart complete 后再读 object metadata | multipart complete 前先用 part size total `check_quota`；`finalize_upload_session_file` 在 DB 事务内 atomic charge、标 completed | `complete_presigned_multipart` -> `complete_object_multipart_upload_session` -> `finalize_verified_opaque_upload_session` -> `finalize_upload_session_file` | completed parts 和 provider uploaded parts 必须连续且数量匹配；preflight size/parts/quota 失败会 abort multipart；complete 出现 retryable storage error 且 object 已存在时继续 finalize；multipart object 一旦 complete，`VerifiedUploadedBlob.cleanup = RetainCompletedMultipartObject`，因此 `finalize_upload_session_file`/DB finalize 失败后不删除已完成对象，留给后续重试或 orphan cleanup；completed retry 返回已有文件 |
| relay object multipart | session status `uploading`；每个 chunk 由服务端 relay 到 object multipart，并把 part metadata 写入 `upload_session_parts` | chunk 阶段按 `expected_chunk_size_for_upload` 验每个 payload；complete 阶段读取服务端 parts 清单，再用 provider part details 求和，必须等于 `session.total_size` | chunk 阶段不 charge；complete multipart 前用 verified part total precheck；`finalize_upload_session_file` 在 DB 事务内 atomic charge、标 completed | `complete_relay_multipart` -> `complete_object_multipart_upload_session` -> `finalize_verified_opaque_upload_session` -> `finalize_upload_session_file` | part claim 防止同一 part 并发重复上传；upload 或 DB 写 part metadata 失败会 release claim；complete preflight 失败会 abort multipart；multipart object 一旦 complete，`VerifiedUploadedBlob.cleanup = RetainCompletedMultipartObject`，因此 `finalize_upload_session_file`/DB finalize 失败后不删除已完成对象，留给后续重试或 orphan cleanup；completed retry 返回已有文件 |
| remote/follower upload transports | remote policy 通过 remote driver 暴露 direct、presigned、presigned multipart 或 relay multipart；session 状态和 `object_temp_key` / `object_multipart_id` 与对应 object-storage transport 相同 | direct relay 使用 streaming direct metadata；remote presigned single 使用 temp/final metadata；remote presigned multipart 和 remote relay multipart 使用 provider part details + final metadata | 与实际选择的 transport 相同；remote relay direct 走 `store_preuploaded_nondedup`，remote presigned / multipart 走 upload session finalize | `init_remote_upload` 只选择 transport；完成阶段复用 `upload_streaming_direct`、`complete_presigned_upload`、`complete_presigned_multipart` 或 `complete_relay_multipart` | cleanup/idempotency 继承实际 transport；remote/follower 的特殊性只在 driver/protocol 层，产品层不应新增一套平行 finalize 语义 |

## session status 与完成计划

`upload::complete::plan::determine_completion_plan` 使用已解析的 `UploadSessionKind` 选择完成计划；session 状态只负责幂等、assembling、过期和失败错误：

- `completed` -> `ReturnCompleted`，通过 `find_file_by_session` 幂等返回已有文件，不应再次 charge quota。
- `provider_presigned_single` / `remote_presigned_single` -> `CompletePresigned`。
- `provider_presigned_multipart` / `remote_presigned_multipart` -> `CompletePresignedMultipart`，客户端必须提交 parts。
- `provider_relay_multipart` / `remote_relay_multipart` -> `CompleteRelayMultipart`，parts 来自服务端已保存的 `upload_session_parts`。
- `offset_staging` / `stream_staging` / `legacy_chunk_files` -> `CompleteChunked`，要求 `received_count == total_chunks`；只有 legacy null row 才需要 compatibility classifier。

`run_upload_completion_stage` 会先把 expected status 切到 `assembling`。非 retryable 失败会把 session 标为 `failed`；retryable storage error 会尝试恢复到原状态，允许客户端重试。

## 后续迁移的验收边界

本文件只是当前契约基线，后续代码迁移仍需要完成这些 acceptance criteria：

- session complete 路径继续使用 `VerifiedUploadedBlob` 或同等明确的 verified finalization input；新 complete 入口必须显式声明 verified size、policy、storage path/blob source 和 DB finalize failure cleanup plan。
- `store_from_temp` 路径继续使用 `VerifiedTempStoreBlob` 或同等明确的 verified finalization input；新 temp-store 入口必须显式声明 staged dedup/preuploaded cleanup 责任。
- `store_preuploaded_nondedup` 路径继续使用 `VerifiedPreuploadedNondedupStoreBlob` 或同等明确的 verified finalization input；新 preuploaded store 入口必须显式校验 prepared blob 的 size/policy/storage path 一致性。
- 每个被迁移路径都要补 quota、size mismatch 和 DB finalize failure cleanup 测试；`completed retry 不重复计费` 只适用于 session complete flow。
- offset-staging 改动必须覆盖：不同 chunk 确实并行、同一 chunk 确实排他、partial range 覆盖、receipt 缺失、receipt 损坏、staging 截断和 legacy assembled 残留重试。并发测试需要 barrier/failpoint 证明任务进入了关键区，不能只用 `join!` 假设发生过竞争。
- 保持 public API request/response、session status 语义和现有成功上传行为不变。
