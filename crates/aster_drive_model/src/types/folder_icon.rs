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
    #[default]
    #[sea_orm(string_value = "default")]
    Default,
    #[sea_orm(string_value = "builtin")]
    Builtin,
    #[sea_orm(string_value = "emoji")]
    Emoji,
}
