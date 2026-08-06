use aster_drive_model::types::EntityType;
use aster_forge_db::transaction;
use aster_forge_webdav::{DavMutationOperation, DavPath, FsError};
use sea_orm::Set;

use crate::db::repository::folder_repo;
use crate::services::{events::storage_change, files::file as file_ops, ops::audit};

use super::super::{
    AsterDavFs, AsterDavMutationError, AtomicTargetRevalidation, DavMutationConditions,
    DeletedResource, ResolvedNode, copy_visible_entity_properties_on, lock,
    map_ancestor_lock_error, map_atomic_lock_error, path_resolver, revalidate_atomic_target,
    to_fs_error,
};

impl AsterDavFs {
    /// Prepares one collection destination for recursive COPY/MOVE while preserving the existing
    /// destination lock scope in the same writer commit.
    pub(crate) async fn prepare_collection_with_locks(
        &self,
        source: &DavPath,
        destination: &DavPath,
        operation: DavMutationOperation,
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
        let source_folder = match path_resolver::resolve_path_in_scope(
            &txn,
            self.scope,
            source,
            self.root_folder_id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?
        {
            ResolvedNode::Folder(folder) => folder_repo::lock_by_id(&txn, folder.id)
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
                check_locks: operation == DavMutationOperation::Move,
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
            ResolvedNode::Folder(_) | ResolvedNode::Root => None,
        });
        if destination_node.is_some() {
            revalidate_atomic_target(
                &txn,
                lock_mutation.namespace_id(),
                self.scope,
                self.root_folder_id,
                AtomicTargetRevalidation {
                    path: destination,
                    check_locks: true,
                    deep: false,
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
        let (destination_parent_id, destination_name) = path_resolver::resolve_parent_in_scope(
            &txn,
            self.scope,
            destination,
            self.root_folder_id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?;
        let destination_folder = match destination_node {
            Some(ResolvedNode::Folder(folder)) => Some(folder),
            Some(ResolvedNode::File(file)) => {
                file_ops::delete_in_scope_on(&txn, self.scope, file.id, true)
                    .await
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?;
                None
            }
            None => None,
            Some(ResolvedNode::Root) => {
                return Err(AsterDavMutationError::FileSystem(FsError::Forbidden));
            }
        };
        let (destination_folder, created) = if let Some(folder) = destination_folder {
            (folder, false)
        } else {
            let created_by_username =
                crate::services::workspace::storage::load_scope_actor_username(&txn, self.scope)
                    .await
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?;
            let now = chrono::Utc::now();
            let folder = folder_repo::create(
                &txn,
                aster_drive_model::entities::folder::ActiveModel {
                    name: Set(destination_name),
                    parent_id: Set(destination_parent_id),
                    team_id: Set(self.scope.team_id()),
                    owner_user_id: Set(self.scope.owner_user_id()),
                    created_by_user_id: Set(Some(self.scope.actor_user_id())),
                    created_by_username: Set(created_by_username),
                    policy_id: Set(source_folder.policy_id),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                },
            )
            .await
            .map_err(to_fs_error)
            .map_err(AsterDavMutationError::FileSystem)?;
            (folder, true)
        };
        copy_visible_entity_properties_on(
            &txn,
            EntityType::Folder,
            source_folder.id,
            EntityType::Folder,
            destination_folder.id,
        )
        .await
        .map_err(AsterDavMutationError::FileSystem)?;
        lock_mutation
            .rebind_destination_root_locks(destination, EntityType::Folder, destination_folder.id)
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
                if created {
                    storage_change::StorageChangeKind::FolderCreated
                } else {
                    storage_change::StorageChangeKind::FolderUpdated
                },
                self.scope,
                vec![],
                vec![destination_folder.id],
                vec![destination_folder.parent_id],
            ),
        );
        if let Some(overwritten) = overwritten.as_ref() {
            self.log_deleted_resource(destination, overwritten).await;
        }
        self.log_folder_transfer(
            if operation == DavMutationOperation::Move {
                audit::AuditAction::FolderMove
            } else {
                audit::AuditAction::FolderCopy
            },
            source,
            destination,
            &source_folder,
            &destination_folder,
        )
        .await;
        Ok(())
    }
}
