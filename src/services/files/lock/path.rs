use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::{AsterError, Result};
use crate::services::files::folder;
use crate::types::EntityType;

/// 从 entity 反查 WebDAV 路径
pub async fn resolve_entity_path<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<String> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            let folder_path = match f.folder_id {
                Some(folder_id) => {
                    let mut folder_paths = folder::build_folder_paths(db, &[folder_id]).await?;
                    let path = folder_paths.remove(&folder_id).ok_or_else(|| {
                        AsterError::record_not_found(format!("folder #{folder_id}"))
                    })?;
                    format!("{path}/")
                }
                None => String::new(),
            };
            if let Some(team_id) = f.team_id {
                let prefix = if folder_path.is_empty() {
                    format!("/teams/{team_id}/")
                } else {
                    format!("/teams/{team_id}{folder_path}")
                };
                Ok(format!("{prefix}{}", f.name))
            } else {
                let prefix = if folder_path.is_empty() {
                    "/"
                } else {
                    &folder_path
                };
                Ok(format!("{}{}", prefix, f.name))
            }
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            let path = folder::build_folder_paths(db, &[f.id])
                .await?
                .remove(&f.id)
                .ok_or_else(|| AsterError::record_not_found(format!("folder #{}", f.id)))?;
            if let Some(team_id) = f.team_id {
                Ok(format!("/teams/{team_id}{path}/"))
            } else {
                Ok(format!("{path}/"))
            }
        }
    }
}
