use super::*;

use sea_orm::ConnectionTrait;

#[path = "persist.rs"]
mod persist;
#[path = "prepare.rs"]
mod prepare;
#[path = "write_record.rs"]
mod write_record;

#[derive(Clone)]
pub(super) struct DedupTarget {
    pub file_hash: String,
    pub storage_path: String,
}

#[derive(Clone)]
pub(super) enum TempBlobPlan {
    Dedup(DedupTarget),
    Preuploaded(PreparedNonDedupBlobUpload),
}

pub(crate) async fn store_from_temp_internal(
    state: &PrimaryAppState,
    params: StoreFromTempParams<'_>,
    hints: StoreFromTempHints<'_>,
    new_file_mode: NewFileMode,
    emit_storage_event: bool,
) -> Result<file::Model> {
    let prepared = prepare::prepare_store_from_temp(state, params, hints).await?;
    let scope = prepared.scope;
    let existing_file_id = prepared.existing_file_id;
    let storage_delta = prepared.storage_delta;
    let overwritten = existing_file_id.is_some();
    let result = persist::persist_temp_store(state, prepared, new_file_mode).await?;

    if emit_storage_event {
        let event_kind = if overwritten {
            storage_change_service::StorageChangeKind::FileUpdated
        } else {
            storage_change_service::StorageChangeKind::FileCreated
        };
        storage_change_service::publish(
            state,
            storage_change_service::StorageChangeEvent::new(
                event_kind,
                scope,
                vec![result.id],
                vec![],
                vec![result.folder_id],
            )
            .with_storage_delta(storage_delta),
        );
    }

    if let Some(existing_id) = existing_file_id {
        crate::services::version_service::cleanup_excess(state, existing_id).await?;
    }

    tracing::debug!(
        scope = ?scope,
        file_id = result.id,
        blob_id = result.blob_id,
        folder_id = result.folder_id,
        overwritten,
        size = result.size,
        "stored file from temp"
    );

    Ok(result)
}

pub(super) async fn revalidate_overwrite_target<C: ConnectionTrait>(
    txn: &C,
    scope: WorkspaceStorageScope,
    old_file: &file::Model,
    skip_lock_check: bool,
) -> Result<file::Model> {
    let current_file = file_repo::lock_by_id(txn, old_file.id).await?;
    crate::services::workspace_storage_service::ensure_active_file_scope(&current_file, scope)?;

    if current_file.blob_id != old_file.blob_id {
        return Err(precondition_failed_with_subcode(
            "file.modified_during_write",
            "file changed while upload body was being received",
        ));
    }

    if current_file.is_locked {
        if !skip_lock_check {
            return Err(AsterError::resource_locked("file is locked"));
        }

        let lock = crate::db::repository::lock_repo::find_by_entity(
            txn,
            crate::types::EntityType::File,
            current_file.id,
        )
        .await?;
        if let Some(lock) = lock
            && lock.owner_id != Some(scope.actor_user_id())
        {
            return Err(AsterError::resource_locked(
                "file is locked by another user",
            ));
        }
    }

    Ok(current_file)
}
