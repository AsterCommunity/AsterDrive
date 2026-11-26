//! SeaORM 实体定义：`passkeys`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::StoredPasskeyCredential;

/// One registered WebAuthn passkey credential. Each user may register several;
/// the raw credential JSON plus its latest attestation-derived flags are kept
/// so login can verify assertions without contacting the authenticator.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "passkeys")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Account owning this passkey.
    pub user_id: i64,
    /// WebAuthn credential id assigned by the authenticator (base64); unique
    /// across the instance.
    pub credential_id: String,
    /// WebAuthn user handle (UUID string) representing this user to the
    /// authenticator; all passkeys of one user share the same handle.
    pub user_handle: String,
    /// Raw credential registration JSON (public key, algorithm, AAGUID, flags).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub credential: StoredPasskeyCredential,
    /// User-visible display name of this passkey.
    pub name: String,
    /// JSON array of transports reported at registration (e.g. `internal`,
    /// `hybrid`); `None` when the authenticator did not report any.
    pub transports: Option<String>,
    /// Whether the credential can be backed up (device capability at last use).
    pub backup_eligible: bool,
    /// Whether the credential is currently backed up in multi-device cloud sync.
    pub backed_up: bool,
    /// Latest assertion signature counter; `0` for authenticators that do not
    /// implement counters.
    pub sign_count: i64,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    /// Last successful authentication with this passkey; `None` before first use.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_used_at: Option<DateTimeUtc>,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Account owning this passkey (FK `user_id`).
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
