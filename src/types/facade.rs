//! Stable root exports for shared domain types.
//!
//! `crate::types` is the compatibility facade for cross-domain enums and stored
//! wrappers used by entities, repositories, services, API DTOs, and tests. New
//! lower-level code can import from concrete submodules such as
//! `crate::types::storage_policy::DriverType` when that makes the domain source
//! clearer; add new root exports only for types that are intentionally shared
//! across module boundaries.

pub use super::archive::ArchiveFilenameEncoding;
pub use super::audit::{AuditAction, AuditEntityType};
pub use super::auth::{
    ExternalAuthProtocol, ExternalAuthProviderKind, MfaFirstFactor, MfaMethod,
    MfaPersistentFactorMethod, TokenType, VerificationChannel, VerificationPurpose,
};
pub use super::config::{SystemConfigSource, SystemConfigValueType, SystemConfigVisibility};
pub use super::entity::EntityType;
pub use super::file_category::FileCategory;
pub use super::mail::{MailOutboxStatus, MailTemplateCode, StoredMailPayload};
pub use super::media_metadata::{
    AudioMediaMetadata, ImageMediaMetadata, MediaMetadataKind, MediaMetadataPayload,
    MediaMetadataStatus, StoredMediaMetadataPayload, VideoMediaMetadata,
};
pub use super::passkey::StoredPasskeyCredential;
pub use super::patch::{NullablePatch, deserialize_nullable_patch_option};
pub use super::preferences::{
    BrowserOpenMode, ColorPreset, Language, PrefViewMode, StoredUserConfig, ThemeMode, UserConfig,
    UserPreferences,
};
pub use super::storage_policy::{
    DriverType, MediaProcessorKind, RemoteDownloadStrategy, RemoteNodeTransportMode,
    RemoteUploadStrategy, S3_MULTIPART_MIN_PART_SIZE, S3DownloadStrategy, S3UploadStrategy,
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UploadMode,
    UploadSessionStatus, effective_s3_multipart_chunk_size, parse_storage_policy_allowed_types,
    parse_storage_policy_options, serialize_storage_policy_allowed_types,
    serialize_storage_policy_options,
};
pub use super::task::{
    BackgroundTaskKind, BackgroundTaskStatus, StoredLockOwnerInfo, StoredTaskPayload,
    StoredTaskResult, StoredTaskRuntime, StoredTaskSteps,
};
pub use super::team::TeamMemberRole;
pub use super::user::{AvatarSource, UserRole, UserStatus};
