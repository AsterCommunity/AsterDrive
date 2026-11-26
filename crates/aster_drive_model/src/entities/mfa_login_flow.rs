//! SeaORM 实体定义：`mfa_login_flows`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::MfaFirstFactor;

/// 一次 MFA 登录挑战的会话：密码等第一因子通过后创建，完成第二步验证或
/// 过期后消费。flow 与创建时的账号状态绑定，账号安全状态变化即作废。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "mfa_login_flows")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 客户端持有的 flow 令牌 hash；令牌明文只在创建响应中出现一次。
    #[serde(skip_serializing)]
    pub flow_token_hash: String,
    /// 发起登录的账号。
    pub user_id: i64,
    /// 创建 flow 时账号的 `session_version` 快照；账号版本变化后 flow 视为
    /// 过期作废。
    #[serde(skip_serializing)]
    pub user_session_version: i64,
    /// 第一步验证使用的方法（如密码登录）。
    pub first_factor: MfaFirstFactor,
    /// 登录完成后前端回跳的路径；未提供时为 `None`。
    pub return_path: Option<String>,
    /// 发起登录的客户端 IP，用于审计展示。
    pub ip_address: Option<String>,
    /// 发起登录的客户端 User-Agent，用于审计展示。
    pub user_agent: Option<String>,
    /// 当前 flow 内已失败的挑战次数；达到上限后 flow 作废。
    pub attempt_count: i32,
    /// 关联的 external recovery flow id；仅当本 flow 是外部恢复流程的 MFA
    /// 关联挑战时非空。
    #[serde(skip_serializing)]
    pub external_recovery_flow_id: Option<i64>,
    /// 发起关联挑战的浏览器 proof hash；设置了关联 flow 时必须回传匹配的
    /// 浏览器 proof，防止挑战 URL 换浏览器使用。
    #[serde(skip_serializing)]
    pub browser_binding_hash: Option<String>,
    /// flow 失效时刻（UTC）；之后不再接受挑战。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// flow 完成（挑战通过）的时刻；`None` 表示未完成。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 发起登录的账号（FK `user_id`）。
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
