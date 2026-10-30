//! Bulk persistence on the caller's transaction. Buffers contain event projections, never
//! duplicate file/folder models (which can include large owned strings).
use super::*;

struct PendingChange {
    model: workspace_activity::ActiveModel,
    details: serde_json::Value,
    location: Option<(Option<i64>, Option<i64>, String)>,
}
pub(crate) struct ActivityBatch<'a> {
    context: &'a ActivityContext,
    files: Vec<i64>,
    folders: Vec<i64>,
    changes: Vec<PendingChange>,
    copies: Vec<workspace_activity::ActiveModel>,
}
impl<'a> ActivityBatch<'a> {
    pub fn new(context: &'a ActivityContext) -> Self {
        Self {
            context,
            files: Vec::new(),
            folders: Vec::new(),
            changes: Vec::new(),
            copies: Vec::new(),
        }
    }
    fn file_change(
        &mut self,
        before: &file::Model,
        action: AuditAction,
        details: serde_json::Value,
        target: Option<Option<i64>>,
    ) {
        self.files.push(before.id);
        self.changes.push(PendingChange {
            model: activity_model(
                self.context,
                ActivityResource::File(before),
                action,
                serde_json::Value::Null,
                Utc::now(),
            ),
            details,
            location: target.map(|parent| (before.folder_id, parent, before.name.clone())),
        });
    }
    fn folder_change(
        &mut self,
        before: &folder::Model,
        action: AuditAction,
        details: serde_json::Value,
        target: Option<Option<i64>>,
    ) {
        self.folders.push(before.id);
        self.changes.push(PendingChange {
            model: activity_model(
                self.context,
                ActivityResource::Folder(before),
                action,
                serde_json::Value::Null,
                Utc::now(),
            ),
            details,
            location: target.map(|parent| (before.parent_id, parent, before.name.clone())),
        });
    }
    pub fn file_moved(&mut self, before: &file::Model, target: Option<i64>) -> Result<()> {
        self.file_change(
            before,
            AuditAction::FileMove,
            serde_json::to_value(FileChangeDetails {
                previous_name: &before.name,
                next_name: &before.name,
                source_folder_id: before.folder_id,
                target_folder_id: target,
                previous_size: before.size,
                size: before.size,
                previous_mime_type: &before.mime_type,
                mime_type: &before.mime_type,
            })
            .map_err(|_| {
                crate::errors::AsterError::internal_error(
                    "failed to serialize file activity details",
                )
            })?,
            Some(target),
        );
        Ok(())
    }
    pub fn folder_moved(&mut self, before: &folder::Model, target: Option<i64>) -> Result<()> {
        self.folder_change(
            before,
            AuditAction::FolderMove,
            serde_json::to_value(FolderChangeDetails {
                previous_name: &before.name,
                next_name: &before.name,
                source_parent_id: before.parent_id,
                target_parent_id: target,
                previous_kind: &before.icon_kind,
                kind: &before.icon_kind,
                previous_key: before.icon_value.as_deref(),
                key: before.icon_value.as_deref(),
            })
            .map_err(|_| {
                crate::errors::AsterError::internal_error(
                    "failed to serialize folder activity details",
                )
            })?,
            Some(target),
        );
        Ok(())
    }
    pub fn file_lifecycle(&mut self, before: &file::Model, action: AuditAction) -> Result<()> {
        debug_assert!(matches!(
            action,
            AuditAction::FileDelete | AuditAction::FileRestore
        ));
        self.file_change(
            before,
            action,
            serde_json::to_value(FileChangeDetails {
                previous_name: &before.name,
                next_name: &before.name,
                source_folder_id: before.folder_id,
                target_folder_id: before.folder_id,
                previous_size: before.size,
                size: before.size,
                previous_mime_type: &before.mime_type,
                mime_type: &before.mime_type,
            })
            .map_err(|_| {
                crate::errors::AsterError::internal_error(
                    "failed to serialize file activity details",
                )
            })?,
            None,
        );
        Ok(())
    }
    pub fn folder_lifecycle(&mut self, before: &folder::Model, action: AuditAction) -> Result<()> {
        debug_assert!(matches!(
            action,
            AuditAction::FolderDelete | AuditAction::FolderRestore
        ));
        self.folder_change(
            before,
            action,
            serde_json::to_value(FolderChangeDetails {
                previous_name: &before.name,
                next_name: &before.name,
                source_parent_id: before.parent_id,
                target_parent_id: before.parent_id,
                previous_kind: &before.icon_kind,
                kind: &before.icon_kind,
                previous_key: before.icon_value.as_deref(),
                key: before.icon_value.as_deref(),
            })
            .map_err(|_| {
                crate::errors::AsterError::internal_error(
                    "failed to serialize folder activity details",
                )
            })?,
            None,
        );
        Ok(())
    }
    pub fn file_copied(&mut self, source: &file::Model, target: &file::Model) {
        file_copy_records(&mut self.copies, self.context, source, target, Utc::now());
    }
    pub fn folder_copied(&mut self, source: &folder::Model, target: &folder::Model) {
        folder_copy_records(&mut self.copies, self.context, source, target, Utc::now());
    }
    /// Producers bound the batch by the API selection or staged tree page. Shared parent
    /// paths are captured before structural writes, once per batch instead of per resource.
    pub async fn persist<C: ConnectionTrait>(self, db: &C) -> Result<()> {
        let parents = self
            .changes
            .iter()
            .filter_map(|change| change.location.as_ref())
            .flat_map(|(source, target, _)| [*source, *target])
            .flatten()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let paths = crate::services::files::folder::build_folder_paths(db, &parents).await?;
        let now = Utc::now();
        let mut records = self.copies;
        records.reserve(self.changes.len());
        for mut change in self.changes {
            if let Some((source, target, name)) = change.location {
                change.details = merge_details(
                    change.details,
                    location_details(&paths, source, &name, target, &name)?,
                );
            }
            change.model.details = Set(change.details.to_string());
            change.model.created_at = Set(now);
            records.push(change.model);
        }
        for ids in self.files.chunks(400) {
            file::Entity::update_many()
                .col_expr(
                    file::Column::ModifiedByUserId,
                    Expr::value(self.context.actor_user_id),
                )
                .col_expr(
                    file::Column::ModifiedByUsername,
                    Expr::value(Some(self.context.actor_name.clone())),
                )
                .col_expr(file::Column::ModifiedAt, Expr::value(Some(now)))
                .filter(file::Column::Id.is_in(ids.iter().copied()))
                .exec(db)
                .await?;
        }
        for ids in self.folders.chunks(400) {
            folder::Entity::update_many()
                .col_expr(
                    folder::Column::ModifiedByUserId,
                    Expr::value(self.context.actor_user_id),
                )
                .col_expr(
                    folder::Column::ModifiedByUsername,
                    Expr::value(Some(self.context.actor_name.clone())),
                )
                .col_expr(folder::Column::ModifiedAt, Expr::value(Some(now)))
                .filter(folder::Column::Id.is_in(ids.iter().copied()))
                .exec(db)
                .await?;
        }
        workspace_activity_repo::append_many(db, records).await
    }
}
