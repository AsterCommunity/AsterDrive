use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 实体类型（文件/文件夹）
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    #[sea_orm(string_value = "file")]
    File,
    #[sea_orm(string_value = "folder")]
    Folder,
}

/// Resource identity used by lock persistence.
///
/// WebDAV mount roots can be virtual workspace resources rather than rows in `files` or
/// `folders`, so lock storage needs a target type that can represent those roots explicitly.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum ResourceLockTargetType {
    #[sea_orm(string_value = "file")]
    File,
    #[sea_orm(string_value = "folder")]
    Folder,
    #[sea_orm(string_value = "personal_root")]
    PersonalRoot,
    #[sea_orm(string_value = "team_root")]
    TeamRoot,
}

impl ResourceLockTargetType {
    #[must_use]
    pub const fn entity_type(self) -> Option<EntityType> {
        match self {
            Self::File => Some(EntityType::File),
            Self::Folder => Some(EntityType::Folder),
            Self::PersonalRoot | Self::TeamRoot => None,
        }
    }
}

impl From<EntityType> for ResourceLockTargetType {
    fn from(value: EntityType) -> Self {
        match value {
            EntityType::File => Self::File,
            EntityType::Folder => Self::Folder,
        }
    }
}

impl EntityType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Folder => "folder",
        }
    }
}
