//! WebDAV account access-mode types.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Access mode of a WebDAV account. It is an account-level permission
/// ceiling: every request still intersects it with the owner's current
/// product permissions, and a `read_only` account fails closed on every
/// mutating operation at the authorization layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum WebdavAccessMode {
    /// Read operations only (PROPFIND / GET / HEAD / OPTIONS / REPORT).
    #[sea_orm(string_value = "read_only")]
    ReadOnly,
    /// Read plus all write operations (PUT / MKCOL / DELETE / COPY / MOVE /
    /// PROPPATCH / LOCK).
    #[sea_orm(string_value = "read_write")]
    ReadWrite,
}

impl WebdavAccessMode {
    /// Returns whether this mode permits mutating operations.
    pub fn allows_mutation(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}
