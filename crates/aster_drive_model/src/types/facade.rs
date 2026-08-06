//! Stable root exports for shared domain types.
//!
//! `crate::types` is the compatibility facade for cross-domain enums and stored
//! wrappers used by entities, repositories, services, API DTOs, and tests. New
//! lower-level code can import from concrete submodules when that makes the
//! domain source clearer; add new root exports only for types that are
//! intentionally shared across module boundaries.

pub use super::archive::ArchiveFilenameEncoding;
pub use super::audit::{AuditAction, AuditEntityType};
pub use super::auth::{
    MfaFirstFactor, MfaMethod, MfaPersistentFactorMethod, TokenType, VerificationChannel,
    VerificationPurpose,
};
pub use super::entity::{EntityType, ResourceLockTargetType};
pub use super::media_metadata::{
    AudioMediaMetadata, ImageMediaMetadata, MediaMetadataKind, MediaMetadataPayload,
    MediaMetadataStatus, StoredMediaMetadataPayload, VideoMediaMetadata,
};
pub use super::passkey::StoredPasskeyCredential;
pub use super::preferences::{
    BrowserOpenMode, ColorPreset, LocaleTag, PrefViewMode, StoredUserConfig, ThemeMode, UserConfig,
    UserPreferences,
};
pub use super::remote_storage_target::RemoteStorageTargetDriverKind;
pub use super::resource_lock::{LockDepth, LockMode, LockOrigin, LockRootKind, LockWorkspaceType};
pub use super::sort::SortBy;
pub use super::storage_credential::{
    MicrosoftGraphCloud, StorageAuthorizationFlowStatus, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus,
};
pub use super::storage_policy::{
    MediaProcessorKind, OBJECT_MULTIPART_MIN_PART_SIZE, ObjectStorageDownloadStrategy,
    ObjectStorageUploadStrategy, ProviderDownloadFilenameMode, ProviderDownloadStrategy,
    ProviderResumableUploadStrategy, RemoteDownloadStrategy, RemoteNodeTransportMode,
    RemoteUploadStrategy, StoredStoragePolicyAllowedTypes, StoredStoragePolicyConfig, UploadMode,
    UploadSessionStatus, effective_object_multipart_chunk_size, parse_storage_policy_allowed_types,
    serialize_storage_policy_allowed_types,
};
pub use super::tag::TagScopeType;
pub use super::task::{
    BackgroundTaskKind, BackgroundTaskStatus, StoredLockOwnerInfo, StoredTaskPayload,
    StoredTaskResult, StoredTaskRuntime, StoredTaskSteps,
};
pub use super::team::TeamMemberRole;
pub use super::upload_session::{UploadChunkOrdering, UploadScheduling, UploadSessionKind};
pub use super::user::{AvatarSource, UserRole, UserStatus};
pub use super::user_invitation::UserInvitationStatus;
