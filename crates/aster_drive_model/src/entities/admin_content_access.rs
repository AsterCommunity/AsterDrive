//! Short-lived, session-bound administrator content authorizations.
use sea_orm::entity::prelude::*;

/// Durable administrator content-access grant, written before a privileged workspace
/// inspection starts so authorization survives cache loss.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "admin_content_accesses")]
pub struct Model {
    /// Opaque grant identifier (UUID in simple format), not derived from any business row.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    /// User ID of the administrator who created the grant; references `users.id`.
    pub actor_user_id: i64,
    /// Authentication session (`auth_sessions.id`) the grant is bound to; the grant is
    /// meaningless outside that session.
    pub session_id: String,
    /// JSON-encoded authorization payload naming the workspace scope, reason, and remark.
    pub payload: String,
    /// Absolute expiry instant; grants are short-lived (minutes) and never renewed.
    pub expires_at: DateTimeUtc,
}

/// No database-level relations are declared for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
