use crate::db::repository::file_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace::storage::{
    StorageOperationContext, WorkspaceStorageScope, check_quota, cleanup_preuploaded_blob_upload,
    persist_preuploaded_blob,
};
use aster_drive_model::entities::{file, file_blob};
use sea_orm::{ConnectionTrait, DbBackend};

use super::TempBlobPlan;
use super::contract::{
    TempStoreBlobCleanupPlan, VerifiedTempStoreBlob, VerifiedTempStoreBlobSource,
};
use super::prepare::PreparedStoreFromTemp;
use super::write_record::WriteFileRecordFromTempParams;

#[derive(Clone)]
enum TempBlobPublication {
    ExistingDedup {
        blob_id: i64,
        hash: String,
        size: i64,
        policy_id: i64,
    },
    Staged(VerifiedTempStoreBlob),
}

pub(super) async fn persist_temp_store(
    state: &PrimaryAppState,
    prepared: PreparedStoreFromTemp,
    new_file_mode: super::NewFileMode,
) -> Result<(file::Model, bool)> {
    prepared.operation_context.checkpoint()?;
    if prepared.storage_delta > 0 && !prepared.quota_prechecked {
        check_quota(state.writer_db(), prepared.scope, prepared.storage_delta).await?;
    }
    prepared.operation_context.checkpoint()?;
    match &prepared.blob_plan {
        TempBlobPlan::Dedup(target) => {
            if let Some(existing) = file_repo::find_active_blob_by_hash(
                state.writer_db(),
                &target.file_hash,
                prepared.policy.id,
            )
            .await?
                && !existing.is_virtual_empty()
                && let Some(published) = publish_temp_store(
                    state,
                    &prepared,
                    TempBlobPublication::ExistingDedup {
                        blob_id: existing.id,
                        hash: target.file_hash.clone(),
                        size: prepared.size,
                        policy_id: prepared.policy.id,
                    },
                    new_file_mode,
                )
                .await?
            {
                return Ok(published);
            }
        }
        TempBlobPlan::Preuploaded(_) => {}
    }
    let staged_dedup_target = stage_temp_blob_before_transaction(
        &prepared.blob_plan,
        prepared.driver.as_ref(),
        prepared.size,
        &prepared.temp_path,
        &prepared.operation_context,
    )
    .await?;
    let verified_blob = match VerifiedTempStoreBlob::from_staged_plan(
        &prepared.blob_plan,
        prepared.size,
        prepared.policy.id,
        staged_dedup_target,
    ) {
        Ok(verified) => verified,
        Err(error) => {
            cleanup_staged_blob_plan_after_contract_failure(
                &prepared.blob_plan,
                staged_dedup_target,
                prepared.driver.as_ref(),
                "verified temp store contract failure",
            )
            .await;
            return Err(error);
        }
    };
    if let Err(error) = prepared.operation_context.checkpoint() {
        cleanup_unpublished_temp_blob(
            &verified_blob,
            prepared.driver.as_ref(),
            "cancellation after temp file upload",
        )
        .await;
        return Err(error);
    }
    publish_temp_store(
        state,
        &prepared,
        TempBlobPublication::Staged(verified_blob),
        new_file_mode,
    )
    .await?
    .ok_or_else(|| AsterError::internal_error("staged blob publication resolved no object"))
}

/// One metadata transaction publishes either a currently adoptable blob or an owned candidate.
async fn publish_temp_store(
    state: &PrimaryAppState,
    prepared: &PreparedStoreFromTemp,
    publication_source: TempBlobPublication,
    new_file_mode: super::NewFileMode,
) -> Result<Option<(file::Model, bool)>> {
    let PreparedStoreFromTemp {
        scope,
        access_constraints,
        folder_id,
        filename,
        temp_path: _,
        size: _,
        existing_file_id: _,
        policy: _,
        driver,
        blob_plan: _,
        overwrite_ctx,
        operation_context,
        storage_delta,
        quota_prechecked: _,
        mime,
        now,
        actor_username,
        lock_credentials,
        file_precondition,
        expected_current_revision_id,
        expected_current_revision_etag,
        revision_etag,
        complete_upload_id,
        task_publication,
    } = prepared.clone();

    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_access_constraints = access_constraints.clone();
    let transaction_operation_context = operation_context.clone();
    let transaction_source = publication_source.clone();
    let transaction_filename = filename.clone();
    let transaction_mime = mime.clone();
    let transaction_overwrite_ctx = overwrite_ctx.clone();
    let transaction_actor_username = actor_username.clone();
    let transaction_lock_credentials = lock_credentials.clone();
    let transaction_file_precondition = file_precondition;
    let transaction_expected_current_revision_id = expected_current_revision_id;
    let transaction_expected_current_revision_etag = expected_current_revision_etag;
    let transaction_revision_etag = revision_etag;
    let transaction_complete_upload_id = complete_upload_id;
    let transaction_now = now;
    let create_result = aster_forge_db::transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig::deadlock(),
        move |txn| {
            let access_constraints = transaction_access_constraints.clone();
            let operation_context = transaction_operation_context.clone();
            let publication_source = transaction_source.clone();
            let filename = transaction_filename.clone();
            let mime = transaction_mime.clone();
            let overwrite_ctx = transaction_overwrite_ctx.clone();
            let actor_username = transaction_actor_username.clone();
            let lock_credentials = transaction_lock_credentials.clone();
            let file_precondition = transaction_file_precondition;
            let expected_current_revision_id = transaction_expected_current_revision_id;
            let expected_current_revision_etag = transaction_expected_current_revision_etag.clone();
            let revision_etag = transaction_revision_etag.clone();
            let complete_upload_id = transaction_complete_upload_id.clone();
            let now = transaction_now;
            let publication = task_publication.clone();
            Box::pin(async move {
                // 每次重试重新授权，绝不把第一次事务的授权证明带进重试事务。
                let access = crate::services::workspace::scope::authorize_workspace_on(
                    txn,
                    scope,
                    crate::services::workspace::scope::WorkspaceOperation::UpdateContent,
                    &access_constraints,
                )
                .await?;
                if let Some(publication) = &publication {
                    publication.lock_owner(txn).await?;
                    if let Some(file) = publication.find_file(txn).await? {
                        access.file(&file).await?;
                        return Ok(Some((file, false, false)));
                    }
                }
                let workspace = match scope {
                    crate::services::workspace::storage::WorkspaceStorageScope::Personal {
                        user_id,
                    } => crate::services::files::lock::LockWorkspace::Personal { user_id },
                    crate::services::workspace::storage::WorkspaceStorageScope::Team {
                        team_id,
                        ..
                    } => crate::services::files::lock::LockWorkspace::Team { team_id },
                };
                crate::services::files::lock::lock_workspace_for_mutation_on(txn, workspace)
                    .await?;
                if matches!(
                    file_precondition,
                    Some(super::FileWritePrecondition::Missing)
                ) {
                    let existing = match scope {
                        WorkspaceStorageScope::Personal { user_id } => {
                            file_repo::find_by_name_in_folder(txn, user_id, folder_id, &filename)
                                .await?
                        }
                        WorkspaceStorageScope::Team { team_id, .. } => {
                            file_repo::find_by_name_in_team_folder(
                                txn, team_id, folder_id, &filename,
                            )
                            .await?
                        }
                    };
                    if existing.is_some() {
                        return Err(crate::errors::precondition_failed_with_code(
                            crate::api::api_error_code::ApiErrorCode::FileModifiedDuringWrite,
                            "file appeared while upload body was being received",
                        ));
                    }
                }
                crate::services::workspace::storage::lock_storage_usage(txn, scope).await?;
                operation_context.checkpoint()?;
                if storage_delta > 0 {
                    check_quota(txn, scope, storage_delta).await?;
                }
                operation_context.checkpoint()?;

                let Some(blob) = resolve_publication_blob(txn, &publication_source).await? else {
                    // Cleanup or migration won the row before the adoption lock.
                    // The caller will materialize a private candidate outside this Tx.
                    return Ok(None);
                };
                operation_context.checkpoint()?;
                let result = super::write_record::write_file_record_from_temp(
                    access,
                    WriteFileRecordFromTempParams {
                        activity_source: operation_context.activity_source(),
                        activity_action: operation_context.content_action(),
                        folder_id,
                        filename: &filename,
                        mime: &mime,
                        blob: &blob,
                        overwrite_ctx,
                        now,
                        storage_delta,
                        new_file_mode,
                        actor_username: actor_username.as_deref(),
                        lock_credentials: &lock_credentials,
                        file_precondition: file_precondition.as_ref(),
                        expected_current_revision_id,
                        expected_current_revision_etag: expected_current_revision_etag.as_deref(),
                        revision_etag: revision_etag.as_deref(),
                    },
                )
                .await?;
                if let Some(upload_id) = complete_upload_id.as_deref()
                    && !crate::db::repository::upload_session_repo::complete_if_assembling(
                        txn, upload_id, result.id,
                    )
                    .await?
                {
                    return Err(AsterError::conflict(
                        "upload session changed while publishing the file",
                    )
                    .with_api_error_code(
                        crate::api::api_error_code::ApiErrorCode::UploadStatusConflict,
                    ));
                }
                if let Some(publication) = &publication {
                    publication.insert_file(txn, &result).await?;
                }
                operation_context.checkpoint()?;
                let retained_candidate = match &publication_source {
                    TempBlobPublication::Staged(verified) => {
                        blob.storage_path.as_deref() == Some(verified.storage_path())
                    }
                    TempBlobPublication::ExistingDedup { .. } => false,
                };
                Ok::<Option<(file::Model, bool, bool)>, AsterError>(Some((
                    result,
                    true,
                    retained_candidate,
                )))
            })
        },
        move |error: &AsterError| {
            retry_on_mysql_deadlock
                && error.database_error_kind() == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        },
    )
    .await;

    match create_result {
        Ok(Some((result, created, retained_candidate))) => {
            match (&publication_source, retained_candidate) {
                (TempBlobPublication::Staged(verified), false) => {
                    cleanup_unpublished_temp_blob(
                        verified,
                        driver.as_ref(),
                        "candidate was not selected for publication",
                    )
                    .await;
                }
                (TempBlobPublication::Staged(_), true)
                | (TempBlobPublication::ExistingDedup { .. }, _) => {}
            }
            Ok(Some((result, created)))
        }
        Ok(None) => Ok(None),
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                match &publication_source {
                    TempBlobPublication::Staged(verified) => {
                        cleanup_unpublished_temp_blob(
                            verified,
                            driver.as_ref(),
                            "DB error after temp file upload",
                        )
                        .await;
                    }
                    TempBlobPublication::ExistingDedup { .. } => {}
                }
            }
            Err(error)
        }
    }
}

async fn stage_temp_blob_before_transaction(
    blob_plan: &TempBlobPlan,
    driver: &dyn aster_drive_storage::StorageDriver,
    size: i64,
    temp_path: &str,
    operation_context: &StorageOperationContext,
) -> Result<bool> {
    match blob_plan {
        TempBlobPlan::Dedup(target) => {
            let promoted = crate::storage::drivers::local::promote_local_file_if_absent(
                driver,
                &target.storage_path,
                temp_path,
                size,
                || operation_context.checkpoint(),
            )
            .await?;
            if !promoted.created() {
                return Err(AsterError::conflict(
                    "upload candidate storage key already exists",
                ));
            }
            Ok(true)
        }
        TempBlobPlan::Preuploaded(_) => Ok(false),
    }
}

async fn cleanup_staged_blob_plan_after_contract_failure(
    blob_plan: &TempBlobPlan,
    staged_dedup_target: bool,
    driver: &dyn aster_drive_storage::StorageDriver,
    reason: &str,
) {
    match blob_plan {
        TempBlobPlan::Dedup(target) if staged_dedup_target => {
            delete_owned_dedup_candidate(driver, &target.storage_path, reason).await;
        }
        TempBlobPlan::Dedup(_) => {}
        TempBlobPlan::Preuploaded(preuploaded_blob) => {
            cleanup_preuploaded_blob_upload(driver, preuploaded_blob, reason).await;
        }
    }
}

async fn cleanup_unpublished_temp_blob(
    verified_blob: &VerifiedTempStoreBlob,
    driver: &dyn aster_drive_storage::StorageDriver,
    reason: &str,
) {
    match verified_blob.cleanup() {
        TempStoreBlobCleanupPlan::DeleteOwnedDedupCandidate => {
            delete_owned_dedup_candidate(driver, verified_blob.storage_path(), reason).await;
        }
        TempStoreBlobCleanupPlan::CleanupPreuploadedBlobOnDbFailure => {
            if let VerifiedTempStoreBlobSource::PreuploadedNonDedup { prepared } =
                verified_blob.source()
            {
                cleanup_preuploaded_blob_upload(driver, prepared, reason).await;
            }
        }
    }
}

async fn delete_owned_dedup_candidate(
    driver: &dyn aster_drive_storage::StorageDriver,
    storage_path: &str,
    reason: &str,
) {
    // The key is generated for this attempt and is never reused by hash. No
    // reference lookup can authorize deleting another attempt's private key.
    if let Err(error) = driver.delete(storage_path).await {
        tracing::warn!(
            storage_path,
            "failed to cleanup private dedup candidate after {reason}: {error}"
        );
    }
}

async fn resolve_publication_blob<C: ConnectionTrait>(
    txn: &C,
    publication: &TempBlobPublication,
) -> Result<Option<file_blob::Model>> {
    match publication {
        TempBlobPublication::ExistingDedup {
            blob_id,
            hash,
            size,
            policy_id,
        } => {
            crate::services::workspace::storage::try_adopt_existing_dedup_blob_on(
                txn, *blob_id, hash, *size, *policy_id,
            )
            .await
        }
        TempBlobPublication::Staged(verified_blob) => {
            let blob = match verified_blob.source() {
                VerifiedTempStoreBlobSource::DeduplicatedContent { file_hash } => {
                    file_repo::find_or_create_blob(
                        txn,
                        file_hash,
                        verified_blob.size(),
                        verified_blob.policy_id(),
                        verified_blob.storage_path(),
                    )
                    .await?
                    .model
                }
                VerifiedTempStoreBlobSource::PreuploadedNonDedup { prepared } => {
                    persist_preuploaded_blob(txn, prepared).await?
                }
            };
            Ok(Some(blob))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::workspace::storage::store::from_temp::prepare::prepare_store_from_temp;
    use crate::services::workspace::storage::tests::{build_test_state, replace_test_policy};
    use crate::services::workspace::storage::{StoreFromTempHints, StoreFromTempParams};

    async fn stage_candidate(
        state: &PrimaryAppState,
        root: &std::path::Path,
        policy: &aster_drive_model::entities::storage_policy::Model,
        user_id: i64,
        name: &str,
        payload: &[u8],
    ) -> (PreparedStoreFromTemp, VerifiedTempStoreBlob) {
        let temp = root.join(name);
        tokio::fs::write(&temp, payload).await.unwrap();
        let prepared = prepare_store_from_temp(
            state,
            StoreFromTempParams::new(
                WorkspaceStorageScope::Personal { user_id },
                None,
                name,
                temp.to_str().unwrap(),
                payload.len() as i64,
            ),
            StoreFromTempHints {
                resolved_policy: Some(policy.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let created = stage_temp_blob_before_transaction(
            &prepared.blob_plan,
            prepared.driver.as_ref(),
            prepared.size,
            &prepared.temp_path,
            &prepared.operation_context,
        )
        .await
        .unwrap();
        let verified = VerifiedTempStoreBlob::from_staged_plan(
            &prepared.blob_plan,
            prepared.size,
            policy.id,
            created,
        )
        .unwrap();
        (prepared, verified)
    }

    #[tokio::test]
    async fn failed_dedup_publisher_cleanup_preserves_another_staged_publication() {
        let (state, root, policy, user) = build_test_state().await;
        let replacement =
            crate::storage::connectors::test_support::with_local_content_dedup(&policy, true);
        let policy = replace_test_policy(&state, &policy, replacement).await;
        let payload = b"same bytes from two independent uploads";
        let (first, first_blob) =
            stage_candidate(&state, &root, &policy, user.id, "first.bin", payload).await;
        let (second, second_blob) =
            stage_candidate(&state, &root, &policy, user.id, "second.bin", payload).await;

        // A fails before metadata commit while B has already accepted its staged bytes.
        cleanup_unpublished_temp_blob(
            &first_blob,
            first.driver.as_ref(),
            "failed independent publisher",
        )
        .await;
        let (published, _) = publish_temp_store(
            &state,
            &second,
            TempBlobPublication::Staged(second_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        let blob = file_repo::find_blob_by_id(state.writer_db(), published.blob_id)
            .await
            .unwrap();
        assert_eq!(
            first
                .driver
                .get(blob.storage_path_for_connector().unwrap())
                .await
                .unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn dedup_publication_after_last_reference_cleanup_keeps_its_source_bytes() {
        let (state, root, policy, user) = build_test_state().await;
        let replacement =
            crate::storage::connectors::test_support::with_local_content_dedup(&policy, true);
        let policy = replace_test_policy(&state, &policy, replacement).await;
        let payload = b"new publication overlaps the previous blob retirement";
        let (first, first_blob) =
            stage_candidate(&state, &root, &policy, user.id, "first.bin", payload).await;
        let (old, _) = publish_temp_store(
            &state,
            &first,
            TempBlobPublication::Staged(first_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        let (second, second_blob) =
            stage_candidate(&state, &root, &policy, user.id, "second.bin", payload).await;
        let driver = second.driver.clone();

        // The last reference disappears after B staged, before B opens its metadata Tx.
        crate::services::files::file::purge(&state, old.id, user.id)
            .await
            .unwrap();
        let (published, _) = publish_temp_store(
            &state,
            &second,
            TempBlobPublication::Staged(second_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        let blob = file_repo::find_blob_by_id(state.writer_db(), published.blob_id)
            .await
            .unwrap();
        assert_eq!(
            driver
                .get(blob.storage_path_for_connector().unwrap())
                .await
                .unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn successful_dedup_loser_discards_only_its_candidate_and_preserves_retries() {
        let (state, root, policy, user) = build_test_state().await;
        let replacement =
            crate::storage::connectors::test_support::with_local_content_dedup(&policy, true);
        let policy = replace_test_policy(&state, &policy, replacement).await;
        let payload = b"same logical blob, distinct unpublished candidates";
        let (first, first_blob) =
            stage_candidate(&state, &root, &policy, user.id, "first.bin", payload).await;
        let (second, second_blob) =
            stage_candidate(&state, &root, &policy, user.id, "second.bin", payload).await;
        assert_ne!(first_blob.storage_path(), second_blob.storage_path());
        let unused_path = second_blob.storage_path().to_string();
        let driver = first.driver.clone();
        let (first_file, _) = publish_temp_store(
            &state,
            &first,
            TempBlobPublication::Staged(first_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        let (second_file, _) = publish_temp_store(
            &state,
            &second,
            TempBlobPublication::Staged(second_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(first_file.blob_id, second_file.blob_id);
        assert!(!driver.exists(&unused_path).await.unwrap());
        let blob = file_repo::find_blob_by_id(state.writer_db(), first_file.blob_id)
            .await
            .unwrap();
        assert_eq!(blob.ref_count, 2);
        assert_eq!(
            driver
                .get(blob.storage_path_for_connector().unwrap())
                .await
                .unwrap(),
            payload
        );
        assert!(root.join("first.bin").exists());
        assert!(root.join("second.bin").exists());
    }

    #[tokio::test]
    async fn dedup_adoption_race_falls_back_to_owned_bytes_after_cleanup_wins() {
        let (state, root, policy, user) = build_test_state().await;
        let replacement =
            crate::storage::connectors::test_support::with_local_content_dedup(&policy, true);
        let policy = replace_test_policy(&state, &policy, replacement).await;
        let payload = b"dedup cache hit becomes stale before metadata admission";
        let (first, first_blob) =
            stage_candidate(&state, &root, &policy, user.id, "first.bin", payload).await;
        let (old, _) = publish_temp_store(
            &state,
            &first,
            TempBlobPublication::Staged(first_blob),
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap()
        .unwrap();
        let old_blob = file_repo::find_blob_by_id(state.writer_db(), old.blob_id)
            .await
            .unwrap();
        let source = root.join("second.bin");
        tokio::fs::write(&source, payload).await.unwrap();
        let prepared = prepare_store_from_temp(
            &state,
            StoreFromTempParams::new(
                WorkspaceStorageScope::Personal { user_id: user.id },
                None,
                "second.bin",
                source.to_str().unwrap(),
                payload.len() as i64,
            ),
            StoreFromTempHints {
                resolved_policy: Some(policy),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let driver = prepared.driver.clone();
        crate::services::files::file::purge(&state, old.id, user.id)
            .await
            .unwrap();
        let stale_adoption = publish_temp_store(
            &state,
            &prepared,
            TempBlobPublication::ExistingDedup {
                blob_id: old_blob.id,
                hash: old_blob.hash,
                size: old_blob.size,
                policy_id: old_blob.policy_id,
            },
            super::super::NewFileMode::ResolveUnique,
        )
        .await
        .unwrap();
        assert!(stale_adoption.is_none());
        let (published, _) =
            persist_temp_store(&state, prepared, super::super::NewFileMode::ResolveUnique)
                .await
                .unwrap();
        let blob = file_repo::find_blob_by_id(state.writer_db(), published.blob_id)
            .await
            .unwrap();
        assert_eq!(
            driver
                .get(blob.storage_path_for_connector().unwrap())
                .await
                .unwrap(),
            payload
        );
        assert!(source.exists());
    }
}
