//! Persisted resource-lock domain primitives.
//!
//! 这些枚举描述资源锁的归属工作空间、锁根身份、覆盖深度、互斥方式与来源集成。

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
    /// Lock root belongs to a user's personal workspace.
    #[sea_orm(string_value = "personal")]
    Personal,
    /// Lock root belongs to a team workspace.
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
    /// The workspace root itself.
    #[sea_orm(string_value = "workspace_root")]
    WorkspaceRoot,
    /// A folder row.
    #[sea_orm(string_value = "folder")]
    Folder,
    /// A file row.
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
    /// Lock applies only to the target resource itself.
    #[sea_orm(string_value = "resource")]
    Resource,
    /// Lock applies to the target and its entire subtree, per RFC 4918.
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
    /// Conflicts with any other lock on the hierarchy.
    #[sea_orm(string_value = "exclusive")]
    Exclusive,
    /// Coexists with other shared locks on the hierarchy.
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
    /// Lock created by Drive's own file operations.
    #[sea_orm(string_value = "product")]
    Product,
    /// Lock created by a WebDAV client request.
    #[sea_orm(string_value = "webdav")]
    WebDav,
    /// Lock created by a WOPI client session.
    #[sea_orm(string_value = "wopi")]
    Wopi,
}
