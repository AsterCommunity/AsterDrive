//! `tags` API DTO 定义。

use crate::services::content::tag::TAG_NAME_MAX_CHARS;
use aster_forge_validation::filename::char_count;
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

/// Default number of tags returned per page when no limit is requested.
pub const DEFAULT_TAG_LIMIT: u64 = 50;

/// Create a new tag.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateTagReq {
    /// Tag display name.
    #[validate(custom(function = "validate_tag_name"))]
    pub name: String,
    /// Tag color as a `#rrggbb` hex string.
    #[validate(custom(function = "validate_tag_color"))]
    pub color: String,
}

/// Update an existing tag.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTagReq {
    /// New tag name; `None` keeps the current name.
    #[validate(custom(function = "validate_tag_name"))]
    pub name: Option<String>,
    /// New tag color as a `#rrggbb` hex string; `None` keeps the current color.
    #[validate(custom(function = "validate_tag_color"))]
    pub color: Option<String>,
}

/// Replace the full tag set attached to an entity.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ReplaceEntityTagsReq {
    /// Complete tag list to attach to the entity, replacing the previous set.
    #[validate(length(max = 64, message = "tag_ids cannot contain more than 64 items"))]
    pub tag_ids: Vec<i64>,
}

/// Batch attach or detach one tag to multiple files and folders.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchTagBindingReq {
    /// Files the attach/detach operation applies to.
    #[validate(length(max = 1024, message = "file_ids cannot contain more than 1024 items"))]
    pub file_ids: Vec<i64>,
    /// Folders the attach/detach operation applies to.
    #[validate(length(max = 1024, message = "folder_ids cannot contain more than 1024 items"))]
    pub folder_ids: Vec<i64>,
}

/// Query parameters for listing tags.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagListQuery {
    /// Optional tag name filter (max 64 characters).
    #[validate(length(max = 64, message = "q cannot exceed 64 characters"))]
    pub q: Option<String>,
}

/// Path parameters for tag-scoped routes.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagPath {
    /// Tag ID; must be positive.
    #[validate(range(min = 1, message = "tag_id must be greater than 0"))]
    pub tag_id: i64,
}

/// Path parameters for the tag list of an entity.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct EntityTagsPath {
    /// Entity kind carrying the tags (`file` or `folder`).
    pub entity_type: aster_drive_model::types::EntityType,
    /// Entity ID; must be positive.
    #[validate(range(min = 1, message = "entity_id must be greater than 0"))]
    pub entity_id: i64,
}

/// Path parameters identifying a single tag-entity binding.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct TagEntityPath {
    /// Tag ID; must be positive.
    #[validate(range(min = 1, message = "tag_id must be greater than 0"))]
    pub tag_id: i64,
    /// Entity kind carrying the tag (`file` or `folder`).
    pub entity_type: aster_drive_model::types::EntityType,
    /// Entity ID; must be positive.
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
