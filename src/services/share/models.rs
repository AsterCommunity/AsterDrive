//! 分享服务子模块：`models`。

use chrono::Utc;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

use crate::errors::{AsterError, Result};
use crate::services::{user::account, user::profile};
use aster_drive_model::entities::share;
use aster_drive_model::types::EntityType;

/// Lifecycle state of a share as shown to its owner, derived from expiry,
/// download limit, and whether the target resource still exists.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ShareStatus {
    /// Share is usable: not expired and its download limit is not exhausted.
    Active,
    /// Share passed its configured expiry time.
    Expired,
    /// Share reached its configured maximum download count.
    Exhausted,
    /// Share's target resource (file or folder) has been deleted.
    Deleted,
}

/// Target resource a share points at: a single file or a single folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ShareTarget {
    /// Kind of resource being shared.
    #[serde(rename = "type")]
    pub r#type: EntityType,
    /// Database ID of the shared file or folder; validated to be positive.
    #[validate(range(min = 1, message = "share target id must be greater than 0"))]
    pub id: i64,
}

impl ShareTarget {
    /// Creates a share target pointing at a file.
    pub const fn file(id: i64) -> Self {
        Self {
            r#type: EntityType::File,
            id,
        }
    }

    /// Creates a share target pointing at a folder.
    pub const fn folder(id: i64) -> Self {
        Self {
            r#type: EntityType::Folder,
            id,
        }
    }

    pub(super) const fn into_ids(self) -> (Option<i64>, Option<i64>) {
        match self.r#type {
            EntityType::File => (Some(self.id), None),
            EntityType::Folder => (None, Some(self.id)),
        }
    }
}

/// Full share record as returned to authenticated management endpoints.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ShareInfo {
    /// Share database ID.
    pub id: i64,
    /// Public share token used in anonymous share URLs; keep it out of logs.
    pub token: String,
    /// Summary of the owning user; `None` when the owner no longer resolves.
    pub user: Option<account::UserSummary>,
    /// Owning team workspace ID; `None` for personal shares.
    pub team_id: Option<i64>,
    /// File or folder the share points at.
    pub target: ShareTarget,
    /// Current display name of the target; `None` once the target is deleted.
    pub target_name: Option<String>,
    /// Expiry instant; `None` means the share never expires.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    /// Maximum allowed downloads; `0` means unlimited.
    pub max_downloads: i64,
    /// Number of completed downloads so far.
    pub download_count: i64,
    /// Number of times the share page has been viewed.
    pub view_count: i64,
    /// Creation time, serialized as RFC 3339.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    /// Last modification time, serialized as RFC 3339.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

pub(super) fn share_target_from_columns(
    file_id: Option<i64>,
    folder_id: Option<i64>,
) -> Option<ShareTarget> {
    match (file_id, folder_id) {
        (Some(file_id), None) => Some(ShareTarget::file(file_id)),
        (None, Some(folder_id)) => Some(ShareTarget::folder(folder_id)),
        _ => None,
    }
}

pub(crate) fn share_target_for_share(share: &share::Model) -> Result<ShareTarget> {
    share_target_from_columns(share.file_id, share.folder_id).ok_or_else(|| {
        AsterError::internal_error(format!(
            "share #{} has invalid target columns: file_id={:?}, folder_id={:?}",
            share.id, share.file_id, share.folder_id
        ))
    })
}

pub(super) fn share_info_from_model(
    model: share::Model,
    user: Option<account::UserSummary>,
    target_name: Option<String>,
) -> Result<ShareInfo> {
    let target = share_target_from_columns(model.file_id, model.folder_id).ok_or_else(|| {
        AsterError::internal_error(format!(
            "share #{} has invalid target columns: file_id={:?}, folder_id={:?}",
            model.id, model.file_id, model.folder_id
        ))
    })?;

    Ok(ShareInfo {
        id: model.id,
        token: model.token,
        user,
        team_id: model.team_id,
        target,
        target_name,
        expires_at: model.expires_at,
        max_downloads: model.max_downloads,
        download_count: model.download_count,
        view_count: model.view_count,
        created_at: model.created_at,
        updated_at: model.updated_at,
    })
}

pub(crate) struct ShareUpdateOutcome {
    pub share: ShareInfo,
    pub has_password: bool,
}

/// Share summary shown to its owner in "my shares" listings.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MyShareInfo {
    /// Share database ID.
    pub id: i64,
    /// Public share token used to build the share link; keep it out of logs.
    pub token: String,
    /// Database ID of the shared file or folder.
    pub resource_id: i64,
    /// Display name of the shared resource; a placeholder when it is gone.
    pub resource_name: String,
    /// Whether the share targets a file or a folder.
    pub resource_type: EntityType,
    /// Resolved icon for a shared folder; `None` for file shares or when the
    /// folder no longer resolves.
    pub folder_icon: Option<crate::services::files::folder::FolderIcon>,
    /// Whether the shared resource has been moved to trash.
    pub resource_deleted: bool,
    /// Whether anonymous access requires a password.
    pub has_password: bool,
    /// Lifecycle state derived from expiry, download limit, and target state.
    pub status: ShareStatus,
    /// Expiry instant; `None` means the share never expires.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    /// Maximum allowed downloads; `0` means unlimited.
    pub max_downloads: i64,
    /// Number of completed downloads so far.
    pub download_count: i64,
    /// Number of times the share page has been viewed.
    pub view_count: i64,
    /// Downloads left before the limit is reached; `None` when unlimited.
    pub remaining_downloads: Option<i64>,
    /// Creation time, serialized as RFC 3339.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    /// Last modification time, serialized as RFC 3339.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

/// Owner identity shown on the public share page.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SharePublicOwnerInfo {
    /// Owner display name shown to anonymous visitors; falls back to the
    /// username when no display name is set.
    pub name: String,
    /// Owner avatar reference resolved for the public share page.
    pub avatar: profile::AvatarInfo,
}

/// Share details exposed to anonymous visitors through a share token.
///
/// Deliberately excludes owner account internals; only a display name and
/// avatar reference are revealed via `shared_by`.
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SharePublicInfo {
    /// Share token echoed back so clients can build further public requests.
    pub token: String,
    /// Display name of the shared file or folder.
    pub name: String,
    /// Target kind, either `"file"` or `"folder"`.
    pub share_type: String,
    /// Resolved icon for a shared folder; `None` for file shares.
    pub folder_icon: Option<crate::services::files::folder::FolderIcon>,
    /// Whether anonymous access requires a password; the password itself is
    /// never exposed.
    pub has_password: bool,
    /// Expiry time serialized as RFC 3339; `None` means the share never
    /// expires.
    pub expires_at: Option<String>,
    /// Whether the share has already passed its expiry time.
    pub is_expired: bool,
    /// Number of completed downloads so far.
    pub download_count: i64,
    /// Number of times the share page has been viewed.
    pub view_count: i64,
    /// Maximum allowed downloads; `0` means unlimited.
    pub max_downloads: i64,
    /// MIME type of the shared file; `None` for folder shares.
    pub mime_type: Option<String>,
    /// Size of the shared file in bytes; `None` for folder shares.
    pub size: Option<i64>,
    /// Owner identity shown on the public share page.
    pub shared_by: SharePublicOwnerInfo,
}
