//! 统一错误类型与映射。

use actix_web::http::StatusCode;
use std::any::Any;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::response::{ApiErrorDiagnostic, ApiErrorInfo};
use crate::storage::error::{
    StorageErrorContext, StorageErrorKind, storage_driver_error_display_message,
    storage_driver_error_kind_from_message,
};

#[derive(Debug, Clone)]
pub struct AsterErrorPayload {
    message: String,
    api_code: Option<ApiErrorCode>,
    storage_context: Option<StorageErrorContext>,
    database_error_kind: Option<aster_forge_db::DatabaseErrorKind>,
}

impl AsterErrorPayload {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            api_code: None,
            storage_context: None,
            database_error_kind: None,
        }
    }

    fn with_api_code(mut self, api_code: ApiErrorCode) -> Self {
        self.api_code = Some(api_code);
        self
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn api_code(&self) -> Option<ApiErrorCode> {
        self.api_code
    }

    fn with_storage_context(mut self, context: StorageErrorContext) -> Self {
        self.storage_context = Some(context);
        self
    }

    fn storage_context(&self) -> Option<&StorageErrorContext> {
        self.storage_context.as_ref()
    }

    fn with_database_error_kind(mut self, kind: aster_forge_db::DatabaseErrorKind) -> Self {
        self.database_error_kind = Some(kind);
        self
    }

    fn database_error_kind(&self) -> Option<aster_forge_db::DatabaseErrorKind> {
        self.database_error_kind
    }
}

impl PartialEq<&str> for AsterErrorPayload {
    fn eq(&self, other: &&str) -> bool {
        self.message == *other
    }
}

impl PartialEq<str> for AsterErrorPayload {
    fn eq(&self, other: &str) -> bool {
        self.message == other
    }
}

impl std::fmt::Display for AsterErrorPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
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
            $($variant(AsterErrorPayload),)*
        }

        /// 变体总数，用于内部错误映射穷举检查。
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
                    $(AsterError::$variant(payload) => payload.message(),)*
                }
            }

            pub(crate) fn api_error_code_override(&self) -> Option<ApiErrorCode> {
                match self {
                    $(AsterError::$variant(payload) => payload.api_code(),)*
                }
            }

            pub(crate) fn storage_error_context(&self) -> Option<&StorageErrorContext> {
                match self {
                    $(AsterError::$variant(payload) => payload.storage_context(),)*
                }
            }

            pub(crate) fn database_error_kind(&self) -> Option<aster_forge_db::DatabaseErrorKind> {
                match self {
                    $(AsterError::$variant(payload) => payload.database_error_kind(),)*
                }
            }

            pub fn with_api_error_code(self, api_code: ApiErrorCode) -> Self {
                match self {
                    $(AsterError::$variant(payload) => {
                        AsterError::$variant(payload.with_api_code(api_code))
                    })*
                }
            }

            pub(crate) fn with_storage_error_context(self, context: StorageErrorContext) -> Self {
                match self {
                    $(AsterError::$variant(payload) => {
                        AsterError::$variant(payload.with_storage_context(context))
                    })*
                }
            }

            pub(crate) fn with_database_error_kind(
                self,
                kind: aster_forge_db::DatabaseErrorKind,
            ) -> Self {
                match self {
                    $(AsterError::$variant(payload) => {
                        AsterError::$variant(payload.with_database_error_kind(kind))
                    })*
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
                storage_driver_error_display_message(self.raw_message())
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
                        Self::$variant(AsterErrorPayload::new(msg))
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
    AuthTokenMissing(      "E017", "Token Missing"),
    AuthMfaFailed(         "E018", "MFA Failed"),
    AuthRefreshTokenStale( "E019", "Refresh Token Stale"),

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

    // ========== E062: Refresh token 安全事件 ==========
    AuthRefreshTokenReuseDetected("E062", "Refresh Token Reuse Detected"),
}

impl AsterError {
    pub fn storage_error_kind(&self) -> Option<StorageErrorKind> {
        match self {
            Self::StorageDriverError(message) => {
                Some(storage_driver_error_kind_from_message(message.message()))
            }
            Self::PreconditionFailed(_) => Some(StorageErrorKind::Precondition),
            Self::UnsupportedDriver(_) => Some(StorageErrorKind::Unsupported),
            _ => None,
        }
    }

    pub fn api_error_info(&self) -> ApiErrorInfo {
        ApiErrorInfo {
            retryable: self.api_error_retryable(),
            diagnostic: ApiErrorDiagnostic::from_error(self),
        }
    }

    pub fn api_error_code(&self) -> ApiErrorCode {
        if let Some(code) = self.api_error_code_override() {
            return code;
        }

        match self {
            Self::DatabaseConnection(_) | Self::DatabaseOperation(_) => ApiErrorCode::DatabaseError,
            Self::ConfigError(_) => ApiErrorCode::ConfigError,
            Self::InternalError(_) => ApiErrorCode::InternalServerError,
            Self::ValidationError(_) => ApiErrorCode::BadRequest,
            Self::RecordNotFound(_) => ApiErrorCode::NotFound,
            Self::RateLimited(_) => ApiErrorCode::RateLimited,
            Self::MailNotConfigured(_) => ApiErrorCode::MailNotConfigured,
            Self::MailDeliveryFailed(_) => ApiErrorCode::MailDeliveryFailed,
            Self::AuthInvalidCredentials(_) => ApiErrorCode::CredentialsFailed,
            Self::AuthTokenExpired(_) => ApiErrorCode::TokenExpired,
            Self::AuthTokenInvalid(_) => ApiErrorCode::TokenInvalid,
            Self::AuthForbidden(_) => ApiErrorCode::Forbidden,
            Self::AuthPendingActivation(_) => ApiErrorCode::PendingActivation,
            Self::ContactVerificationInvalid(_) => ApiErrorCode::ContactVerificationInvalid,
            Self::ContactVerificationExpired(_) => ApiErrorCode::ContactVerificationExpired,
            Self::AuthTokenMissing(_) => ApiErrorCode::TokenMissing,
            Self::AuthMfaFailed(_) => ApiErrorCode::MfaFailed,
            Self::AuthRefreshTokenStale(_) => ApiErrorCode::RefreshTokenStale,
            Self::AuthRefreshTokenReuseDetected(_) => ApiErrorCode::RefreshTokenReuseDetected,
            Self::FileNotFound(_) => ApiErrorCode::FileNotFound,
            Self::FileTooLarge(_) | Self::PayloadTooLarge(_) => ApiErrorCode::FileTooLarge,
            Self::FileTypeNotAllowed(_) => ApiErrorCode::FileTypeNotAllowed,
            Self::FileUploadFailed(_) => ApiErrorCode::FileUploadFailed,
            Self::StoragePolicyNotFound(_) => ApiErrorCode::StoragePolicyNotFound,
            Self::StorageDriverError(_) => {
                storage_error_kind_api_error_code(self.storage_error_kind().unwrap_or_default())
            }
            Self::StorageQuotaExceeded(_) => ApiErrorCode::StorageQuotaExceeded,
            Self::UnsupportedDriver(_) => ApiErrorCode::UnsupportedDriver,
            Self::FolderNotFound(_) => ApiErrorCode::FolderNotFound,
            Self::ShareNotFound(_) => ApiErrorCode::ShareNotFound,
            Self::ShareExpired(_) => ApiErrorCode::ShareExpired,
            Self::SharePasswordRequired(_) => ApiErrorCode::SharePasswordRequired,
            Self::ShareDownloadLimit(_) => ApiErrorCode::ShareDownloadLimitReached,
            Self::UploadSessionNotFound(_) => ApiErrorCode::UploadSessionNotFound,
            Self::UploadSessionExpired(_) => ApiErrorCode::UploadSessionExpired,
            Self::ChunkUploadFailed(_) => ApiErrorCode::ChunkUploadFailed,
            Self::UploadAssemblyFailed(_) => ApiErrorCode::UploadAssemblyFailed,
            Self::ThumbnailGenerationFailed(_) => ApiErrorCode::ThumbnailFailed,
            Self::ResourceLocked(_) => ApiErrorCode::ResourceLocked,
            Self::PreconditionFailed(_) => ApiErrorCode::PreconditionFailed,
            Self::UploadAssembling(_) => ApiErrorCode::UploadAssembling,
        }
    }

    pub(crate) fn api_error_retryable(&self) -> bool {
        match self {
            Self::RateLimited(_) | Self::UploadAssembling(_) => true,
            Self::StorageDriverError(_) => matches!(
                self.storage_error_kind(),
                Some(StorageErrorKind::Transient | StorageErrorKind::RateLimited)
            ),
            _ => false,
        }
    }

    /// HTTP 状态码映射
    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::ValidationError(_)
            | Self::FileTooLarge(_)
            | Self::FileTypeNotAllowed(_)
            | Self::UnsupportedDriver(_) => StatusCode::BAD_REQUEST,

            Self::FileUploadFailed(_) if file_upload_failure_is_client_error(self) => {
                StatusCode::BAD_REQUEST
            }

            Self::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,

            Self::ChunkUploadFailed(_) if chunk_upload_failure_is_client_error(self) => {
                StatusCode::BAD_REQUEST
            }

            Self::ContactVerificationInvalid(_) => StatusCode::BAD_REQUEST,

            Self::AuthInvalidCredentials(_)
            | Self::AuthTokenExpired(_)
            | Self::AuthTokenInvalid(_)
            | Self::AuthRefreshTokenStale(_)
            | Self::AuthRefreshTokenReuseDetected(_)
            | Self::AuthTokenMissing(_)
            | Self::AuthMfaFailed(_) => StatusCode::UNAUTHORIZED,

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
        let database_error_kind = aster_forge_db::database_error_kind(&e);
        let error = match e {
            sea_orm::DbErr::RecordNotFound(msg) => Self::record_not_found(msg),
            other => Self::database_operation(other.to_string()),
        };
        match database_error_kind {
            Some(kind) => error.with_database_error_kind(kind),
            None => error,
        }
    }
}

impl From<aster_forge_db::DbError> for AsterError {
    fn from(value: aster_forge_db::DbError) -> Self {
        match value {
            aster_forge_db::DbError::DatabaseConnection(message) => {
                Self::database_connection(message)
            }
            aster_forge_db::DbError::DatabaseOperation(message) => {
                Self::database_operation(message)
            }
            aster_forge_db::DbError::RetryExhausted => {
                Self::database_operation("database retry exhausted")
            }
            aster_forge_db::DbError::NonRetryable(message) => Self::database_operation(message),
        }
    }
}

impl From<aster_forge_mail::MailConfigError> for AsterError {
    fn from(value: aster_forge_mail::MailConfigError) -> Self {
        Self::validation_error(value.to_string())
    }
}

impl From<aster_forge_mail::MailDeliveryError> for AsterError {
    fn from(value: aster_forge_mail::MailDeliveryError) -> Self {
        match value {
            aster_forge_mail::MailDeliveryError::NotConfigured(message) => {
                Self::mail_not_configured(message)
            }
            aster_forge_mail::MailDeliveryError::InvalidMessage(message) => {
                Self::validation_error(message)
            }
            aster_forge_mail::MailDeliveryError::Config(message) => Self::config_error(message),
            aster_forge_mail::MailDeliveryError::Delivery(message) => {
                Self::mail_delivery_failed(message)
            }
            aster_forge_mail::MailDeliveryError::Internal(message) => Self::internal_error(message),
        }
    }
}

impl From<aster_forge_api::ApiError> for AsterError {
    fn from(value: aster_forge_api::ApiError) -> Self {
        Self::validation_error(value.to_string())
    }
}

impl From<aster_forge_config::ConfigCoreError> for AsterError {
    fn from(value: aster_forge_config::ConfigCoreError) -> Self {
        match value {
            aster_forge_config::ConfigCoreError::InvalidValue(message) => {
                Self::validation_error(message)
            }
            aster_forge_config::ConfigCoreError::UnknownKey(key) => {
                Self::record_not_found(format!("config key '{key}'"))
            }
            aster_forge_config::ConfigCoreError::Json(error) => {
                Self::validation_error(error.to_string())
            }
            aster_forge_config::ConfigCoreError::Store(message)
            | aster_forge_config::ConfigCoreError::Notification(message) => {
                Self::internal_error(message)
            }
        }
    }
}

impl From<aster_forge_crypto::CryptoError> for AsterError {
    fn from(value: aster_forge_crypto::CryptoError) -> Self {
        Self::internal_error(value.to_string())
    }
}

impl From<aster_forge_tasks::TaskCoreError> for AsterError {
    fn from(value: aster_forge_tasks::TaskCoreError) -> Self {
        let message = value.to_string();
        match value {
            aster_forge_tasks::TaskCoreError::LeaseLost { .. } => {
                precondition_failed_with_code(ApiErrorCode::TaskLeaseLost, message)
            }
            aster_forge_tasks::TaskCoreError::LeaseRenewalTimedOut { .. } => {
                precondition_failed_with_code(ApiErrorCode::TaskLeaseRenewalTimedOut, message)
            }
            aster_forge_tasks::TaskCoreError::WorkerShutdownRequested { .. } => {
                precondition_failed_with_code(ApiErrorCode::TaskWorkerShutdownRequested, message)
            }
            aster_forge_tasks::TaskCoreError::Codec(_)
            | aster_forge_tasks::TaskCoreError::InvalidValue(_)
            | aster_forge_tasks::TaskCoreError::Io(_) => Self::internal_error(message),
        }
    }
}

impl From<aster_forge_utils::UtilsError> for AsterError {
    fn from(value: aster_forge_utils::UtilsError) -> Self {
        Self::internal_error(value.to_string())
    }
}

impl From<aster_forge_actix_middleware::csrf::CsrfError> for AsterError {
    fn from(error: aster_forge_actix_middleware::csrf::CsrfError) -> Self {
        csrf_error(error.kind(), error.message())
    }
}

fn csrf_error(
    kind: aster_forge_actix_middleware::csrf::CsrfErrorKind,
    message: impl Into<String>,
) -> AsterError {
    use aster_forge_actix_middleware::csrf::CsrfErrorKind;

    let message = message.into();
    match kind {
        CsrfErrorKind::TokenNameInvalid => AsterError::config_error(message),
        CsrfErrorKind::CookieMissing => {
            auth_forbidden_with_code(ApiErrorCode::AuthCsrfCookieMissing, message)
        }
        CsrfErrorKind::HeaderMissing => {
            auth_forbidden_with_code(ApiErrorCode::AuthCsrfHeaderMissing, message)
        }
        CsrfErrorKind::TokenInvalid => {
            auth_forbidden_with_code(ApiErrorCode::AuthCsrfTokenInvalid, message)
        }
        CsrfErrorKind::RequestSourceUntrusted => {
            auth_forbidden_with_code(ApiErrorCode::AuthRequestSourceUntrusted, message)
        }
        CsrfErrorKind::RequestOriginUntrusted => {
            auth_forbidden_with_code(ApiErrorCode::AuthRequestOriginUntrusted, message)
        }
        CsrfErrorKind::RequestRefererUntrusted => {
            auth_forbidden_with_code(ApiErrorCode::AuthRequestRefererUntrusted, message)
        }
        CsrfErrorKind::RequestSourceMissing => {
            auth_forbidden_with_code(ApiErrorCode::AuthRequestSourceMissing, message)
        }
        CsrfErrorKind::RequestSchemeInvalid => {
            validation_error_with_code(ApiErrorCode::ValidationRequestSchemeInvalid, message)
        }
        CsrfErrorKind::RequestHostInvalid => {
            validation_error_with_code(ApiErrorCode::ValidationRequestHostInvalid, message)
        }
        CsrfErrorKind::RequestOriginInvalid => {
            validation_error_with_code(ApiErrorCode::ValidationRequestOriginInvalid, message)
        }
        CsrfErrorKind::RequestRefererInvalid => {
            validation_error_with_code(ApiErrorCode::ValidationRequestRefererInvalid, message)
        }
        CsrfErrorKind::RequestHeaderValueInvalid => {
            validation_error_with_code(ApiErrorCode::ValidationRequestHeaderValueInvalid, message)
        }
    }
}

impl From<aster_forge_file_classification::FileClassificationError> for AsterError {
    fn from(error: aster_forge_file_classification::FileClassificationError) -> Self {
        Self::validation_error(error.message())
    }
}

impl From<aster_forge_external_auth::ExternalAuthError> for AsterError {
    fn from(error: aster_forge_external_auth::ExternalAuthError) -> Self {
        match error {
            aster_forge_external_auth::ExternalAuthError::Validation(message) => {
                Self::validation_error(message)
            }
            aster_forge_external_auth::ExternalAuthError::Config(message) => {
                Self::config_error(message)
            }
            aster_forge_external_auth::ExternalAuthError::InvalidCredentials(message) => {
                Self::auth_invalid_credentials(message)
            }
            aster_forge_external_auth::ExternalAuthError::State(message) => {
                Self::database_operation(message)
            }
            aster_forge_external_auth::ExternalAuthError::Internal(message) => {
                Self::internal_error(message)
            }
        }
    }
}

impl From<aster_forge_validation::ValidationError> for AsterError {
    fn from(error: aster_forge_validation::ValidationError) -> Self {
        Self::validation_error(error.message())
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

    fn client_message(&self) -> String {
        if matches!(self, Self::StorageDriverError(_)) {
            return self.error_type().to_string();
        }
        match self.response_log_level() {
            ResponseLogLevel::Error => self.error_type().to_string(),
            ResponseLogLevel::Warn | ResponseLogLevel::Skip => self.message().to_string(),
        }
    }
}

impl ApiErrorDiagnostic {
    pub fn from_error(error: &AsterError) -> Option<Self> {
        let kind = error.storage_error_kind()?;
        Some(Self {
            kind: kind.as_str().to_string(),
            message: sanitize_storage_driver_client_message(storage_driver_error_display_message(
                error.message(),
            )),
        })
    }
}

pub(crate) fn sanitize_storage_driver_client_message(message: &str) -> String {
    let mut sanitized = message
        .split_whitespace()
        .map(sanitize_url_token)
        .collect::<Vec<_>>()
        .join(" ");

    for marker in [
        "AccountKey=",
        "SharedAccessSignature=",
        "sig=",
        "signature=",
        "X-Amz-Signature=",
        "AWSAccessKeyId=",
        "access_key=",
        "secret_key=",
        "SecretAccessKey=",
    ] {
        sanitized = redact_key_value_after_marker(&sanitized, marker);
    }

    sanitized
}

fn sanitize_url_token(token: &str) -> String {
    let leading_len = token
        .chars()
        .take_while(|ch| matches!(ch, '(' | '[' | '\'' | '"' | '<'))
        .map(char::len_utf8)
        .sum::<usize>();
    let trailing_len = token
        .chars()
        .rev()
        .take_while(|ch| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '\'' | '"' | '>'))
        .map(char::len_utf8)
        .sum::<usize>();
    let split_at = token.len().saturating_sub(trailing_len);
    let (without_trailing, trailing) = token.split_at(split_at);
    let (leading, candidate) = without_trailing.split_at(leading_len.min(without_trailing.len()));

    let Ok(mut url) = url::Url::parse(candidate) else {
        return token.to_string();
    };

    if !url.username().is_empty() || url.password().is_some() {
        let _ = url.set_username("");
        let _ = url.set_password(None);
    }

    if url.query().is_some() {
        let redacted_pairs = url
            .query_pairs()
            .map(|(key, value)| {
                let value = if is_sensitive_storage_query_key(&key) {
                    "[redacted]".into()
                } else {
                    value
                };
                (key.into_owned(), value.into_owned())
            })
            .collect::<Vec<_>>();
        url.set_query(None);
        {
            let mut query = url.query_pairs_mut();
            for (key, value) in redacted_pairs {
                query.append_pair(&key, &value);
            }
        }
    }

    format!("{leading}{url}{trailing}")
}

fn is_sensitive_storage_query_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "sig"
            | "signature"
            | "x-amz-signature"
            | "awsaccesskeyid"
            | "access_key"
            | "secret_key"
            | "secretaccesskey"
            | "sharedaccesssignature"
            | "accountkey"
    )
}

fn redact_key_value_after_marker(input: &str, marker: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(index) = rest.find(marker) {
        let (before, after_before) = rest.split_at(index);
        output.push_str(before);
        output.push_str(marker);
        output.push_str("[redacted]");

        let value_start = marker.len();
        let value = &after_before[value_start..];
        let value_end = value
            .find(|ch: char| {
                matches!(
                    ch,
                    ';' | '&' | ' ' | '\n' | '\r' | '\t' | '\'' | '"' | ')' | ']' | '>'
                )
            })
            .unwrap_or(value.len());
        let (redacted_value, after_value) = value.split_at(value_end);
        let _ = redacted_value;
        rest = after_value;
    }

    output.push_str(rest);
    output
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

        let error_code = self.api_error_code();
        let mut response = actix_web::HttpResponse::build(status);
        if self.share_error_should_disable_cache() {
            response.insert_header(("Cache-Control", "no-store, max-age=0"));
        }
        response.json(ApiResponse::<()>::error_with_details(
            error_code,
            &self.client_message(),
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
    error.to_string()
}

pub(crate) fn task_error_display_message(raw_message: &str) -> &str {
    raw_message
}

pub fn validation_error_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::validation_error(message).with_api_error_code(api_code)
}

pub fn auth_forbidden_with_code(api_code: ApiErrorCode, message: impl Into<String>) -> AsterError {
    AsterError::auth_forbidden(message).with_api_error_code(api_code)
}

pub fn auth_invalid_credentials_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::auth_invalid_credentials(message).with_api_error_code(api_code)
}

pub fn auth_mfa_failed_with_code(api_code: ApiErrorCode, message: impl Into<String>) -> AsterError {
    AsterError::auth_mfa_failed(message).with_api_error_code(api_code)
}

pub fn precondition_failed_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::precondition_failed(message).with_api_error_code(api_code)
}

pub fn file_upload_error_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::file_upload_failed(message).with_api_error_code(api_code)
}

pub fn payload_too_large_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::payload_too_large(message).with_api_error_code(api_code)
}

pub fn chunk_upload_error_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::chunk_upload_failed(message).with_api_error_code(api_code)
}

pub fn upload_assembly_error_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::upload_assembly_failed(message).with_api_error_code(api_code)
}

pub fn thumbnail_generation_error_with_code(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    AsterError::thumbnail_generation_failed(message).with_api_error_code(api_code)
}

fn storage_error_kind_api_error_code(kind: StorageErrorKind) -> ApiErrorCode {
    match kind {
        StorageErrorKind::Auth => ApiErrorCode::StorageAuth,
        StorageErrorKind::Misconfigured => ApiErrorCode::StorageMisconfigured,
        StorageErrorKind::NotFound => ApiErrorCode::StorageNotFound,
        StorageErrorKind::Permission => ApiErrorCode::StoragePermission,
        StorageErrorKind::Precondition => ApiErrorCode::StoragePrecondition,
        StorageErrorKind::RateLimited => ApiErrorCode::StorageRateLimited,
        StorageErrorKind::Transient => ApiErrorCode::StorageTransient,
        StorageErrorKind::Unsupported => ApiErrorCode::StorageUnsupported,
        StorageErrorKind::Unknown => ApiErrorCode::StorageUnknown,
    }
}

fn chunk_upload_failure_is_client_error(error: &AsterError) -> bool {
    matches!(
        error.api_error_code_override(),
        Some(
            ApiErrorCode::UploadChunkNumberOutOfRange
                | ApiErrorCode::UploadChunkSizeMismatch
                | ApiErrorCode::UploadChunkTooLarge
                | ApiErrorCode::UploadChunkSizeOverflow
                | ApiErrorCode::UploadChunkTransportMismatch
                | ApiErrorCode::UploadChunkSessionInvalid
                | ApiErrorCode::UploadRequestBodyReadFailed
        )
    )
}

fn file_upload_failure_is_client_error(error: &AsterError) -> bool {
    matches!(
        error.api_error_code_override(),
        Some(ApiErrorCode::UploadFieldReadFailed | ApiErrorCode::AvatarUploadReadFailed)
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
        AsterError, MapAsterErr, ResponseLogLevel, auth_forbidden_with_code,
        auth_invalid_credentials_with_code, auth_mfa_failed_with_code,
        chunk_upload_error_with_code, file_upload_error_with_code, payload_too_large_with_code,
        precondition_failed_with_code, sanitize_storage_driver_client_message,
        thumbnail_generation_error_with_code, upload_assembly_error_with_code,
        validation_error_with_code,
    };
    use crate::api::api_error_code::ApiErrorCode;
    use crate::api::response::ApiErrorInfo;
    use crate::storage::error::{StorageErrorKind, storage_driver_error};
    use actix_web::body;
    use actix_web::http::StatusCode;
    use aster_forge_actix_middleware::csrf::CsrfErrorKind;

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
    fn forge_external_auth_errors_preserve_product_error_categories() {
        let cases = [
            (
                aster_forge_external_auth::ExternalAuthError::validation_error("validation"),
                "E005",
            ),
            (
                aster_forge_external_auth::ExternalAuthError::config_error("config"),
                "E003",
            ),
            (
                aster_forge_external_auth::ExternalAuthError::auth_invalid_credentials(
                    "credentials",
                ),
                "E010",
            ),
            (
                aster_forge_external_auth::ExternalAuthError::state_error("state"),
                "E002",
            ),
            (
                aster_forge_external_auth::ExternalAuthError::internal_error("internal"),
                "E004",
            ),
        ];

        for (forge_error, expected_code) in cases {
            let error = AsterError::from(forge_error);
            assert_eq!(error.code(), expected_code);
        }
    }

    #[test]
    fn forge_csrf_errors_preserve_every_product_error_code() {
        let cases = [
            (CsrfErrorKind::TokenNameInvalid, ApiErrorCode::ConfigError),
            (
                CsrfErrorKind::CookieMissing,
                ApiErrorCode::AuthCsrfCookieMissing,
            ),
            (
                CsrfErrorKind::HeaderMissing,
                ApiErrorCode::AuthCsrfHeaderMissing,
            ),
            (
                CsrfErrorKind::TokenInvalid,
                ApiErrorCode::AuthCsrfTokenInvalid,
            ),
            (
                CsrfErrorKind::RequestSourceUntrusted,
                ApiErrorCode::AuthRequestSourceUntrusted,
            ),
            (
                CsrfErrorKind::RequestOriginUntrusted,
                ApiErrorCode::AuthRequestOriginUntrusted,
            ),
            (
                CsrfErrorKind::RequestRefererUntrusted,
                ApiErrorCode::AuthRequestRefererUntrusted,
            ),
            (
                CsrfErrorKind::RequestSourceMissing,
                ApiErrorCode::AuthRequestSourceMissing,
            ),
            (
                CsrfErrorKind::RequestSchemeInvalid,
                ApiErrorCode::ValidationRequestSchemeInvalid,
            ),
            (
                CsrfErrorKind::RequestHostInvalid,
                ApiErrorCode::ValidationRequestHostInvalid,
            ),
            (
                CsrfErrorKind::RequestOriginInvalid,
                ApiErrorCode::ValidationRequestOriginInvalid,
            ),
            (
                CsrfErrorKind::RequestRefererInvalid,
                ApiErrorCode::ValidationRequestRefererInvalid,
            ),
            (
                CsrfErrorKind::RequestHeaderValueInvalid,
                ApiErrorCode::ValidationRequestHeaderValueInvalid,
            ),
        ];

        for (kind, expected_code) in cases {
            let error = super::csrf_error(kind, "csrf boundary error");
            assert_eq!(error.api_error_code(), expected_code, "{kind:?}");
        }
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
    fn multipart_read_failures_map_to_bad_request() {
        let err = file_upload_error_with_code(ApiErrorCode::UploadFieldReadFailed, "overflow");
        assert_eq!(err.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.response_log_level(), ResponseLogLevel::Warn);

        let err = file_upload_error_with_code(ApiErrorCode::AvatarUploadReadFailed, "overflow");
        assert_eq!(err.http_status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.response_log_level(), ResponseLogLevel::Warn);
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
    fn api_error_info_serializes_retryable_only() {
        let info = ApiErrorInfo {
            retryable: true,
            diagnostic: None,
        };

        let payload = serde_json::to_value(&info).expect("ApiErrorInfo should serialize");

        assert_eq!(payload["retryable"], true);
        assert!(payload.get("code").is_none());
        assert!(payload.get("internal_code").is_none());
        assert!(payload.get("subcode").is_none());
    }

    #[test]
    fn api_error_info_ignores_removed_compatibility_fields_on_deserialize() {
        let payload = serde_json::json!({
            "retryable": false,
            "code": "remote.dynamic",
            "internal_code": "E005",
            "subcode": "legacy.subcode"
        });

        let info =
            serde_json::from_value::<ApiErrorInfo>(payload).expect("ApiErrorInfo should parse");

        assert!(!info.retryable);
        assert!(info.diagnostic.is_none());
    }

    #[test]
    fn legacy_encoded_messages_are_not_decoded() {
        let raw = "__ASTER_API_SUBCODE__=remote.dynamic::remote message";
        let err = AsterError::validation_error(raw);

        assert_eq!(err.api_error_code_override(), None);
        assert_eq!(err.api_error_code(), ApiErrorCode::BadRequest);
        assert_eq!(err.message(), raw);
    }

    #[test]
    fn api_error_code_uses_default_mapping_without_override() {
        let err = AsterError::auth_token_expired("expired");

        assert_eq!(err.api_error_code_override(), None);
        assert_eq!(err.api_error_code(), ApiErrorCode::TokenExpired);
    }

    #[test]
    fn api_error_code_prefers_structured_override_over_default_mapping() {
        let err = validation_error_with_code(
            ApiErrorCode::UploadChunkSizeMismatch,
            "chunk size mismatch",
        );

        assert_eq!(err.api_error_code(), ApiErrorCode::UploadChunkSizeMismatch);
    }

    #[test]
    fn api_error_code_helpers_attach_structured_codes() {
        let message = "structured code marker";
        let cases = [
            (
                "validation",
                validation_error_with_code(ApiErrorCode::AuthEmailExists, message),
                ApiErrorCode::AuthEmailExists,
                "E005",
            ),
            (
                "auth_forbidden",
                auth_forbidden_with_code(ApiErrorCode::AuthAdminRequired, message),
                ApiErrorCode::AuthAdminRequired,
                "E013",
            ),
            (
                "auth_invalid_credentials",
                auth_invalid_credentials_with_code(ApiErrorCode::CredentialsFailed, message),
                ApiErrorCode::CredentialsFailed,
                "E010",
            ),
            (
                "auth_mfa_failed",
                auth_mfa_failed_with_code(ApiErrorCode::AuthMfaCodeInvalid, message),
                ApiErrorCode::AuthMfaCodeInvalid,
                "E018",
            ),
            (
                "precondition_failed",
                precondition_failed_with_code(ApiErrorCode::FileEtagMismatch, message),
                ApiErrorCode::FileEtagMismatch,
                "E060",
            ),
            (
                "file_upload",
                file_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message),
                ApiErrorCode::UploadTempFileWriteFailed,
                "E023",
            ),
            (
                "payload_too_large",
                payload_too_large_with_code(ApiErrorCode::UploadBodySizeOverflow, message),
                ApiErrorCode::UploadBodySizeOverflow,
                "E024",
            ),
            (
                "chunk_upload",
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkTooLarge, message),
                ApiErrorCode::UploadChunkTooLarge,
                "E056",
            ),
            (
                "upload_assembly",
                upload_assembly_error_with_code(ApiErrorCode::UploadTempObjectMissing, message),
                ApiErrorCode::UploadTempObjectMissing,
                "E057",
            ),
            (
                "thumbnail_generation",
                thumbnail_generation_error_with_code(ApiErrorCode::ThumbnailOutputInvalid, message),
                ApiErrorCode::ThumbnailOutputInvalid,
                "E058",
            ),
        ];

        for (label, error, expected_code, expected_internal_code) in cases {
            assert_eq!(
                error.api_error_code_override(),
                Some(expected_code),
                "{label} did not store the explicit ApiErrorCode"
            );
            assert_eq!(error.api_error_code(), expected_code, "{label}");
            assert_eq!(error.code(), expected_internal_code, "{label}");
            assert_eq!(error.message(), message, "{label}");
        }
    }

    #[actix_web::test]
    async fn auth_mfa_failed_response_uses_structured_api_error_code() {
        let api_code = ApiErrorCode::AuthMfaCodeInvalid;
        let err = auth_mfa_failed_with_code(api_code, "invalid MFA code");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], api_code.as_str());
        assert_eq!(payload["msg"], "invalid MFA code");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
    }

    #[actix_web::test]
    async fn auth_token_missing_response_uses_token_missing_code() {
        let err = AsterError::auth_token_missing("missing token");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "auth.token_missing");
        assert_eq!(payload["msg"], "missing token");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
    }

    #[actix_web::test]
    async fn stale_refresh_response_uses_refresh_token_stale_code() {
        let err = AsterError::auth_refresh_token_stale("stale refresh token");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "auth.refresh_token_stale");
        assert_eq!(payload["msg"], "stale refresh token");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
    }

    #[actix_web::test]
    async fn refresh_reuse_response_uses_reuse_detected_code() {
        let err = AsterError::auth_refresh_token_reuse_detected("refresh token reuse detected");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "auth.refresh_token_reuse_detected");
        assert_eq!(payload["msg"], "refresh token reuse detected");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
    }

    #[actix_web::test]
    async fn storage_transient_error_response_redacts_details_but_keeps_specific_code() {
        let err = storage_driver_error(StorageErrorKind::Transient, "remote timeout");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "storage.transient");
        assert_eq!(payload["msg"], "Storage Driver Error");
        assert_eq!(payload["error"]["retryable"], true);
        assert_eq!(payload["error"]["diagnostic"]["kind"], "transient");
        assert_eq!(payload["error"]["diagnostic"]["message"], "remote timeout");
        assert!(payload["error"]["diagnostic"].get("api_code").is_none());
        assert!(payload["error"]["diagnostic"].get("retryable").is_none());
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
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

        assert_eq!(payload["code"], "storage.permission");
        assert_eq!(payload["msg"], "Storage Driver Error");
        assert_eq!(payload["error"]["retryable"], false);
        assert_eq!(payload["error"]["diagnostic"]["kind"], "permission");
        assert_eq!(payload["error"]["diagnostic"]["message"], "access denied");
        assert!(payload["error"]["diagnostic"].get("api_code").is_none());
        assert!(payload["error"]["diagnostic"].get("retryable").is_none());
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
    }

    #[actix_web::test]
    async fn storage_driver_error_response_redacts_details() {
        let err = storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Azure Blob failed for https://acct.blob.core.windows.net/container/file.txt?sig=topsecret&sp=rw AccountKey=supersecret;EndpointSuffix=core.windows.net",
        );
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["msg"], "Storage Driver Error");
        assert_eq!(
            err.message(),
            "Azure Blob failed for https://acct.blob.core.windows.net/container/file.txt?sig=topsecret&sp=rw AccountKey=supersecret;EndpointSuffix=core.windows.net"
        );
    }

    #[test]
    fn storage_driver_diagnostic_message_sanitizer_redacts_secrets() {
        let quoted = sanitize_storage_driver_client_message(
            "Azure URL 'https://acct.blob.core.windows.net/file?sig=quotedsecret'.",
        );
        assert!(quoted.contains("'https://acct.blob.core.windows.net/file?sig="));
        assert!(quoted.contains("redacted"));
        assert!(quoted.ends_with("'."), "{quoted}");
        assert!(!quoted.contains("quotedsecret"));
    }

    #[test]
    fn storage_driver_client_message_sanitizes_url_and_marker_boundaries() {
        let cases = [
            (
                "quoted URL",
                "failed: 'https://acct.blob.core.windows.net/file?sig=secret&sp=rw'.",
                "failed: 'https://acct.blob.core.windows.net/file?sig=[redacted]&sp=rw'.",
                vec!["secret"],
            ),
            (
                "url userinfo",
                "failed: https://user:password@example.com/container/blob?x=1",
                "failed: https://example.com/container/blob?x=1",
                vec!["user:password", "@"],
            ),
            (
                "sensitive query aliases",
                "failed: https://example.com/blob?X-Amz-Signature=awssecret&AWSAccessKeyId=ak&signature=sig&keep=value",
                "failed: https://example.com/blob?X-Amz-Signature=[redacted]&AWSAccessKeyId=[redacted]&signature=[redacted]&keep=value",
                vec!["awssecret", "ak&", "sig&"],
            ),
            (
                "azure marker values preserve following segments",
                "AccountKey=key;EndpointSuffix=core.windows.net SharedAccessSignature=sas) done",
                "AccountKey=[redacted];EndpointSuffix=core.windows.net SharedAccessSignature=[redacted]) done",
                vec!["key;Endpoint", "sas)"],
            ),
            (
                "non-url text untouched",
                "Tencent COS PUT Bucket cors failed with HTTP 400 Bad Request code=InvalidRequest request_id=req-1",
                "Tencent COS PUT Bucket cors failed with HTTP 400 Bad Request code=InvalidRequest request_id=req-1",
                vec![],
            ),
        ];

        for (name, input, expected, forbidden) in cases {
            let sanitized = sanitize_storage_driver_client_message(input);
            assert_eq!(sanitized, expected, "{name}");
            for value in forbidden {
                assert!(
                    !sanitized.contains(value),
                    "{name} should redact {value}: {sanitized}"
                );
            }
        }
    }

    #[actix_web::test]
    async fn validation_response_uses_structured_code_and_preserves_message() {
        let err = validation_error_with_code(ApiErrorCode::AuthEmailExists, "email already exists");
        let response = actix_web::ResponseError::error_response(&err);

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "auth.email_exists");
        assert_eq!(payload["msg"], "email already exists");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
    }

    #[actix_web::test]
    async fn upload_assembly_response_uses_structured_code_on_server_error() {
        let err = upload_assembly_error_with_code(
            ApiErrorCode::UploadTempObjectMissing,
            "object missing",
        );
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "upload.temp_object_missing");
        assert_eq!(payload["msg"], "Upload Assembly Failed");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
    }

    #[actix_web::test]
    async fn thumbnail_generation_response_uses_structured_code_on_server_error() {
        let err = thumbnail_generation_error_with_code(
            ApiErrorCode::ThumbnailOutputInvalid,
            "decode ffmpeg thumbnail output",
        );
        let response = actix_web::ResponseError::error_response(&err);

        let body = body::to_bytes(response.into_body())
            .await
            .expect("response body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response body should be valid json");

        assert_eq!(payload["code"], "thumbnail.output_invalid");
        assert_eq!(payload["msg"], "Thumbnail Generation Failed");
        assert_eq!(payload["error"]["retryable"], false);
        assert!(payload["error"].get("code").is_none());
        assert!(payload["error"].get("internal_code").is_none());
        assert!(payload["error"].get("subcode").is_none());
        assert!(payload["error"].get("api_code").is_none());
    }
}
