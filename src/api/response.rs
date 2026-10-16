//! 统一 API 响应封装。

use aster_forge_api::response::{
    ApiErrorInfo as ForgeApiErrorInfo, ApiResponse as ForgeApiResponse, ApiResponseCode,
    ResponseEnvelopeError,
};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::api_error_code::ApiErrorCode;

impl ApiResponseCode for ApiErrorCode {
    fn is_success(&self) -> bool {
        *self == ApiErrorCode::Success
    }
}

pub type ApiErrorInfo = ForgeApiErrorInfo<ApiErrorDiagnostic>;

/// Drive's REST adapter binds product-owned code and diagnostic types to Forge's envelope.
/// Serialization and OpenAPI composition are delegated to the shared Forge response.
#[derive(Serialize)]
#[serde(transparent)]
pub struct ApiResponse<T: Serialize>(ForgeApiResponse<T, ApiErrorCode, ApiErrorDiagnostic>);

#[cfg(all(debug_assertions, feature = "openapi"))]
impl<T: Serialize + ToSchema> ToSchema for ApiResponse<T> {
    fn name() -> std::borrow::Cow<'static, str> {
        // Utoipa appends this product adapter's generic arguments itself.
        std::borrow::Cow::Borrowed("ApiResponse")
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        <ForgeApiResponse<T, ApiErrorCode, ApiErrorDiagnostic> as ToSchema>::schemas(schemas);
    }
}

#[cfg(all(debug_assertions, feature = "openapi"))]
impl<T: Serialize + ToSchema> utoipa::__dev::ComposeSchema for ApiResponse<T> {
    fn compose(
        generics: Vec<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>,
    ) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <ForgeApiResponse<T, ApiErrorCode, ApiErrorDiagnostic> as utoipa::__dev::ComposeSchema>::compose(generics)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ApiErrorDiagnostic {
    pub kind: String,
    pub message: String,
    /// Connector-owned field that caused a configuration validation failure.
    ///
    /// This is intentionally optional so existing diagnostics remain wire-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Connector field scope, for example `connector_config` or `static_credential`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

#[expect(
    clippy::expect_used,
    reason = "Drive supplies a fixed success code or an AsterError-mapped failure code; a classification mismatch is a product programming error"
)]
fn adapt_envelope<T: Serialize>(
    response: Result<ForgeApiResponse<T, ApiErrorCode, ApiErrorDiagnostic>, ResponseEnvelopeError>,
) -> ApiResponse<T> {
    ApiResponse(response.expect("Drive response code must match the envelope state"))
}

/// Injects AsterDrive's success code while keeping the wire envelope owned by Forge.
pub fn success<T: Serialize>(data: T) -> ApiResponse<T> {
    adapt_envelope(ForgeApiResponse::ok(ApiErrorCode::Success, data))
}

/// Builds the product's successful response whose `data` field is omitted.
pub fn empty_success() -> ApiResponse<()> {
    adapt_envelope(ForgeApiResponse::ok_empty(ApiErrorCode::Success))
}

/// Builds the product's successful response with `data: {}`.
pub fn empty_object_success() -> ApiResponse<aster_forge_api::response::ApiEmptyData> {
    adapt_envelope(ForgeApiResponse::ok_empty_data(ApiErrorCode::Success))
}

/// Adapts a Drive error code and policy into Forge's validated REST envelope.
pub fn failure(code: ApiErrorCode, message: impl Into<String>) -> ApiResponse<()> {
    adapt_envelope(ForgeApiResponse::error(code, message))
}

/// Adapts Drive-owned retryability and typed diagnostics into Forge's error metadata container.
pub fn failure_with_details(
    code: ApiErrorCode,
    message: impl Into<String>,
    error: Option<ApiErrorInfo>,
) -> ApiResponse<()> {
    adapt_envelope(ForgeApiResponse::error_with_details(code, message, error))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn forge_envelope_preserves_drive_wire_shapes() {
        assert_eq!(
            serde_json::to_value(success(json!({"id": 7}))).unwrap(),
            json!({"code": "success", "msg": "", "data": {"id": 7}})
        );
        assert_eq!(
            serde_json::to_value(empty_success()).unwrap(),
            json!({"code": "success", "msg": ""})
        );
        assert_eq!(
            serde_json::to_value(empty_object_success()).unwrap(),
            json!({"code": "success", "msg": "", "data": {}})
        );
        assert_eq!(
            serde_json::to_value(success(None::<u64>)).unwrap(),
            json!({"code": "success", "msg": "", "data": null})
        );
        assert_eq!(
            serde_json::to_value(success(Vec::<u64>::new())).unwrap(),
            json!({"code": "success", "msg": "", "data": []})
        );
        assert_eq!(
            serde_json::to_value(failure_with_details(
                ApiErrorCode::CredentialsFailed,
                "Invalid Credentials",
                None,
            ))
            .unwrap(),
            json!({
                "code": "auth.credentials_failed",
                "msg": "Invalid Credentials"
            })
        );
        assert_eq!(
            serde_json::to_value(failure(
                ApiErrorCode::CredentialsFailed,
                "Invalid Credentials",
            ))
            .unwrap(),
            json!({
                "code": "auth.credentials_failed",
                "msg": "Invalid Credentials",
                "error": {"retryable": false}
            })
        );
    }

    #[test]
    fn typed_diagnostics_round_trip_and_invalid_states_are_rejected() {
        let response = failure_with_details(
            ApiErrorCode::StorageDriverError,
            "storage unavailable",
            Some(ApiErrorInfo {
                retryable: true,
                diagnostic: Some(ApiErrorDiagnostic {
                    kind: "misconfigured".into(),
                    message: "bucket is missing".into(),
                    field: Some("bucket".into()),
                    scope: Some("connector_config".into()),
                }),
            }),
        );
        let encoded = serde_json::to_vec(&response).unwrap();
        let decoded: ForgeApiResponse<(), ApiErrorCode, ApiErrorDiagnostic> =
            serde_json::from_slice(&encoded).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);

        let invalid_success = json!({
            "code": "success",
            "msg": "",
            "error": {"retryable": false}
        });
        assert!(
            serde_json::from_value::<ForgeApiResponse<(), ApiErrorCode, ApiErrorDiagnostic>>(
                invalid_success
            )
            .is_err()
        );

        let invalid_failure = json!({
            "code": "not_found",
            "msg": "missing",
            "data": null
        });
        assert!(
            serde_json::from_value::<ForgeApiResponse<Option<()>, ApiErrorCode, ApiErrorDiagnostic>>(
                invalid_failure
            )
            .is_err()
        );
    }
}
