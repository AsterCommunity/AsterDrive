//! Immutable workspace business activity. No cascading actor or resource relations.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Immutable workspace business activity record. Rows are historical facts:
/// no foreign keys or cascading relations, and actor/resource names are kept
/// as snapshot strings so deletion of the underlying entities preserves the
/// record.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "workspace_activities")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// UUID uniquely identifying this activity event (unique index).
    pub event_id: String,
    /// UUID grouping all events produced by one logical operation (e.g. a
    /// single API call that touches several entities).
    pub operation_id: String,
    /// Team scope of the affected resource; `None` for personal-scope
    /// resources.
    pub team_id: Option<i64>,
    /// Owner of the affected resource; `None` for team-owned resources.
    pub owner_user_id: Option<i64>,
    /// Kind of affected resource (`file` / `folder`).
    pub entity_type: String,
    /// Id of the affected resource within its type.
    pub entity_id: i64,
    /// Resource name snapshot at event time; survives later renames/deletes.
    pub entity_name: String,
    /// User who performed the action; `None` for system/service actors.
    pub actor_user_id: Option<i64>,
    /// Actor display name snapshot at event time.
    pub actor_name: String,
    /// Business action performed (e.g. upload, move, delete).
    pub action: String,
    /// Entry channel that produced the event: `rest`, `webdav`, `wopi`,
    /// `task` or `service`.
    pub source: String,
    /// Structured JSON payload with action-specific details (paths, sizes,
    /// move source/target, ...).
    pub details: String,
    /// When the event happened (UTC).
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity: intentionally empty, activity
/// rows are immutable facts with no cascading actor/resource relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
