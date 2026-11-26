//! Persistence only; authorization belongs to the administrator access service.
use crate::errors::Result;
use aster_drive_model::entities::admin_content_access::{self, Entity};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

/// Records one temporary administrator content-access grant; the row carries
/// its own expiry and no result is needed beyond persistence.
pub async fn insert<C: ConnectionTrait>(
    db: &C,
    model: admin_content_access::ActiveModel,
) -> Result<()> {
    model.insert(db).await?;
    Ok(())
}

/// Loads an access grant by id. Returns `None` when the grant is unknown;
/// expiry checking is the caller's job.
pub async fn find<C: ConnectionTrait>(
    db: &C,
    id: &str,
) -> Result<Option<admin_content_access::Model>> {
    Ok(Entity::find_by_id(id.to_owned()).one(db).await?)
}

/// Removes grants whose expiry has passed, returning the number of deleted rows.
pub async fn delete_expired<C: ConnectionTrait>(db: &C) -> Result<u64> {
    Ok(Entity::delete_many()
        .filter(admin_content_access::Column::ExpiresAt.lte(Utc::now()))
        .exec(db)
        .await?
        .rows_affected)
}
