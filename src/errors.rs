//! 统一错误类型与映射。

use actix_web::http::StatusCode;
use std::any::Any;

use crate::api::response::ApiErrorInfo;
use crate::api::subcode::ApiSubcode;
use crate::storage::error::{
    StorageErrorKind, storage_driver_error_display_message, storage_driver_error_kind_from_message,
};

const API_ERROR_SUBCODE_PREFIX: &str = "__ASTER_API_SUBCODE__=";
const API_ERROR_SUBCODE_SEPARATOR: &str = "::";
const AUTH_TOKEN_INVALID_REASON_PREFIX: &str = "__ASTER_AUTH_TOKEN_INVALID_REASON__=";
const AUTH_TOKEN_INVALID_REASON_SEPARATOR: &str = "::";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTokenInvalidReason {
    Invalid,
    ReuseDetected,
    Stale,
}

impl AuthTokenInvalidReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Invalid => "invalid",
            Self::ReuseDetected => "reuse_detected",
            Self::Stale => "stale",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "invalid" => Some(Self::Invalid),
            "reuse_detected" => Some(Self::ReuseDetected),
            "stale" => Some(Self::Stale),
            _ => None,
        }
    }
}

/// 计数宏：计算传入标识符的数量（放在 define_errors! 之前，因为后者在展开时会调用此宏）
macro_rules! count {
    () => { 0 };
    ($head:ident $($tail:ident)*) => { 1 + count!($($tail)*) };
}

/// 内部错误类型，字符串错误码（E001-E0xx），用于 Rust 内部、日志、调试
macro_rules! define_errors {
    ($(
        $variant:ident($code:literal, $type_name:literal)
    ),* $(,)?) => {
        #[derive(Debug, Clone)]
        pub enum AsterError {
            $($variant(String),)*
        }

        /// 变体总数，用于 error_code.rs 编译期穷举检查
        pub const ASTER_ERROR_VARIANT_COUNT: usize = count!($($variant)*);

        impl AsterError {
            /// 内部错误码（字符串，如 "E001"），用于日志和调试
            pub fn code(&self) -> &'static str {
                match self {
                    $(AsterError::$variant(_) => $code,)*
                }
            }

            pub(crate) fn raw_message(&self) -> &str {
                match self {
                    $(AsterError::$variant(msg) => msg.as_str(),)*
                }
            }

            /// 错误类型名称
            pub fn error_type(&self) -> &'static str {
                match self {
                    $(AsterError::$variant(_) => $type_name,)*
                }
            }

            /// 错误详情
            pub fn message(&self) -> &str {
                api_error_display_message(auth_token_invalid_display_message(
                    storage_driver_error_display_message(self.raw_message()),
                ))
            }
        }

        impl std::fmt::Display for AsterError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}: {}", self.error_type(), self.message())
            }
        }

        impl std::error::Error for AsterError {}

        // snake_case 构造函数
        paste::paste! {
            impl AsterError {
                $(
                    pub fn [<$variant:snake>](msg: impl Into<String>) -> Self {
                        Self::$variant(msg.into())
                    }
                )*
            }
        }
    };
}

define_errors! {
    // ========== E001-E009: 基础设施错误 ==========
    DatabaseConnection(  "E001", "Database Connection Error"),
    DatabaseOperation(   "E002", "Database Operation Error"),
    ConfigError(         "E003", "Configuration Error"),
    InternalError(       "E004", "Internal Server Error"),
    ValidationError(     "E005", "Validation Error"),
    RecordNotFound(      "E006", "Record Not Found"),
    RateLimited(         "E007", "Rate Limited"),
    MailNotConfigured(   "E008", "Mail Not Configured"),
    MailDeliveryFailed(  "E009", "Mail Delivery Failed"),

    // ========== E010-E019: 认证错误 ==========
    AuthInvalidCredentials("E010", "Invalid Credentials"),
    AuthTokenExpired(      "E011", "Token Expired"),
    AuthTokenInvalid(      "E012", "Token Invalid"),
    AuthForbidden(         "E013", "Forbidden"),
    AuthPendingActivation( "E014", "Pending Activation"),
    ContactVerificationInvalid("E015", "Contact Verification Invalid"),
    ContactVerificationExpired("E016", "Contact Verification Expired"),

    // ========== E020-E029: 文件错误 ==========
    FileNotFound(         "E020", "File Not Found"),
    FileTooLarge(         "E021", "File Too Large"),
    FileTypeNotAllowed(   "E022", "File Type Not Allowed"),
    FileUploadFailed(     "E023", "Upload Failed"),
    PayloadTooLarge(      "E024", "Payload Too Large"),

    // ========== E030-E039: 存储策略错误 ==========
    StoragePolicyNotFound("E030", "Storage Policy Not Found"),
    StorageDriverError(   "E031", "Storage Driver Error"),
    StorageQuotaExceeded( "E032", "Quota Exceeded"),
    UnsupportedDriver(    "E033", "Unsupported Driver"),

    // ========== E040-E049: 文件夹错误 ==========
    FolderNotFound(       "E040", "Folder Not Found"),

    // ========== E050-E059: 分享错误 ==========
    ShareNotFound(         "E050", "Share Not Found"),
    ShareExpired(          "E051", "Share Expired"),
    SharePasswordRequired( "E052", "Share Password Required"),
    ShareDownloadLimit(    "E053", "Share Download Limit Reached"),

    // ========== E054-E057: 分片上传错误 ==========
    UploadSessionNotFound( "E054", "Upload Session Not Found"),
    UploadSessionExpired(  "E055", "Upload Session Expired"),
    ChunkUploadFailed(     "E056", "Chunk Upload Failed"),
    UploadAssemblyFailed(  "E057", "Upload Assembly Failed"),

    // ========== E058-E058: 缩略图错误 ==========
    ThumbnailGenerationFailed("E058", "Thumbnail Generation Failed"),

    // ========== E059-E059: 资源锁定 ==========
    ResourceLocked("E059", "Resource Locked"),

    // ========== E060: 前置条件失败 ==========
    PreconditionFailed("E060", "Precondition Failed"),

    // ========== E061: 上传处理中 ==========
    UploadAssembling("E061", "Upload Assembling"),
}

impl AsterError {
    pub fn storage_error_kind(&self) -> Option<StorageErrorKind> {
        match self {
            Self::StorageDriverError(message) => {
                Some(storage_driver_error_kind_from_message(message))
            }
            Self::PreconditionFailed(_) => Some(StorageErrorKind::Precondition),
            Self::UnsupportedDriver(_) => Some(StorageErrorKind::Unsupported),
            _ => None,
        }
    }

    pub fn auth_token_invalid_reason(&self) -> Option<AuthTokenInvalidReason> {
        match self {
            Self::AuthTokenInvalid(message) => auth_token_invalid_reason_from_message(message),
            _ => None,
        }
    }

    pub fn api_error_info(&self) -> ApiErrorInfo {
        ApiErrorInfo {
            internal_code: self.code().to_string(),
            subcode: self.api_error_subcode(),
            retryable: self.api_error_retryable(),
        }
    }

    pub fn api_error_subcode(&self) -> Option<ApiSubcode> {
        if let Some(subcode) = api_error_subcode_from_message(self.raw_message()) {
            return Some(subcode);
        }

        match self.storage_error_kind()? {
            StorageErrorKind::Auth => Some(ApiSubcode::StorageAuth),
            StorageErrorKind::Misconfigured => Some(ApiSubcode::StorageMisconfigured),
            StorageErrorKind::NotFound => Some(ApiSubcode::StorageNotFound),
            StorageErrorKind::Permission => Some(ApiSubcode::StoragePermission),
            StorageErrorKind::Precondition => Some(ApiSubcode::StoragePrecondition),
            StorageErrorKind::RateLimited => Some(ApiSubcode::StorageRateLimited),
            StorageErrorKind::Transient => Some(ApiSubcode::StorageTransient),
            StorageErrorKind::Unsupported => Some(ApiSubcode::StorageUnsupported),
            StorageErrorKind::Unknown => Some(ApiSubcode::StorageUnknown),
        }
    }

    fn api_error_retryable(&self) -> Option<bool> {
        match self {
            Self::RateLimited(_) | Self::UploadAssembling(_) => Some(true),
            Self::StorageDriverError(_) => Some(matches!(
                self.storage_error_kind(),
                Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited)
            )),
            _ => None,
        }
    }

    /// HTTP 状态码映射
    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::ValidationError(_)
            | Self::FileTooLarge(_)
            | Self::FileTypeNotAllowed(_)
            | Self::UnsupportedDriver(_) => StatusCode::BAD_REQUEST,

            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,

            Self::ChunkUploadFailed(_) if chunk_upload_failure_is_client_error(self) => {
                StatusCode::BAD_REQUEST
            }

            Self::ContactVerificationInvalid(_) => StatusCode::BAD_REQUEST,

            Self::AuthInvalidCredentials(_)
            | Self::AuthTokenExpired(_)
            | Self::AuthTokenInvalid(_) => StatusCode::UNAUTHORIZED,

            Self::AuthForbidden(_) | Self::AuthPendingActivation(_) => StatusCode::FORBIDDEN,

            Self::ResourceLocked(_) => StatusCode::LOCKED,

            Self::PreconditionFailed(_) => StatusCode::PRECONDITION_FAILED,

            Self::UploadAssembling(_) => StatusCode::ACCEPTED,
            Self::RecordNotFound(_)
            | Self::FileNotFound(_)
            | Self::StoragePolicyNotFound(_)
            | Self::FolderNotFound(_)
            | Self::ShareNotFound(_)
            | Self::UploadSessionNotFound(_) => StatusCode::NOT_FOUND,

            Self::ShareExpired(_) => StatusCode::NOT_FOUND,

            Self::UploadSessionExpired(_) => StatusCode::GONE,

            Self::ContactVerificationExpired(_) => StatusCode::GONE,

            Self::SharePasswordRequired(_) | Self::ShareDownloadLimit(_) => StatusCode::FORBIDDEN,

            Self::StorageQuotaExceeded(_) => StatusCode::INSUFFICIENT_STORAGE,

            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,

            Self::MailNotConfigured(_) | Self::MailDeliveryFailed(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }

            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<sea_orm::DbErr> for AsterError {
    fn from(e: sea_orm::DbErr) -> Self {
        match e {
            sea_orm::DbErr::RecordNotFound(msg) => Self::RecordNotFound(msg),
            other => Self::DatabaseOperation(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseLogLevel {
    Skip,
    Warn,
    Error,
}

impl AsterError {
    fn response_log_level(&self) -> ResponseLogLevel {
        match self {
            // 507 在这里表示用户配额耗尽，属于可预期业务限制，不按服务故障记录。
            Self::StorageQuotaExceeded(_)
            | Self::RateLimited(_)
            | Self::MailNotConfigured(_)
            | Self::MailDeliveryFailed(_) => ResponseLogLevel::Warn,
            _ => {
                let status = self.http_status();
                if status.is_server_error() {
                    ResponseLogLevel::Error
                } else if status.is_client_error()
                    && status != StatusCode::UNAUTHORIZED
                    && status != StatusCode::FORBIDDEN
                    && status != StatusCode::NOT_FOUND
                {
                    ResponseLogLevel::Warn
                } else {
                    ResponseLogLevel::Skip
                }
            }
        }
    }

    fn client_message(&self) -> &str {
        match self.response_log_level() {
            ResponseLogLevel::Error => self.error_type(),
            ResponseLogLevel::Warn | ResponseLogLevel::Skip => self.message(),
        }
    }
}

impl actix_web::ResponseError for AsterError {
    fn status_code(&self) -> StatusCode {
        self.http_status()
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        use crate::api::response::ApiResponse;
        let status = self.http_status();

        match self.response_log_level() {
            ResponseLogLevel::Error => {
                tracing::error!(status = %status, error = %self, "server error");
            }
            ResponseLogLevel::Warn => {
                tracing::warn!(status = %status, error = %self, "request error");
            }
            ResponseLogLevel::Skip => {}
        }

        let error_code: crate::api::error_code::ErrorCode = self.into();
        let mut response = actix_web::HttpResponse::build(status);
        if self.share_error_should_disable_cache() {
            response.insert_header(("Cache-Control", "no-store, max-age=0"));
        }
        response.json(ApiResponse::<()>::error_with_details(
            error_code,
            self.client_message(),
            Some(self.api_error_info()),
        ))
    }
}

impl AsterError {
    fn share_error_should_disable_cache(&self) -> bool {
        matches!(
            self,
            Self::ShareNotFound(_)
                | Self::ShareExpired(_)
                | Self::SharePasswordRequired(_)
                | Self::ShareDownloadLimit(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, AsterError>;

pub fn display_error(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub(crate) fn encode_task_error_for_storage(error: &AsterError) -> String {
    if api_error_subcode_from_message(error.raw_message()).is_some() {
        return error.raw_message().to_string();
    }
    if let Some(subcode) = error.api_error_subcode() {
        return encode_api_error_subcode_message(subcode, error.to_string());
    }
    error.to_string()
}

pub(crate) fn task_error_subcode_from_storage(raw_message: &str) -> Option<ApiSubcode> {
    api_error_subcode_from_message(raw_message)
}

pub(crate) fn task_error_display_message(raw_message: &str) -> &str {
    api_error_display_message(raw_message)
}

pub fn validation_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::validation_error)
}

pub fn auth_forbidden_with_subcode(subcode: ApiSubcode, message: impl Into<String>) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::auth_forbidden)
}

pub fn auth_token_invalid_with_reason(
    reason: AuthTokenInvalidReason,
    message: impl Into<String>,
) -> AsterError {
    AsterError::auth_token_invalid(encode_auth_token_invalid_reason_message(
        reason,
        message.into(),
    ))
}

pub fn precondition_failed_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::precondition_failed)
}

pub fn file_upload_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::file_upload_failed)
}

pub fn payload_too_large_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::payload_too_large)
}

pub fn chunk_upload_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::chunk_upload_failed)
}

pub fn upload_assembly_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::upload_assembly_failed)
}

pub fn thumbnail_generation_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    tag_error_with_subcode(subcode, message, AsterError::thumbnail_generation_failed)
}

fn tag_error_with_subcode(
    subcode: ApiSubcode,
    message: impl Into<String>,
    f: impl FnOnce(String) -> AsterError,
) -> AsterError {
    f(encode_api_error_subcode_message(subcode, message.into()))
}

pub(crate) fn encode_api_error_subcode_message(subcode: ApiSubcode, message: String) -> String {
    format!(
        "{API_ERROR_SUBCODE_PREFIX}{}{API_ERROR_SUBCODE_SEPARATOR}{message}",
        subcode.as_str()
    )
}

fn split_encoded_api_error_message(raw_message: &str) -> Option<(&str, &str)> {
    let encoded = raw_message.strip_prefix(API_ERROR_SUBCODE_PREFIX)?;
    encoded.split_once(API_ERROR_SUBCODE_SEPARATOR)
}

fn split_encoded_api_error_subcode_message(raw_message: &str) -> Option<(ApiSubcode, &str)> {
    let (subcode, message) = split_encoded_api_error_message(raw_message)?;
    Some((subcode.parse::<ApiSubcode>().ok()?, message))
}

fn api_error_display_message(raw_message: &str) -> &str {
    split_encoded_api_error_message(raw_message)
        .map(|(_, message)| message)
        .unwrap_or(raw_message)
}

fn encode_auth_token_invalid_reason_message(
    reason: AuthTokenInvalidReason,
    message: String,
) -> String {
    format!(
        "{AUTH_TOKEN_INVALID_REASON_PREFIX}{}{AUTH_TOKEN_INVALID_REASON_SEPARATOR}{message}",
        reason.as_str()
    )
}

fn split_encoded_auth_token_invalid_reason_message(
    raw_message: &str,
) -> Option<(AuthTokenInvalidReason, &str)> {
    let encoded = raw_message.strip_prefix(AUTH_TOKEN_INVALID_REASON_PREFIX)?;
    let (reason, message) = encoded.split_once(AUTH_TOKEN_INVALID_REASON_SEPARATOR)?;
    Some((AuthTokenInvalidReason::parse(reason)?, message))
}

fn auth_token_invalid_display_message(raw_message: &str) -> &str {
    split_encoded_auth_token_invalid_reason_message(raw_message)
        .map(|(_, message)| message)
        .unwrap_or(raw_message)
}

fn auth_token_invalid_reason_from_message(raw_message: &str) -> Option<AuthTokenInvalidReason> {
    split_encoded_auth_token_invalid_reason_message(raw_message).map(|(reason, _)| reason)
}

fn api_error_subcode_from_message(raw_message: &str) -> Option<ApiSubcode> {
    split_encoded_api_error_subcode_message(raw_message)
        .map(|(subcode, _)| subcode)
        .or_else(|| {
            let storage_message = storage_driver_error_display_message(raw_message);
            (storage_message != raw_message)
                .then(|| {
                    split_encoded_api_error_subcode_message(storage_message)
                        .map(|(subcode, _)| subcode)
                })
                .flatten()
        })
}

fn chunk_upload_failure_is_client_error(error: &AsterError) -> bool {
    matches!(
        error.api_error_subcode(),
        Some(
            ApiSubcode::UploadChunkNumberOutOfRange
                | ApiSubcode::UploadChunkSizeMismatch
                | ApiSubcode::UploadChunkTooLarge
                | ApiSubcode::UploadChunkSizeOverflow
                | ApiSubcode::UploadChunkTransportMismatch
                | ApiSubcode::UploadChunkSessionInvalid
                | ApiSubcode::UploadRequestBodyReadFailed
        )
    )
}

fn map_display_error<E: std::fmt::Display + 'static>(
    err: E,
    ctx: Option<&str>,
    f: impl FnOnce(String) -> AsterError,
) -> AsterError {
    if let Some(db_err) = (&err as &dyn Any).downcast_ref::<sea_orm::DbErr>()
        && let sea_orm::DbErr::RecordNotFound(message) = db_err
    {
        let message = match ctx {
            Some(ctx) => format!("{ctx}: {message}"),
            None => message.clone(),
        };
        return AsterError::record_not_found(message);
    }

    let message = match ctx {
        Some(ctx) => format!("{ctx}: {err}"),
        None => err.to_string(),
    };
    f(message)
}

/// Extension trait to reduce common `map_err` boilerplate.
pub trait MapAsterErr<T> {
    /// Map any `Display` error to an `AsterError` variant via its constructor.
    ///
    /// ```ignore
    /// io_op().map_aster_err(AsterError::storage_driver_error)?;
    /// ```
    fn map_aster_err(self, f: impl FnOnce(String) -> AsterError) -> Result<T>;

    /// Like `map_aster_err` but prepends a static context string.
    ///
    /// ```ignore
    /// s3_op().map_aster_err_ctx("S3 put failed", AsterError::storage_driver_error)?;
    /// ```
    fn map_aster_err_ctx(self, ctx: &str, f: impl FnOnce(String) -> AsterError) -> Result<T>;

    /// Map any error to a prebuilt `AsterError`, ignoring the original error value.
    ///
    /// ```ignore
    /// decode().map_aster_err_with(|| AsterError::validation_error("invalid token"))?;
    /// ```
    fn map_aster_err_with(self, f: impl FnOnce() -> AsterError) -> Result<T>;
}

impl<T, E: std::fmt::Display + 'static> MapAsterErr<T> for std::result::Result<T, E> {
    fn map_aster_err(self, f: impl FnOnce(String) -> AsterError) -> Result<T> {
        self.map_err(|e| map_display_error(e, None, f))
    }

    fn map_aster_err_ctx(self, ctx: &str, f: impl FnOnce(String) -> AsterError) -> Result<T> {
        self.map_err(|e| map_display_error(e, Some(ctx), f))
    }

    fn map_aster_err_with(self, f: impl FnOnce() -> AsterError) -> Result<T> {
        self.map_err(|_| f())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsterError, MapAsterErr, ResponseLogLevel, thumbnail_generation_error_with_subcode,
        upload_assembly_error_with_subcode, validation_error_with_subcode,
    };
    use crate::api::error_code::ErrorCode;
    use crate::api::response::ApiErrorInfo;
    use crate::api::subcode::ApiSubcode;
    use crate::storage::error::{StorageErrorKind, storage_driver_error};
    use actix_web::body;
    use actix_web::http::StatusCode;

    #[test]
    fn quota_exceeded_507_logs_as_warn() {
        let err = AsterError::storage_quota_exceeded("quota 1024, used 1000, need 100");
        assert_eq!(err.http_status(), StatusCode::INSUFFICIENT_STORAGE);
        assert_eq!(err.response_log_level(), ResponseLogLevel::Warn);
    }

    #[test]
    fn validation_error_logs_as_warn() {
        let err = AsterError::validation_error("invalid filename");
        assert_eq!(err.response_log_level(), ResponseLogLevel::Warn);
    }

    #[test]
    fn auth_error_is_skipped() {
        let err = AsterError::auth_token_invalid("invalid token");
        assert_eq!(err.response_log_level(), ResponseLogLevel::Skip);
    }

    #[test]
    fn internal_error_logs_as_error() {
        let err = AsterError::internal_error("db pool poisoned");
        assert_eq!(err.response_log_level(), ResponseLogLevel::Error);
        assert_eq!(err.client_message(), "Internal Server Error");
    }

    #[test]
    fn quota_exceeded_keeps_client_message() {
        let err = AsterError::storage_quota_exceeded("quota 1024, used 1000, need 100");
        assert_eq!(err.client_message(), "quota 1024, used 1000, need 100");
    }

    #[test]
    fn share_expired_maps_to_not_found_and_disables_cache() {
        let err = AsterError::share_expired("expired");
        assert_eq!(err.http_status(), StatusCode::NOT_FOUND);

        let response = actix_web::ResponseError::error_response(&err);
        assert_eq!(
            response
                .headers()
                .get("Cache-Control")
                .and_then(|value| value.to_str().ok()),
            Some("no-store, max-age=0")
        );
    }

    #[test]
    fn thumbnail_generation_failed_maps_to_server_error() {
        let err = AsterError::thumbnail_generation_failed("decode failed");
        assert_eq!(err.http_status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.response_log_level(), ResponseLogLevel::Error);
    }

    #[test]
    fn map_aster_err_with_ignores_source_error() {
        let err = std::result::Result::<(), std::io::Error>::Err(std::io::Error::other("boom"))
            .map_aster_err_with(|| AsterError::validation_error("invalid token"))
            .unwrap_err();

        assert_eq!(err.code(), "E005");
        assert_eq!(err.message(), "invalid token");
    }

    #[test]
    fn map_aster_err_preserves_db_record_not_found() {
        let err = Err::<(), sea_orm::DbErr>(sea_orm::DbErr::RecordNotFound("user#42".to_string()))
            .map_aster_err(AsterError::database_operation)
            .unwrap_err();

        assert_eq!(err.code(), "E006");
        assert_eq!(err.message(), "user#42");
    }

    #[test]
    fn map_aster_err_ctx_preserves_db_record_not_found_context() {
        let err = Err::<(), sea_orm::DbErr>(sea_orm::DbErr::RecordNotFound("user#42".to_string()))
            .map_aster_err_ctx("load user", AsterError::database_operation)
            .unwrap_err();

        assert_eq!(err.code(), "E006");
        assert_eq!(err.message(), "load user: user#42");
    }

    #[test]
    fn api_error_info_serializes_subcode_as_stable_wire_value() {
        let info = ApiErrorInfo {
            internal_code: "E005".to_string(),
            subcode: Some(ApiSubcode::UploadChunkSizeMismatch),
            retryable: None,
        };

        let payload = serde_json::to_value(&info).expect("ApiErrorInfo should serialize");

        assert_eq!(payload["internal_code"], "E005");
        assert_eq!(payload["subcode"], "upload.chunk_size_mismatch");
        assert!(payload.get("retryable").is_none());
    }

    #[test]
    fn api_error_info_rejects_unknown_subcode_on_deserialize() {
        let payload = serde_json::json!({
            "internal_code": "E005",
            "subcode": "remote.dynamic"
        });

        let err = serde_json::from_value::<ApiErrorInfo>(payload)
            .expect_err("unknown ApiErrorInfo subcode should not deserialize");

        assert!(err.is_data());
    }

    #[test]
    fn unknown_encoded_subcode_is_not_exposed_as_api_subcode() {
        let raw = "__ASTER_API_SUBCODE__=remote.dynamic::remote message";
        let err = AsterError::validation_error(raw);

        assert_eq!(err.api_error_subcode(), None);
        assert_eq!(err.message(), "remote message");
    }

    #[actix_web::test]
    async fn storage_transient_error_response_includes_specific_code_and_details() {
        let err = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(
            payload["code"],
            serde_json::json!(ErrorCode::StorageTransientFailure as i32)
        );
        assert_eq!(payload["msg"], "Storage Driver Error");
        assert_eq!(payload["error"]["internal_code"], "E031");
        assert_eq!(payload["error"]["subcode"], "storage.transient");
        assert_eq!(payload["error"]["retryable"], true);
    }

    #[actix_web::test]
    async fn storage_permission_error_response_includes_specific_code_and_details() {
        let err = storage_driver_error(StorageErrorKind::Permission, "access denied");
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(
            payload["code"],
            serde_json::json!(ErrorCode::StoragePermissionDenied as i32)
        );
        assert_eq!(payload["error"]["internal_code"], "E031");
        assert_eq!(payload["error"]["subcode"], "storage.permission");
        assert_eq!(payload["error"]["retryable"], false);
    }

    #[actix_web::test]
    async fn validation_subcode_response_uses_conflict_code_and_preserves_message() {
        let err =
            validation_error_with_subcode(ApiSubcode::AuthEmailExists, "email already exists");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(
            payload["code"],
            serde_json::json!(ErrorCode::Conflict as i32)
        );
        assert_eq!(payload["msg"], "email already exists");
        assert_eq!(payload["error"]["internal_code"], "E005");
        assert_eq!(payload["error"]["subcode"], "auth.email_exists");
        assert!(payload["error"]["retryable"].is_null());
    }

    #[actix_web::test]
    async fn upload_assembly_response_preserves_subcode_on_server_error() {
        let err = upload_assembly_error_with_subcode(
            ApiSubcode::UploadTempObjectMissing,
            "object missing",
        );
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(
            payload["code"],
            serde_json::json!(ErrorCode::UploadAssemblyFailed as i32)
        );
        assert_eq!(payload["msg"], "Upload Assembly Failed");
        assert_eq!(payload["error"]["subcode"], "upload.temp_object_missing");
    }

    #[actix_web::test]
    async fn thumbnail_generation_response_preserves_subcode_on_server_error() {
        let err = thumbnail_generation_error_with_subcode(
            ApiSubcode::ThumbnailOutputInvalid,
            "decode ffmpeg thumbnail output",
        );
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(
            payload["code"],
            serde_json::json!(ErrorCode::ThumbnailFailed as i32)
        );
        assert_eq!(payload["msg"], "Thumbnail Generation Failed");
        assert_eq!(payload["error"]["subcode"], "thumbnail.output_invalid");
    }
}
