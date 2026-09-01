//! Unified upload service.
//!
//! `plan` fixes metadata, placement and transport; `session` owns durable state;
//! `ingest` accepts bytes; `complete` publishes the file; `cleanup` handles all
//! terminal and recovery paths.

mod cleanup;
mod complete;
mod ingest;
mod plan;
mod session;

pub use cleanup::{
    ForceCleanupByPolicyResult, cancel_upload, cancel_upload_for_team, cleanup_expired,
    force_cleanup_by_policy,
};
pub use complete::{
    complete_upload, complete_upload_for_team, complete_upload_for_team_with_audit,
    complete_upload_with_audit,
};
pub(crate) use ingest::ingest_stream;
#[cfg(debug_assertions)]
pub use ingest::test_support;
pub use ingest::{
    upload_chunk, upload_chunk_bytes, upload_chunk_bytes_for_team, upload_chunk_for_team,
    upload_chunk_payload, upload_chunk_payload_for_team,
};
pub use plan::{
    InitUploadParams, init_upload, init_upload_for_team, init_upload_for_team_with_frontend_client,
    init_upload_with_frontend_client,
};
pub use session::{
    ChunkUploadResponse, InitUploadResponse, ProviderResumableUploadResponse,
    RecoverableUploadSessionResponse, UploadProgressResponse, get_progress, get_progress_for_team,
    list_recoverable_sessions, list_recoverable_sessions_for_team, presign_parts,
    presign_parts_for_team,
};
