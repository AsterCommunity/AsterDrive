//! `webdav` API DTO 定义。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

/// WebDAV account settings for the current user.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavSettingsInfo {
    pub prefix: String,
    pub endpoint: String,
}

/// Test WebDAV credentials.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestConnectionReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub username: String,
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub password: String,
}

/// Create a new WebDAV sub-account.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateWebdavAccountReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_webdav_username"))]
    pub username: String,
    pub password: Option<String>,
    #[validate(range(min = 1, message = "root_folder_id must be greater than 0"))]
    pub root_folder_id: Option<i64>,
}
