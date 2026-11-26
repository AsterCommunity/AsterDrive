//! SeaORM 实体定义：`share`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// 一条对外分享链接：指向一个文件或文件夹，携带访问口令、过期和下载/浏览
/// 限额。同一资源同时只保留一条活跃 share。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = ShareInfo))]
#[sea_orm(table_name = "shares")]
pub struct Model {
    /// 主键。
    #[sea_orm(primary_key)]
    pub id: i64,
    /// 分享链接的访问令牌，出现在公开 URL 中；唯一。
    #[sea_orm(unique)]
    pub token: String,
    /// 创建分享的账号。
    pub user_id: i64,
    /// 团队空间中的分享归属团队；个人分享为 `None`。
    pub team_id: Option<i64>,
    /// 被分享的文件；分享文件夹时为 `None`。
    pub file_id: Option<i64>,
    /// 被分享的文件夹；分享文件时为 `None`。`file_id` 与 `folder_id` 恰有一个非空。
    pub folder_id: Option<i64>,
    /// 访问口令的 hash；口令明文不落库。`None` 表示分享无需口令。
    #[serde(skip_serializing)]
    pub password: Option<String>,
    /// 过期时刻（UTC）；`None` 表示不过期。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<DateTimeUtc>,
    /// 最多允许的下载次数；`0` = 不限制。
    pub max_downloads: i64,
    /// 累计完成的下载次数。
    pub download_count: i64,
    /// 累计的公开页浏览次数。
    pub view_count: i64,
    /// 行创建时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// 行最近更新时间（UTC）。
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// 创建分享的账号（FK `user_id`）。
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
    /// 被分享的文件（FK `file_id`）。
    #[sea_orm(
        belongs_to = "super::file::Entity",
        from = "Column::FileId",
        to = "super::file::Column::Id"
    )]
    File,
    /// 团队分享归属的团队（FK `team_id`）。
    #[sea_orm(
        belongs_to = "super::team::Entity",
        from = "Column::TeamId",
        to = "super::team::Column::Id"
    )]
    Team,
    /// 被分享的文件夹（FK `folder_id`）。
    #[sea_orm(
        belongs_to = "super::folder::Entity",
        from = "Column::FolderId",
        to = "super::folder::Column::Id"
    )]
    Folder,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::file::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::File.def()
    }
}

impl Related<super::team::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}

impl Related<super::folder::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Folder.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
