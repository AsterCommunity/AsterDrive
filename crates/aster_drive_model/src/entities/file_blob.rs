//! SeaORM 实体定义：`file_blob`。

use crate::types::file_blob::FileBlobBacking;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Content-addressed blob shared by file revisions and files, with reference counting.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "file_blobs")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Content identity: lowercase SHA-256 hex digest for stored content, or a synthetic
    /// blob key for content without a real object.
    pub hash: String,
    /// Logical content size in bytes.
    pub size: i64,
    /// Storage policy the blob lives under; references `storage_policies.id`.
    pub policy_id: i64,
    /// Object path inside the connector's storage, relative to the policy's target;
    /// `None` for blobs that have no stored object.
    pub storage_path: Option<String>,
    /// Backing mode deciding whether a connector object exists for this blob.
    pub backing: FileBlobBacking,
    /// Path of the generated thumbnail object; `None` while no thumbnail exists.
    pub thumbnail_path: Option<String>,
    /// Identifier of the processor that generated the thumbnail; `None` without a thumbnail.
    pub thumbnail_processor: Option<String>,
    /// Thumbnail format/generation version tag; `None` without a thumbnail. Changed
    /// versions trigger regeneration.
    pub thumbnail_version: Option<String>,
    /// Number of live references (file rows and revisions) pointing at this blob; zero
    /// means the blob is eligible for cleanup.
    pub ref_count: i32,
    /// First time this blob entered its current unreferenced interval; retained across cleanup retries.
    pub cleanup_pending_since: Option<DateTimeUtc>,
    /// UTC instant the blob row was created.
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest row update.
    pub updated_at: DateTimeUtc,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Storage policy hosting the blob, matched on `policy_id` to `storage_policies.id`.
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id"
    )]
    StoragePolicy,
    /// Files whose current content is this blob.
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
    /// SHA-256 digest of the empty byte string; the canonical content hash of
    /// virtual-empty blobs.
    pub const EMPTY_SHA256: &'static str =
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    /// Validates this blob's backing fields against the invariants of [`FileBlobBacking`];
    /// the error text describes the violated invariant.
    pub fn validate_backing(&self) -> Result<(), &'static str> {
        Self::validate_backing_fields(
            self.backing,
            self.storage_path.as_deref(),
            self.size,
            &self.hash,
        )
    }

    /// Validates the backing invariants for raw field values: `Stored` requires a
    /// `storage_path`, `VirtualEmpty` requires zero size, the canonical empty SHA-256,
    /// and no `storage_path`.
    pub fn validate_backing_fields(
        backing: FileBlobBacking,
        storage_path: Option<&str>,
        size: i64,
        hash: &str,
    ) -> Result<(), &'static str> {
        match backing {
            FileBlobBacking::Stored if storage_path.is_some() => Ok(()),
            FileBlobBacking::Stored => Err("stored blob is missing storage_path"),
            FileBlobBacking::VirtualEmpty
                if storage_path.is_none() && size == 0 && hash == Self::EMPTY_SHA256 =>
            {
                Ok(())
            }
            FileBlobBacking::VirtualEmpty => Err(
                "virtual_empty blob must have zero size, canonical empty SHA-256, and no storage_path",
            ),
        }
    }

    /// Returns the connector object path, or `None` when the backing mode has no stored
    /// object (virtual-empty blobs never resolve to a path).
    pub fn storage_path_for_connector(&self) -> Option<&str> {
        self.backing
            .has_connector_object()
            .then_some(self.storage_path.as_deref())
            .flatten()
    }

    /// Returns whether this blob represents empty content without a stored object.
    pub fn is_virtual_empty(&self) -> bool {
        self.backing == FileBlobBacking::VirtualEmpty
    }
}
