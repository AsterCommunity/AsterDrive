//! `shares` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

/// Create a new share.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateShareReq {
    #[validate(nested)]
    pub target: crate::services::share::ShareTarget,
    pub password: Option<String>,
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    #[validate(range(min = 0, message = "max_downloads cannot be negative"))]
    pub max_downloads: i64,
}

/// Update an existing share.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct UpdateShareReq {
    /// `None` = keep existing password, `Some("")` = remove password,
    /// non-empty = replace password.
    pub password: Option<String>,
    #[cfg_attr(
        all(debug_assertions, feature = "openapi"),
        schema(value_type = Option<String>)
    )]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[validate(range(min = 0, message = "max_downloads cannot be negative"))]
    pub max_downloads: i64,
}

/// Batch delete shares.
#[derive(Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct BatchDeleteSharesReq {
    #[serde(default)]
    #[validate(custom(function = "validate_batch_share_ids"))]
    pub share_ids: Vec<i64>,
}

fn validate_batch_share_ids(value: &[i64]) -> std::result::Result<(), ValidationError> {
    if value.iter().any(|id| *id <= 0) {
        return Err(crate::api::dto::validation::message_validation_error(
            "share_ids must contain only positive IDs",
        ));
    }
    crate::services::share::validate_batch_share_ids(value)
        .map_err(|error| crate::api::dto::validation::message_validation_error(error.message()))
}
