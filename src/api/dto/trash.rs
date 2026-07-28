//! `trash` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Path parameters for trash restore/purge operations.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TrashItemPath {
    pub entity_type: aster_drive_model::types::EntityType,
    pub id: i64,
}
