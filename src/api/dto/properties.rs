//! `properties` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

/// Path parameters for entity-scoped property routes.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct EntityPath {
    /// Entity kind the properties belong to (`file` or `folder`).
    pub entity_type: aster_drive_model::types::EntityType,
    /// Entity ID; must be positive.
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
}

/// Path parameters for individual property routes.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct PropPath {
    /// Entity kind the property belongs to (`file` or `folder`).
    pub entity_type: aster_drive_model::types::EntityType,
    /// Entity ID; must be positive.
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
    /// Property namespace; the DAV namespace is reserved.
    #[validate(custom(function = "crate::api::dto::validation::validate_property_namespace"))]
    pub namespace: String,
    /// Property name within `namespace`.
    #[validate(custom(function = "crate::api::dto::validation::validate_property_name"))]
    pub name: String,
}

/// Set or delete a custom property on an entity.
#[derive(Deserialize, Validate)]
#[validate(schema(function = "validate_set_prop"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetPropReq {
    /// Namespace to write the property into; the DAV namespace is reserved.
    #[validate(custom(function = "crate::api::dto::validation::validate_property_namespace"))]
    pub namespace: String,
    /// Property name within `namespace`.
    #[validate(custom(function = "crate::api::dto::validation::validate_property_name"))]
    pub name: String,
    /// Property value; `None` upserts a null-valued property (deleting the
    /// property requires the DELETE route).
    pub value: Option<String>,
}

fn validate_set_prop(value: &SetPropReq) -> std::result::Result<(), ValidationError> {
    if let Some(prop_value) = value.value.as_deref() {
        crate::api::dto::validation::validate_property_value(prop_value)?;
    }
    Ok(())
}
