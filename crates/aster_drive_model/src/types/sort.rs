//! 文件列表排序偏好。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// File and folder ordering used by persisted user preferences and listing APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SortBy {
    /// Sort by item name.
    #[default]
    Name,
    /// Sort by file size.
    Size,
    /// Sort by creation time.
    CreatedAt,
    /// Sort by last update time.
    UpdatedAt,
    /// Sort by entity type (file / folder).
    #[serde(rename = "type")]
    Type,
}
