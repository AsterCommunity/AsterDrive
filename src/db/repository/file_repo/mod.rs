//! `file_repo` 仓储聚合入口。

mod blob;
mod common;
mod mutation;
mod query;
mod trash;

pub use blob::{
    AdminFileBlobFilters, BLOB_CLEANUP_CLAIMED_REF_COUNT, FindOrCreateBlobResult,
    StoragePolicyBlobHashKindSummary, StoragePolicyBlobSummary, StoragePolicyMissingBlobSummary,
    blob_storage_path_exists_for_policy, claim_blob_cleanup, clear_thumbnail_metadata,
    count_all_blobs, count_blob_refs_from_files, count_blob_refs_from_files_for_blob,
    count_blob_refs_from_files_for_blobs, count_blobs_by_policy,
    count_matching_hashes_between_policies, count_opaque_hash_conflicts_between_policies,
    create_blob, decrement_blob_ref_count, decrement_blob_ref_count_by,
    decrement_blob_ref_counts_by, delete_blob, delete_blob_by_id, delete_blob_if_cleanup_claimed,
    delete_blobs, find_active_blob_by_hash, find_admin_blobs_paginated, find_blob_by_hash,
    find_blob_by_id, find_blob_storage_paths_by_storage_paths, find_blobs_by_ids,
    find_blobs_by_policy_paginated, find_blobs_paginated, find_or_create_blob,
    increment_blob_ref_count, increment_blob_ref_count_by, increment_blob_ref_counts_by,
    lock_blob_by_id, move_blob_policy_if_current, reset_blob_ref_count_to_zero,
    restore_blob_cleanup_claim, set_blob_ref_count, set_thumbnail_metadata, sum_blob_bytes,
    sum_blob_bytes_by_policy, summarize_blob_hash_kinds_by_policy, summarize_blobs_by_policy,
    summarize_missing_blobs_between_policies,
};
pub(crate) use common::FileScope;
pub use common::{
    duplicate_name_error, duplicate_name_message, is_any_duplicate_name_error,
    is_duplicate_name_error, is_name_conflict_db_err, map_bulk_name_db_err, map_name_db_err,
};
pub use mutation::{
    CreateFileWithBlobInput, create, create_many, create_with_blob, move_many_to_folder,
    replace_file_blob_refs,
};
pub use query::{
    AdminBlobUploaderRef, AdminFileFilters, count_live_files,
    find_admin_blob_uploader_refs_for_blobs, find_admin_file_by_id, find_admin_files_paginated,
    find_all_in_folders, find_by_blob_id, find_by_folder, find_by_folder_cursor, find_by_folders,
    find_by_id, find_by_ids, find_by_ids_in_personal_scope, find_by_ids_in_team_scope,
    find_by_name_in_folder, find_by_name_in_team_folder, find_by_names_in_folder,
    find_by_names_in_team_folder, find_by_team_folder, find_by_team_folder_cursor,
    find_by_team_folders, lock_by_id, resolve_unique_filename, resolve_unique_team_filename,
    sum_live_file_bytes,
};
pub(crate) use query::{FileIdSize, find_id_size_by_folders};
pub use trash::{
    delete, delete_many, find_all_by_team, find_all_by_team_paginated, find_all_by_user,
    find_all_by_user_paginated, find_deleted_by_user, find_deleted_in_folder, find_expired_deleted,
    find_top_level_deleted_by_team_paginated, find_top_level_deleted_paginated, restore,
    restore_many, soft_delete, soft_delete_many,
};
