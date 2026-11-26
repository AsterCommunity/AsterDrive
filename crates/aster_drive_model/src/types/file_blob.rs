//! 文件 blob 物理承载方式。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Physical backing for a file blob.
///
/// A virtual empty blob has canonical zero-byte content but deliberately has no
/// object in the configured storage connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
#[serde(rename_all = "snake_case")]
pub enum FileBlobBacking {
    /// Content lives as an object in the policy's storage connector.
    #[sea_orm(string_value = "stored")]
    Stored,
    /// Canonical zero-byte content kept entirely in metadata, with no object.
    #[sea_orm(string_value = "virtual_empty")]
    VirtualEmpty,
}

impl FileBlobBacking {
    /// Returns `true` when this backing keeps a real object in the connector.
    pub const fn has_connector_object(self) -> bool {
        matches!(self, Self::Stored)
    }
}
