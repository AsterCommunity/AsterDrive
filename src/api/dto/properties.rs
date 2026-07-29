//! `properties` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

/// Path parameters for entity-scoped property routes.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct EntityPath {
    pub entity_type: aster_drive_model::types::EntityType,
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
}

/// Path parameters for individual property routes.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct PropPath {
    pub entity_type: aster_drive_model::types::EntityType,
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
    #[validate(custom(function = "crate::api::dto::validation::validate_property_namespace"))]
    pub namespace: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_property_name"))]
    pub name: String,
}

/// Set or delete a custom property on an entity.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_set_prop"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetPropReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_property_namespace"))]
    pub namespace: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_property_name"))]
    pub name: String,
    pub value: Option<String>,
}

fn validate_set_prop(value: &SetPropReq) -> std::result::Result<(), ValidationError> {
    if let Some(prop_value) = value.value.as_deref() {
        crate::api::dto::validation::validate_property_value(prop_value)?;
    }
    Ok(())
}
