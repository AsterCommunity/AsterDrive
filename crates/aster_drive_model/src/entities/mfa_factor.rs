//! SeaORM 实体定义：`mfa_factors`。
//!
//! 这张表只保存用户长期绑定的 MFA factor。当前唯一的持久化 factor 是 TOTP，
//! 因为它需要保存加密后的共享密钥、记录启用时间和最近使用时间。
//! 登录邮箱验证码不属于这张表；它是每次登录 flow 生成的短期 challenge code，
//! 保存在 `mfa_email_codes`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::MfaPersistentFactorMethod;

/// 用户长期绑定的 MFA factor。每个账号每类方法最多一条；登录邮箱验证码等
/// 临时 challenge 不落这张表。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "mfa_factors")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 绑定该 factor 的账号。
    pub user_id: i64,
    /// 长期 factor 的方法类型。不要在这里加入 `email_code` 这类临时 challenge 方法。
    pub method: MfaPersistentFactorMethod,
    /// 用户可见的 factor 显示名。
    pub name: String,
    /// TOTP 共享密钥的密文；AAD 绑定 user_id 和持久化 factor method。
    #[serde(skip_serializing)]
    pub secret_ciphertext: String,
    /// `secret_ciphertext` 的加密信封版本；新写入恒为 `1`，为密钥/格式轮换
    /// 保留。
    #[serde(skip_serializing)]
    pub secret_version: i32,
    /// factor 启用（绑定完成）的时刻（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub enabled_at: DateTimeUtc,
    /// 最近一次成功使用该 factor 的时刻；未使用过时为 `None`。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_used_at: Option<DateTimeUtc>,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// 行最近更新时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 绑定该 factor 的账号（FK `user_id`）。
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
