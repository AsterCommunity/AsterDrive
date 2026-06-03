//! 工作空间文件主链路的稳定核心动作。
//!
//! 这里尽量只保留“上传方式无关、HTTP 接入无关”的底层语义，例如：
//! 策略解析、目录路径补齐、blob / 文件记录创建、配额读写和 upload session
//! 最终落账。这样不同入口才能共享同一套文件一致性规则。

mod blob;
mod file_record;
mod finalize;
mod path;
mod policy;
mod quota;

pub(crate) use blob::{
    create_google_drive_nondedup_blob, create_nondedup_blob, create_nondedup_blob_with_key,
    create_remote_nondedup_blob, create_s3_nondedup_blob,
};
pub(crate) use file_record::{create_exact_file_from_blob, create_new_file_from_blob};
pub(crate) use file_record::{
    create_exact_file_from_blob_with_actor_username, create_new_file_from_blob_with_actor_username,
};
pub(crate) use finalize::{
    FinalizeUploadSessionFileParams, finalize_upload_session_blob_with_actor_username,
    finalize_upload_session_file,
};
#[allow(unused_imports)]
pub(crate) use path::{ParsedUploadPath, ResolvedUploadParent};
pub(crate) use path::{ensure_upload_parent_path, parse_relative_upload_path};
pub(crate) use policy::{
    VerifiedFolderPolicyHint, load_storage_limits, local_content_dedup_enabled,
    resolve_policy_for_size, resolve_policy_for_size_with_verified_folder,
};
pub(crate) use quota::{check_quota, update_storage_used, update_storage_used_for_resource_scope};
