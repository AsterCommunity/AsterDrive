//! SeaORM 实体定义：`file_blob`。

use crate::types::file_blob::FileBlobBacking;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_blobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub hash: String, // sha256 or synthetic blob key
    pub size: i64,
    pub policy_id: i64,
    pub storage_path: Option<String>,
    pub backing: FileBlobBacking,
    pub thumbnail_path: Option<String>,
    pub thumbnail_processor: Option<String>,
    pub thumbnail_version: Option<String>,
    pub ref_count: i32,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id"
    )]
    StoragePolicy,
    #[sea_orm(has_many = "super::file::Entity")]
    Files,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Files.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub const EMPTY_SHA256: &'static str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    pub fn validate_backing(&self) -> Result<(), &'static str> {
        match self.backing {
            FileBlobBacking::Stored if self.storage_path.is_some() => Ok(()),
            FileBlobBacking::Stored => Err("stored blob is missing storage_path"),
            FileBlobBacking::VirtualEmpty
                if self.storage_path.is_none()
                    && self.size == 0
                    && self.hash == Self::EMPTY_SHA256 =>
            {
                Ok(())
            }
            FileBlobBacking::VirtualEmpty => Err(
                "virtual_empty blob must have zero size, canonical empty SHA-256, and no storage_path",
            ),
        }
    }

    pub fn storage_path_for_connector(&self) -> Option<&str> {
        self.backing
            .has_connector_object()
            .then(|| self.storage_path.as_deref())
            .flatten()
    }

    pub fn is_virtual_empty(&self) -> bool {
        self.backing == FileBlobBacking::VirtualEmpty
    }
}
