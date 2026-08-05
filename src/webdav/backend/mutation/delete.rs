use aster_drive_model::entities::file as file_entity;
use aster_forge_db::transaction;
use aster_forge_webdav::{
    DavMutationOperation, DavMutationTargetRole, DavPath, FsError, parent_relative_path,
};

use crate::runtime::SharedRuntimeState;
use crate::services::{
    events::storage_change,
    files::{file as file_ops, folder},
};
use crate::webdav::handlers::resources::MUTATION_FOLDER_TREE_LIMITS;

use super::super::{
    AsterDavFs, AsterDavMutationError, AtomicTargetRevalidation, DavMutationConditions,
    ResolvedNode, lock, map_ancestor_lock_error, map_atomic_lock_error, path_resolver,
    revalidate_atomic_target, to_fs_error,
};

impl AsterDavFs {
    /// Deletes one resource and all locks rooted in its namespace in one writer commit.
    pub(crate) async fn delete_with_locks(
        &self,
        path: &DavPath,
        is_collection: bool,
        operation: DavMutationOperation,
        role: DavMutationTargetRole,
        conditions: DavMutationConditions<'_>,
    ) -> Result<(), AsterDavMutationError> {
        let txn = transaction::begin(self.state.writer_db())
            .await
            .map_err(|_| AsterDavMutationError::Backend)?;
        let mut lock_mutation = lock::WebDavLockMutation::begin(&txn, self.scope)
            .await
            .map_err(map_ancestor_lock_error)?;
        lock_mutation
            .lock_ancestor_entities(self.scope, self.root_folder_id, path)
            .await
            .map_err(map_ancestor_lock_error)?;

        let node =
            path_resolver::resolve_path_in_scope(&txn, self.scope, path, self.root_folder_id)
                .await
                .map_err(AsterDavMutationError::FileSystem)?;

        enum Deleted {
            File(file_entity::Model),
            Folder(folder::FolderTreeDeletion),
        }

        let prepared_folder = match (&node, is_collection) {
            (ResolvedNode::Folder(folder), true) => Some(
                folder::lock_tree_for_deletion_on(
                    &txn,
                    self.scope,
                    folder.id,
                    Some(MUTATION_FOLDER_TREE_LIMITS),
                    true,
                )
                .await
                .map_err(to_fs_error)
                .map_err(AsterDavMutationError::FileSystem)?,
            ),
            (ResolvedNode::File(_), false) => None,
            _ => return Err(AsterDavMutationError::FileSystem(FsError::Forbidden)),
        };

        revalidate_atomic_target(
            &txn,
            lock_mutation.namespace_id(),
            self.scope,
            self.root_folder_id,
            AtomicTargetRevalidation {
                path,
                check_locks: true,
                deep: is_collection,
            },
            &conditions,
        )
        .await?;
        if let Some(parent) = parent_relative_path(path.as_str())
            && let Ok(parent) = DavPath::new(&parent)
        {
            revalidate_atomic_target(
                &txn,
                lock_mutation.namespace_id(),
                self.scope,
                self.root_folder_id,
                AtomicTargetRevalidation {
                    path: &parent,
                    check_locks: true,
                    deep: false,
                },
                &conditions,
            )
            .await?;
        }

        let deleted = match (node, prepared_folder) {
            (ResolvedNode::File(file), None) => {
                file_ops::delete_in_scope_on(&txn, self.scope, file.id, true)
                    .await
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?;
                Deleted::File(file)
            }
            (ResolvedNode::Folder(_), Some(locked)) => Deleted::Folder(
                folder::apply_locked_tree_deletion_on(&txn, locked, chrono::Utc::now())
                    .await
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?,
            ),
            _ => return Err(AsterDavMutationError::FileSystem(FsError::Forbidden)),
        };
        lock_mutation
            .delete_rooted_locks(path)
            .await
            .map_err(map_atomic_lock_error)?;
        lock_mutation
            .finish()
            .await
            .map_err(map_atomic_lock_error)?;
        transaction::commit(txn)
            .await
            .map_err(|_| AsterDavMutationError::Backend)?;

        match deleted {
            Deleted::File(file) => {
                storage_change::publish(
                    &self.state,
                    storage_change::StorageChangeEvent::new(
                        storage_change::StorageChangeKind::FileTrashed,
                        self.scope,
                        vec![file.id],
                        vec![],
                        vec![file.folder_id],
                    ),
                );
                if operation == DavMutationOperation::Delete
                    || role == DavMutationTargetRole::Destination
                {
                    self.log_deleted_file(path, &file).await;
                }
            }
            Deleted::Folder(outcome) => {
                storage_change::publish(
                    &self.state,
                    storage_change::StorageChangeEvent::new(
                        storage_change::StorageChangeKind::FolderTrashed,
                        self.scope,
                        vec![],
                        vec![outcome.folder.id],
                        vec![outcome.folder.parent_id],
                    ),
                );
                folder::invalidate_folder_path_cache_for_ids(&self.state, &[outcome.folder.id])
                    .await;
                if operation == DavMutationOperation::Delete
                    || role == DavMutationTargetRole::Destination
                {
                    self.log_deleted_folder(path, &outcome.folder).await;
                }
            }
        }
        Ok(())
    }
}
