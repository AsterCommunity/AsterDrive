//! 分享服务子模块：`models`。

use chrono::Utc;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

use crate::entities::share;
use crate::errors::{AsterError, Result};
use crate::services::{profile_service, user_service};
use crate::types::EntityType;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ShareStatus {
    Active,
    Expired,
    Exhausted,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ShareTarget {
    #[serde(rename = "type")]
    pub r#type: EntityType,
    #[validate(range(min = 1, message = "share target id must be greater than 0"))]
    pub id: i64,
}

impl ShareTarget {
    pub const fn file(id: i64) -> Self {
        Self {
            r#type: EntityType::File,
            id,
        }
    }

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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ShareInfo {
    pub id: i64,
    pub token: String,
    pub user: Option<user_service::UserSummary>,
    pub team_id: Option<i64>,
    pub target: ShareTarget,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub max_downloads: i64,
    pub download_count: i64,
    pub view_count: i64,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
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
    user: Option<user_service::UserSummary>,
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MyShareInfo {
    pub id: i64,
    pub token: String,
    pub resource_id: i64,
    pub resource_name: String,
    pub resource_type: EntityType,
    pub resource_deleted: bool,
    pub has_password: bool,
    pub status: ShareStatus,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub max_downloads: i64,
    pub download_count: i64,
    pub view_count: i64,
    pub remaining_downloads: Option<i64>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SharePublicOwnerInfo {
    pub name: String,
    pub avatar: profile_service::AvatarInfo,
}

#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct SharePublicInfo {
    pub token: String,
    pub name: String,
    pub share_type: String,
    pub has_password: bool,
    pub expires_at: Option<String>,
    pub is_expired: bool,
    pub download_count: i64,
    pub view_count: i64,
    pub max_downloads: i64,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub shared_by: SharePublicOwnerInfo,
}
