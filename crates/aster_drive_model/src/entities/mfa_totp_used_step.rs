//! SeaORM 实体定义：`mfa_totp_used_steps`。
//!
//! 记录每个 TOTP factor 已被成功消费的时间步（30 秒窗口），用唯一约束
//! 阻止同一个验证码在可接受窗口内被重复用于登录挑战或敏感操作确认。
//! 只保留当前窗口附近的记录，过期行在验证成功时顺带清理。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "mfa_totp_used_steps")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub factor_id: i64,
    /// 成功匹配时的时间步：`unix_timestamp / 30`。
    pub time_step: i64,
    pub used_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::mfa_factor::Entity",
        from = "Column::FactorId",
        to = "super::mfa_factor::Column::Id"
    )]
    MfaFactor,
}

impl Related<super::mfa_factor::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::MfaFactor.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
