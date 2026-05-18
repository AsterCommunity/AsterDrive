//! 共享领域类型定义。

mod audit;
mod auth;
mod config;
mod entity;
mod file_category;
mod mail;
mod passkey;
mod patch;
mod preferences;
mod storage_policy;
mod task;
mod team;
mod user;

pub use audit::{AuditAction, AuditEntityType};
pub use auth::{
    ExternalAuthProtocol, ExternalAuthProviderKind, TokenType, VerificationChannel,
    VerificationPurpose,
};
pub use config::{SystemConfigSource, SystemConfigValueType};
pub use entity::EntityType;
pub use file_category::FileCategory;
pub use mail::{MailOutboxStatus, MailTemplateCode, StoredMailPayload};
pub use passkey::StoredPasskeyCredential;
pub use patch::{NullablePatch, deserialize_nullable_patch_option};
pub use preferences::{
    BrowserOpenMode, ColorPreset, Language, PrefViewMode, StoredUserConfig, ThemeMode, UserConfig,
    UserPreferences,
};
pub use storage_policy::{
    DriverType, MediaProcessorKind, RemoteDownloadStrategy, RemoteUploadStrategy,
    S3_MULTIPART_MIN_PART_SIZE, S3DownloadStrategy, S3UploadStrategy, StoragePolicyOptions,
    StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UploadMode, UploadSessionStatus,
    effective_s3_multipart_chunk_size, parse_storage_policy_allowed_types,
    parse_storage_policy_options, serialize_storage_policy_allowed_types,
    serialize_storage_policy_options,
};
pub use task::{
    BackgroundTaskKind, BackgroundTaskStatus, StoredLockOwnerInfo, StoredTaskPayload,
    StoredTaskResult, StoredTaskSteps,
};
pub use team::TeamMemberRole;
pub use user::{AvatarSource, UserRole, UserStatus};
