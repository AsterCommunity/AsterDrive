use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, thumbnail_generation_error_with_code};

pub(super) fn thumbnail_render_failed(message: impl Into<String>) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailRenderFailed, message)
}

pub(super) fn thumbnail_output_invalid(message: impl Into<String>) -> AsterError {
    thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailOutputInvalid, message)
}
