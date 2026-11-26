//! SeaORM 实体定义：`mfa_email_codes`。
//!
//! 这张表保存登录 MFA flow 中发送到邮箱的一次性验证码。
//! 它不是 `mfa_factors` 的补充行，也不表示用户绑定了一个长期 email factor；
//! code 只在当前 flow 内有效，验证成功、过期或邮件发送失败后都会被消费。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 登录 flow 的一次性邮箱验证码。同一账号同时只允许一个未消费的 code；
/// 验证成功、过期或邮件发送失败后该行即被消费。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "mfa_email_codes")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 绑定到一次 MFA 登录 flow；邮箱验证码不能跨 flow 使用。
    pub flow_id: i64,
    /// 接收验证码的账号。
    pub user_id: i64,
    /// 一次性验证码的 hash，避免明文落库。
    #[serde(skip_serializing)]
    pub code_hash: String,
    /// 验证码失效时刻（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// 验证码被消费（验证成功或失效作废）的时刻；`None` 表示仍可用。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 发放该验证码的 MFA 登录 flow（FK `flow_id`）。
    #[sea_orm(
        belongs_to = "super::mfa_login_flow::Entity",
        from = "Column::FlowId",
        to = "super::mfa_login_flow::Column::Id"
    )]
    MfaLoginFlow,
    /// 接收验证码的账号（FK `user_id`）。
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::mfa_login_flow::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MfaLoginFlow.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
