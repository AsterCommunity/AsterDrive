//! Persisted resource-lock domain primitives.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Workspace namespace owning a lock root.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum LockWorkspaceType {
    #[sea_orm(string_value = "personal")]
    Personal,
    #[sea_orm(string_value = "team")]
    Team,
}

/// Stable identity kind of a lock root.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum LockRootKind {
    #[sea_orm(string_value = "workspace_root")]
    WorkspaceRoot,
    #[sea_orm(string_value = "folder")]
    Folder,
    #[sea_orm(string_value = "file")]
    File,
}

/// Hierarchy depth covered by a resource lock.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum LockDepth {
    #[sea_orm(string_value = "resource")]
    Resource,
    #[sea_orm(string_value = "infinity")]
    Infinity,
}

/// Compatibility mode for locks covering the same resource hierarchy.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum LockMode {
    #[sea_orm(string_value = "exclusive")]
    Exclusive,
    #[sea_orm(string_value = "shared")]
    Shared,
}

/// Product integration that created a lock.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum LockOrigin {
    #[sea_orm(string_value = "product")]
    Product,
    #[sea_orm(string_value = "webdav")]
    WebDav,
    #[sea_orm(string_value = "wopi")]
    Wopi,
}
