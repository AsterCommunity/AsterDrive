//! API 分页参数与响应封装。

use crate::errors::Result;
use aster_drive_model::entities::file;
pub use aster_drive_model::types::SortBy;
use serde::{Deserialize, Serialize};
use std::future::Future;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use aster_forge_api::{
    ApiSchema, DEFAULT_FILE_LIMIT, DEFAULT_FOLDER_LIMIT, MAX_PAGE_SIZE, OffsetPage, SortOrder,
};

/// 拉取一页 offset 分页数据并封装为 Forge `OffsetPage`。
///
/// `limit` 先收敛到 `1..=max_limit`；`fetch` 收到收敛后的 limit 与原始 offset，
/// 返回 `(当前页数据, 总数)`。
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

/// 从文件 Model 提取对应排序字段的字符串值，用于 cursor。
pub fn file_cursor_value(f: &file::Model, sort_by: SortBy) -> String {
    match sort_by {
        SortBy::Name => f.name.clone(),
        SortBy::Size => f.size.to_string(),
        SortBy::CreatedAt => f.created_at.to_rfc3339(),
        SortBy::UpdatedAt => f.updated_at.to_rfc3339(),
        SortBy::Type => f.mime_type.clone(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端用户列表的排序字段。
pub enum AdminUserSortBy {
    /// 用户 ID。
    Id,
    /// 用户名。
    Username,
    /// 邮箱。
    Email,
    /// 角色。
    Role,
    /// 账号状态。
    Status,
    /// 已用存储量（字节）。
    StorageUsed,
    /// 存储配额上限（字节）。
    StorageQuota,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端团队列表的排序字段。
pub enum AdminTeamSortBy {
    /// 团队 ID。
    Id,
    /// 团队名称。
    Name,
    /// 已用存储量（字节）。
    StorageUsed,
    /// 存储配额上限（字节）。
    StorageQuota,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
    /// 归档时间。
    ArchivedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端团队成员列表的排序字段。
pub enum AdminTeamMemberSortBy {
    /// 用户名。
    Username,
    /// 邮箱。
    Email,
    /// 成员角色。
    Role,
    /// 成员状态。
    Status,
    /// 加入团队时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端存储策略列表的排序字段。
pub enum AdminPolicySortBy {
    /// 策略 ID。
    Id,
    /// 策略名称。
    Name,
    /// 绑定的存储连接器 ID。
    ConnectorId,
    /// 是否为默认策略。
    IsDefault,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端策略组列表的排序字段。
pub enum AdminPolicyGroupSortBy {
    /// 策略组 ID。
    Id,
    /// 策略组名称。
    Name,
    /// 是否启用。
    IsEnabled,
    /// 是否为默认策略组。
    IsDefault,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端远端节点列表的排序字段。
pub enum AdminRemoteNodeSortBy {
    /// 节点 ID。
    Id,
    /// 节点名称。
    Name,
    /// 节点基础 URL。
    BaseUrl,
    /// 是否启用。
    IsEnabled,
    /// 最近一次探测时间。
    LastProbeAt,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端分享列表的排序字段。
pub enum AdminShareSortBy {
    /// 分享 ID。
    Id,
    /// 分享 token。
    Token,
    /// 创建者用户 ID。
    UserId,
    /// 累计下载次数。
    DownloadCount,
    /// 下载次数上限（0 表示不限）。
    MaxDownloads,
    /// 过期时间。
    ExpiresAt,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端后台任务列表的排序字段。
pub enum AdminTaskSortBy {
    /// 任务 ID。
    Id,
    /// 展示名称。
    DisplayName,
    /// 任务类型。
    Kind,
    /// 任务状态。
    Status,
    /// 当前进度（已完成步数 `progress_current`）。
    Progress,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
    /// 开始执行时间。
    StartedAt,
    /// 执行结束时间。
    FinishedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端资源锁列表的排序字段。
pub enum AdminLockSortBy {
    /// 锁 ID。
    Id,
    /// 锁定的 lockroot 路径。
    LockrootPath,
    /// 锁根类型（文件或文件夹）。
    RootKind,
    /// 持锁用户 ID。
    HolderUserId,
    /// 锁超时时间。
    TimeoutAt,
    /// 锁模式（`exclusive` 或 `shared`）。
    Mode,
    /// 锁深度（`resource` 或 `infinity`）。
    Depth,
    /// 创建时间。
    CreatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端审计日志列表的排序字段。
pub enum AdminAuditLogSortBy {
    /// 日志 ID。
    Id,
    /// 记录时间。
    CreatedAt,
    /// 操作者用户 ID。
    UserId,
    /// 操作动作。
    Action,
    /// 目标实体类型。
    EntityType,
    /// 目标实体名称。
    EntityName,
    /// 客户端 IP 地址。
    IpAddress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端文件列表的排序字段。
pub enum AdminFileSortBy {
    /// 文件 ID。
    Id,
    /// 文件名。
    Name,
    /// 文件大小（字节）。
    Size,
    /// 关联 blob ID。
    BlobId,
    /// 存储策略 ID。
    PolicyId,
    /// 所有者用户 ID。
    OwnerUserId,
    /// 所属团队 ID。
    TeamId,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
    UpdatedAt,
    /// 删除时间。
    DeletedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
/// 管理端文件 blob 列表的排序字段。
pub enum AdminFileBlobSortBy {
    /// blob ID。
    Id,
    /// 内容哈希。
    Hash,
    /// blob 大小（字节）。
    Size,
    /// 存储策略 ID。
    PolicyId,
    /// 存储路径。
    StoragePath,
    /// 引用计数。
    RefCount,
    /// 创建时间。
    CreatedAt,
    /// 更新时间。
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
    /// 生效的文件夹上限：显式值截断到 `MAX_PAGE_SIZE`，缺省取 `DEFAULT_FOLDER_LIMIT`（0 表示跳过查询）。
    pub fn folder_limit(&self) -> u64 {
        self.folder_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FOLDER_LIMIT)
    }

    /// 生效的文件夹偏移量，缺省为 0。
    pub fn folder_offset(&self) -> u64 {
        self.folder_offset.unwrap_or(0)
    }

    /// 生效的文件上限：显式值截断到 `MAX_PAGE_SIZE`，缺省取 `DEFAULT_FILE_LIMIT`（0 表示跳过查询）。
    pub fn file_limit(&self) -> u64 {
        self.file_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FILE_LIMIT)
    }

    /// 生效的排序字段，未指定时用默认的 name。
    pub fn sort_by(&self) -> SortBy {
        self.sort_by.unwrap_or_default()
    }

    /// 生效的排序方向，未指定时用默认的 asc。
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
    /// 生效的文件夹上限：显式值截断到 `MAX_PAGE_SIZE`，缺省取 `DEFAULT_FOLDER_LIMIT`（0 表示跳过查询）。
    pub fn folder_limit(&self) -> u64 {
        self.folder_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FOLDER_LIMIT)
    }

    /// 生效的文件夹偏移量，缺省为 0。
    pub fn folder_offset(&self) -> u64 {
        self.folder_offset.unwrap_or(0)
    }

    /// 生效的文件上限：显式值截断到 `MAX_PAGE_SIZE`，缺省取 `DEFAULT_FILE_LIMIT`（0 表示跳过查询）。
    pub fn file_limit(&self) -> u64 {
        self.file_limit
            .map(|v| v.min(MAX_PAGE_SIZE))
            .unwrap_or(DEFAULT_FILE_LIMIT)
    }

    /// 解析 cursor：`expires_at` 与 `id` 必须同时存在且时间可解析为 UTC，否则返回 `None`。
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
