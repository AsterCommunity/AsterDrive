use aster_drive_model::types::EntityType;
use aster_forge_db::transaction;
use aster_forge_webdav::{DavPath, FsError};

use crate::db::repository::file_repo;
use crate::services::{
    events::storage_change,
    files::{file as file_ops, folder},
    ops::audit,
};
use crate::webdav::handlers::resources::MUTATION_FOLDER_TREE_LIMITS;

use super::super::{
    AsterDavFs, AsterDavMutationError, AtomicTargetRevalidation, DavMutationConditions,
    DeletedResource, ResolvedNode, copy_visible_entity_properties_on, lock,
    map_ancestor_lock_error, map_atomic_lock_error, path_resolver, revalidate_atomic_target,
    to_fs_error,
};

impl AsterDavFs {
    /// Copies one file and atomically preserves/rebinds locks rooted at an overwritten destination.
    pub(crate) async fn copy_file_with_locks(
        &self,
        source: &DavPath,
        destination: &DavPath,
        conditions: DavMutationConditions<'_>,
    ) -> Result<(), AsterDavMutationError> {
        let txn = transaction::begin(self.state.writer_db())
            .await
            .map_err(|_| AsterDavMutationError::Backend)?;
        let mut lock_mutation = lock::WebDavLockMutation::begin(&txn, self.scope)
            .await
            .map_err(map_ancestor_lock_error)?;
        lock_mutation
            .lock_ancestor_entities(self.scope, self.root_folder_id, destination)
            .await
            .map_err(map_ancestor_lock_error)?;
        let source_file = match path_resolver::resolve_path_in_scope(
            &txn,
            self.scope,
            source,
            self.root_folder_id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?
        {
            ResolvedNode::File(file) => file_repo::lock_by_id(&txn, file.id)
                .await
                .map_err(to_fs_error)
                .map_err(AsterDavMutationError::FileSystem)?,
            _ => return Err(AsterDavMutationError::FileSystem(FsError::Forbidden)),
        };
        revalidate_atomic_target(
            &txn,
            lock_mutation.namespace_id(),
            self.scope,
            self.root_folder_id,
            AtomicTargetRevalidation {
                path: source,
                check_locks: false,
                deep: false,
            },
            &conditions,
        )
        .await?;
        let destination_node = match path_resolver::resolve_path_in_scope(
            &txn,
            self.scope,
            destination,
            self.root_folder_id,
        )
        .await
        {
            Ok(node) => Some(node),
            Err(FsError::NotFound) => None,
            Err(error) => return Err(AsterDavMutationError::FileSystem(error)),
        };
        let overwritten = destination_node.as_ref().and_then(|node| match node {
            ResolvedNode::File(file) => Some(DeletedResource::File(file.clone())),
            ResolvedNode::Folder(folder) => Some(DeletedResource::Folder(folder.clone())),
            ResolvedNode::Root => None,
        });
        let prepared_destination_folder = match &destination_node {
            Some(ResolvedNode::Folder(folder)) => Some(
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
            _ => None,
        };
        if let Some(node) = &destination_node {
            revalidate_atomic_target(
                &txn,
                lock_mutation.namespace_id(),
                self.scope,
                self.root_folder_id,
                AtomicTargetRevalidation {
                    path: destination,
                    check_locks: true,
                    deep: matches!(node, ResolvedNode::Folder(_)),
                },
                &conditions,
            )
            .await?;
        }
        if let Some(parent) = destination.parent() {
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
        if let Some(node) = destination_node {
            match (node, prepared_destination_folder) {
                (ResolvedNode::File(file), None) => {
                    file_ops::delete_in_scope_on(&txn, self.scope, file.id, true)
                        .await
                        .map_err(to_fs_error)
                        .map_err(AsterDavMutationError::FileSystem)?;
                }
                (ResolvedNode::Folder(_), Some(locked)) => {
                    folder::apply_locked_tree_deletion_on(&txn, locked, chrono::Utc::now())
                        .await
                        .map_err(to_fs_error)
                        .map_err(AsterDavMutationError::FileSystem)?;
                }
                _ => return Err(AsterDavMutationError::FileSystem(FsError::Forbidden)),
            }
            lock_mutation
                .delete_descendant_rooted_locks(destination)
                .await
                .map_err(map_atomic_lock_error)?;
        }
        let (destination_parent_id, destination_name) = path_resolver::resolve_parent_in_scope(
            &txn,
            self.scope,
            destination,
            self.root_folder_id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?;
        let copied = file_ops::duplicate_file_record_in_scope_on(
            &txn,
            self.scope,
            &source_file,
            destination_parent_id,
            &destination_name,
        )
        .await
        .map_err(to_fs_error)
        .map_err(AsterDavMutationError::FileSystem)?;
        copy_visible_entity_properties_on(
            &txn,
            EntityType::File,
            source_file.id,
            EntityType::File,
            copied.id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?;
        lock_mutation
            .rebind_destination_root_locks(destination, EntityType::File, copied.id)
            .await
            .map_err(map_atomic_lock_error)?;
        lock_mutation
            .finish()
            .await
            .map_err(map_atomic_lock_error)?;
        transaction::commit(txn)
            .await
            .map_err(|_| AsterDavMutationError::Backend)?;
        storage_change::publish(
            &self.state,
            storage_change::StorageChangeEvent::new(
                storage_change::StorageChangeKind::FileCreated,
                self.scope,
                vec![copied.id],
                vec![],
                vec![copied.folder_id],
            )
            .with_storage_delta(copied.size),
        );
        if let Some(overwritten) = overwritten.as_ref() {
            self.log_deleted_resource(destination, overwritten).await;
        }
        self.log_file_transfer(
            audit::AuditAction::FileCopy,
            source,
            destination,
            &source_file,
            &copied,
        )
        .await;
        Ok(())
    }
}
