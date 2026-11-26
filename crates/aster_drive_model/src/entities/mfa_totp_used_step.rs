//! SeaORM 实体定义：`mfa_totp_used_steps`。
//!
//! 记录每个 TOTP factor 已被成功消费的时间步（30 秒窗口），用唯一约束
//! 阻止同一个验证码在可接受窗口内被重复用于登录挑战或敏感操作确认。
//! 只保留当前窗口附近的记录，过期行在验证成功时顺带清理。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 单条 TOTP 验证码的时间步消费记录，配合唯一索引提供防重放：同一 factor
/// 的同一时间步只能成功验证一次。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "mfa_totp_used_steps")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 消费该时间步的 TOTP factor；与 `time_step` 组合唯一。
    pub factor_id: i64,
    /// 成功匹配时的时间步：`unix_timestamp / 30`。
    pub time_step: i64,
    /// 该时间步被成功消费的时刻（UTC）。
    pub used_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 记录所属的 TOTP factor（FK `factor_id`）。
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
