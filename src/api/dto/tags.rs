//! `tags` API DTO 定义。

use crate::services::content::tag::TAG_NAME_MAX_CHARS;
use aster_forge_validation::filename::char_count;
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

pub const DEFAULT_TAG_LIMIT: u64 = 50;

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateTagReq {
    #[validate(custom(function = "validate_tag_name"))]
    pub name: String,
    #[validate(custom(function = "validate_tag_color"))]
    pub color: String,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTagReq {
    #[validate(custom(function = "validate_tag_name"))]
    pub name: Option<String>,
    #[validate(custom(function = "validate_tag_color"))]
    pub color: Option<String>,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ReplaceEntityTagsReq {
    #[validate(length(max = 64, message = "tag_ids cannot contain more than 64 items"))]
    pub tag_ids: Vec<i64>,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchTagBindingReq {
    #[validate(length(max = 1024, message = "file_ids cannot contain more than 1024 items"))]
    pub file_ids: Vec<i64>,
    #[validate(length(max = 1024, message = "folder_ids cannot contain more than 1024 items"))]
    pub folder_ids: Vec<i64>,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagListQuery {
    #[validate(length(max = 64, message = "q cannot exceed 64 characters"))]
    pub q: Option<String>,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagPath {
    #[validate(range(min = 1, message = "tag_id must be greater than 0"))]
    pub tag_id: i64,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct EntityTagsPath {
    pub entity_type: crate::types::EntityType,
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
}

#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagEntityPath {
    #[validate(range(min = 1, message = "tag_id must be greater than 0"))]
    pub tag_id: i64,
    pub entity_type: crate::types::EntityType,
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
}

fn validate_tag_name(value: &str) -> std::result::Result<(), ValidationError> {
    let name = value.trim();
    if name.is_empty() {
        return Err(crate::api::dto::validation::message_validation_error(
            "tag name cannot be empty",
        ));
    }
    if char_count(name) > TAG_NAME_MAX_CHARS {
        return Err(crate::api::dto::validation::message_validation_error(
            format!("tag name too long (max {TAG_NAME_MAX_CHARS})"),
        ));
    }
    Ok(())
}

fn validate_tag_color(value: &str) -> std::result::Result<(), ValidationError> {
    let color = value.trim();
    let valid = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(crate::api::dto::validation::message_validation_error(
            "tag color must be a hex color like #3b82f6",
        ));
    }
    Ok(())
}
