//! Account-owned MFA attempt budgets; independent of HTTP/IP throttling and login flows.
use sea_orm::entity::prelude::*;

/// Account-level MFA attempt budget for one purpose (login, sensitive
/// confirmation, email-code sending). Independent of HTTP/IP throttling and of
/// login-flow rows; mutations are serialized through the account writer lock.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mfa_attempt_budgets")]
pub struct Model {
    /// Account this budget belongs to; part of the composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    /// Purpose the budget guards; `login`, `sensitive`, or `email_send`; part
    /// of the composite primary key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub purpose: String,
    /// Failed attempts counted inside the current window; saturating counter.
    pub failure_count: i32,
    /// Start of the current failure-counting window; failures outside the
    /// window reset the budget.
    pub window_started_at: DateTimeUtc,
    /// Cooldown deadline set once the failure limit is reached; `None` while
    /// attempts are still accepted.
    pub blocked_until: Option<DateTimeUtc>,
    /// Last budget mutation (UTC).
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The account this budget belongs to (FK `user_id`).
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
