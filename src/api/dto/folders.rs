//! `folders` API DTO 定义。

use aster_forge_api::NullablePatch;
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

/// Create a new folder.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateFolderReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: String,
    pub parent_id: Option<i64>,
}

/// Patch (partial update) a folder.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchFolderReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_name"))]
    pub name: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<i64>)
    )]
    pub parent_id: NullablePatch<i64>,
    #[serde(default, rename = "policy_id")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(ignore))]
    forbidden_policy_id: NullablePatch<i64>,
}

impl PatchFolderReq {
    pub fn includes_policy_id(&self) -> bool {
        self.forbidden_policy_id.is_present()
    }
}

/// Lock or unlock a folder.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SetLockReq {
    pub locked: bool,
}

/// Copy a folder to a target location.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CopyFolderReq {
    /// Target parent folder ID (`None` = root directory).
    pub parent_id: Option<i64>,
}
