//! API 分页参数与响应封装。

use crate::entities::file;
use crate::errors::Result;
use serde::{Deserialize, Serialize};
use std::future::Future;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

pub use aster_forge_api::{
    ApiSchema, DEFAULT_FILE_LIMIT, DEFAULT_FOLDER_LIMIT, LimitOffsetQuery, MAX_PAGE_SIZE,
    OffsetPage, SortOrder,
};

pub async fn load_offset_page<T, F, Fut>(
    limit: u64,
    offset: u64,
    max_limit: u64,
    fetch: F,
) -> Result<OffsetPage<T>>
where
    T: Serialize + ApiSchema,
    F: FnOnce(u64, u64) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, u64)>>,
{
    let limit = limit.clamp(1, max_limit);
    let (items, total) = fetch(limit, offset).await?;
    Ok(OffsetPage::new(items, total, limit, offset))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    CreatedAt,
    UpdatedAt,
    #[serde(rename = "type")]
    Type,
}

impl SortBy {
    /// 从文件 Model 提取对应排序字段的字符串值，用于 cursor
    pub fn cursor_value(f: &file::Model, sort_by: SortBy) -> String {
        match sort_by {
            SortBy::Name => f.name.clone(),
            SortBy::Size => f.size.to_string(),
            SortBy::CreatedAt => f.created_at.to_rfc3339(),
            SortBy::UpdatedAt => f.updated_at.to_rfc3339(),
            SortBy::Type => f.mime_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminUserSortBy {
    Id,
    Username,
    Email,
    Role,
    Status,
    StorageUsed,
    StorageQuota,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminTeamSortBy {
    Id,
    Name,
    StorageUsed,
    StorageQuota,
    CreatedAt,
    UpdatedAt,
    ArchivedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminTeamMemberSortBy {
    Username,
    Email,
    Role,
    Status,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminPolicySortBy {
    Id,
    Name,
    DriverType,
    Endpoint,
    Bucket,
    IsDefault,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminPolicyGroupSortBy {
    Id,
    Name,
    IsEnabled,
    IsDefault,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminRemoteNodeSortBy {
    Id,
    Name,
    BaseUrl,
    IsEnabled,
    LastCheckedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminShareSortBy {
    Id,
    Token,
    UserId,
    DownloadCount,
    MaxDownloads,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminTaskSortBy {
    Id,
    DisplayName,
    Kind,
    Status,
    Progress,
    CreatedAt,
    UpdatedAt,
    StartedAt,
    FinishedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminLockSortBy {
    Id,
    Path,
    EntityType,
    OwnerId,
    TimeoutAt,
    Shared,
    Deep,
    CreatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminAuditLogSortBy {
    Id,
    CreatedAt,
    UserId,
    Action,
    EntityType,
    EntityName,
    IpAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminFileSortBy {
    Id,
    Name,
    Size,
    BlobId,
    PolicyId,
    OwnerUserId,
    TeamId,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminFileBlobSortBy {
    Id,
    Hash,
    Size,
    PolicyId,
    StoragePath,
    RefCount,
    CreatedAt,
    UpdatedAt,
}

/// 文件列表分页参数（文件夹用 offset 分页，文件用 cursor 分页）
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct FolderListQuery {
    /// 文件夹最大返回数量（默认 200，最大 1000；传 0 跳过文件夹查询）
    pub folder_limit: Option<u64>,
    /// 文件夹偏移量（默认 0）
    pub folder_offset: Option<u64>,
    /// 文件最大返回数量（默认 100，最大 1000；传 0 跳过文件查询）
    pub file_limit: Option<u64>,
    /// 排序字段（name|size|created_at|updated_at|type，默认 name）
    pub sort_by: Option<SortBy>,
    /// 排序方向（asc|desc，默认 asc）
    pub sort_order: Option<SortOrder>,
    /// cursor 分页：上一页最后一条文件的排序字段值（序列化为字符串）
    pub file_after_value: Option<String>,
    /// cursor 分页：上一页最后一条文件的 id
    pub file_after_id: Option<i64>,
}

impl FolderListQuery {
    pub fn folder_limit(&self) -> u64 {
        self.folder_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FOLDER_LIMIT)
    }

    pub fn folder_offset(&self) -> u64 {
        self.folder_offset.unwrap_or(0)
    }

    pub fn file_limit(&self) -> u64 {
        self.file_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FILE_LIMIT)
    }

    pub fn sort_by(&self) -> SortBy {
        self.sort_by.unwrap_or_default()
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or_default()
    }

    /// 返回 cursor，两个字段必须同时存在才有效
    pub fn file_cursor(&self) -> Option<(String, i64)> {
        match (&self.file_after_value, self.file_after_id) {
            (Some(val), Some(id)) => Some((val.clone(), id)),
            _ => None,
        }
    }
}

/// 回收站列表分页参数
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct TrashListQuery {
    /// 文件夹最大返回数量（默认 200，最大 1000；传 0 跳过文件夹查询）
    pub folder_limit: Option<u64>,
    /// 文件夹偏移量（默认 0）
    pub folder_offset: Option<u64>,
    /// 文件最大返回数量（默认 100，最大 1000；传 0 跳过文件查询）
    pub file_limit: Option<u64>,
    /// cursor 分页：上一页最后一条文件的 expires_at（ISO 8601）
    pub file_after_expires_at: Option<String>,
    /// cursor 分页：上一页最后一条文件的 id
    pub file_after_id: Option<i64>,
}

impl TrashListQuery {
    pub fn folder_limit(&self) -> u64 {
        self.folder_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FOLDER_LIMIT)
    }

    pub fn folder_offset(&self) -> u64 {
        self.folder_offset.unwrap_or(0)
    }

    pub fn file_limit(&self) -> u64 {
        self.file_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FILE_LIMIT)
    }

    pub fn file_cursor(&self) -> Option<(chrono::DateTime<chrono::Utc>, i64)> {
        match (&self.file_after_expires_at, self.file_after_id) {
            (Some(dt_str), Some(id)) => dt_str
                .parse::<chrono::DateTime<chrono::Utc>>()
                .ok()
                .map(|dt| (dt, id)),
            _ => None,
        }
    }
}
