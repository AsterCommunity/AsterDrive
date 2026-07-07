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

## 模式矩阵

| 上传模式 / transport | 初始状态和写入位置 | trusted size source | quota precheck / atomic charge | finalize function | cleanup / idempotency |
| --- | --- | --- | --- | --- | --- |
| regular multipart/server path | 不创建 upload session；`upload_with_hints` 读取 `actix_multipart::Multipart` 到 runtime temp file | 服务端读取 multipart body 时累计的 `size`；如有 `declared_size`，必须和累计值相等 | policy resolved by actual `size`；preuploaded non-dedup blob 会在对象写入前 precheck；DB 事务内再次 `check_quota`，再 `update_storage_used` | `store_from_temp_with_hints` -> `store::from_temp` / `persist_temp_store` / `write_file_record_from_temp` | 请求临时文件在 `store_from_temp_with_hints` 返回后删除；preuploaded 对象在 DB 失败时 cleanup；dedup staged 对象只有在确认没有 blob row 引用时回滚，否则交给 orphan GC |
| local direct | 不创建 upload session；local policy 且有 `declared_size` 时直接写入 local staging path | 写入 local staging file 时累计的 `size`，必须等于 `declared_size`；dedup 时同流计算 hash | 使用已解析 local policy；和 server path 一样通过 `store_from_temp_with_hints` 做 precheck / 事务内 atomic charge | `upload_local_direct` -> `store_from_temp_with_hints` | 写入、大小不匹配、空文件或 store 结束后删除 staging file；重复请求不会通过 session 幂等，只按普通创建语义处理 |
| streaming direct | 不创建 upload session；relay request body 到 driver 的 prepared non-dedup blob | driver `metadata(storage_path).size`，必须等于 `declared_size`，并再次检查 policy max file size | relay 前先用 `declared_size` 做 quota precheck；metadata 复验后再用 `actual_size` precheck；DB 事务内再次 `check_quota` 并 `update_storage_used` | `upload_streaming_direct` -> `store_preuploaded_nondedup` | storage upload、relay、metadata、size validation、quota validation 或 DB finalize 失败时 cleanup prepared blob；成功后按正式 blob 管理 |
| local chunked | session status `uploading`；chunk 写入本地 upload temp dir；complete 时拼 assembled temp file | chunk 阶段每块必须等于 `expected_chunk_size_for_upload`；complete 阶段 assembled 文件累计 `size` 是最终落账大小 | 当前路径主要依赖 `finalize_upload_session_blob_with_actor_username` 内的 atomic `update_storage_used`；本地 non-dedup preuploaded blob 没有单独 quota precheck | `complete_chunked_upload_with_actor_username` -> `finalize_chunked_upload_session` -> `persist_assembled_upload` -> `finalize_upload_session_blob_with_actor_username` | 重复 chunk 通过无覆盖发布和 existing chunk size 检查幂等；complete 成功后删除 upload temp dir；preuploaded blob 在 DB 失败时 cleanup；dedup 对象不主动删，避免并发引用误删，交给 orphan GC |
| presigned single | session status `presigned`；客户端 PUT 到 `object_temp_key` | complete 前读取 temp object metadata；copy 到 final key 后再次读取 final object metadata；两者都必须等于 `session.total_size` | complete 阶段没有独立 quota precheck；`finalize_upload_session_file` 在 DB 事务内创建 blob/file、atomic charge、标 completed | `complete_presigned_upload` -> `copy_presigned_object_to_final_key` -> `finalize_opaque_upload_session` -> `finalize_upload_session_file` | temp object 缺失或大小不匹配会失败，大小不匹配会尝试删除 temp object；DB finalize 失败后删除 copied final object；成功后 best-effort 删除 temp object；completed retry 通过 `find_file_by_session` 返回已有文件 |
| presigned object multipart | session status `presigned`；客户端直传 object multipart parts，complete 时客户端回传 parts | provider `list_uploaded_part_details` 的 part size 求和，必须等于 `session.total_size`；multipart complete 后再读 object metadata | multipart complete 前先用 part size total `check_quota`；`finalize_upload_session_file` 在 DB 事务内 atomic charge、标 completed | `complete_presigned_multipart` -> `complete_object_multipart_upload_session` -> `finalize_opaque_upload_session` -> `finalize_upload_session_file` | completed parts 和 provider uploaded parts 必须连续且数量匹配；preflight size/parts/quota 失败会 abort multipart；complete 出现 retryable storage error 且 object 已存在时继续 finalize；completed retry 返回已有文件 |
| relay object multipart | session status `uploading`；每个 chunk 由服务端 relay 到 object multipart，并把 part metadata 写入 `upload_session_parts` | chunk 阶段按 `expected_chunk_size_for_upload` 验每个 payload；complete 阶段读取服务端 parts 清单，再用 provider part details 求和，必须等于 `session.total_size` | chunk 阶段不 charge；complete multipart 前用 verified part total precheck；`finalize_upload_session_file` 在 DB 事务内 atomic charge、标 completed | `complete_relay_multipart` -> `complete_object_multipart_upload_session` -> `finalize_opaque_upload_session` -> `finalize_upload_session_file` | part claim 防止同一 part 并发重复上传；upload 或 DB 写 part metadata 失败会 release claim；complete preflight 失败会 abort multipart；completed retry 返回已有文件 |
| remote/follower upload transports | remote policy 通过 remote driver 暴露 direct、presigned、presigned multipart 或 relay multipart；session 状态和 `object_temp_key` / `object_multipart_id` 与对应 object-storage transport 相同 | direct relay 使用 streaming direct metadata；remote presigned single 使用 temp/final metadata；remote presigned multipart 和 remote relay multipart 使用 provider part details + final metadata | 与实际选择的 transport 相同；remote relay direct 走 `store_preuploaded_nondedup`，remote presigned / multipart 走 upload session finalize | `init_remote_upload` 只选择 transport；完成阶段复用 `upload_streaming_direct`、`complete_presigned_upload`、`complete_presigned_multipart` 或 `complete_relay_multipart` | cleanup/idempotency 继承实际 transport；remote/follower 的特殊性只在 driver/protocol 层，产品层不应新增一套平行 finalize 语义 |

## session status 与完成计划

`upload::complete::plan::determine_completion_plan` 当前用 session 状态和 object 字段选择完成计划：

- `completed` -> `ReturnCompleted`，通过 `find_file_by_session` 幂等返回已有文件，不应再次 charge quota。
- `presigned` + `object_multipart_id = None` -> `CompletePresigned`。
- `presigned` + `object_multipart_id = Some` -> `CompletePresignedMultipart`，客户端必须提交 parts。
- `uploading` + `object_multipart_id = Some` -> `CompleteRelayMultipart`，parts 来自服务端已保存的 `upload_session_parts`。
- 其他 `uploading` session -> `AssembleChunks`，要求 `received_count == total_chunks`。

`run_upload_completion_stage` 会先把 expected status 切到 `assembling`。非 retryable 失败会把 session 标为 `failed`；retryable storage error 会尝试恢复到原状态，允许客户端重试。

## 后续迁移的验收边界

本文件只是当前契约基线，后续代码迁移仍需要完成这些 acceptance criteria：

- 继续减少各路径中直接拼 blob/file finalization 参数的代码；新上传入口必须产出 `VerifiedUploadedBlob`、`VerifiedTempStoreBlob`、`VerifiedPreuploadedNondedupStoreBlob` 或同等明确的 verified finalization input。
- 继续减少 complete 路径中直接拼 blob/file finalization 参数的代码；新路径必须产出 `VerifiedUploadedBlob` 或同等明确的 verified finalization input。
- 给每个被迁移路径补 quota、size mismatch、DB finalize failure cleanup、completed retry 不重复计费测试。
- 保持 public API request/response、session status 语义和现有成功上传行为不变。
