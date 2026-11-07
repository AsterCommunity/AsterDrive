//! Product authorization. Resource ownership, authentication and lock possession are distinct facts.
//!
//! Operation grants borrow the actual database connection used by the operation. They are never
//! serialized into tasks or cached as authority; a delayed publication obtains a fresh grant.

use sea_orm::ConnectionTrait;

use super::{WorkspaceStorageScope, ensure_active_file_scope, ensure_active_folder_scope};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{
    folder_repo, team_member_repo, team_repo, user_repo, webdav_account_repo,
};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use aster_drive_model::entities::{file, folder};

/// Stable product actions. The RBAC permission catalog can map these actions without teaching
/// protocol adapters how roles or workspace membership are represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceOperation {
    ReadMetadata,
    ReadContent,
    CreateFile,
    CreateFolder,
    UpdateMetadata,
    UpdateContent,
    Copy,
    Move,
    Delete,
    Restore,
    ManageLock,
    ManageWorkspace,
    RetryTask,
    CancelTask,
}

impl WorkspaceOperation {
    fn mutates(self) -> bool {
        !matches!(self, Self::ReadMetadata | Self::ReadContent)
    }
}

/// Restrictions carried across body receipt/provider staging, not an authorization decision.
#[derive(Clone, Default)]
pub(crate) struct WorkspaceAccessConstraints {
    root_folder_id: Option<i64>,
    webdav_credential: Option<WebdavCredentialBinding>,
    wopi_session: Option<WopiSessionBinding>,
}

#[derive(Clone)]
struct WopiSessionBinding {
    session_id: i64,
    session_version: i64,
    file_id: i64,
    /// PUT_RELATIVE 只派生到启动文件当时的父集合，不能扩成 workspace 任意写。
    relative_parent: Option<Option<i64>>,
}

#[derive(Clone)]
struct WebdavCredentialBinding {
    account_id: i64,
    verifier_fingerprint: String,
}

impl std::fmt::Debug for WorkspaceAccessConstraints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceAccessConstraints")
            .field("root_folder_id", &self.root_folder_id)
            .field(
                "webdav_account_id",
                &self.webdav_credential.as_ref().map(|c| c.account_id),
            )
            .field(
                "wopi_session_id",
                &self.wopi_session.as_ref().map(|s| s.session_id),
            )
            .finish()
    }
}

impl WorkspaceAccessConstraints {
    pub(crate) fn for_webdav(
        account_id: i64,
        verifier_fingerprint: String,
        root_folder_id: Option<i64>,
    ) -> Self {
        Self {
            root_folder_id,
            webdav_credential: Some(WebdavCredentialBinding {
                account_id,
                verifier_fingerprint,
            }),
            wopi_session: None,
        }
    }

    pub(crate) fn for_root(root_folder_id: Option<i64>) -> Self {
        Self {
            root_folder_id,
            webdav_credential: None,
            wopi_session: None,
        }
    }

    pub(in crate::services) fn for_wopi_session(
        session_id: i64,
        session_version: i64,
        file_id: i64,
    ) -> Self {
        Self {
            root_folder_id: None,
            webdav_credential: None,
            wopi_session: Some(WopiSessionBinding {
                session_id,
                session_version,
                file_id,
                relative_parent: None,
            }),
        }
    }

    pub(crate) fn for_relative_target(mut self, folder_id: Option<i64>) -> Self {
        if let Some(session) = self.wopi_session.as_mut() {
            session.relative_parent = Some(folder_id)
        }
        self
    }
}

/// An operation-specific, non-persistable proof. Its scope and restrictions cannot be rewritten.
pub(crate) struct AuthorizedWorkspaceAccess<'db, C: ConnectionTrait> {
    db: &'db C,
    scope: WorkspaceStorageScope,
    operation: WorkspaceOperation,
    root_folder_id: Option<i64>,
    wopi_session: Option<WopiSessionBinding>,
    constraints: WorkspaceAccessConstraints,
}

/// 新增文件的提交凭证同时绑定数据库事务和父集合。底层 record writer 不再接收
/// 可被替换的 actor/workspace/folder 参数，因此不能在校验 A 后偷偷写入 B。
pub(crate) struct AuthorizedFileCreation<'db, C: ConnectionTrait> {
    access: AuthorizedWorkspaceAccess<'db, C>,
    folder_id: Option<i64>,
}

/// token lifecycle 只能操作本次已验证的锁根，不能重新拿 token 全局选择另一个目标。
pub(crate) struct AuthorizedResourceLock<'db, C: ConnectionTrait> {
    access: AuthorizedWorkspaceAccess<'db, C>,
    lock: aster_drive_model::entities::resource_lock::Model,
}

impl<'db, C: ConnectionTrait> AuthorizedResourceLock<'db, C> {
    pub(crate) fn db(&self) -> &'db C {
        self.access.db
    }
    pub(crate) fn snapshot(&self) -> &aster_drive_model::entities::resource_lock::Model {
        &self.lock
    }
}

impl<'db, C: ConnectionTrait> AuthorizedFileCreation<'db, C> {
    pub(crate) fn db(&self) -> &'db C {
        self.access.db
    }
    pub(crate) fn scope(&self) -> WorkspaceStorageScope {
        self.access.scope
    }
    pub(crate) fn folder_id(&self) -> Option<i64> {
        self.folder_id
    }
}

impl<'db, C: ConnectionTrait> AuthorizedWorkspaceAccess<'db, C> {
    pub(crate) fn connection(&self) -> &'db C {
        self.db
    }
    pub(crate) fn scope(&self) -> WorkspaceStorageScope {
        self.scope
    }
    /// 读取 projection 消费受约束的资源凭证，而非可随意拼装的资源 owner ID。
    pub(crate) fn into_resource_read(self) -> super::AuthorizedResourceRead {
        super::AuthorizedResourceRead::user(self.scope, self.constraints)
    }

    pub(crate) async fn for_resource_lock(
        self,
        lock: aster_drive_model::entities::resource_lock::Model,
    ) -> Result<AuthorizedResourceLock<'db, C>> {
        match self.operation {
            WorkspaceOperation::ManageLock => {}
            _ => {
                return Err(scope_denied(
                    "operation does not permit lock lifecycle mutation",
                ));
            }
        }
        let namespace =
            crate::db::repository::lock_namespace_repo::find_by_id(self.db, lock.namespace_id)
                .await?
                .ok_or_else(|| scope_denied("lock namespace unavailable"))?;
        let expected = match self.scope {
            WorkspaceStorageScope::Personal { user_id } => {
                crate::services::files::lock::LockWorkspace::Personal { user_id }
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                crate::services::files::lock::LockWorkspace::Team { team_id }
            }
        }
        .persistence_key();
        if (namespace.workspace_type, namespace.workspace_id) != expected {
            return Err(scope_denied("lock belongs to another workspace"));
        }
        match crate::services::files::lock::LockRoot::from_model(&lock)? {
            crate::services::files::lock::LockRoot::WorkspaceRoot => self.parent(None).await?,
            crate::services::files::lock::LockRoot::File { file_id } => {
                let file = crate::db::repository::file_repo::find_by_id(self.db, file_id).await?;
                self.file(&file).await?;
            }
            crate::services::files::lock::LockRoot::Folder { folder_id } => {
                let folder = folder_repo::find_by_id(self.db, folder_id).await?;
                self.folder(&folder).await?;
            }
        }
        Ok(AuthorizedResourceLock { access: self, lock })
    }

    pub(crate) async fn for_file_creation(
        self,
        folder_id: Option<i64>,
        credentials: &crate::services::files::lock::SubmittedLockCredentials<'_>,
    ) -> Result<AuthorizedFileCreation<'db, C>> {
        match self.operation {
            WorkspaceOperation::CreateFile
            | WorkspaceOperation::Copy
            | WorkspaceOperation::UpdateContent => {}
            _ => return Err(scope_denied("operation does not permit file creation")),
        }
        self.collection(folder_id, credentials).await?;
        Ok(AuthorizedFileCreation {
            access: self,
            folder_id,
        })
    }

    /// 一个操作可修改多个集合（MOVE 的源/目标、批量 COPY 的多个目标）。
    /// 调用者按实际最终实体收集、排序、去重；本方法同时检查挂载范围和集合锁。
    pub(crate) async fn collection(
        &self,
        folder_id: Option<i64>,
        credentials: &crate::services::files::lock::SubmittedLockCredentials<'_>,
    ) -> Result<()> {
        self.parent(folder_id).await?;
        let workspace = match self.scope {
            WorkspaceStorageScope::Personal { user_id } => {
                crate::services::files::lock::LockWorkspace::Personal { user_id }
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                crate::services::files::lock::LockWorkspace::Team { team_id }
            }
        };
        // 新增文件修改的是父集合。只验证新文件或祖先 Infinity 锁会漏掉父集合本身的
        // Resource-depth 锁；这里与文件记录发布共享 transaction 和 namespace guard。
        crate::services::files::lock::enforce_collection_membership_mutation_on(
            self.db,
            workspace,
            folder_id,
            credentials,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn folder(&self, folder: &folder::Model) -> Result<()> {
        ensure_active_folder_scope(folder, self.scope)?;
        self.parent(Some(folder.id)).await
    }

    pub(crate) async fn file(&self, file: &file::Model) -> Result<()> {
        ensure_active_file_scope(file, self.scope)?;
        match &self.wopi_session {
            Some(session)
                if session.file_id != file.id
                    && session.relative_parent != Some(file.folder_id) =>
            {
                return Err(scope_denied(
                    "WOPI operation targets another file or collection",
                ));
            }
            _ => {}
        }
        self.parent(file.folder_id).await
    }

    /// Validate the current canonical ancestry, including the root itself and deleted ancestors.
    pub(crate) async fn parent(&self, folder_id: Option<i64>) -> Result<()> {
        match &self.wopi_session {
            Some(session)
                if session.relative_parent.is_some()
                    && session.relative_parent != Some(folder_id) =>
            {
                return Err(scope_denied(
                    "WOPI relative target is outside the authorized collection",
                ));
            }
            _ => {}
        }
        let (root_id, folder_id) = match (self.root_folder_id, folder_id) {
            (None, Some(folder_id)) => {
                let folder = folder_repo::find_by_id(self.db, folder_id).await?;
                ensure_active_folder_scope(&folder, self.scope)?;
                return Ok(());
            }
            (None, None) => return Ok(()),
            (Some(_), None) => {
                return Err(scope_denied(
                    "workspace root is outside the delegated folder",
                ));
            }
            (Some(root_id), Some(folder_id)) => (root_id, folder_id),
        };
        let root = folder_repo::find_by_id(self.db, root_id).await?;
        ensure_active_folder_scope(&root, self.scope)?;
        let ancestors = match self.scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_ancestor_models(self.db, user_id, folder_id).await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_team_ancestor_models(self.db, team_id, folder_id).await?
            }
        };
        let mut within_root = folder_id == root_id;
        for ancestor in ancestors {
            ensure_active_folder_scope(&ancestor, self.scope)?;
            within_root |= ancestor.id == root_id;
        }
        match within_root {
            true => Ok(()),
            false => Err(scope_denied("resource is outside the delegated folder")),
        }
    }
}

fn scope_denied(message: &str) -> AsterError {
    auth_forbidden_with_code(ApiErrorCode::WorkspaceScopeDenied, message)
}

/// 任务 creator/owner 只描述归属。手动重启或取消特权任务是一次新的管理操作，
/// 即使任务最初由同一人创建，也必须检查他当前仍具有对应管理权限。
pub(crate) async fn require_task_control_on<C: ConnectionTrait>(
    db: &C,
    actor_user_id: i64,
    kind: aster_drive_model::types::BackgroundTaskKind,
) -> Result<()> {
    use aster_drive_model::types::BackgroundTaskKind;
    let administrative = match kind {
        BackgroundTaskKind::StoragePolicyTempCleanup
        | BackgroundTaskKind::StoragePolicyMigration
        | BackgroundTaskKind::StoragePolicyForcedPurge
        | BackgroundTaskKind::BlobMaintenance => true,
        BackgroundTaskKind::ArchiveExtract
        | BackgroundTaskKind::ArchiveCompress
        | BackgroundTaskKind::ArchivePreviewGenerate
        | BackgroundTaskKind::ThumbnailGenerate
        | BackgroundTaskKind::ImagePreviewGenerate
        | BackgroundTaskKind::MediaMetadataExtract
        | BackgroundTaskKind::TrashPurgeAll
        | BackgroundTaskKind::FolderTreeMutation
        | BackgroundTaskKind::OfflineDownload => false,
        BackgroundTaskKind::SystemRuntime => {
            return Err(scope_denied(
                "runtime records cannot be controlled as file tasks",
            ));
        }
    };
    match administrative {
        false => Ok(()),
        true => {
            let actor = user_repo::find_by_id(db, actor_user_id).await?;
            match actor.status.is_active() && actor.role.is_admin() {
                true => Ok(()),
                false => Err(auth_forbidden_with_code(
                    ApiErrorCode::AuthAdminRequired,
                    "current administrator authority is required for this task kind",
                )),
            }
        }
    }
}

/// Check current product authority. Mutation callers use their writer transaction. Namespace
/// serialization precedes owner/actor/credential rows, matching file publication lock order.
pub(crate) async fn authorize_workspace_on<'db, C: ConnectionTrait>(
    db: &'db C,
    scope: WorkspaceStorageScope,
    operation: WorkspaceOperation,
    constraints: &WorkspaceAccessConstraints,
) -> Result<AuthorizedWorkspaceAccess<'db, C>> {
    let mutates = operation.mutates();
    if mutates {
        let workspace = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                crate::services::files::lock::LockWorkspace::Personal { user_id }
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                crate::services::files::lock::LockWorkspace::Team { team_id }
            }
        };
        crate::services::files::lock::lock_workspace_for_mutation_on(db, workspace).await?;
    }
    match scope {
        WorkspaceStorageScope::Personal { .. } => {}
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => {
            match mutates {
                true => {
                    team_repo::lock_active_by_id(db, team_id).await?;
                }
                false => {
                    team_repo::find_active_by_id(db, team_id).await?;
                }
            }
            // 持有 team guard 后再作 current/locking read，避免 MySQL RR 的旧快照。
            // 成员移除/归档持同一行锁，因此撤权与发布有确定的数据库先后顺序。
            match team_member_repo::find_by_team_and_user_for_authorization(
                db,
                team_id,
                actor_user_id,
                mutates,
            )
            .await?
            {
                Some(_) => {}
                None => {
                    return Err(auth_forbidden_with_code(
                        ApiErrorCode::TeamNotMember,
                        "not a member of this team",
                    ));
                }
            }
        }
    }
    // 保留操作/作用域的显式模式分支，后续 RBAC 权限矩阵在同一位置扩展。
    #[expect(
        clippy::single_match,
        reason = "explicit operation and workspace policy matrix"
    )]
    match (scope, operation) {
        (
            WorkspaceStorageScope::Team {
                team_id,
                actor_user_id,
            },
            WorkspaceOperation::ManageWorkspace,
        ) => {
            let role = team_member_repo::find_active_team_access(db, team_id, actor_user_id)
                .await?
                .ok_or_else(|| {
                    auth_forbidden_with_code(
                        ApiErrorCode::TeamNotMember,
                        "not a member of this team",
                    )
                })?
                .role;
            match role.can_manage_team() {
                true => {}
                false => {
                    return Err(auth_forbidden_with_code(
                        ApiErrorCode::TeamAdminOrOwnerRequired,
                        "team owner or administrator role is required",
                    ));
                }
            }
        }
        _ => {}
    }
    let actor = match mutates {
        true => user_repo::lock_by_id(db, scope.actor_user_id()).await?,
        false => user_repo::find_by_id(db, scope.actor_user_id()).await?,
    };
    match actor.status.is_active() {
        true => {}
        false => {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::AuthAccountDisabled,
                "account is disabled",
            ));
        }
    }
    match &constraints.webdav_credential {
        None => {}
        Some(binding) => {
            let account =
                webdav_account_repo::find_for_authorization(db, binding.account_id, mutates)
                    .await?
                    .ok_or_else(|| scope_denied("WebDAV credential revoked"))?;
            // 比较 verifier 指纹而非持有密码。改密/停用/改归属/改挂载范围后，旧请求
            // 不能在发布时默默换用新凭据的权限；缓存回填也不能恢复已撤销的资格。
            match account.is_active
                && account.user_id == scope.actor_user_id()
                && account.team_id == scope.team_id()
                && account.root_folder_id == constraints.root_folder_id
                && aster_forge_crypto::sha256_hex(account.password_hash.as_bytes())
                    == binding.verifier_fingerprint
            {
                true => {}
                false => return Err(scope_denied("WebDAV credential changed or revoked")),
            }
        }
    }
    match &constraints.wopi_session {
        None => {}
        Some(binding) => {
            let session = crate::db::repository::wopi_session_repo::find_for_authorization(
                db,
                binding.session_id,
                mutates,
            )
            .await?
            .ok_or_else(|| scope_denied("WOPI session revoked"))?;
            match session.actor_user_id == actor.id
                && session.team_id == scope.team_id()
                && session.file_id == binding.file_id
                && session.session_version == actor.session_version
                && session.session_version == binding.session_version
                && session.expires_at > chrono::Utc::now()
            {
                true => {}
                false => return Err(scope_denied("WOPI session changed or expired")),
            }
            if let Some(parent) = binding.relative_parent {
                let origin =
                    crate::db::repository::file_repo::find_by_id(db, binding.file_id).await?;
                ensure_active_file_scope(&origin, scope)?;
                match origin.folder_id == parent {
                    true => {}
                    false => {
                        return Err(scope_denied(
                            "WOPI source moved while receiving relative content",
                        ));
                    }
                }
            }
        }
    }
    Ok(AuthorizedWorkspaceAccess {
        db,
        scope,
        operation,
        root_folder_id: constraints.root_folder_id,
        wopi_session: constraints.wopi_session.clone(),
        constraints: constraints.clone(),
    })
}
