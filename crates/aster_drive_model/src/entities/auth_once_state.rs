//! Writer-owned one-time authentication state; never an independently fallible cache copy.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_once_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key_hash: String,
    pub payload: String,
    pub expires_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
