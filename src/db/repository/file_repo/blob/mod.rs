//! `file_repo` 仓储子模块：`blob`。

mod cleanup;
mod lookup;
mod maintenance;
mod ref_count;
#[cfg(test)]
mod tests;

pub use cleanup::{
    BLOB_CLEANUP_CLAIMED_REF_COUNT, claim_blob_cleanup, count_blobs_by_policy, delete_blob,
    delete_blob_if_cleanup_claimed, delete_blobs, reset_blob_ref_count_to_zero,
    restore_blob_cleanup_claim, set_blob_ref_count,
};
pub use lookup::{
    AdminFileBlobFilters, FindOrCreateBlobResult, StoragePolicyBlobHashKindSummary,
    StoragePolicyBlobSummary, StoragePolicyMissingBlobSummary,
    count_matching_hashes_between_policies, count_opaque_hash_conflicts_between_policies,
    create_blob, find_admin_blobs_paginated, find_blob_by_hash, find_blob_by_id, find_blobs_by_ids,
    find_blobs_by_policy_paginated, find_blobs_paginated, find_or_create_blob, lock_blob_by_id,
    summarize_blob_hash_kinds_by_policy, summarize_blobs_by_policy,
    summarize_missing_blobs_between_policies,
};
pub use maintenance::{
    blob_storage_path_exists_for_policy, clear_thumbnail_metadata, count_all_blobs,
    count_blob_refs_from_files, count_blob_refs_from_files_for_blob,
    count_blob_refs_from_files_for_blobs, delete_blob_by_id,
    find_blob_storage_paths_by_storage_paths, move_blob_policy_if_current, set_thumbnail_metadata,
    sum_blob_bytes, sum_blob_bytes_by_policy,
};
pub use ref_count::{
    decrement_blob_ref_count, decrement_blob_ref_count_by, decrement_blob_ref_counts_by,
    find_active_blob_by_hash, increment_blob_ref_count, increment_blob_ref_count_by,
    increment_blob_ref_counts_by,
};
