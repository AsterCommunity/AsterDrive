//! 只读资源授权与 actor 所有权分离。
//!
//! 管理员受控查看和分享均有自己的授权依据，不能为了调用文件 service 而伪造
//! workspace owner。此模块统一范围、生命周期及下游读取凭证；密码证明、协议验签、
//! 分享计数预留和管理员必要审计仍由各自用例拥有。

use super::{WorkspaceResourceScope, ensure_file_resource_scope, ensure_folder_resource_scope};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{
    admin_content_access_repo, auth_session_repo, config_repo, team_repo, user_repo,
};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::auth::local::Claims;
use crate::services::ops::content_access::{AccessPayload, AdminScopeKind, WorkspaceAccess};
use aster_drive_model::entities::{file, folder};
use sea_orm::ConnectionTrait;

/// 这些是目前真实存在的管理权限边界，#663 可替换此处的角色决策，不改协议调用方。
#[derive(Debug, Clone, Copy)]
pub(crate) enum AdminContentOperation {
    Inspect,
    Download,
}

#[derive(Debug, Clone)]
enum ReadAuthority {
    User {
        actor_user_id: i64,
        constraints: super::WorkspaceAccessConstraints,
    },
    Administrator {
        actor_user_id: i64,
        access_id: String,
    },
    Share {
        share_id: i64,
    },
}

/// 只能由产品授权模块构造。公共 projection 不再以一个可任意拼装的 workspace ID
/// 当成“调用者已授权”的证据；真实身份与资源归属始终分开。
pub(crate) struct AuthorizedResourceRead {
    scope: WorkspaceResourceScope,
    authority: ReadAuthority,
    root_folder_id: Option<i64>,
    file_id: Option<i64>,
    include_deleted: bool,
}

impl AuthorizedResourceRead {
    pub(super) fn user(
        scope: super::WorkspaceStorageScope,
        constraints: super::WorkspaceAccessConstraints,
    ) -> Self {
        Self {
            scope: scope.into(),
            authority: ReadAuthority::User {
                actor_user_id: scope.actor_user_id(),
                constraints,
            },
            root_folder_id: None,
            file_id: None,
            include_deleted: false,
        }
    }

    pub(crate) fn scope(&self) -> WorkspaceResourceScope {
        self.scope
    }

    pub(crate) fn allow_trash(mut self) -> Self {
        self.include_deleted = true;
        self
    }

    pub(crate) fn audit_actor(&self) -> Option<i64> {
        match &self.authority {
            ReadAuthority::User { actor_user_id, .. }
            | ReadAuthority::Administrator { actor_user_id, .. } => Some(*actor_user_id),
            ReadAuthority::Share { .. } => None,
        }
    }

    pub(crate) async fn file_on<C: ConnectionTrait>(
        &self,
        db: &C,
        file: &file::Model,
    ) -> Result<()> {
        // 资源凭证不是可长期复用的权限缓存；访问具体文件前仍拒绝已删除的 grant。
        match &self.authority {
            ReadAuthority::Administrator {
                actor_user_id,
                access_id,
            } => {
                let grant = admin_content_access_repo::find(db, access_id)
                    .await?
                    .ok_or_else(|| admin_denied(ApiErrorCode::AdminContentContextInvalid))?;
                match grant.actor_user_id == *actor_user_id && grant.expires_at > chrono::Utc::now()
                {
                    true => {}
                    false => return Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
                }
            }
            ReadAuthority::Share { share_id } => {
                crate::db::repository::share_repo::find_by_id(db, *share_id).await?;
            }
            ReadAuthority::User {
                actor_user_id,
                constraints,
            } => {
                let scope = match self.scope {
                    WorkspaceResourceScope::Personal { user_id } => {
                        super::WorkspaceStorageScope::Personal { user_id }
                    }
                    WorkspaceResourceScope::Team { team_id } => {
                        super::WorkspaceStorageScope::Team {
                            team_id,
                            actor_user_id: *actor_user_id,
                        }
                    }
                };
                let access = super::authorize_workspace_on(
                    db,
                    scope,
                    super::WorkspaceOperation::ReadMetadata,
                    constraints,
                )
                .await?;
                access.file(file).await?;
            }
        }
        // 内容 projection 可来自 reader，但 scope/parent/deleted 是授权事实，不能
        // 用副本上移动前的 Model 决定当前 root 范围。重新加载当前 writer 实体。
        let current = crate::db::repository::file_repo::find_by_id(db, file.id).await?;
        let file = &current;
        ensure_file_resource_scope(file, self.scope)?;
        match (self.file_id, self.include_deleted, file.deleted_at) {
            (Some(id), _, _) if id != file.id => {
                return Err(AsterError::auth_forbidden(
                    "file is outside the granted resource",
                ));
            }
            (_, false, Some(_)) => return Err(AsterError::file_not_found("file is in trash")),
            _ => {}
        }
        self.parent_on(db, file.folder_id).await
    }

    pub(crate) async fn folder_on<C: ConnectionTrait>(
        &self,
        db: &C,
        folder: &folder::Model,
    ) -> Result<()> {
        match &self.authority {
            ReadAuthority::User {
                actor_user_id,
                constraints,
            } => {
                let scope = match self.scope {
                    WorkspaceResourceScope::Personal { user_id } => {
                        super::WorkspaceStorageScope::Personal { user_id }
                    }
                    WorkspaceResourceScope::Team { team_id } => {
                        super::WorkspaceStorageScope::Team {
                            team_id,
                            actor_user_id: *actor_user_id,
                        }
                    }
                };
                let access = super::authorize_workspace_on(
                    db,
                    scope,
                    super::WorkspaceOperation::ReadMetadata,
                    constraints,
                )
                .await?;
                access.folder(folder).await?;
            }
            ReadAuthority::Administrator { .. } | ReadAuthority::Share { .. } => {}
        }
        let current = crate::db::repository::folder_repo::find_by_id(db, folder.id).await?;
        let folder = &current;
        ensure_folder_resource_scope(folder, self.scope)?;
        if let (false, Some(_)) = (self.include_deleted, folder.deleted_at) {
            return Err(AsterError::folder_not_found("folder is in trash"));
        }
        self.parent_on(db, Some(folder.id)).await
    }

    async fn parent_on<C: ConnectionTrait>(&self, db: &C, parent: Option<i64>) -> Result<()> {
        let root = match self.root_folder_id {
            Some(id) => id,
            None => return Ok(()),
        };
        let parent = parent
            .ok_or_else(|| AsterError::auth_forbidden("resource is outside the granted folder"))?;
        let chain = match self.scope {
            WorkspaceResourceScope::Personal { user_id } => {
                crate::db::repository::folder_repo::find_ancestor_models(db, user_id, parent)
                    .await?
            }
            WorkspaceResourceScope::Team { team_id } => {
                crate::db::repository::folder_repo::find_team_ancestor_models(db, team_id, parent)
                    .await?
            }
        };
        match (parent == root || chain.iter().any(|folder| folder.id == root))
            && chain
                .iter()
                .all(|folder| self.include_deleted || folder.deleted_at.is_none())
        {
            true => Ok(()),
            false => Err(AsterError::auth_forbidden(
                "resource is outside the granted folder",
            )),
        }
    }
}

pub(crate) struct AuthorizedAdminContentAccess {
    pub(crate) read: AuthorizedResourceRead,
    payload: AccessPayload,
}

impl AuthorizedAdminContentAccess {
    pub(crate) fn workspace(&self) -> Result<&WorkspaceAccess> {
        match &self.payload {
            AccessPayload::Workspace(access) => Ok(access),
            _ => Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
        }
    }
    pub(crate) fn payload(&self) -> &AccessPayload {
        &self.payload
    }
}

fn admin_denied(code: ApiErrorCode) -> AsterError {
    auth_forbidden_with_code(code, "administrator content access denied")
}

/// 每次查看/能力兑换都重新核对当前管理员与 session，不能让 access_id 成为永久
/// 文件访问权。此检查故意使用 writer，不读取配置/认证缓存的旧资格。
pub(crate) async fn authorize_admin_actor(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    operation: AdminContentOperation,
) -> Result<()> {
    let enabled = config_repo::find_by_key(
        state.writer_db(),
        crate::config::definitions::ADMIN_WORKSPACE_CONTENT_ACCESS_ENABLED_KEY,
    )
    .await?
    .and_then(|entry| crate::config::bool_like::parse_bool_like(&entry.value))
    .unwrap_or(false);
    match enabled {
        true => {}
        false => return Err(admin_denied(ApiErrorCode::AdminContentDisabled)),
    }
    let actor = user_repo::find_by_id(state.writer_db(), claims.user_id).await?;
    let permits = match operation {
        AdminContentOperation::Inspect | AdminContentOperation::Download => actor.role.is_admin(),
    };
    match permits
        && actor.status.is_active()
        && actor.session_version == claims.session_version
        && !claims.password_change
    {
        true => {}
        false => return Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
    }
    let session =
        auth_session_repo::find_by_id_for_user(state.writer_db(), claims.user_id, &claims.sid)
            .await?
            .ok_or_else(|| admin_denied(ApiErrorCode::AdminContentContextInvalid))?;
    match session.revoked_at.is_none() && session.refresh_expires_at > chrono::Utc::now() {
        true => Ok(()),
        false => Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
    }
}

pub(crate) async fn authorize_admin_content_access(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    id: &str,
    operation: AdminContentOperation,
) -> Result<AuthorizedAdminContentAccess> {
    authorize_admin_actor(state, claims, operation).await?;
    match id.len() {
        0 => return Err(admin_denied(ApiErrorCode::AdminContentContextRequired)),
        1..=64 => {}
        _ => return Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
    }
    let grant = admin_content_access_repo::find(state.writer_db(), id)
        .await?
        .ok_or_else(|| admin_denied(ApiErrorCode::AdminContentContextInvalid))?;
    match grant.actor_user_id == claims.user_id
        && grant.session_id == claims.sid
        && grant.expires_at > chrono::Utc::now()
    {
        true => {}
        false => return Err(admin_denied(ApiErrorCode::AdminContentContextInvalid)),
    }
    let payload: AccessPayload = serde_json::from_str(&grant.payload)
        .map_err(|_| admin_denied(ApiErrorCode::AdminContentContextInvalid))?;
    let scope = payload.scope();
    let resource = match scope.scope_kind {
        AdminScopeKind::Personal => WorkspaceResourceScope::Personal {
            user_id: scope.target_id,
        },
        AdminScopeKind::Team => WorkspaceResourceScope::Team {
            team_id: scope.target_id,
        },
    };
    authorize_resource_lifecycle_on(state.writer_db(), resource)
        .await
        .map_err(|error| match error {
            AsterError::RecordNotFound(_) | AsterError::AuthForbidden(_) => {
                admin_denied(ApiErrorCode::AdminContentTargetUnavailable)
            }
            error => error,
        })?;
    let file_id = match &payload {
        AccessPayload::Download { file_id, .. } => Some(*file_id),
        _ => None,
    };
    Ok(AuthorizedAdminContentAccess {
        read: AuthorizedResourceRead {
            scope: resource,
            authority: ReadAuthority::Administrator {
                actor_user_id: claims.user_id,
                access_id: id.to_owned(),
            },
            root_folder_id: None,
            file_id,
            include_deleted: true,
        },
        payload,
    })
}

/// 生命周期事实是资源授权规则，管理员查看与公开分享都使用同一个检查。
pub(crate) async fn authorize_resource_lifecycle_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceResourceScope,
) -> Result<()> {
    match scope {
        WorkspaceResourceScope::Personal { user_id } => {
            match user_repo::find_by_id(db, user_id).await?.status.is_active() {
                true => Ok(()),
                false => Err(admin_denied(ApiErrorCode::AdminContentTargetUnavailable)),
            }
        }
        WorkspaceResourceScope::Team { team_id } => {
            team_repo::find_active_by_id(db, team_id).await?;
            Ok(())
        }
    }
}

/// 分享 adapter 在验证密码/公开能力后调用。该凭证只允许读取 share 指定的文件或
/// 当前目录子树，不继承分享创建者的其他权限，也不要求匿名访问者伪造一个 actor。
pub(crate) async fn authorize_share_resources_on<C: ConnectionTrait>(
    db: &C,
    share: &aster_drive_model::entities::share::Model,
) -> Result<AuthorizedResourceRead> {
    let current = crate::db::repository::share_repo::find_by_id(db, share.id).await?;
    match current.token == share.token
        && current.password == share.password
        && current.team_id == share.team_id
        && current.user_id == share.user_id
        && current.file_id == share.file_id
        && current.folder_id == share.folder_id
    {
        true => {}
        false => {
            return Err(AsterError::share_not_found(
                "share changed while authorizing resources",
            ));
        }
    }
    if current
        .expires_at
        .is_some_and(|expiry| expiry < chrono::Utc::now())
    {
        return Err(AsterError::share_expired("share has expired"));
    }
    let scope = match share.team_id {
        Some(team_id) => WorkspaceResourceScope::Team { team_id },
        None => WorkspaceResourceScope::Personal {
            user_id: share.user_id,
        },
    };
    match scope {
        WorkspaceResourceScope::Team { .. } => authorize_resource_lifecycle_on(db, scope).await?,
        WorkspaceResourceScope::Personal { .. } => {}
    }
    Ok(AuthorizedResourceRead {
        scope,
        authority: ReadAuthority::Share { share_id: share.id },
        root_folder_id: share.folder_id,
        file_id: share.file_id,
        include_deleted: false,
    })
}
