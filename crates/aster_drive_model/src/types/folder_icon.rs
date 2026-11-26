//! 文件夹展示图标类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Persisted discriminator for a folder presentation icon.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum FolderIconKind {
    /// Render the standard folder icon; no `icon_value` payload.
    #[default]
    #[sea_orm(string_value = "default")]
    Default,
    /// Icon chosen from the product's built-in icon set.
    #[sea_orm(string_value = "builtin")]
    Builtin,
    /// Icon rendered from a user-selected emoji stored in `icon_value`.
    #[sea_orm(string_value = "emoji")]
    Emoji,
}
