//! SeaORM 实体定义：`background_task`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{
    BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload, StoredTaskResult,
    StoredTaskRuntime, StoredTaskSteps,
};

/// Durable background-task record: queueing, claiming, progress, and result retention.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "background_tasks")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Task type selecting the executor and payload schema.
    pub kind: BackgroundTaskKind,
    /// Lifecycle stage of the task.
    pub status: BackgroundTaskStatus,
    /// Account that requested the task; `None` for system-initiated work.
    pub creator_user_id: Option<i64>,
    /// Team workspace the task operates on; `None` for personal or non-workspace scopes.
    pub team_id: Option<i64>,
    /// Share the task operates on; `None` when the task is not share-related.
    pub share_id: Option<i64>,
    /// Human-facing name shown in task listings; not a lookup key.
    pub display_name: String,
    /// Unique idempotency key for scheduled runtime tasks; `None` when dedupe does not apply.
    pub dedupe_key: Option<String>,
    /// Raw JSON task input consumed by the executor.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub payload_json: StoredTaskPayload,
    /// Raw JSON execution result; `None` until the task finishes.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub result_json: Option<StoredTaskResult>,
    /// Raw JSON executor-managed runtime state persisted across retries.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub runtime_json: Option<StoredTaskRuntime>,
    /// Raw JSON per-step progress breakdown; `None` for tasks without step tracking.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub steps_json: Option<StoredTaskSteps>,
    /// Completed work units; semantics depend on the task kind.
    pub progress_current: i64,
    /// Total work units expected; `0` when progress is indeterminate.
    pub progress_total: i64,
    /// Short human-readable status line; `None` when the executor has none.
    pub status_text: Option<String>,
    /// Number of execution attempts already started.
    pub attempt_count: i32,
    /// Retry ceiling; a failed task is abandoned once `attempt_count` reaches it.
    pub max_attempts: i32,
    /// UTC instant the task becomes eligible for the next dispatch or retry.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub next_run_at: DateTimeUtc,
    /// CAS token a worker receives on claim; progress and finish updates must match it,
    /// so stale workers silently lose.
    pub processing_token: i64,
    /// UTC instant the current (or latest) execution attempt claimed the task.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub processing_started_at: Option<DateTimeUtc>,
    /// UTC instant of the latest progress report from the worker; used for liveness checks.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_heartbeat_at: Option<DateTimeUtc>,
    /// UTC instant the worker lease lapses; an expired lease lets another worker reclaim
    /// an in-flight task. `None` while not leased.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub lease_expires_at: Option<DateTimeUtc>,
    /// UTC instant the first execution attempt started; `None` while never dispatched.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub started_at: Option<DateTimeUtc>,
    /// UTC instant the task reached a terminal status; `None` while still pending/processing.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub finished_at: Option<DateTimeUtc>,
    /// Message from the latest failure; `None` when nothing has failed or the text was cleared.
    pub last_error: Option<String>,
    /// `Some(true)`/`Some(false)` records whether the latest failure may retry; `None` when
    /// the task has not failed.
    pub failure_can_retry: Option<bool>,
    /// UTC instant after which the finished task record is eligible for cleanup.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// UTC instant the record was created.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// UTC instant of the latest state-changing update.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// Declared database relations for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The requesting user, matched on `creator_user_id` to `users.id`.
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::CreatorUserId",
        to = "super::user::Column::Id"
    )]
    User,
    /// The team workspace target, matched on `team_id` to `teams.id`.
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    /// The share target, matched on `share_id` to `shares.id`.
    #[sea_orm(
        belongs_to = "super::share::Entity",
        from = "Column::ShareId",
        to = "super::share::Column::Id"
    )]
    Share,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::share::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Share.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

impl aster_forge_tasks::TaskRecord<crate::types::BackgroundTaskKind> for Model {
    fn id(&self) -> i64 {
        self.id
    }

    fn kind(&self) -> crate::types::BackgroundTaskKind {
        self.kind
    }

    fn payload_json(&self) -> &str {
        self.payload_json.as_ref()
    }

    fn result_json(&self) -> Option<&str> {
        self.result_json.as_ref().map(AsRef::as_ref)
    }
}

impl aster_forge_tasks::ClaimableTaskRecord<crate::types::BackgroundTaskKind> for Model {
    fn processing_token(&self) -> i64 {
        self.processing_token
    }
}

impl aster_forge_tasks::ExecutableTaskRecord<crate::types::BackgroundTaskKind> for Model {
    fn cancellation_requested(&self) -> bool {
        self.status == crate::types::BackgroundTaskStatus::Canceling
    }

    fn attempt_count(&self) -> i32 {
        self.attempt_count
    }

    fn max_attempts(&self) -> i32 {
        self.max_attempts
    }
}
