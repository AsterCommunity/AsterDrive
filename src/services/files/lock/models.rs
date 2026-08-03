use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::user::account;
use aster_drive_model::types::{LockDepth, LockMode, LockOrigin, LockRootKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WopiLockOwnerInfo {
    pub app_key: String,
    pub lock: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavLockOwnerInfo {
    pub xml: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TextLockOwnerInfo {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceLockOwnerInfo {
    Wopi(WopiLockOwnerInfo),
    Webdav(WebdavLockOwnerInfo),
    Text(TextLockOwnerInfo),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResourceLock {
    pub id: i64,
    pub token: String,
    pub namespace_id: i64,
    pub root_kind: LockRootKind,
    pub root_folder_id: Option<i64>,
    pub root_file_id: Option<i64>,
    pub depth: LockDepth,
    pub mode: LockMode,
    pub origin: LockOrigin,
    pub lockroot_path: Option<String>,
    pub owner: Option<account::UserSummary>,
    pub owner_info: Option<ResourceLockOwnerInfo>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
}
