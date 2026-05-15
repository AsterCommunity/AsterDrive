//! `share_public` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

/// Verify a share password.
#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct VerifyPasswordReq {
    pub password: String,
}

/// Query parameters for direct link downloads.
/// NOTE: The `force_download()` method is defined in `src/api/routes/share_public.rs`.
#[derive(Deserialize, Default)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct DirectLinkQuery {
    pub download: Option<String>,
}
