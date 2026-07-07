//! 团队服务子模块：`archive`。

use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::db::repository::{
    file_repo, folder_repo, lock_repo, property_repo, share_repo, team_repo, upload_session_repo,
};
use crate::entities::{team, upload_session};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit;
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::types::EntityType;

const DEFAULT_TEAM_ARCHIVE_RETENTION_DAYS: i64 = 7;
const TEAM_ARCHIVE_BATCH_SIZE: u64 = 1_000;

fn load_team_archive_retention_days(state: &PrimaryAppState) -> i64 {
    let Some(raw) = state.runtime_config().get("team_archive_retention_days") else {
        return DEFAULT_TEAM_ARCHIVE_RETENTION_DAYS;
    };

    match raw.trim().parse::<i64>() {
        Ok(value) if value >= 0 => value,
        Ok(_) | Err(_) => {
            tracing::warn!(
                "invalid team_archive_retention_days value '{}', using default",
                raw
            );
            DEFAULT_TEAM_ARCHIVE_RETENTION_DAYS
        }
    }
}

async fn cleanup_team_upload_sessions(
    state: &PrimaryAppState,
    sessions: &[upload_session::Model],
) -> Result<bool> {
    let mut cleaned_temp_objects = 0u64;
    let mut aborted_multipart_uploads = 0u64;
    let mut incomplete_cleanups = 0u64;
    for session in sessions {
        let Some(temp_key) = session.object_temp_key.as_deref() else {
            continue;
        };
        let Some(policy) = state.policy_snapshot().get_policy(session.policy_id) else {
            tracing::warn!(
                upload_id = %session.id,
                policy_id = session.policy_id,
                temp_key,
                "failed to load storage policy while cleaning team upload session"
            );
            incomplete_cleanups += 1;
            continue;
        };
        let Ok(driver) = state.driver_registry().get_driver(&policy) else {
            tracing::warn!(
                upload_id = %session.id,
                policy_id = session.policy_id,
                temp_key,
                "failed to resolve storage driver while cleaning team upload session"
            );
            incomplete_cleanups += 1;
            continue;
        };

        {
            if let Some(multipart_id) = session.object_multipart_id.as_deref() {
                if let Some(multipart) = driver.as_multipart() {
                    if let Err(err) = multipart
                        .abort_multipart_upload(temp_key, multipart_id)
                        .await
                    {
                        tracing::warn!(
                            upload_id = %session.id,
                            "failed to abort team multipart upload during cleanup: {err}"
                        );
                        incomplete_cleanups += 1;
                    } else if cleanup_team_temp_object(driver.as_ref(), session, temp_key).await {
                        aborted_multipart_uploads += 1;
                    } else {
                        incomplete_cleanups += 1;
                    }
                } else if cleanup_team_temp_object(driver.as_ref(), session, temp_key).await {
                    cleaned_temp_objects += 1;
                } else {
                    incomplete_cleanups += 1;
                }
            } else if cleanup_team_temp_object(driver.as_ref(), session, temp_key).await {
                cleaned_temp_objects += 1;
            } else {
                incomplete_cleanups += 1;
            }
        }

        let temp_dir = crate::utils::paths::upload_temp_dir(
            &state.config().server.upload_temp_dir,
            &session.id,
        );
        crate::utils::cleanup_temp_dir(&temp_dir).await;
    }

    if !sessions.is_empty() {
        tracing::debug!(
            upload_session_count = sessions.len(),
            cleaned_temp_objects,
            aborted_multipart_uploads,
            incomplete_cleanups,
            "cleaned team upload sessions"
        );
    }
    Ok(incomplete_cleanups == 0)
}

async fn cleanup_team_temp_object(
    driver: &dyn crate::storage::StorageDriver,
    session: &upload_session::Model,
    temp_key: &str,
) -> bool {
    match driver.delete(temp_key).await {
        Ok(()) => true,
        Err(err) => match driver.exists(temp_key).await {
            Ok(false) => true,
            Ok(true) => {
                tracing::warn!(
                    upload_id = %session.id,
                    temp_key,
                    "failed to delete team temp upload object during cleanup: {err}"
                );
                false
            }
            Err(exists_err) => {
                tracing::warn!(
                    upload_id = %session.id,
                    temp_key,
                    "failed to delete team temp upload object and verify existence during cleanup: delete_error={err}, exists_error={exists_err}"
                );
                false
            }
        },
    }
}

fn is_missing_cleanup_target(err: &AsterError) -> bool {
    matches!(
        err,
        AsterError::RecordNotFound(_) | AsterError::FileNotFound(_) | AsterError::FolderNotFound(_)
    )
}

async fn clear_team_locks<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<()> {
    let prefix = format!("/teams/{team_id}/");
    let locks = lock_repo::find_by_path_prefix(db, &prefix).await?;
    for lock in &locks {
        if let Err(err) = crate::services::files::lock::set_entity_locked(
            db,
            lock.entity_type,
            lock.entity_id,
            false,
        )
        .await
            && !is_missing_cleanup_target(&err)
        {
            tracing::warn!(
                lock_id = lock.id,
                team_id,
                "failed to clear team lock flag during cleanup: {err}"
            );
        }
    }
    lock_repo::delete_by_path_prefix(db, &prefix).await?;
    Ok(())
}

async fn purge_archived_team_files(state: &PrimaryAppState, team: &team::Model) -> Result<()> {
    let scope = WorkspaceStorageScope::Team {
        team_id: team.id,
        actor_user_id: team.created_by,
    };
    let mut after_file_id = None;

    loop {
        let files = file_repo::find_all_by_team_paginated(
            state.writer_db(),
            team.id,
            after_file_id,
            TEAM_ARCHIVE_BATCH_SIZE,
        )
        .await?;
        if files.is_empty() {
            break;
        }

        after_file_id = files.last().map(|file| file.id);
        crate::services::files::file::batch_purge_in_scope(state, scope, files).await?;
    }

    Ok(())
}

async fn delete_archived_team_folders<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<()> {
    let mut after_folder_id = None;

    loop {
        let folders = folder_repo::find_all_by_team_paginated(
            db,
            team_id,
            after_folder_id,
            TEAM_ARCHIVE_BATCH_SIZE,
        )
        .await?;
        if folders.is_empty() {
            break;
        }

        after_folder_id = folders.last().map(|folder| folder.id);
        let folder_ids: Vec<i64> = folders.into_iter().map(|folder| folder.id).collect();
        property_repo::delete_all_for_entities(db, EntityType::Folder, &folder_ids).await?;
        folder_repo::delete_many(db, &folder_ids).await?;
    }

    Ok(())
}

async fn force_delete_archived_team(state: &PrimaryAppState, team: team::Model) -> Result<()> {
    let team_id = team.id;
    let team_name = team.name.clone();
    tracing::info!(
        team_id,
        team_name = %team_name,
        "force deleting archived team"
    );
    let upload_sessions = upload_session_repo::find_by_team(state.writer_db(), team_id).await?;
    if !cleanup_team_upload_sessions(state, &upload_sessions).await? {
        // Upload sessions are the only durable handles for temp objects. Abort
        // the team delete before removing those rows so the next cleanup run can retry.
        return Err(AsterError::storage_driver_error(
            "team upload session cleanup incomplete",
        ));
    }

    purge_archived_team_files(state, &team).await?;
    crate::webdav::auth::invalidate_webdav_auth_for_team(state, team_id).await?;

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    team_repo::lock_archived_by_id(&txn, team_id).await?;
    upload_session_repo::delete_all_by_team(&txn, team_id).await?;
    crate::db::repository::webdav_account_repo::delete_all_by_team(&txn, team_id).await?;
    clear_team_locks(&txn, team_id).await?;
    let deleted_shares = share_repo::delete_all_by_team(&txn, team_id).await?;

    delete_archived_team_folders(&txn, team_id).await?;
    team_repo::delete(&txn, team_id).await?;
    crate::db::transaction::commit(txn).await?;
    if deleted_shares > 0 {
        crate::services::share::invalidate_active_share_target_cache_for_scope(
            state,
            crate::services::workspace::storage::WorkspaceStorageScope::Team {
                team_id,
                actor_user_id: team.created_by,
            },
        )
        .await;
        crate::services::share::invalidate_all_share_token_record_cache(state).await;
    }
    crate::services::files::folder::invalidate_folder_path_cache(state).await;
    tracing::info!(
        team_id,
        team_name = %team_name,
        upload_session_count = upload_sessions.len(),
        deleted_shares,
        "force deleted archived team"
    );

    Ok(())
}

pub async fn cleanup_expired_archived_teams(state: &PrimaryAppState) -> Result<u64> {
    let retention_days = load_team_archive_retention_days(state);
    let cutoff = Utc::now() - chrono::Duration::days(retention_days);
    let expired = team_repo::find_archived_before(state.writer_db(), cutoff).await?;

    let mut deleted = 0u64;
    let ctx = audit::AuditContext::system();
    for team in expired {
        let team_id = team.id;
        let team_name = team.name.clone();
        let archived_at = team.archived_at;
        if let Err(err) = force_delete_archived_team(state, team).await {
            tracing::warn!(team_id, "failed to delete expired archived team: {err}");
            continue;
        }
        audit::log_with_details(
            state,
            &ctx,
            audit::AuditAction::TeamCleanupExpired,
            crate::services::ops::audit::AuditEntityType::Team,
            Some(team_id),
            Some(&team_name),
            || {
                audit::details(audit::TeamCleanupAuditDetails {
                    archived_at,
                    retention_days,
                })
            },
        )
        .await;
        deleted += 1;
    }

    Ok(deleted)
}
