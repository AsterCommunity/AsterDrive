//! API 路由聚合入口。

use crate::services::workspace_storage_service::WorkspaceStorageScope;

pub mod admin;
pub mod auth;
pub mod batch;
pub mod files;
pub mod folders;
pub mod frontend;
pub mod health;
pub mod internal_storage;
pub mod properties;
pub mod public;
pub mod remote_tunnel;
pub mod search;
pub mod share_public;
pub mod shares;
pub mod tags;
pub mod tasks;
pub mod teams;
pub mod trash;
pub mod webdav_accounts;
pub mod wopi;
pub mod workspace_transfer;

pub(crate) fn team_scope(team_id: i64, user_id: i64) -> WorkspaceStorageScope {
    WorkspaceStorageScope::Team {
        team_id,
        actor_user_id: user_id,
    }
}
