//! `webdav` API DTO 定义。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

/// WebDAV account settings for the current user.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavSettingsInfo {
    /// URL path prefix the WebDAV service is mounted under.
    pub prefix: String,
    /// Fully qualified WebDAV endpoint URL for the current request's scheme
    /// and host.
    pub endpoint: String,
}

/// Test WebDAV credentials.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TestConnectionReq {
    /// WebDAV sub-account username to verify.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub username: String,
    /// WebDAV sub-account password to verify.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub password: String,
}

/// Create a new WebDAV sub-account.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateWebdavAccountReq {
    /// Username of the new WebDAV sub-account.
    #[validate(custom(function = "crate::api::dto::validation::validate_webdav_username"))]
    pub username: String,
    /// Sub-account password; a random one is generated when omitted and
    /// returned once in the response.
    pub password: Option<String>,
    /// Folder the sub-account is rooted at; `None` = workspace root.
    #[validate(range(min = 1, message = "root_folder_id must be greater than 0"))]
    pub root_folder_id: Option<i64>,
}
