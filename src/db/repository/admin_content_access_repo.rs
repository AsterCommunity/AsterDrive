//! Persistence only; authorization belongs to the administrator access service.
use crate::errors::Result;
use aster_drive_model::entities::admin_content_access::{self, Entity};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
pub async fn insert<C: ConnectionTrait>(
    db: &C,
    model: admin_content_access::ActiveModel,
) -> Result<()> {
    model.insert(db).await?;
    Ok(())
}
pub async fn find<C: ConnectionTrait>(
    db: &C,
    id: &str,
) -> Result<Option<admin_content_access::Model>> {
    Ok(Entity::find_by_id(id.to_owned()).one(db).await?)
}
pub async fn delete_expired<C: ConnectionTrait>(db: &C) -> Result<u64> {
    Ok(Entity::delete_many()
        .filter(admin_content_access::Column::ExpiresAt.lte(Utc::now()))
        .exec(db)
        .await?
        .rows_affected)
}
