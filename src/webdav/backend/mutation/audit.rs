use aster_drive_model::entities::file as file_entity;
use aster_forge_webdav::DavPath;

use crate::services::ops::audit;

use super::super::{AsterDavFs, DeletedResource};

impl AsterDavFs {
    pub(super) async fn log_deleted_resource(&self, path: &DavPath, resource: &DeletedResource) {
        match resource {
            DeletedResource::File(file) => self.log_deleted_file(path, file).await,
            DeletedResource::Folder(folder) => self.log_deleted_folder(path, folder).await,
        }
    }

    pub(super) async fn log_deleted_file(&self, path: &DavPath, file: &file_entity::Model) {
        let details = Some(serde_json::json!({
            "folder_id": file.folder_id,
            "path": path.as_str(),
            "team_id": self.scope.team_id(),
        }));
        audit::log_with_details(
            &self.state,
            &self.audit_ctx,
            audit::AuditAction::FileDelete,
            crate::services::ops::audit::AuditEntityType::File,
            Some(file.id),
            Some(&file.name),
            || details.clone(),
        )
        .await;
    }

    pub(super) async fn log_deleted_folder(
        &self,
        path: &DavPath,
        folder: &aster_drive_model::entities::folder::Model,
    ) {
        let details = Some(serde_json::json!({
            "parent_id": folder.parent_id,
            "path": path.as_str(),
            "team_id": self.scope.team_id(),
        }));
        audit::log_with_details(
            &self.state,
            &self.audit_ctx,
            audit::AuditAction::FolderDelete,
            crate::services::ops::audit::AuditEntityType::Folder,
            Some(folder.id),
            Some(&folder.name),
            || details.clone(),
        )
        .await;
    }

    pub(super) async fn log_file_transfer(
        &self,
        action: audit::AuditAction,
        source: &DavPath,
        destination: &DavPath,
        previous: &file_entity::Model,
        current: &file_entity::Model,
    ) {
        let details = Some(serde_json::json!({
            "source_folder_id": previous.folder_id,
            "source_path": source.as_str(),
            "target_folder_id": current.folder_id,
            "target_path": destination.as_str(),
            "previous_name": previous.name,
            "next_name": current.name,
            "team_id": self.scope.team_id(),
        }));
        audit::log_with_details(
            &self.state,
            &self.audit_ctx,
            action,
            crate::services::ops::audit::AuditEntityType::File,
            Some(current.id),
            Some(&current.name),
            || details.clone(),
        )
        .await;
    }

    pub(super) async fn log_folder_transfer(
        &self,
        action: audit::AuditAction,
        source: &DavPath,
        destination: &DavPath,
        previous: &aster_drive_model::entities::folder::Model,
        current: &aster_drive_model::entities::folder::Model,
    ) {
        let details = Some(serde_json::json!({
            "source_parent_id": previous.parent_id,
            "source_path": source.as_str(),
            "target_parent_id": current.parent_id,
            "target_path": destination.as_str(),
            "previous_name": previous.name,
            "next_name": current.name,
            "team_id": self.scope.team_id(),
        }));
        audit::log_with_details(
            &self.state,
            &self.audit_ctx,
            action,
            crate::services::ops::audit::AuditEntityType::Folder,
            Some(current.id),
            Some(&current.name),
            || details.clone(),
        )
        .await;
    }
}
