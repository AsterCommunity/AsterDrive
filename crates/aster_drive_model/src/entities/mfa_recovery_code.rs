//! SeaORM 实体定义：`mfa_recovery_codes`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 单个 MFA 恢复码：第一因子不可用时的一次性替代验证凭据。每个账号持有一
/// 组恢复码，每条只能使用一次，整组重新生成后旧行删除。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "mfa_recovery_codes")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 恢复码所属账号。
    pub user_id: i64,
    /// 恢复码内容的 hash；明文只在生成时展示一次。
    #[serde(skip_serializing)]
    pub code_hash: String,
    /// 恢复码被使用（消费）的时刻；`None` 表示仍可用。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub used_at: Option<DateTimeUtc>,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 恢复码所属账号（FK `user_id`）。
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
