//! SeaORM entity definition for `storage_connector_application_configs`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::StorageCredentialProvider;

#[derive(Clone, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(table_name = "storage_connector_application_configs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub policy_id: i64,
    pub provider: StorageCredentialProvider,
    pub tenant_id: Option<String>,
    pub scopes: String,
    pub client_id: Option<String>,
    #[serde(skip_serializing)]
    pub client_secret_ciphertext: Option<String>,
    pub metadata: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("id", &self.id)
            .field("policy_id", &self.policy_id)
            .field("provider", &self.provider)
            .field("tenant_id", &self.tenant_id)
            .field("scopes", &self.scopes)
            .field("client_id", &self.client_id)
            .field(
                "client_secret_ciphertext",
                &self
                    .client_secret_ciphertext
                    .as_ref()
                    .map(|_| "***REDACTED***"),
            )
            .field("metadata", &"***REDACTED***")
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    StoragePolicy,
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
