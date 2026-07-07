use sea_orm::ConnectionTrait;

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::types::EntityType;

/// 校验资源归属
pub(in crate::services::files::lock) async fn check_entity_ownership<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<bool> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            Ok(f.owner_user_id == Some(user_id))
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            Ok(f.owner_user_id == Some(user_id))
        }
    }
}
