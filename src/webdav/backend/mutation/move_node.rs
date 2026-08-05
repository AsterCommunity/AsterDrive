use aster_drive_model::entities::file as file_entity;
use aster_drive_model::types::EntityType;
use aster_forge_db::transaction;
use aster_forge_webdav::{DavPath, FsError, parent_relative_path};
use sea_orm::{ActiveModelTrait, Set};

use crate::runtime::SharedRuntimeState;
use crate::services::{
    events::storage_change,
    files::{file as file_ops, folder},
    ops::audit,
};
use crate::webdav::handlers::resources::MUTATION_FOLDER_TREE_LIMITS;

use super::super::{
    AsterDavFs, AsterDavMutationError, AtomicTargetRevalidation, DavMutationConditions,
    DeletedResource, ResolvedNode, lock, map_ancestor_lock_error, map_atomic_lock_error,
    path_resolver, revalidate_atomic_target, to_fs_error,
};

impl AsterDavFs {
    /// Moves one resource, destination overwrite state, rooted locks, and derived lock flags in
    /// one writer commit.
    pub(crate) async fn move_with_locks(
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
        let mut lock_paths = [source, destination];
        lock_paths.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        for path in lock_paths {
            lock_mutation
                .lock_ancestor_entities(self.scope, self.root_folder_id, path)
                .await
                .map_err(map_ancestor_lock_error)?;
        }

        let source_node =
            path_resolver::resolve_path_in_scope(&txn, self.scope, source, self.root_folder_id)
                .await
                .map_err(AsterDavMutationError::FileSystem)?;
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

        let source_is_collection = matches!(source_node, ResolvedNode::Folder(_));
        let _source_tree_lock = match &source_node {
            ResolvedNode::Folder(folder) => Some(
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
            ResolvedNode::File(_) => None,
            ResolvedNode::Root => {
                return Err(AsterDavMutationError::FileSystem(FsError::Forbidden));
            }
        };
        revalidate_atomic_target(
            &txn,
            lock_mutation.namespace_id(),
            self.scope,
            self.root_folder_id,
            AtomicTargetRevalidation {
                path: source,
                check_locks: true,
                deep: source_is_collection,
            },
            &conditions,
        )
        .await?;
        if let Some(parent) = parent_relative_path(source.as_str())
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
        if let Some(parent) = parent_relative_path(destination.as_str())
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
        enum Moved {
            File {
                previous: file_entity::Model,
                current: file_entity::Model,
            },
            Folder {
                previous: aster_drive_model::entities::folder::Model,
                current: aster_drive_model::entities::folder::Model,
            },
        }

        let (entity_type, entity_id, event, moved) = match source_node {
            ResolvedNode::File(file) => {
                let mut active: file_entity::ActiveModel = file.clone().into();
                active.name = Set(destination_name);
                active.folder_id = Set(destination_parent_id);
                active.updated_at = Set(chrono::Utc::now());
                let moved = active
                    .update(&txn)
                    .await
                    .map_err(crate::errors::AsterError::from)
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?;
                (
                    EntityType::File,
                    moved.id,
                    storage_change::StorageChangeEvent::new(
                        storage_change::StorageChangeKind::FileUpdated,
                        self.scope,
                        vec![moved.id],
                        vec![],
                        vec![file.folder_id, moved.folder_id],
                    ),
                    Moved::File {
                        previous: file,
                        current: moved,
                    },
                )
            }
            ResolvedNode::Folder(folder) => {
                let mut active: aster_drive_model::entities::folder::ActiveModel =
                    folder.clone().into();
                active.name = Set(destination_name);
                active.parent_id = Set(destination_parent_id);
                active.updated_at = Set(chrono::Utc::now());
                let moved = active
                    .update(&txn)
                    .await
                    .map_err(crate::errors::AsterError::from)
                    .map_err(to_fs_error)
                    .map_err(AsterDavMutationError::FileSystem)?;
                (
                    EntityType::Folder,
                    moved.id,
                    storage_change::StorageChangeEvent::new(
                        storage_change::StorageChangeKind::FolderUpdated,
                        self.scope,
                        vec![],
                        vec![moved.id],
                        vec![folder.parent_id, moved.parent_id],
                    ),
                    Moved::Folder {
                        previous: folder,
                        current: moved,
                    },
                )
            }
            ResolvedNode::Root => {
                return Err(AsterDavMutationError::FileSystem(FsError::Forbidden));
            }
        };
        lock_mutation
            .delete_rooted_locks(source)
            .await
            .map_err(map_atomic_lock_error)?;
        lock_mutation
            .rebind_destination_root_locks(destination, entity_type, entity_id)
            .await
            .map_err(map_atomic_lock_error)?;
        lock_mutation
            .finish()
            .await
            .map_err(map_atomic_lock_error)?;
        transaction::commit(txn)
            .await
            .map_err(|_| AsterDavMutationError::Backend)?;
        storage_change::publish(&self.state, event);
        if let Some(overwritten) = overwritten.as_ref() {
            self.log_deleted_resource(destination, overwritten).await;
        }
        match moved {
            Moved::File { previous, current } => {
                self.log_file_transfer(
                    audit::AuditAction::FileMove,
                    source,
                    destination,
                    &previous,
                    &current,
                )
                .await;
            }
            Moved::Folder { previous, current } => {
                self.log_folder_transfer(
                    audit::AuditAction::FolderMove,
                    source,
                    destination,
                    &previous,
                    &current,
                )
                .await;
            }
        }
        if entity_type == EntityType::Folder {
            folder::invalidate_folder_path_cache_for_ids(&self.state, &[entity_id]).await;
        }
        Ok(())
    }
}
