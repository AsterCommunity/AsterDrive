//! 统一工作空间文件链路的 façade。
//!
//! route 层通常不直接区分“个人上传逻辑”和“团队上传逻辑”，而是先构造
//! `WorkspaceStorageScope`，再从这里进入统一的文件主链路。这个模块本身
//! 主要负责把 scope 校验、核心存储动作和不同上传入口重新导出成一个稳定入口。

mod blob_upload;
mod follower_stream;
mod multipart;
mod operation_context;
mod store;
mod stream_attempt;
#[cfg(test)]
mod tests;

// 调用方只需要依赖 `workspace::storage`，不必同时了解 scope helper
// 和底层核心实现分别散落在哪个文件里。
pub(crate) use crate::services::workspace::scope::{
    WorkspaceResourceScope, WorkspaceStorageScope, ensure_active_file_scope,
    ensure_active_folder_scope, ensure_file_resource_scope, ensure_file_scope,
    ensure_folder_resource_scope, ensure_folder_scope, ensure_personal_file_scope,
    invalidate_team_access_cache_for_member, invalidate_team_access_cache_for_team,
    list_files_in_folder, load_scope_actor_username, load_team_member_role, lock_folder_access_on,
    require_scope_access, require_scope_access_with_db, require_team_access,
    require_team_access_with_db, require_team_management_access, verify_file_access,
    verify_file_access_for_read, verify_folder_access, verify_folder_access_for_read,
};
pub(crate) use crate::services::workspace::storage_core::{
    BlobPolicyRequest, FinalizeUploadSessionFileParams, ParsedUploadPath, VerifiedFolderPolicyHint,
    check_quota, create_exact_file_from_blob, create_exact_file_from_blob_with_actor_username,
    create_new_file_from_blob, create_new_file_from_blob_with_actor_username,
    create_nondedup_blob_with_key, create_opaque_nondedup_blob,
    ensure_policy_available_for_folder_binding, ensure_upload_parent_path,
    ensure_upload_parent_path_on, finalize_upload_session_blob_with_actor_username,
    finalize_upload_session_file, load_storage_limits, local_content_dedup_enabled,
    lock_storage_usage, lock_storage_usage_for_resource_scope, parse_relative_upload_path,
    resolve_blob_policy_for_write, resolve_verified_folder_policy_hint,
    resolve_verified_folder_policy_hint_on, update_storage_used,
    update_storage_used_for_resource_scope,
};

pub(crate) use crate::services::workspace::scope::load_scope_actor_username_cached;
pub(crate) use crate::storage::connectors::{
    StorageConnectorUploadTransport as PolicyUploadTransport, resolve_policy_upload_transport,
    streaming_direct_upload_eligible,
};
pub(crate) use blob_upload::{
    PreparedNonDedupBlobUpload, cleanup_preuploaded_blob_upload, nondedup_storage_path_for_policy,
    persist_preuploaded_blob, prepare_non_dedup_blob_upload, upload_reader_to_prepared_blob,
    upload_temp_file_to_prepared_blob, upload_temp_file_to_prepared_blob_cancellable,
};
pub(crate) use follower_stream::{
    FollowerUploadBody, compose_follower_objects, write_follower_object,
};
pub(crate) use multipart::{WorkspaceUploadHints, upload_with_hints};
pub(crate) use operation_context::{StorageCancellationCheck, StorageOperationContext};
pub(crate) use store::from_temp::store_from_temp_internal;
pub(crate) use store::{
    EmptyFileNameMode, FileWritePrecondition, PreparedEmptyFile, StoreFromTempHints,
    StoreFromTempParams, StorePreuploadedNondedupParams, create_empty,
    create_empty_from_relative_path_with_idempotency, create_empty_with_idempotency,
    store_from_temp_exact_name_silent_with_hints, store_from_temp_exact_name_with_hints,
    store_from_temp_with_hints, store_preuploaded_nondedup,
};
pub(crate) use stream_attempt::{
    StreamUploadMetricsGuard, abort_direct_stream_attempt, cleanup_stream_upload_attempt,
};

// Local content-dedup 会在不把整文件读入内存的前提下流式计算 SHA-256。
const HASH_BUF_SIZE: usize = 65536;

#[derive(Clone, Copy)]
pub(crate) enum NewFileMode {
    ResolveUnique,
    Exact,
}
