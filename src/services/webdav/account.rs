//! 服务模块：`webdav::account`。

use chrono::Utc;
use sea_orm::{ActiveModelTrait, DbErr, Set, SqlErr};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::pagination::{OffsetPage, load_offset_page};
use crate::db::repository::webdav_account_repo;
use crate::entities::webdav_account;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    user::{
        account::{self, UserSummary},
        profile,
    },
    workspace::storage::WorkspaceStorageScope,
};
use crate::utils::hash;

fn webdav_username_exists_error() -> AsterError {
    validation_error_with_code(
        ApiErrorCode::WebdavUsernameExists,
        "WebDAV username already exists",
    )
}

fn map_webdav_account_create_db_err(err: DbErr) -> AsterError {
    if matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        webdav_username_exists_error()
    } else {
        AsterError::from(err)
    }
}

/// 创建账号后返回的响应（包含一次性明文密码）
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavAccountCreated {
    pub id: i64,
    pub username: String,
    pub team_id: Option<i64>,
    /// 明文密码，只返回一次
    pub password: String,
    pub root_folder_path: Option<String>,
}

/// 列表返回用的带路径的账号信息
#[derive(Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavAccountInfo {
    pub id: i64,
    pub username: String,
    pub user_id: i64,
    pub user: Option<UserSummary>,
    pub team_id: Option<i64>,
    pub root_folder_id: Option<i64>,
    /// 文件夹路径，如 "/Documents/Photos"，None 表示全部访问
    pub root_folder_path: Option<String>,
    pub is_active: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavAccount {
    pub id: i64,
    pub user_id: i64,
    pub team_id: Option<i64>,
    pub username: String,
    pub root_folder_id: Option<i64>,
    pub is_active: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

impl From<webdav_account::Model> for WebdavAccount {
    fn from(model: webdav_account::Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            team_id: model.team_id,
            username: model.username,
            root_folder_id: model.root_folder_id,
            is_active: model.is_active,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

/// 创建 WebDAV 账号
///
/// password 为 None 时自动生成 16 位随机密码
pub async fn create(
    state: &impl SharedRuntimeState,
    user_id: i64,
    username: &str,
    password: Option<&str>,
    root_folder_id: Option<i64>,
) -> Result<WebdavAccountCreated> {
    create_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        username,
        password,
        root_folder_id,
    )
    .await
}

pub async fn create_for_team(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    team_id: i64,
    username: &str,
    password: Option<&str>,
    root_folder_id: Option<i64>,
) -> Result<WebdavAccountCreated> {
    crate::services::workspace::storage::require_team_access(state, team_id, actor_user_id).await?;
    create_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        },
        username,
        password,
        root_folder_id,
    )
    .await
}

async fn create_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    username: &str,
    password: Option<&str>,
    root_folder_id: Option<i64>,
) -> Result<WebdavAccountCreated> {
    crate::services::workspace::storage::require_scope_access(state, scope).await?;

    // 检查用户名是否已存在
    if webdav_account_repo::find_by_username(state.writer_db(), username)
        .await?
        .is_some()
    {
        return Err(webdav_username_exists_error());
    }

    // 生成或使用指定密码
    let plain_password = match password {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => generate_random_password(16),
    };

    let password_hash = hash::hash_password(&plain_password)?;
    let now = Utc::now();

    // 如果指定了 root_folder_id，验证文件夹属于账号所在工作空间。
    let root_folder_path = if let Some(fid) = root_folder_id {
        crate::services::workspace::storage::verify_folder_access(state, scope, fid).await?;
        crate::services::files::folder::build_folder_paths_cached(state, &[fid])
            .await?
            .remove(&fid)
    } else {
        None
    };

    let actor_user_id = scope.actor_user_id();
    let model = webdav_account::ActiveModel {
        user_id: Set(actor_user_id),
        team_id: Set(scope.team_id()),
        username: Set(username.to_string()),
        password_hash: Set(password_hash),
        root_folder_id: Set(root_folder_id),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let created = model
        .insert(state.writer_db())
        .await
        .map_err(map_webdav_account_create_db_err)?;
    crate::webdav::auth::invalidate_webdav_auth_for_username(state, &created.username).await;

    Ok(WebdavAccountCreated {
        id: created.id,
        username: created.username,
        team_id: created.team_id,
        password: plain_password,
        root_folder_path,
    })
}

/// 列出用户的所有 WebDAV 账号（带文件夹路径）
pub async fn list(state: &impl SharedRuntimeState, user_id: i64) -> Result<Vec<WebdavAccountInfo>> {
    let accounts = webdav_account_repo::find_by_user(state.writer_db(), user_id).await?;
    build_account_infos(state, accounts).await
}

pub async fn list_for_team(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    team_id: i64,
) -> Result<Vec<WebdavAccountInfo>> {
    let role =
        crate::services::workspace::storage::load_team_member_role(state, team_id, actor_user_id)
            .await?;
    let accounts = if role.can_manage_team() {
        webdav_account_repo::find_by_team(state.writer_db(), team_id).await?
    } else {
        webdav_account_repo::find_by_team_and_user(state.writer_db(), team_id, actor_user_id)
            .await?
    };
    build_account_infos(state, accounts).await
}

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<WebdavAccountInfo>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) =
            webdav_account_repo::find_by_user_paginated(state.writer_db(), user_id, limit, offset)
                .await?;
        let items = build_account_infos(state, items).await?;
        Ok((items, total))
    })
    .await
}

pub async fn list_team_paginated(
    state: &impl SharedRuntimeState,
    actor_user_id: i64,
    team_id: i64,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<WebdavAccountInfo>> {
    let role =
        crate::services::workspace::storage::load_team_member_role(state, team_id, actor_user_id)
            .await?;
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) = if role.can_manage_team() {
            webdav_account_repo::find_by_team_paginated(state.writer_db(), team_id, limit, offset)
                .await?
        } else {
            webdav_account_repo::find_by_team_and_user_paginated(
                state.writer_db(),
                team_id,
                actor_user_id,
                limit,
                offset,
            )
            .await?
        };
        let items = build_account_infos(state, items).await?;
        Ok((items, total))
    })
    .await
}

async fn build_account_infos(
    state: &impl SharedRuntimeState,
    accounts: Vec<webdav_account::Model>,
) -> Result<Vec<WebdavAccountInfo>> {
    let folder_ids: Vec<i64> = accounts
        .iter()
        .filter_map(|acc| acc.root_folder_id)
        .collect();
    let paths =
        crate::services::files::folder::build_folder_paths_cached(state, &folder_ids).await?;
    let user_ids: Vec<i64> = accounts.iter().map(|acc| acc.user_id).collect();
    let users =
        account::user_summaries_by_ids(state, &user_ids, profile::AvatarAudience::AdminUser)
            .await?;

    let mut result = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let root_folder_path = acc.root_folder_id.and_then(|fid| paths.get(&fid).cloned());
        let user = users.get(&acc.user_id).cloned();
        result.push(WebdavAccountInfo {
            id: acc.id,
            username: acc.username,
            user_id: acc.user_id,
            user,
            team_id: acc.team_id,
            root_folder_id: acc.root_folder_id,
            root_folder_path,
            is_active: acc.is_active,
            created_at: acc.created_at,
            updated_at: acc.updated_at,
        });
    }

    Ok(result)
}

/// 删除 WebDAV 账号（需要验证归属）
pub async fn delete(state: &impl SharedRuntimeState, id: i64, user_id: i64) -> Result<()> {
    let account = webdav_account_repo::find_by_id(state.writer_db(), id).await?;
    if account.team_id.is_some() {
        return Err(AsterError::auth_forbidden(
            "team WebDAV account must be managed from the team workspace",
        ));
    }
    crate::utils::verify_owner(account.user_id, user_id, "account")?;
    webdav_account_repo::delete(state.writer_db(), id).await?;
    crate::webdav::auth::invalidate_webdav_auth_for_username(state, &account.username).await;
    tracing::debug!(
        webdav_account_id = id,
        user_id,
        username = %account.username,
        "deleted WebDAV account"
    );
    Ok(())
}

pub async fn delete_for_team(
    state: &impl SharedRuntimeState,
    id: i64,
    actor_user_id: i64,
    team_id: i64,
) -> Result<()> {
    let role =
        crate::services::workspace::storage::load_team_member_role(state, team_id, actor_user_id)
            .await?;
    let account = webdav_account_repo::find_by_id(state.writer_db(), id).await?;
    if account.team_id != Some(team_id) {
        return Err(AsterError::record_not_found(format!(
            "webdav_account #{id}"
        )));
    }
    if account.user_id != actor_user_id && !role.can_manage_team() {
        return Err(AsterError::auth_forbidden(
            "team WebDAV account can only be managed by its owner or a team manager",
        ));
    }
    webdav_account_repo::delete(state.writer_db(), id).await?;
    crate::webdav::auth::invalidate_webdav_auth_for_username(state, &account.username).await;
    tracing::debug!(
        webdav_account_id = id,
        team_id,
        actor_user_id,
        username = %account.username,
        "deleted team WebDAV account"
    );
    Ok(())
}

/// 切换启用/禁用
pub async fn toggle_active(
    state: &impl SharedRuntimeState,
    id: i64,
    user_id: i64,
) -> Result<WebdavAccount> {
    let account = webdav_account_repo::find_by_id(state.writer_db(), id).await?;
    if account.team_id.is_some() {
        return Err(AsterError::auth_forbidden(
            "team WebDAV account must be managed from the team workspace",
        ));
    }
    crate::utils::verify_owner(account.user_id, user_id, "account")?;
    let new_is_active = !account.is_active;
    let username = account.username.clone();
    let mut active: webdav_account::ActiveModel = account.into();
    active.is_active = Set(new_is_active);
    active.updated_at = Set(Utc::now());
    let updated = webdav_account_repo::update(state.writer_db(), active)
        .await
        .map(Into::into)?;
    crate::webdav::auth::invalidate_webdav_auth_for_username(state, &username).await;
    Ok(updated)
}

pub async fn toggle_team_active(
    state: &impl SharedRuntimeState,
    id: i64,
    actor_user_id: i64,
    team_id: i64,
) -> Result<WebdavAccount> {
    let role =
        crate::services::workspace::storage::load_team_member_role(state, team_id, actor_user_id)
            .await?;
    let account = webdav_account_repo::find_by_id(state.writer_db(), id).await?;
    if account.team_id != Some(team_id) {
        return Err(AsterError::record_not_found(format!(
            "webdav_account #{id}"
        )));
    }
    if account.user_id != actor_user_id && !role.can_manage_team() {
        return Err(AsterError::auth_forbidden(
            "team WebDAV account can only be managed by its owner or a team manager",
        ));
    }
    let new_is_active = !account.is_active;
    let username = account.username.clone();
    let mut active: webdav_account::ActiveModel = account.into();
    active.is_active = Set(new_is_active);
    active.updated_at = Set(Utc::now());
    let updated = webdav_account_repo::update(state.writer_db(), active)
        .await
        .map(Into::into)?;
    crate::webdav::auth::invalidate_webdav_auth_for_username(state, &username).await;
    Ok(updated)
}

/// 测试 WebDAV 凭据是否正确
pub async fn test_credentials(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
) -> Result<()> {
    let account = webdav_account_repo::find_by_username(state.writer_db(), username)
        .await?
        .ok_or_else(|| AsterError::auth_invalid_credentials("WebDAV account not found"))?;

    if !account.is_active {
        return Err(AsterError::auth_forbidden("WebDAV account is disabled"));
    }

    if !hash::verify_password(password, &account.password_hash)? {
        return Err(AsterError::auth_invalid_credentials("wrong password"));
    }

    let user =
        crate::db::repository::user_repo::find_by_id(state.writer_db(), account.user_id).await?;
    if !user.status.is_active() {
        return Err(AsterError::auth_forbidden("user account is disabled"));
    }

    if let Some(team_id) = account.team_id {
        crate::services::workspace::storage::require_team_access(state, team_id, account.user_id)
            .await?;
    }

    Ok(())
}

/// 生成随机密码
fn generate_random_password(len: usize) -> String {
    use rand::RngExt;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
