//! 统一 API 响应封装。

use actix_web::HttpResponse;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::api_error_code::ApiErrorCode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ApiErrorInfo {
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<ApiErrorDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ApiErrorDiagnostic {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ApiEmptyData {}

/// 统一 API 响应格式
///
/// 成功: `{ "code": "success", "msg": "", "data": {...} }`
/// 失败: `{ "code": "auth.credentials_failed", "msg": "Invalid Credentials" }`
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ApiResponse<T: Serialize> {
    pub code: ApiErrorCode,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorInfo>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: ApiErrorCode::Success,
            msg: String::new(),
            data: Some(data),
            error: None,
        }
    }

    pub fn ok_empty() -> ApiResponse<()> {
        ApiResponse {
            code: ApiErrorCode::Success,
            msg: String::new(),
            data: None,
            error: None,
        }
    }

    pub fn ok_empty_data() -> ApiResponse<ApiEmptyData> {
        ApiResponse {
            code: ApiErrorCode::Success,
            msg: String::new(),
            data: Some(ApiEmptyData::default()),
            error: None,
        }
    }

    pub fn error(code: ApiErrorCode, msg: &str) -> ApiResponse<()> {
        Self::error_with_details(
            code,
            msg,
            Some(ApiErrorInfo {
                retryable: false,
                diagnostic: None,
            }),
        )
    }

    pub fn error_with_details(
        code: ApiErrorCode,
        msg: &str,
        error: Option<ApiErrorInfo>,
    ) -> ApiResponse<()> {
        ApiResponse {
            code,
            msg: msg.to_string(),
            data: None,
            error,
        }
    }

    pub fn into_response(self) -> HttpResponse {
        HttpResponse::Ok().json(self)
    }
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SystemInfoResponse {
    pub version: String,
    pub build_time: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MemoryStatsResponse {
    pub heap_allocated_mb: String,
    pub heap_peak_mb: String,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PurgedCountResponse {
    pub purged: u32,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemovedCountResponse {
    pub removed: u64,
}
