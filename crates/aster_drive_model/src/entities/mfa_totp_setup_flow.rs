//! SeaORM 实体定义：`mfa_totp_setup_flows`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// TOTP 绑定流程：为账号生成临时共享密钥并等待用户完成首次验证。验证通过
/// 后密钥落入 `mfa_factors`，flow 即被消费；过期或取消的 flow 作废。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "mfa_totp_setup_flows")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 客户端持有的 flow 令牌 hash；令牌明文只在创建响应中出现一次。
    #[serde(skip_serializing)]
    pub flow_token_hash: String,
    /// 正在绑定 TOTP 的账号。
    pub user_id: i64,
    /// 待绑定的 TOTP 共享密钥密文；AAD 绑定 user_id。
    #[serde(skip_serializing)]
    pub secret_ciphertext: String,
    /// `secret_ciphertext` 的加密信封版本；新写入恒为 `1`，为密钥/格式轮换
    /// 保留。
    #[serde(skip_serializing)]
    pub secret_version: i32,
    /// flow 失效时刻（UTC）；之后不再接受确认。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTimeUtc,
    /// flow 完成（绑定确认）的时刻；`None` 表示未完成。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub consumed_at: Option<DateTimeUtc>,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 正在绑定 TOTP 的账号（FK `user_id`）。
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
