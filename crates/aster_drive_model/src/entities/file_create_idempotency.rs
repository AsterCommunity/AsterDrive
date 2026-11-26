//! Persistent request/result mapping for idempotent metadata-only file creation.

use sea_orm::entity::prelude::*;

/// Idempotency record for metadata-only file creation: maps a client-supplied
/// idempotency key to the created file within its claim window.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "file_create_idempotencies")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Account that issued the request; references `users.id`. Part of the lookup scope.
    pub actor_user_id: i64,
    /// Workspace family the request targeted (`"personal"` or `"team"`). Part of the
    /// lookup scope.
    pub workspace_kind: String,
    /// Primary key of the workspace (`users.id` for personal, `teams.id` for team).
    /// Part of the lookup scope.
    pub workspace_id: i64,
    /// Hex hash of the client idempotency key; the plaintext key is never stored.
    /// Part of the lookup scope.
    pub key_hash: String,
    /// Canonical fingerprint of the create request; a repeated key with a different
    /// fingerprint is rejected as a conflict.
    pub request_fingerprint: String,
    /// File produced by the completed request; `None` while the create is still in
    /// flight or failed.
    pub result_file_id: Option<i64>,
    /// UTC instant the claim was created.
    pub created_at: DateTimeUtc,
    /// Absolute expiry instant; expired claims are garbage-collected and their keys reusable.
    pub expires_at: DateTimeUtc,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The file produced by the request, matched on `result_file_id` to `files.id`.
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::ResultFileId",
        to = "super::file::Column::Id"
    )]
    File,
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
