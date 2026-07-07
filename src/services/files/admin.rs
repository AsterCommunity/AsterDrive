//! Admin observability service for files and file blobs.

use std::collections::{HashMap, HashSet};

use crate::api::dto::admin::{
    AdminFileBlobDetail, AdminFileBlobHashKind, AdminFileBlobHealth, AdminFileBlobInfo,
    AdminFileBlobListQuery, AdminFileBlobReferenceFile, AdminFileBlobReferenceVersion,
    AdminFileBlobSummary, AdminFileDetail, AdminFileInfo, AdminFileListQuery,
    AdminFileVersionSummary,
};
use crate::api::pagination::{OffsetPage, load_offset_page};
use crate::db::repository::{file_repo, version_repo};
use crate::entities::{file, file_blob, file_version};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{profile_service, user_service};

pub async fn list_files(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    query: &AdminFileListQuery,
) -> Result<OffsetPage<AdminFileInfo>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) = file_repo::find_admin_files_paginated(
            state.reader_db(),
            limit,
            offset,
            file_repo::AdminFileFilters {
                name: query.name.as_deref(),
                blob_id: query.blob_id,
                policy_id: query.policy_id,
                owner_user_id: query.owner_user_id,
                team_id: query.team_id,
                deleted: query.deleted,
                sort_by: query.sort_by(),
                sort_order: query.sort_order(),
            },
        )
        .await?;
        let creator_ids = items
            .iter()
            .filter_map(|(file, _)| file.created_by_user_id)
            .collect::<Vec<_>>();
        let creators = user_service::user_summaries_by_ids(
            state,
            &creator_ids,
            profile_service::AvatarAudience::AdminUser,
        )
        .await?;
        Ok((
            items
                .into_iter()
                .map(|item| to_admin_file_info(item, &creators))
                .collect(),
            total,
        ))
    })
    .await
}

pub async fn get_file(state: &impl SharedRuntimeState, file_id: i64) -> Result<AdminFileDetail> {
    let (file, blob) = file_repo::find_admin_file_by_id(state.reader_db(), file_id).await?;
    let versions = version_repo::find_by_file_id(state.reader_db(), file_id).await?;
    let version_blob_ids = versions
        .iter()
        .map(|version| version.blob_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let version_blobs = file_repo::find_blobs_by_ids(state.reader_db(), &version_blob_ids).await?;
    let versions = versions
        .into_iter()
        .map(|version| {
            let blob = version_blobs
                .get(&version.blob_id)
                .cloned()
                .ok_or_else(|| {
                    AsterError::internal_error(format!(
                        "file_version #{} references missing blob #{}",
                        version.id, version.blob_id
                    ))
                })?;
            Ok(to_admin_version_summary(version, blob))
        })
        .collect::<Result<Vec<_>>>()?;

    let creators = user_service::user_summaries_by_ids(
        state,
        &file.created_by_user_id.into_iter().collect::<Vec<_>>(),
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    Ok(AdminFileDetail {
        file: to_admin_file_info((file, blob), &creators),
        versions,
    })
}

pub async fn list_blobs(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    query: &AdminFileBlobListQuery,
) -> Result<OffsetPage<AdminFileBlobInfo>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) = file_repo::find_admin_blobs_paginated(
            state.reader_db(),
            limit,
            offset,
            file_repo::AdminFileBlobFilters {
                hash: query.hash.as_deref(),
                policy_id: query.policy_id,
                storage_path: query.storage_path.as_deref(),
                ref_count_min: query.ref_count_min,
                ref_count_max: query.ref_count_max,
                size_min: query.size_min,
                size_max: query.size_max,
                sort_by: query.sort_by(),
                sort_order: query.sort_order(),
            },
        )
        .await?;
        let items = enrich_admin_blob_infos(state, items).await?;
        Ok((items, total))
    })
    .await
}

pub async fn get_blob(
    state: &impl SharedRuntimeState,
    blob_id: i64,
) -> Result<AdminFileBlobDetail> {
    let blob = file_repo::find_blob_by_id(state.reader_db(), blob_id).await?;
    let files = file_repo::find_by_blob_id(state.reader_db(), blob_id).await?;
    let versions = version_repo::find_by_blob_id(state.reader_db(), blob_id).await?;
    let file_ref_count = i64::try_from(files.len())
        .map_err(|_| AsterError::internal_error("blob file reference count overflow"))?;
    let version_ref_count = i64::try_from(versions.len())
        .map_err(|_| AsterError::internal_error("blob version reference count overflow"))?;
    let uploader_ids = collect_file_uploader_ids(&files);
    let users = user_service::user_summaries_by_ids(
        state,
        &uploader_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;
    let uploaders = summarize_blob_uploaders(&files, &users);
    let uploader_count = i64::try_from(uploader_ids.len())
        .map_err(|_| AsterError::internal_error("blob uploader count overflow"))?;

    Ok(AdminFileBlobDetail {
        blob: to_admin_blob_info(
            blob,
            file_ref_count,
            version_ref_count,
            uploader_count,
            uploaders,
        )?,
        files: files
            .into_iter()
            .map(|file| to_blob_reference_file(file, &users))
            .collect(),
        file_versions: versions
            .into_iter()
            .map(to_blob_reference_version)
            .collect(),
    })
}

fn to_admin_file_info(
    (file, blob): (file::Model, file_blob::Model),
    creators: &HashMap<i64, user_service::UserSummary>,
) -> AdminFileInfo {
    let created_by = file
        .created_by_user_id
        .and_then(|user_id| creators.get(&user_id).cloned());
    AdminFileInfo {
        id: file.id,
        name: file.name,
        folder_id: file.folder_id,
        team_id: file.team_id,
        blob_id: file.blob_id,
        size: file.size,
        owner_user_id: file.owner_user_id,
        created_by_user_id: file.created_by_user_id,
        created_by_username: file.created_by_username,
        created_by,
        mime_type: file.mime_type,
        extension: file.extension,
        compound_extension: file.compound_extension,
        file_category: file.file_category,
        created_at: file.created_at,
        updated_at: file.updated_at,
        deleted_at: file.deleted_at,
        is_locked: file.is_locked,
        blob: to_blob_summary(blob),
    }
}

fn to_admin_version_summary(
    version: file_version::Model,
    blob: file_blob::Model,
) -> AdminFileVersionSummary {
    AdminFileVersionSummary {
        id: version.id,
        file_id: version.file_id,
        blob_id: version.blob_id,
        version: version.version,
        size: version.size,
        created_at: version.created_at,
        blob: to_blob_summary(blob),
    }
}

fn to_blob_summary(blob: file_blob::Model) -> AdminFileBlobSummary {
    AdminFileBlobSummary {
        id: blob.id,
        hash: blob.hash,
        size: blob.size,
        policy_id: blob.policy_id,
        storage_path: blob.storage_path,
    }
}

fn to_admin_blob_info(
    blob: file_blob::Model,
    file_ref_count: i64,
    version_ref_count: i64,
    uploader_count: i64,
    uploaders: Vec<user_service::UserSummary>,
) -> Result<AdminFileBlobInfo> {
    let hash_kind = blob_hash_kind(&blob.hash);
    let actual_ref_count = file_ref_count
        .checked_add(version_ref_count)
        .ok_or_else(|| AsterError::internal_error("blob actual reference count overflow"))?;
    let health = blob_health(blob.ref_count, actual_ref_count);
    Ok(AdminFileBlobInfo {
        id: blob.id,
        hash: blob.hash,
        size: blob.size,
        policy_id: blob.policy_id,
        storage_path: blob.storage_path,
        thumbnail_path: blob.thumbnail_path,
        thumbnail_processor: blob.thumbnail_processor,
        thumbnail_version: blob.thumbnail_version,
        ref_count: blob.ref_count,
        created_at: blob.created_at,
        updated_at: blob.updated_at,
        hash_kind,
        file_ref_count,
        version_ref_count,
        actual_ref_count,
        health,
        uploader_count,
        uploaders,
    })
}

async fn enrich_admin_blob_infos(
    state: &impl SharedRuntimeState,
    blobs: Vec<file_blob::Model>,
) -> Result<Vec<AdminFileBlobInfo>> {
    let blob_ids = blobs.iter().map(|blob| blob.id).collect::<Vec<_>>();
    let file_ref_counts =
        file_repo::count_blob_refs_from_files_for_blobs(state.reader_db(), &blob_ids).await?;
    let version_ref_counts =
        version_repo::count_blob_refs_from_versions_for_blobs(state.reader_db(), &blob_ids).await?;
    let uploader_refs =
        file_repo::find_admin_blob_uploader_refs_for_blobs(state.reader_db(), &blob_ids).await?;
    let uploader_ids = uploader_refs
        .values()
        .flatten()
        .map(|uploader| uploader.user_id)
        .collect::<Vec<_>>();
    let users = user_service::user_summaries_by_ids(
        state,
        &uploader_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    blobs
        .into_iter()
        .map(|blob| {
            let file_ref_count = file_ref_counts.get(&blob.id).copied().unwrap_or(0);
            let version_ref_count = version_ref_counts.get(&blob.id).copied().unwrap_or(0);
            let uploaders = uploader_refs
                .get(&blob.id)
                .map(|refs| summarize_blob_uploader_refs(refs, &users))
                .unwrap_or_default();
            let uploader_count = uploader_refs
                .get(&blob.id)
                .map(|refs| i64::try_from(refs.len()))
                .transpose()
                .map_err(|_| AsterError::internal_error("blob uploader count overflow"))?
                .unwrap_or(0);
            to_admin_blob_info(
                blob,
                file_ref_count,
                version_ref_count,
                uploader_count,
                uploaders,
            )
        })
        .collect()
}

fn collect_file_uploader_ids(files: &[file::Model]) -> Vec<i64> {
    let mut seen = HashSet::new();
    let mut user_ids = Vec::new();
    for file in files {
        if let Some(user_id) = file.created_by_user_id
            && seen.insert(user_id)
        {
            user_ids.push(user_id);
        }
    }
    user_ids
}

fn summarize_blob_uploaders(
    files: &[file::Model],
    users: &HashMap<i64, user_service::UserSummary>,
) -> Vec<user_service::UserSummary> {
    collect_file_uploader_ids(files)
        .into_iter()
        .filter_map(|user_id| users.get(&user_id).cloned())
        .collect()
}

fn summarize_blob_uploader_refs(
    refs: &[file_repo::AdminBlobUploaderRef],
    users: &HashMap<i64, user_service::UserSummary>,
) -> Vec<user_service::UserSummary> {
    refs.iter()
        .filter_map(|uploader| users.get(&uploader.user_id).cloned())
        .collect()
}

fn blob_health(recorded_ref_count: i32, actual_ref_count: i64) -> AdminFileBlobHealth {
    if recorded_ref_count == file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT {
        AdminFileBlobHealth::CleanupClaimed
    } else if i64::from(recorded_ref_count) != actual_ref_count {
        AdminFileBlobHealth::RefCountMismatch
    } else if recorded_ref_count == 0 && actual_ref_count == 0 {
        AdminFileBlobHealth::Orphan
    } else {
        AdminFileBlobHealth::Healthy
    }
}

fn to_blob_reference_file(
    file: file::Model,
    users: &HashMap<i64, user_service::UserSummary>,
) -> AdminFileBlobReferenceFile {
    let created_by = file
        .created_by_user_id
        .and_then(|user_id| users.get(&user_id).cloned());
    AdminFileBlobReferenceFile {
        id: file.id,
        name: file.name,
        folder_id: file.folder_id,
        team_id: file.team_id,
        owner_user_id: file.owner_user_id,
        created_by_user_id: file.created_by_user_id,
        created_by_username: file.created_by_username,
        created_by,
        size: file.size,
        mime_type: file.mime_type,
        created_at: file.created_at,
        updated_at: file.updated_at,
        deleted_at: file.deleted_at,
    }
}

fn to_blob_reference_version(version: file_version::Model) -> AdminFileBlobReferenceVersion {
    AdminFileBlobReferenceVersion {
        id: version.id,
        file_id: version.file_id,
        version: version.version,
        size: version.size,
        created_at: version.created_at,
    }
}

fn blob_hash_kind(hash: &str) -> AdminFileBlobHashKind {
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        AdminFileBlobHashKind::ContentSha256
    } else {
        AdminFileBlobHashKind::Opaque
    }
}

#[cfg(test)]
mod tests {
    use super::{AdminFileBlobHashKind, AdminFileBlobHealth, blob_hash_kind, blob_health};

    #[test]
    fn blob_hash_kind_detects_content_sha256() {
        assert_eq!(
            blob_hash_kind("0123456789abcdef0123456789abcdef0123456789ABCDEF0123456789ABCDEF"),
            AdminFileBlobHashKind::ContentSha256
        );
        assert_eq!(blob_hash_kind("not-sha256"), AdminFileBlobHashKind::Opaque);
    }

    #[test]
    fn blob_health_marks_operational_states() {
        assert_eq!(blob_health(2, 2), AdminFileBlobHealth::Healthy);
        assert_eq!(blob_health(0, 0), AdminFileBlobHealth::Orphan);
        assert_eq!(blob_health(7, 2), AdminFileBlobHealth::RefCountMismatch);
        assert_eq!(blob_health(-1, 0), AdminFileBlobHealth::CleanupClaimed);
    }
}
