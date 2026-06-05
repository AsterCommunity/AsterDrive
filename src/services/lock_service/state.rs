use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::{file_repo, folder_repo, lock_repo};
use crate::errors::{AsterError, Result};
use crate::types::EntityType;

pub(crate) async fn clear_entity_locked_if_unlocked(
    db: &impl ConnectionTrait,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            lock_repo::clear_file_locked_flag_without_lock(db, entity_id).await?;
        }
        EntityType::Folder => {
            lock_repo::clear_folder_locked_flag_without_lock(db, entity_id).await?;
        }
    }
    Ok(())
}

/// 同步 is_locked boolean 缓存（pub 给 db_lock_system 调用）
pub async fn set_entity_locked(
    db: &impl ConnectionTrait,
    entity_type: EntityType,
    entity_id: i64,
    locked: bool,
) -> Result<()> {
    let now = Utc::now();

    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            let mut active: crate::entities::file::ActiveModel = f.into();
            active.is_locked = Set(locked);
            active.updated_at = Set(now);
            active.update(db).await.map_err(|e| {
                tracing::error!("failed to sync is_locked for file #{entity_id}: {e}");
                AsterError::from(e)
            })?;
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            let mut active: crate::entities::folder::ActiveModel = f.into();
            active.is_locked = Set(locked);
            active.updated_at = Set(now);
            active.update(db).await.map_err(|e| {
                tracing::error!("failed to sync is_locked for folder #{entity_id}: {e}");
                AsterError::from(e)
            })?;
        }
    }
    Ok(())
}
