use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::user::account;
use aster_drive_model::types::{LockDepth, LockMode, LockOrigin, LockRootKind};

/// Owner details for a lock created through the WOPI protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WopiLockOwnerInfo {
    /// WOPI app identity that created the lock.
    pub app_key: String,
    /// Opaque lock id supplied by the WOPI app.
    pub lock: String,
}

/// Owner details for a lock created through the WebDAV protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavLockOwnerInfo {
    /// Raw `<owner>` XML from the WebDAV LOCK request, kept verbatim for
    /// the lock discovery response.
    pub xml: String,
}

/// Owner details for a lock created outside the WOPI/WebDAV protocols.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TextLockOwnerInfo {
    /// Free-form owner string from non-protocol lock creators.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Protocol-specific owner details persisted with a lock row.
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceLockOwnerInfo {
    /// Lock created through the WOPI protocol.
    Wopi(WopiLockOwnerInfo),
    /// Lock created through the WebDAV protocol.
    Webdav(WebdavLockOwnerInfo),
    /// Lock created with a plain-text owner label.
    Text(TextLockOwnerInfo),
}

/// Admin view of a persisted resource lock.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResourceLock {
    /// Lock row id.
    pub id: i64,
    /// Secret token presented by lock holders to satisfy the lock.
    pub token: String,
    /// Lock namespace the lock belongs to (one per workspace).
    pub namespace_id: i64,
    /// Kind of resource the lock anchors on.
    pub root_kind: LockRootKind,
    /// Anchoring folder id when `root_kind` is folder.
    pub root_folder_id: Option<i64>,
    /// Anchoring file id when `root_kind` is file.
    pub root_file_id: Option<i64>,
    /// How far the lock reaches below its root.
    pub depth: LockDepth,
    /// Lock mode, e.g. exclusive.
    pub mode: LockMode,
    /// Which surface created the lock, e.g. product UI or WebDAV.
    pub origin: LockOrigin,
    /// Presentation path captured at creation; may be stale after renames.
    pub lockroot_path: Option<String>,
    /// Drive account summary of the lock holder, when the lock was
    /// created by an authenticated product user.
    pub owner: Option<account::UserSummary>,
    /// Protocol-specific owner details, when the lock has any.
    pub owner_info: Option<ResourceLockOwnerInfo>,
    /// Expiry instant; `None` means the lock never expires.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Creation timestamp.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
}
