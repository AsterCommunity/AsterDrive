//! 统一描述“当前操作落在哪个工作空间里”。
//!
//! 个人空间和团队空间共用大部分文件主链路，但权限和资源归属并不完全相同。
//! 这里负责把 scope 相关规则收口，避免每个上层 service 都自己拼一套
//! `user_id` / `team_id` / `actor_user_id` 判断。

mod cache;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, folder_repo, team_member_repo, team_repo, user_repo};
use crate::entities::{file, folder};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::types::TeamMemberRole;
use sea_orm::ConnectionTrait;
use serde::{Deserialize, Serialize};

/// scope 同时表达“资源属于哪个空间”和“是谁在操作”。
///
/// 个人空间里两者通常是同一个人；团队空间里则必须同时保留 `team_id`
/// 和 `actor_user_id`，否则后续无法同时做成员校验和归属校验。
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorkspaceStorageScope {
    Personal { user_id: i64 },
    Team { team_id: i64, actor_user_id: i64 },
}

/// 只描述资源归属空间，不携带当前操作者。
///
/// 后台维护、公开分享等路径经常只需要“按哪个用户/团队记账或过滤资源”，
/// 不能为了复用 `WorkspaceStorageScope` 而伪造 `actor_user_id`。
#[derive(Clone, Copy, Debug)]
pub(crate) enum WorkspaceResourceScope {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

impl From<WorkspaceStorageScope> for WorkspaceResourceScope {
    fn from(scope: WorkspaceStorageScope) -> Self {
        match scope {
            WorkspaceStorageScope::Personal { user_id } => Self::Personal { user_id },
            WorkspaceStorageScope::Team { team_id, .. } => Self::Team { team_id },
        }
    }
}

impl WorkspaceResourceScope {
    pub(crate) fn from_folder_model(folder: &folder::Model) -> Result<Self> {
        if let Some(team_id) = folder.team_id {
            Ok(Self::Team { team_id })
        } else {
            let user_id = folder.owner_user_id.ok_or_else(|| {
                auth_forbidden_with_code(
                    ApiErrorCode::WorkspaceScopeDenied,
                    "folder missing both team_id and owner_user_id: data integrity violation",
                )
            })?;
            Ok(Self::Personal { user_id })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedTeamAccess {
    team_id: i64,
    policy_group_id: Option<i64>,
    role: TeamMemberRole,
}

impl From<team_member_repo::ActiveTeamAccessSnapshot> for CachedTeamAccess {
    fn from(snapshot: team_member_repo::ActiveTeamAccessSnapshot) -> Self {
        Self {
            team_id: snapshot.team_id,
            policy_group_id: snapshot.policy_group_id,
            role: snapshot.role,
        }
    }
}

pub(crate) async fn invalidate_team_access_cache_for_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
) {
    cache::invalidate_team_access_for_team(state, team_id).await;
}

pub(crate) async fn invalidate_team_access_cache_for_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) {
    cache::invalidate_team_access_for_member(state, team_id, user_id).await;
}

impl WorkspaceStorageScope {
    pub(crate) fn actor_user_id(self) -> i64 {
        match self {
            Self::Personal { user_id } => user_id,
            Self::Team { actor_user_id, .. } => actor_user_id,
        }
    }

    pub(crate) fn owner_user_id(self) -> Option<i64> {
        match self {
            Self::Personal { user_id } => Some(user_id),
            Self::Team { .. } => None,
        }
    }

    pub(crate) fn team_id(self) -> Option<i64> {
        match self {
            Self::Personal { .. } => None,
            Self::Team { team_id, .. } => Some(team_id),
        }
    }
}

async fn load_team_access<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    team_id: i64,
    user_id: i64,
) -> Result<CachedTeamAccess> {
    if let Some(cached) = cache::load_team_access(state, team_id, user_id).await {
        let access = match load_team_access_from_database(db, team_id, user_id).await {
            Ok(access) => access,
            Err(error @ (AsterError::RecordNotFound(_) | AsterError::AuthForbidden(_))) => {
                cache::invalidate_team_access_for_member(state, team_id, user_id).await;
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        if access == cached {
            tracing::debug!(team_id, user_id, "team access cache hit");
        } else {
            cache::store_team_access(state, team_id, user_id, &access).await;
            tracing::debug!(
                team_id,
                user_id,
                "team access cache stale; refreshed from database"
            );
        }
        return Ok(access);
    }

    let access = load_team_access_from_database(db, team_id, user_id).await?;
    cache::store_team_access(state, team_id, user_id, &access).await;
    tracing::debug!(team_id, user_id, "team access cache miss");
    Ok(access)
}

async fn load_team_access_from_database<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    user_id: i64,
) -> Result<CachedTeamAccess> {
    if let Some(snapshot) = team_member_repo::find_active_team_access(db, team_id, user_id).await? {
        return Ok(snapshot.into());
    }

    // 保持旧语义：团队不存在或已归档时返回 not found；团队仍活跃但用户不是成员时返回 forbidden。
    team_repo::find_active_by_id(db, team_id).await?;
    Err(auth_forbidden_with_code(
        ApiErrorCode::TeamNotMember,
        "not a member of this team",
    ))
}

pub(crate) async fn require_scope_access(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    require_scope_access_with_db(state, state.reader_db(), scope).await
}

pub(crate) async fn require_scope_access_with_db<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    // 个人空间天然只需要“用户正在操作自己的空间”这个前提；
    // 团队空间则必须先确认 actor 当前仍然是团队成员。
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        require_team_access_with_db(state, db, team_id, actor_user_id).await?;
    }

    Ok(())
}

pub(crate) async fn load_scope_actor_username<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
) -> Result<String> {
    let user = user_repo::find_by_id(db, scope.actor_user_id()).await?;
    Ok(user.username)
}

pub(crate) async fn load_scope_actor_username_cached(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
) -> Result<String> {
    let user_id = scope.actor_user_id();
    if let Some(username) = cache::load_actor_username(state, user_id).await {
        tracing::debug!(user_id, "actor username cache hit");
        return Ok(username);
    }

    let username = load_scope_actor_username(state.reader_db(), scope).await?;
    cache::store_actor_username(state, user_id, &username).await;
    tracing::debug!(user_id, "actor username cache miss");
    Ok(username)
}

pub(crate) fn ensure_personal_file_scope(file: &file::Model) -> Result<()> {
    if file.team_id.is_some() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::WorkspaceScopeDenied,
            "file belongs to a team workspace",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_personal_folder_scope(folder: &folder::Model) -> Result<()> {
    if folder.team_id.is_some() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::WorkspaceScopeDenied,
            "folder belongs to a team workspace",
        ));
    }
    Ok(())
}

pub(crate) fn ensure_file_scope(file: &file::Model, scope: WorkspaceStorageScope) -> Result<()> {
    ensure_file_resource_scope(file, scope.into())
}

pub(crate) fn ensure_file_resource_scope(
    file: &file::Model,
    scope: WorkspaceResourceScope,
) -> Result<()> {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            ensure_personal_file_scope(file)?;
            crate::types::ownership::verify_owner(
                file.owner_user_id.ok_or_else(|| {
                    auth_forbidden_with_code(
                        ApiErrorCode::WorkspaceScopeDenied,
                        "file has no personal owner",
                    )
                })?,
                user_id,
                "file",
            )?;
        }
        WorkspaceResourceScope::Team { team_id } => {
            if file.team_id != Some(team_id) {
                return Err(auth_forbidden_with_code(
                    ApiErrorCode::WorkspaceScopeDenied,
                    "file is outside team workspace",
                ));
            }
        }
    }

    Ok(())
}

pub(crate) fn ensure_active_file_scope(
    file: &file::Model,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    ensure_file_scope(file, scope)?;

    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{} is in trash",
            file.id
        )));
    }

    Ok(())
}

pub(crate) fn ensure_folder_scope(
    folder: &folder::Model,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    ensure_folder_resource_scope(folder, scope.into())
}

pub(crate) fn ensure_folder_resource_scope(
    folder: &folder::Model,
    scope: WorkspaceResourceScope,
) -> Result<()> {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            ensure_personal_folder_scope(folder)?;
            crate::types::ownership::verify_owner(
                folder.owner_user_id.ok_or_else(|| {
                    auth_forbidden_with_code(
                        ApiErrorCode::WorkspaceScopeDenied,
                        "folder has no personal owner",
                    )
                })?,
                user_id,
                "folder",
            )?;
        }
        WorkspaceResourceScope::Team { team_id } => {
            if folder.team_id != Some(team_id) {
                return Err(auth_forbidden_with_code(
                    ApiErrorCode::WorkspaceScopeDenied,
                    "folder is outside team workspace",
                ));
            }
        }
    }

    Ok(())
}

pub(crate) fn ensure_active_folder_scope(
    folder: &folder::Model,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    ensure_folder_scope(folder, scope)?;

    if folder.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "folder #{} is in trash",
            folder.id
        )));
    }

    Ok(())
}

pub(crate) async fn require_team_access(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<()> {
    load_team_access(state, state.reader_db(), team_id, user_id)
        .await
        .map(|_| ())
}

pub(crate) async fn load_team_member_role(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<TeamMemberRole> {
    load_team_access(state, state.reader_db(), team_id, user_id)
        .await
        .map(|access| access.role)
}

pub(crate) async fn require_team_access_with_db<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    team_id: i64,
    user_id: i64,
) -> Result<()> {
    load_team_access(state, db, team_id, user_id)
        .await
        .map(|_| ())
}

pub(crate) async fn require_team_policy_group_id(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<i64> {
    require_team_policy_group_id_with_db(state, state.reader_db(), team_id, user_id).await
}

pub(crate) async fn require_team_policy_group_id_with_db<C: ConnectionTrait>(
    state: &impl SharedRuntimeState,
    db: &C,
    team_id: i64,
    user_id: i64,
) -> Result<i64> {
    let access = load_team_access(state, db, team_id, user_id).await?;
    access.policy_group_id.ok_or_else(|| {
        AsterError::storage_policy_not_found(format!(
            "no storage policy group assigned to team #{}",
            access.team_id
        ))
    })
}

pub(crate) async fn require_team_management_access(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<()> {
    let access = load_team_access(state, state.reader_db(), team_id, user_id).await?;
    if !access.role.can_manage_team() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::TeamAdminOrOwnerRequired,
            "team owner or admin role is required",
        ));
    }
    Ok(())
}

pub(crate) async fn verify_folder_access(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    verify_folder_access_with_db(state.writer_db(), state, scope, folder_id).await
}

pub(crate) async fn verify_folder_access_for_read(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    verify_folder_access_with_db(state.reader_db(), state, scope, folder_id).await
}

async fn verify_folder_access_with_db<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<folder::Model> {
    // 先校验当前 scope 还有效，再取实体做归属检查。
    // 这样所有调用方都能拿到“存在 + 属于当前空间 + 未进回收站”的 folder。
    require_scope_access_with_db(state, db, scope).await?;
    let folder = folder_repo::find_by_id(db, folder_id).await?;
    ensure_active_folder_scope(&folder, scope)?;

    Ok(folder)
}

pub(crate) async fn verify_file_access(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<file::Model> {
    verify_file_access_with_db(state.writer_db(), state, scope, file_id).await
}

pub(crate) async fn verify_file_access_for_read(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<file::Model> {
    verify_file_access_with_db(state.reader_db(), state, scope, file_id).await
}

async fn verify_file_access_with_db<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<file::Model> {
    // 文件访问和文件夹访问保持同样语义：返回值一旦成功，就已经完成 scope
    // 校验和 trash 过滤，上层不需要再手写重复判断。
    require_scope_access_with_db(state, db, scope).await?;
    let file = file_repo::find_by_id(db, file_id).await?;
    ensure_active_file_scope(&file, scope)?;

    Ok(file)
}

pub(crate) async fn list_files_in_folder(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_folder(state.reader_db(), user_id, folder_id).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_team_folder(state.reader_db(), team_id, folder_id).await
        }
    }
}

pub(crate) async fn list_folders_in_parent(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
) -> Result<Vec<folder::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            folder_repo::find_children(state.reader_db(), user_id, parent_id).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            folder_repo::find_team_children(state.reader_db(), team_id, parent_id).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkspaceStorageScope, require_team_access, require_team_policy_group_id};
    use crate::config::{Config, RuntimeConfig};
    use crate::db::repository::{policy_group_repo, policy_repo, team_member_repo, team_repo};
    use crate::entities::{
        storage_policy, storage_policy_group, storage_policy_group_item, team, team_member, user,
    };
    use crate::runtime::PrimaryAppState;
    use crate::services::workspace::scope::SharedRuntimeState;
    use crate::services::{files::folder, mail::sender};
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, TeamMemberRole,
        UserRole, UserStatus,
    };
    use aster_forge_cache as cache;
    use aster_forge_cache::CacheConfig;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    use std::sync::Arc;

    async fn build_cached_state() -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("test database should migrate");

        let cache = cache::create_cache(&CacheConfig {
            backend: "memory".to_string(),
            ..Default::default()
        })
        .await;
        let runtime_config = Arc::new(RuntimeConfig::new());
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_bus,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
            upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
        }
    }

    async fn create_user(state: &impl SharedRuntimeState, username: &str) -> user::Model {
        let now = Utc::now();
        user::ActiveModel {
            username: Set(username.to_string()),
            email: Set(format!("{username}@example.test")),
            password_hash: Set("not-used".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("test user should insert")
    }

    async fn create_policy_group_with_policy(
        state: &impl SharedRuntimeState,
        name: &str,
    ) -> storage_policy_group::Model {
        let now = Utc::now();
        let policy = policy_repo::create(
            state.writer_db(),
            storage_policy::ActiveModel {
                name: Set(format!("{name} Policy")),
                driver_type: Set(DriverType::Local),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set(format!("/tmp/asterdrive-{name}")),
                remote_node_id: Set(None),
                max_file_size: Set(0),
                allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
                options: Set(StoredStoragePolicyOptions::empty()),
                is_default: Set(false),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("test policy should insert");
        let group = policy_group_repo::create_group(
            state.writer_db(),
            storage_policy_group::ActiveModel {
                name: Set(format!("{name} Group")),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(false),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("test policy group should insert");
        policy_group_repo::create_group_item(
            state.writer_db(),
            storage_policy_group_item::ActiveModel {
                group_id: Set(group.id),
                policy_id: Set(policy.id),
                priority: Set(0),
                min_file_size: Set(0),
                max_file_size: Set(0),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("test policy group item should insert");
        state
            .policy_snapshot()
            .reload(state.writer_db())
            .await
            .expect("policy snapshot should reload");
        group
    }

    async fn create_team_with_member(
        state: &impl SharedRuntimeState,
        owner: &user::Model,
        member: &user::Model,
        policy_group_id: i64,
    ) -> (team::Model, team_member::Model) {
        let now = Utc::now();
        let team = team_repo::create(
            state.writer_db(),
            team::ActiveModel {
                name: Set("Cache Test Team".to_string()),
                description: Set(String::new()),
                created_by: Set(owner.id),
                storage_used: Set(0),
                storage_quota: Set(0),
                policy_group_id: Set(Some(policy_group_id)),
                created_at: Set(now),
                updated_at: Set(now),
                archived_at: Set(None),
                ..Default::default()
            },
        )
        .await
        .expect("test team should insert");
        team_member_repo::create(
            state.writer_db(),
            team_member::ActiveModel {
                team_id: Set(team.id),
                user_id: Set(owner.id),
                role: Set(TeamMemberRole::Owner),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("test owner membership should insert");
        let membership = team_member_repo::create(
            state.writer_db(),
            team_member::ActiveModel {
                team_id: Set(team.id),
                user_id: Set(member.id),
                role: Set(TeamMemberRole::Member),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("test member membership should insert");
        (team, membership)
    }

    #[tokio::test]
    async fn team_access_cache_hit_rechecks_membership_and_policy_group() {
        let state = build_cached_state().await;
        let owner = create_user(&state, "owner").await;
        let member = create_user(&state, "member").await;
        let first_group = create_policy_group_with_policy(&state, "first").await;
        let second_group = create_policy_group_with_policy(&state, "second").await;
        let (team, membership) =
            create_team_with_member(&state, &owner, &member, first_group.id).await;

        require_team_access(&state, team.id, member.id)
            .await
            .expect("first access should populate cache");

        let mut active = team.clone().into_active_model();
        active.policy_group_id = Set(Some(second_group.id));
        team_repo::update(state.writer_db(), active)
            .await
            .expect("team policy group should update");
        let policy_group_id = require_team_policy_group_id(&state, team.id, member.id)
            .await
            .expect("cached access should refresh policy group from database");
        assert_eq!(policy_group_id, second_group.id);

        team_member_repo::delete(state.writer_db(), membership.id)
            .await
            .expect("membership should delete");
        let error = require_team_access(&state, team.id, member.id)
            .await
            .expect_err("cached access must not authorize removed members");
        assert!(matches!(error, crate::errors::AsterError::AuthForbidden(_)));
    }

    #[tokio::test]
    async fn folder_path_cache_hit_rebuilds_path_from_database_names() {
        let state = build_cached_state().await;
        let user = create_user(&state, "folder-owner").await;
        let now = Utc::now();
        let parent = crate::db::repository::folder_repo::create(
            state.writer_db(),
            crate::entities::folder::ActiveModel {
                name: Set("Parent".to_string()),
                parent_id: Set(None),
                team_id: Set(None),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("parent folder should insert");
        let child = crate::db::repository::folder_repo::create(
            state.writer_db(),
            crate::entities::folder::ActiveModel {
                name: Set("Child".to_string()),
                parent_id: Set(Some(parent.id)),
                team_id: Set(None),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("child folder should insert");

        let first = folder::build_folder_paths_cached(&state, &[child.id])
            .await
            .expect("first path build should succeed");
        assert_eq!(
            first.get(&child.id).map(String::as_str),
            Some("/Parent/Child")
        );

        let mut active = parent.into_active_model();
        active.name = Set("Renamed".to_string());
        active.updated_at = Set(Utc::now());
        active
            .update(state.writer_db())
            .await
            .expect("parent should rename");

        let second = folder::build_folder_paths_cached(&state, &[child.id])
            .await
            .expect("cached path build should use current database names");
        assert_eq!(
            second.get(&child.id).map(String::as_str),
            Some("/Renamed/Child")
        );
    }

    #[tokio::test]
    async fn team_scope_access_cache_hit_rejects_archived_team() {
        let state = build_cached_state().await;
        let owner = create_user(&state, "archived-owner").await;
        let member = create_user(&state, "archived-member").await;
        let group = create_policy_group_with_policy(&state, "archived").await;
        let (team, _) = create_team_with_member(&state, &owner, &member, group.id).await;

        require_team_access(&state, team.id, member.id)
            .await
            .expect("first access should populate cache");

        let team_id = team.id;
        let mut active = team.into_active_model();
        active.archived_at = Set(Some(Utc::now()));
        team_repo::update(state.writer_db(), active)
            .await
            .expect("team should archive");

        let scope = WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: member.id,
        };
        let error = super::require_scope_access(&state, scope)
            .await
            .expect_err("cached access must not authorize archived teams");
        assert!(matches!(
            error,
            crate::errors::AsterError::RecordNotFound(_)
        ));
    }
}
