//! WebDAV 子模块：`db_lock_system`。

use aster_forge_db::transaction;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use aster_forge_webdav::DavXmlElement;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, IntoActiveModel, Set};

use crate::config::webdav;
use crate::db::repository::{
    file_repo, folder_repo, lock_namespace_repo, lock_repo, team_repo, user_repo,
};
use crate::runtime::PrimaryAppState;
use crate::services::files::lock::{
    LockAcquireCommand, LockMutationCredentials, LockRoot, LockTarget, LockWorkspace,
    acquire_after_namespace_lock_on,
};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::workspace::storage::{
    EmptyFileNameMode, PreparedEmptyFile, WorkspaceStorageScope,
};
use crate::webdav::backend::path_resolver::{self, ResolvedNode};
use aster_drive_model::entities::{resource_lock, resource_lock_namespace};
use aster_drive_model::types::{
    EntityType, LockDepth, LockMode, LockOrigin, ResourceLockTargetType,
};
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavLock, DavLockAcquireRequest, DavLockAcquireResult,
    DavLockError, DavLockPreflightError, DavLockSystem, DavPath, FsError, LsFuture, encode_href,
    href_for_relative, submitted_lock_tokens,
};

mod canonical;
pub(crate) use canonical::submitted_credentials_on;

#[derive(Debug)]
enum LockAcquireTransactionError {
    TargetBecameMissing,
    LimitExceeded,
    Product(crate::errors::AsterError),
}

impl From<crate::errors::AsterError> for LockAcquireTransactionError {
    fn from(error: crate::errors::AsterError) -> Self {
        Self::Product(error)
    }
}

impl From<aster_forge_db::DbError> for LockAcquireTransactionError {
    fn from(error: aster_forge_db::DbError) -> Self {
        Self::Product(error.into())
    }
}

impl std::fmt::Display for LockAcquireTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetBecameMissing => formatter.write_str("WebDAV LOCK target became missing"),
            Self::LimitExceeded => formatter.write_str("WebDAV active lock limit exceeded"),
            Self::Product(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

/// 数据库支持的 WebDAV 锁系统
///
/// Per-request 创建（需要 user_id 做 path → entity_id 解析）
#[derive(Clone)]
pub struct DbLockSystem {
    state: PrimaryAppState,
    scope: WorkspaceStorageScope,
    access_constraints: crate::services::workspace::scope::WorkspaceAccessConstraints,
    root_folder_id: Option<i64>,
    audit_ctx: AuditContext,
    #[cfg(test)]
    lock_transaction_test_barrier:
        std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<tokio::sync::Barrier>>>>,
}

impl DbLockSystem {
    pub fn new(state: PrimaryAppState, user_id: i64, root_folder_id: Option<i64>) -> Box<Self> {
        Box::new(Self {
            state,
            scope: WorkspaceStorageScope::Personal { user_id },
            access_constraints:
                crate::services::workspace::scope::WorkspaceAccessConstraints::for_root(
                    root_folder_id,
                ),
            root_folder_id,
            audit_ctx: AuditContext {
                user_id,
                ip_address: None,
                user_agent: None,
            },
            #[cfg(test)]
            lock_transaction_test_barrier: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub(crate) fn new_with_audit(
        state: PrimaryAppState,
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        audit_ctx: AuditContext,
    ) -> Box<Self> {
        Box::new(Self {
            state,
            scope,
            access_constraints:
                crate::services::workspace::scope::WorkspaceAccessConstraints::for_root(
                    root_folder_id,
                ),
            root_folder_id,
            audit_ctx,
            #[cfg(test)]
            lock_transaction_test_barrier: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub(crate) fn with_access_constraints(
        mut self: Box<Self>,
        constraints: crate::services::workspace::scope::WorkspaceAccessConstraints,
    ) -> Box<Self> {
        self.access_constraints = constraints;
        self
    }

    /// 先限定请求 workspace，再以 canonical target 求交集，最后才投影响应路径。
    /// 挂载外祖先的深度锁仍约束写入，但不能借 discovery 泄露其 token。
    async fn matching_locks(
        &self,
        path: &DavPath,
        deep: bool,
        visible_only: bool,
    ) -> Result<Vec<resource_lock::Model>, DavBackendError> {
        let db = self.state.writer_db();
        crate::services::workspace::scope::authorize_workspace_on(
            db,
            self.scope,
            crate::services::workspace::scope::WorkspaceOperation::ReadMetadata,
            &self.access_constraints,
        )
        .await
        .map_err(|error| match error {
            crate::errors::AsterError::AuthForbidden(_) => {
                DavBackendError::new(DavBackendErrorKind::Forbidden)
            }
            _ => DavBackendError::new(DavBackendErrorKind::Internal),
        })?;
        let namespace = match find_lock_namespace(db, self.scope)
            .await
            .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
        {
            Some(namespace) => namespace,
            None => return Ok(Vec::new()),
        };
        let initial = resolve_path_to_entity(db, self.scope, self.root_folder_id, path)
            .await
            .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?;
        let missing_target = matches!(initial, LockPathTarget::Missing);
        let resolved = match initial {
            LockPathTarget::Missing => match path.parent() {
                Some(parent) => {
                    resolve_path_to_entity(db, self.scope, self.root_folder_id, &parent)
                        .await
                        .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
                }
                None => LockPathTarget::Root,
            },
            target => target,
        };
        let target = webdav_lock_target(self.scope, self.root_folder_id, resolved, deep);
        let mut result = Vec::new();
        for lock in lock_repo::find_all_by_namespace(db, namespace.id)
            .await
            .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
        {
            if lock.timeout_at.is_some_and(|expires| expires <= Utc::now()) {
                continue;
            }
            // 不存在子资源上的 If 状态不能直接借父集合 Resource 锁变成 true。
            // 父集合的 membership 另作目标检查；只有 Infinity 锁继承到此 Request-URI。
            if missing_target && lock.depth == LockDepth::Resource {
                continue;
            }
            if !crate::services::files::lock::lock_overlaps_target_on(db, &lock, target)
                .await
                .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
            {
                continue;
            }
            match canonical::project_lock_on(db, self.scope, self.root_folder_id, &lock)
                .await
                .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
            {
                Some(lock) => result.push(lock),
                None => match visible_only {
                    true => {}
                    // 不可见祖先依然拒绝 mutation；冲突响应不披露挂载外的真实路径/token。
                    false => result.push(resource_lock::Model {
                        token: String::new(),
                        lockroot_path: Some("/".to_owned()),
                        ..lock
                    }),
                },
            }
        }
        result.sort_by_key(|lock| lock.id);
        Ok(result)
    }

    /// 授权与 token 更新共享 writer transaction。token 字符串本身不授予访问别的
    /// workspace/挂载根的权力，也不允许用一次事务外预检替代最终绑定。
    async fn mutate_token(
        &self,
        path: &DavPath,
        token: &str,
        mutation: crate::services::files::lock::AuthorizedLockMutation,
    ) -> Result<resource_lock::Model, DavLockError> {
        let txn = transaction::begin(self.state.writer_db())
            .await
            .map_err(|_| DavLockError::Backend)?;
        let access = crate::services::workspace::scope::authorize_workspace_on(
            &txn,
            self.scope,
            crate::services::workspace::scope::WorkspaceOperation::ManageLock,
            &self.access_constraints,
        )
        .await
        .map_err(|error| match error {
            crate::errors::AsterError::AuthForbidden(_)
            | crate::errors::AsterError::RecordNotFound(_)
            | crate::errors::AsterError::FileNotFound(_)
            | crate::errors::AsterError::FolderNotFound(_) => DavLockError::TokenMismatch,
            error => {
                tracing::warn!(error = %error, "lock authorization backend failed");
                DavLockError::Backend
            }
        })?;
        let current = lock_repo::find_by_token(&txn, token)
            .await
            .map_err(|_| DavLockError::Backend)?
            .ok_or(DavLockError::TokenMismatch)?;
        let grant =
            access
                .for_resource_lock(current.clone())
                .await
                .map_err(|error| match error {
                    crate::errors::AsterError::AuthForbidden(_)
                    | crate::errors::AsterError::RecordNotFound(_)
                    | crate::errors::AsterError::FileNotFound(_)
                    | crate::errors::AsterError::FolderNotFound(_) => DavLockError::TokenMismatch,
                    error => {
                        tracing::warn!(error = %error, "lock authorization backend failed");
                        DavLockError::Backend
                    }
                })?;
        let projected = canonical::project_lock_on(&txn, self.scope, self.root_folder_id, &current)
            .await
            .map_err(|_| DavLockError::Backend)?
            .ok_or(DavLockError::TokenMismatch)?;
        if !unlock_request_targets_lock_scope(
            projected.path(),
            projected.deep(),
            &normalize_path(path),
        ) {
            return Err(DavLockError::TokenMismatch);
        }
        let lock = crate::services::files::lock::mutate_authorized_lock_on(grant, mutation)
            .await
            .map_err(|_| DavLockError::Backend)?;
        transaction::commit(txn)
            .await
            .map_err(|_| DavLockError::Backend)?;
        Ok(lock)
    }

    #[cfg(test)]
    pub(crate) fn set_lock_transaction_test_barrier(
        &mut self,
        barrier: std::sync::Arc<tokio::sync::Barrier>,
    ) {
        *self
            .lock_transaction_test_barrier
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(barrier);
    }

    async fn log_lock_action(&self, entity_type: EntityType, entity_id: i64, locked: bool) {
        let state = &self.state;
        let action = match (entity_type, locked) {
            (EntityType::File, true) => audit::AuditAction::FileLock,
            (EntityType::File, false) => audit::AuditAction::FileUnlock,
            (EntityType::Folder, true) => audit::AuditAction::FolderLock,
            (EntityType::Folder, false) => audit::AuditAction::FolderUnlock,
        };
        match entity_type {
            EntityType::File => {
                match file_repo::find_by_id(self.state.writer_db(), entity_id).await {
                    Ok(file) => {
                        let details =
                            crate::services::files::file::audit_location_details_for_model(
                                state, self.scope, &file,
                            )
                            .await;
                        audit::log_with_details(
                            state,
                            &self.audit_ctx,
                            action,
                            audit::AuditEntityType::File,
                            Some(entity_id),
                            Some(&file.name),
                            || details.clone(),
                        )
                        .await;
                    }
                    Err(error) => {
                        tracing::warn!(
                            entity_id,
                            "failed to load WebDAV file lock audit target: {error}"
                        );
                        audit::log_with_details(
                            state,
                            &self.audit_ctx,
                            action,
                            audit::AuditEntityType::File,
                            Some(entity_id),
                            None,
                            || None,
                        )
                        .await;
                    }
                }
            }
            EntityType::Folder => {
                match folder_repo::find_by_id(self.state.writer_db(), entity_id).await {
                    Ok(folder) => {
                        let details =
                            crate::services::files::folder::audit_location_details_for_model(
                                state, self.scope, &folder,
                            )
                            .await;
                        audit::log_with_details(
                            state,
                            &self.audit_ctx,
                            action,
                            audit::AuditEntityType::Folder,
                            Some(entity_id),
                            Some(&folder.name),
                            || details.clone(),
                        )
                        .await;
                    }
                    Err(error) => {
                        tracing::warn!(
                            entity_id,
                            "failed to load WebDAV folder lock audit target: {error}"
                        );
                        audit::log_with_details(
                            state,
                            &self.audit_ctx,
                            action,
                            audit::AuditEntityType::Folder,
                            Some(entity_id),
                            None,
                            || None,
                        )
                        .await;
                    }
                }
            }
        }
    }

    fn max_active_locks_per_owner(&self) -> u64 {
        webdav::max_active_locks_per_user(self.state.runtime_config())
    }

    async fn ensure_lock_quota<C: ConnectionTrait>(
        &self,
        db: &C,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), DavLockPreflightError> {
        let owner_id = self.scope.actor_user_id();
        let max_active_locks = self.max_active_locks_per_owner();
        user_repo::lock_by_id(db, owner_id).await.map_err(|error| {
            tracing::warn!(
                owner_id,
                error = %error,
                "failed to lock WebDAV lock owner row"
            );
            DavLockPreflightError::GeneralFailure
        })?;
        let active_locks = lock_repo::count_active_by_owner(db, owner_id, now)
            .await
            .map_err(|error| {
                tracing::warn!(
                    owner_id,
                    error = %error,
                    "failed to count active WebDAV locks for owner"
                );
                DavLockPreflightError::GeneralFailure
            })?;
        if active_locks >= max_active_locks {
            tracing::warn!(
                owner_id,
                active_locks,
                max_active_locks,
                "WebDAV active lock limit exceeded"
            );
            return Err(DavLockPreflightError::LimitExceeded);
        }
        Ok(())
    }
}

impl DavLockSystem for DbLockSystem {
    fn prepare_lock(&self, _path: &DavPath) -> LsFuture<'_, Result<(), DavLockPreflightError>> {
        Box::pin(async move {
            self.ensure_lock_quota(self.state.writer_db(), Utc::now())
                .await
        })
    }

    fn lock(
        &self,
        request: DavLockAcquireRequest<'_>,
    ) -> LsFuture<'_, Result<DavLockAcquireResult, DavLockError>> {
        let DavLockAcquireRequest {
            path,
            principal,
            owner,
            timeout,
            shared,
            deep,
            credentials,
        } = request;
        let path_str = normalize_path(path);
        let path_owned = path.clone();
        let principal_owned = principal.map(|s| s.to_string());
        let owner_clone = owner.cloned();
        let timeout_dur = timeout;

        Box::pin(async move {
            let owner_xml = owner_clone
                .as_ref()
                .map(serialize_element)
                .transpose()
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to serialize WebDAV lock owner XML");
                    DavLockError::Backend
                })?;
            let owner_info = owner_xml.map(|xml| {
                crate::services::files::lock::ResourceLockOwnerInfo::Webdav(
                    crate::services::files::lock::WebdavLockOwnerInfo { xml },
                )
            });
            let timeout = timeout_dur
                .map(chrono::Duration::from_std)
                .transpose()
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "invalid WebDAV lock timeout");
                    DavLockError::Backend
                })?;
            let initially_resolved = resolve_path_to_entity(
                self.state.writer_db(),
                self.scope,
                self.root_folder_id,
                &path_owned,
            )
            .await
            .map_err(|error| {
                tracing::warn!(error = ?error, path = %path_str, "failed to resolve WebDAV lock target");
                DavLockError::Backend
            })?;
            let mut prepared_empty = if path_owned.is_collection() {
                if initially_resolved == LockPathTarget::Missing {
                    return Err(DavLockError::NotFound);
                }
                None
            } else if initially_resolved == LockPathTarget::Missing {
                let (parent_id, filename) = path_resolver::resolve_parent_in_scope(
                    self.state.writer_db(),
                    self.scope,
                    &path_owned,
                    self.root_folder_id,
                )
                .await
                .map_err(|error| match error {
                    FsError::NotFound => DavLockError::ParentMissing,
                    error => {
                        tracing::warn!(error = ?error, path = %path_str, "failed to resolve WebDAV lock-null parent");
                        DavLockError::Backend
                    }
                })?;
                Some(
                    PreparedEmptyFile::prepare(
                        self.scope,
                        parent_id,
                        &filename,
                        EmptyFileNameMode::Exact,
crate::services::workspace::activity::ActivitySource::Webdav,
)
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to stage WebDAV lock-null resource");
                        DavLockError::Backend
                    })?,
                )
            } else {
                None
            };

            let workspace = lock_workspace(self.scope);
            let (model, target, created) = loop {
                let transaction_result =
                    transaction::with_transaction(self.state.writer_db(), async |txn| {
                    #[cfg(test)]
                    let lock_transaction_test_barrier = self
                        .lock_transaction_test_barrier
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .take();
                    #[cfg(test)]
                    if let Some(barrier) = lock_transaction_test_barrier {
                        barrier.wait().await;
                    }
                    let namespace = crate::services::files::lock::lock_workspace_for_mutation_on(
                        txn, workspace,
                    )
                    .await?;
                    crate::services::workspace::scope::authorize_workspace_on(
                        txn, self.scope, crate::services::workspace::scope::WorkspaceOperation::ManageLock, &self.access_constraints,
                    ).await?;
                    self.ensure_lock_quota(txn, Utc::now())
                        .await
                        .map_err(|error| match error {
                            DavLockPreflightError::LimitExceeded => {
                                LockAcquireTransactionError::LimitExceeded
                            }
                            DavLockPreflightError::GeneralFailure => {
                                LockAcquireTransactionError::Product(
                                    crate::errors::AsterError::internal_error(
                                        "failed to enforce WebDAV lock quota",
                                    ),
                                )
                            }
                        })?;
                    let resolved =
                        resolve_path_to_entity(txn, self.scope, self.root_folder_id, &path_owned)
                            .await
                            .map_err(|error| {
                                crate::errors::AsterError::internal_error(format!(
                                    "failed to re-resolve WebDAV lock target: {error:?}"
                                ))
                            })?;
                    let (target, created) = match resolved {
                        LockPathTarget::Missing => {
                            let Some(prepared) = prepared_empty.as_ref() else {
                                return Err(LockAcquireTransactionError::TargetBecameMissing);
                            };
                            let submitted = LockMutationCredentials::SubmittedTokens(
                                credentials.submitted_lock_tokens.clone(),
                            );
                            crate::services::workspace::storage::lock_storage_usage(
                                txn, self.scope,
                            )
                            .await
                            .map_err(LockAcquireTransactionError::from)?;
                            crate::services::files::lock::enforce_collection_membership_mutation_on(
                                txn,
                                workspace,
                                prepared.folder_id(),
                                &submitted.submitted(),
                            )
                            .await
                            .map_err(LockAcquireTransactionError::from)?;
                            let prepared = prepared
                                .resolve_policy_on(&self.state, txn)
                                .await
                                .map_err(LockAcquireTransactionError::from)?;
                            let blob = prepared
                                .persist_blob_on(txn)
                                .await
                                .map_err(LockAcquireTransactionError::from)?;
                            let created = prepared
                                .create_file_on(txn, &blob, &submitted.submitted())
                                .await
                                .map_err(LockAcquireTransactionError::from)?;
                            (
                                LockTarget {
                                    workspace,
                                    root: LockRoot::File {
                                        file_id: created.id,
                                    },
                                    depth: if deep {
                                        LockDepth::Infinity
                                    } else {
                                        LockDepth::Resource
                                    },
                                },
                                Some(created),
                            )
                        }
                        resolved => (
                            webdav_lock_target(self.scope, self.root_folder_id, resolved, deep),
                            None,
                        ),
                    };
                    let model = acquire_after_namespace_lock_on(
                        txn,
                        namespace,
                        LockAcquireCommand {
                            target,
                            mode: if shared {
                                LockMode::Shared
                            } else {
                                LockMode::Exclusive
                            },
                            origin: LockOrigin::WebDav,
                            holder_user_id: Some(self.scope.actor_user_id()),
                            owner_info: owner_info.clone(),
                            timeout,
                            presentation_path: Some(path_str.clone()),
                        },
                    )
                    .await
                    .map_err(LockAcquireTransactionError::from)?;
                    Ok::<_, LockAcquireTransactionError>((model, target, created))
                    })
                    .await;
                match transaction_result {
                    Ok(result) => break result,
                    Err(LockAcquireTransactionError::TargetBecameMissing) => {
                        if path_owned.is_collection() {
                            return Err(DavLockError::NotFound);
                        }
                        let (parent_id, filename) = path_resolver::resolve_parent_in_scope(
                            self.state.writer_db(),
                            self.scope,
                            &path_owned,
                            self.root_folder_id,
                        )
                        .await
                        .map_err(|error| match error {
                            FsError::NotFound => DavLockError::ParentMissing,
                            error => {
                                tracing::warn!(error = ?error, path = %path_str, "failed to resolve raced WebDAV lock-null parent");
                                DavLockError::Backend
                            }
                        })?;
                        prepared_empty = Some(
                            PreparedEmptyFile::prepare(
                                self.scope,
                                parent_id,
                                &filename,
                                EmptyFileNameMode::Exact,
crate::services::workspace::activity::ActivitySource::Webdav,
)
                            .map_err(|error| {
                                tracing::warn!(error = %error, path = %path_str, "failed to stage raced WebDAV lock-null resource");
                                DavLockError::Backend
                            })?,
                        );
                    }
                    Err(LockAcquireTransactionError::LimitExceeded) => {
                        return Err(DavLockError::LimitExceeded);
                    }
                    Err(LockAcquireTransactionError::Product(error)) => {
                        if matches!(error, crate::errors::AsterError::ResourceLocked(_)) {
                            // 冲突响应也必须遵守 canonical namespace/root 可见性，
                            // 不能在错误分支退回历史 path 查询而泄露另一个挂载的 token。
                            let conflict =
                                match self.matching_locks(&path_owned, deep, false).await {
                                    Ok(locks) => locks.first().map(model_to_dav_lock),
                                    Err(_) => None,
                                }
                                .unwrap_or_else(|| {
                                    dav_lock_conflict_for_request(&path_owned, shared, deep)
                                });
                            return Err(DavLockError::Conflict(Box::new(conflict)));
                        }
                        tracing::warn!(error = %error, path = %path_str, "failed to acquire WebDAV lock");
                        return Err(DavLockError::Backend);
                    }
                }
            };
            if let Some(created) = &created {
                if let Some(prepared) = &prepared_empty {
                    prepared.publish_created(&self.state, created);
                } else {
                    tracing::warn!(
                        file_id = created.id,
                        path = %path_str,
                        "committed WebDAV lock-null resource has no prepared storage event context"
                    );
                }
            }
            if let Some((entity_type, entity_id)) = target.root.entity() {
                self.log_lock_action(entity_type, entity_id, true).await;
            }
            Ok(DavLockAcquireResult {
                lock: DavLock {
                    token: model.token,
                    path: Box::new(path_owned),
                    principal: principal_owned,
                    owner: owner_clone.map(Box::new),
                    timeout_at: timeout_dur.map(|duration| SystemTime::now() + duration),
                    timeout: timeout_dur,
                    shared,
                    deep,
                },
                resource_existed: created.is_none(),
            })
        })
    }

    fn unlock(&self, path: &DavPath, token: &str) -> LsFuture<'_, Result<(), DavLockError>> {
        let path = path.clone();
        let token = token.to_owned();
        Box::pin(async move {
            let lock = self
                .mutate_token(
                    &path,
                    &token,
                    crate::services::files::lock::AuthorizedLockMutation::Release,
                )
                .await?;
            if let (Some(kind), Some(id)) = (lock.entity_type(), lock.entity_id()) {
                self.log_lock_action(kind, id, false).await
            }
            Ok(())
        })
    }

    fn refresh(
        &self,
        path: &DavPath,
        token: &str,
        timeout: Option<Duration>,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
        let path = path.clone();
        let token = token.to_owned();
        Box::pin(async move {
            let timeout_at =
                lock_timeout_at(Utc::now(), timeout).map_err(|_| DavLockError::Backend)?;
            let lock = self
                .mutate_token(
                    &path,
                    &token,
                    crate::services::files::lock::AuthorizedLockMutation::Refresh { timeout_at },
                )
                .await?;
            let mut result = model_to_dav_lock(&lock);
            result.path = Box::new(path);
            result.timeout = timeout;
            Ok(result)
        })
    }

    fn check(
        &self,
        path: &DavPath,
        _principal: Option<&str>,
        _ignore_principal: bool,
        deep: bool,
        submitted_tokens: &[String],
    ) -> LsFuture<'_, Result<(), DavLockError>> {
        let path = path.clone();
        let tokens = submitted_tokens.to_vec();
        Box::pin(async move {
            let locks = self
                .matching_locks(&path, deep, false)
                .await
                .map_err(|_| DavLockError::Backend)?;
            for lock in &locks {
                let satisfied = locks.iter().any(|candidate| {
                    same_lock_root(lock, candidate)
                        && !candidate.token.is_empty()
                        && tokens.contains(&candidate.token)
                });
                match satisfied {
                    true => {}
                    false => return Err(DavLockError::Conflict(Box::new(model_to_dav_lock(lock)))),
                }
            }
            Ok(())
        })
    }

    fn discover(&self, path: &DavPath) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        let path = path.clone();
        Box::pin(async move {
            Ok(self
                .matching_locks(&path, false, true)
                .await?
                .iter()
                .map(model_to_dav_lock)
                .collect())
        })
    }

    fn discover_many<'a>(
        &'a self,
        paths: &'a [DavPath],
    ) -> LsFuture<'a, Result<HashMap<DavPath, Vec<DavLock>>, DavBackendError>> {
        Box::pin(async move {
            let db = self.state.writer_db();
            crate::services::workspace::scope::authorize_workspace_on(
                db,
                self.scope,
                crate::services::workspace::scope::WorkspaceOperation::ReadMetadata,
                &self.access_constraints,
            )
            .await
            .map_err(|error| match error {
                crate::errors::AsterError::AuthForbidden(_) => {
                    DavBackendError::new(DavBackendErrorKind::Forbidden)
                }
                _ => DavBackendError::new(DavBackendErrorKind::Internal),
            })?;
            let locks = match find_lock_namespace(db, self.scope)
                .await
                .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
            {
                Some(namespace) => {
                    canonical::projected_locks_on(db, namespace.id, self.scope, self.root_folder_id)
                        .await
                        .map_err(|_| DavBackendError::new(DavBackendErrorKind::Internal))?
                }
                None => Vec::new(),
            };
            // Parse each owner once, then reserve the complete materialized projection
            // before cloning. Source XML bytes underestimate a wide tree's heap cost.
            let mut remaining_metadata = LOCK_DISCOVERY_METADATA_BUDGET;
            let mut materialized = Vec::new();
            for lock in &locks {
                if !paths.iter().any(|path| {
                    lock_paths_overlap(lock.path(), lock.deep(), &normalize_path(path), false)
                }) {
                    continue;
                }
                let projected = model_to_dav_lock(lock);
                let cost = dav_lock_allocation_cost(&projected);
                reserve_lock_metadata(&mut remaining_metadata, cost)?;
                materialized.push((lock, projected, cost));
            }
            for path in paths {
                reserve_lock_metadata(
                    &mut remaining_metadata,
                    path.as_bytes().len().saturating_add(128),
                )?;
                let normalized = normalize_path(path);
                for (lock, _, cost) in &materialized {
                    if lock_paths_overlap(lock.path(), lock.deep(), &normalized, false) {
                        reserve_lock_metadata(&mut remaining_metadata, *cost)?;
                    }
                }
            }
            Ok(paths
                .iter()
                .map(|path| {
                    let normalized = normalize_path(path);
                    let visible = materialized
                        .iter()
                        .filter(|(lock, _, _)| {
                            lock_paths_overlap(lock.path(), lock.deep(), &normalized, false)
                        })
                        .map(|(_, projected, _)| projected.clone())
                        .collect();
                    (path.clone(), visible)
                })
                .collect())
        })
    }

    fn conflicting_locks(
        &self,
        path: &DavPath,
        deep: bool,
    ) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        let path = path.clone();
        Box::pin(async move {
            Ok(self
                .matching_locks(&path, deep, false)
                .await?
                .iter()
                .map(model_to_dav_lock)
                .collect())
        })
    }

    fn delete(&self, path: &DavPath) -> LsFuture<'_, Result<(), DavLockError>> {
        let path_str = normalize_path(path);
        let path_owned = path.clone();
        Box::pin(async move {
            let txn = transaction::begin(self.state.writer_db()).await.map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to begin WebDAV lock deletion transaction");
                DavLockError::Backend
            })?;
            let mut mutation = WebDavLockMutation::begin(&txn, self.scope)
                .await
                .map_err(|_| DavLockError::Backend)?;
            mutation
                .lock_ancestor_entities(self.scope, self.root_folder_id, &path_owned)
                .await
                .map_err(|_| DavLockError::Backend)?;
            mutation.delete_rooted_locks(&path_owned).await?;
            mutation.finish().await?;

            transaction::commit(txn).await.map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to commit WebDAV lock deletion transaction");
                DavLockError::Backend
            })?;
            Ok(())
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn normalize_path(path: &DavPath) -> String {
    let raw = path.as_str().to_owned();
    if raw.is_empty() || raw == "/" {
        "/".to_string()
    } else {
        raw
    }
}

fn dav_path_ancestors(path: &DavPath) -> Vec<DavPath> {
    let mut ancestors = Vec::new();
    let mut current = Some(path.clone());
    while let Some(path) = current {
        current = path.parent();
        ancestors.push(path);
    }
    ancestors.reverse();
    ancestors
}

fn lock_workspace(scope: WorkspaceStorageScope) -> LockWorkspace {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => LockWorkspace::Personal { user_id },
        WorkspaceStorageScope::Team { team_id, .. } => LockWorkspace::Team { team_id },
    }
}

async fn find_lock_namespace<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
) -> crate::errors::Result<Option<resource_lock_namespace::Model>> {
    let (workspace_type, workspace_id) = lock_workspace(scope).persistence_key();
    lock_namespace_repo::find_by_workspace(db, workspace_type, workspace_id).await
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockPathTarget {
    Entity(EntityType, i64),
    Root,
    Missing,
}

fn webdav_lock_target(
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    resolved: LockPathTarget,
    deep: bool,
) -> LockTarget {
    let workspace = lock_workspace(scope);
    let root = match resolved {
        LockPathTarget::Entity(EntityType::File, file_id) => LockRoot::File { file_id },
        LockPathTarget::Entity(EntityType::Folder, folder_id) => LockRoot::Folder { folder_id },
        LockPathTarget::Root => root_folder_id.map_or(LockRoot::WorkspaceRoot, |folder_id| {
            LockRoot::Folder { folder_id }
        }),
        LockPathTarget::Missing => LockRoot::WorkspaceRoot,
    };
    LockTarget {
        workspace,
        root,
        depth: if deep {
            LockDepth::Infinity
        } else {
            LockDepth::Resource
        },
    }
}

fn dav_lock_conflict_for_request(path: &DavPath, shared: bool, deep: bool) -> DavLock {
    DavLock {
        token: String::new(),
        path: Box::new(path.clone()),
        principal: None,
        owner: None,
        timeout_at: None,
        timeout: None,
        shared,
        deep,
    }
}

/// Resolve a WebDAV path without collapsing virtual roots and missing resources.
async fn resolve_path_to_entity<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    path: &DavPath,
) -> Result<LockPathTarget, FsError> {
    match path_resolver::resolve_path_in_scope(db, scope, path, root_folder_id).await {
        Ok(ResolvedNode::File(f)) => Ok(LockPathTarget::Entity(EntityType::File, f.id)),
        Ok(ResolvedNode::Folder(f)) => Ok(LockPathTarget::Entity(EntityType::Folder, f.id)),
        Ok(ResolvedNode::Root) => Ok(LockPathTarget::Root),
        Err(FsError::NotFound) => Ok(LockPathTarget::Missing),
        Err(error) => Err(error),
    }
}

fn root_lock_target(
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
) -> (ResourceLockTargetType, i64) {
    if let Some(folder_id) = root_folder_id {
        return (ResourceLockTargetType::Folder, folder_id);
    }
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            (ResourceLockTargetType::PersonalRoot, user_id)
        }
        WorkspaceStorageScope::Team { team_id, .. } => (ResourceLockTargetType::TeamRoot, team_id),
    }
}

async fn lock_target_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: ResourceLockTargetType,
    entity_id: i64,
) -> crate::errors::Result<()> {
    match entity_type {
        ResourceLockTargetType::File => {
            file_repo::lock_by_id(db, entity_id).await?;
        }
        ResourceLockTargetType::Folder => {
            folder_repo::lock_by_id(db, entity_id).await?;
        }
        ResourceLockTargetType::PersonalRoot => {
            user_repo::lock_by_id(db, entity_id).await?;
        }
        ResourceLockTargetType::TeamRoot => {
            team_repo::lock_by_id(db, entity_id).await?;
        }
    }
    Ok(())
}

/// Revalidates all current locks that overlap one mutation target on the caller's transaction.
///
/// Lock roots sharing the same path are satisfied when any active shared-lock token for that root
/// was submitted. Backend lookup errors remain typed and fail closed.
pub(crate) async fn revalidate_mutation_locks<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    path: &DavPath,
    deep: bool,
    conditions: &super::DavMutationConditions<'_>,
) -> Result<(), DavLockError> {
    let resolved = resolve_path_to_entity(db, scope, root_folder_id, path)
        .await
        .map_err(|_| DavLockError::Backend)?;
    if resolved == LockPathTarget::Missing {
        return Ok(());
    }
    let target = webdav_lock_target(scope, root_folder_id, resolved, deep);
    let mut conflicts = Vec::new();
    for lock in lock_repo::find_all_by_namespace_for_update(db, namespace_id)
        .await
        .map_err(|_| DavLockError::Backend)?
    {
        if lock.timeout_at.is_some_and(|expires| expires <= Utc::now()) {
            continue;
        }
        if !crate::services::files::lock::lock_overlaps_target_on(db, &lock, target)
            .await
            .map_err(|_| DavLockError::Backend)?
        {
            continue;
        }
        match canonical::project_lock_on(db, scope, root_folder_id, &lock)
            .await
            .map_err(|_| DavLockError::Backend)?
        {
            Some(lock) => conflicts.push(lock),
            // 看不到祖先 token 不表示该锁不存在。仍拒绝修改，但不泄露挂载外信息。
            None => {
                return Err(DavLockError::Conflict(Box::new(
                    dav_lock_conflict_for_request(path, lock.shared(), lock.deep()),
                )));
            }
        }
    }
    for lock in &conflicts {
        let lock_href = href_for_relative(conditions.prefix, lock.path());
        let submitted = conditions.if_header.map_or_else(Vec::new, |header| {
            submitted_lock_tokens(
                header,
                &lock_href,
                conditions.request_scheme,
                conditions.request_host,
            )
        });
        match conflicts.iter().any(|candidate| {
            same_lock_root(candidate, lock) && submitted.contains(&candidate.token)
        }) {
            true => {}
            false => return Err(DavLockError::Conflict(Box::new(model_to_dav_lock(lock)))),
        }
    }
    Ok(())
}

/// Serializes mutation preconditions with LOCK acquisition on the target and every materialized
/// ancestor lock root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockMutationAncestorError {
    Conflict,
    Backend,
}

pub(crate) struct WebDavLockMutation<'a, C: ConnectionTrait> {
    db: &'a C,
    namespace: resource_lock_namespace::Model,
    projection_changed: bool,
    // 事务内、变更前的 canonical 投影。删除/覆盖后路径可能已不可解析，
    // 仍按这些已验证的锁 ID 清理/重绑定，绝不重新用历史 path 全局选锁。
    projected_before: Vec<resource_lock::Model>,
}

impl<'a, C: ConnectionTrait> WebDavLockMutation<'a, C> {
    pub(crate) async fn begin(
        db: &'a C,
        scope: WorkspaceStorageScope,
    ) -> Result<Self, LockMutationAncestorError> {
        let (workspace_type, workspace_id) = lock_workspace(scope).persistence_key();
        let namespace = lock_namespace_repo::ensure_and_lock(db, workspace_type, workspace_id)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, ?scope, "failed to lock WebDAV resource-lock namespace");
                LockMutationAncestorError::Backend
            })?;
        Ok(Self {
            db,
            namespace,
            projection_changed: false,
            projected_before: Vec::new(),
        })
    }

    pub(crate) fn namespace_id(&self) -> i64 {
        self.namespace.id
    }

    pub(crate) async fn lock_ancestor_entities(
        &mut self,
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        path: &DavPath,
    ) -> Result<(), LockMutationAncestorError> {
        let projected =
            canonical::projected_locks_on(self.db, self.namespace.id, scope, root_folder_id)
                .await
                .map_err(|_| LockMutationAncestorError::Backend)?;
        self.projected_before = projected;
        let target = path.as_str();
        for ancestor in dav_path_ancestors(path) {
            match resolve_path_to_entity(self.db, scope, root_folder_id, &ancestor).await {
                Ok(LockPathTarget::Entity(entity_type, entity_id)) => {
                    lock_target_entity(self.db, entity_type.into(), entity_id)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = %error, path = %ancestor.as_str(), "failed to lock WebDAV mutation ancestor");
                            LockMutationAncestorError::Backend
                        })?;
                }
                Ok(LockPathTarget::Root) => {
                    let (entity_type, entity_id) = root_lock_target(scope, root_folder_id);
                    lock_target_entity(self.db, entity_type, entity_id)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = %error, path = %ancestor.as_str(), "failed to lock WebDAV mutation root");
                            LockMutationAncestorError::Backend
                        })?;
                }
                Ok(LockPathTarget::Missing)
                    if ancestor.as_str().trim_end_matches('/') == target.trim_end_matches('/') => {}
                Ok(LockPathTarget::Missing) => {
                    tracing::warn!(path = %ancestor.as_str(), "WebDAV mutation ancestor is missing");
                    return Err(LockMutationAncestorError::Conflict);
                }
                Err(error) => {
                    tracing::warn!(error = %error, path = %ancestor.as_str(), "failed to resolve WebDAV mutation ancestor");
                    return Err(LockMutationAncestorError::Backend);
                }
            }
        }
        Ok(())
    }

    /// Deletes roots selected from the transaction's canonical pre-mutation projection.
    pub(crate) async fn delete_rooted_locks(&mut self, path: &DavPath) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let locks = self.projected_before.clone();
        for lock in locks {
            if lock_path_is_under(&path_str, lock.path()) {
                delete_lock(self.db, &lock).await?;
                self.projection_changed = true;
            }
        }
        Ok(())
    }

    /// Retain the destination root's canonical lock IDs so COPY can rebind every shared lock.
    pub(crate) async fn delete_descendant_rooted_locks(
        &mut self,
        path: &DavPath,
    ) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let locks = self.projected_before.clone();
        for lock in locks {
            if lock.path().trim_end_matches('/') != path_str.trim_end_matches('/')
                && lock_path_is_under(&path_str, lock.path())
            {
                delete_lock(self.db, &lock).await?;
                self.projection_changed = true;
            }
        }
        Ok(())
    }

    pub(crate) async fn rebind_destination_root_locks(
        &mut self,
        path: &DavPath,
        entity_type: EntityType,
        entity_id: i64,
    ) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let locks = self.projected_before.clone();
        for snapshot in locks {
            if snapshot.path().trim_end_matches('/') != path_str.trim_end_matches('/') {
                continue;
            }
            let current = lock_repo::find_by_id_for_update(self.db, snapshot.id)
                .await
                .map_err(|_| DavLockError::Backend)?
                .ok_or(DavLockError::Backend)?;
            let mut active = current.into_active_model();
            let (kind, folder_id, file_id) = match entity_type {
                EntityType::File => (
                    aster_drive_model::types::LockRootKind::File,
                    None,
                    Some(entity_id),
                ),
                EntityType::Folder => (
                    aster_drive_model::types::LockRootKind::Folder,
                    Some(entity_id),
                    None,
                ),
            };
            active.root_kind = Set(kind);
            active.root_folder_id = Set(folder_id);
            active.root_file_id = Set(file_id);
            active.lockroot_path = Set(Some(path_str.clone()));
            active
                .update(self.db)
                .await
                .map_err(|_| DavLockError::Backend)?;
            self.projection_changed = true;
        }
        Ok(())
    }

    pub(crate) async fn finish(self) -> Result<(), DavLockError> {
        if self.projection_changed {
            lock_namespace_repo::increment_generation(self.db, self.namespace)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, "failed to advance WebDAV lock namespace generation");
                    DavLockError::Backend
                })?;
        }
        Ok(())
    }
}

async fn delete_lock<C: ConnectionTrait>(
    db: &C,
    lock: &resource_lock::Model,
) -> Result<(), DavLockError> {
    lock_repo::delete_by_id(db, lock.id)
        .await
        .map_err(|error| {
            tracing::warn!(lock_id = lock.id, error = %error, "failed to delete WebDAV lock");
            DavLockError::Backend
        })?;
    Ok(())
}

fn lock_paths_overlap(
    existing_path: &str,
    existing_deep: bool,
    requested_path: &str,
    requested_deep: bool,
) -> bool {
    if existing_path == requested_path {
        return true;
    }
    if path_is_ancestor(existing_path, requested_path) {
        return existing_deep;
    }
    if path_is_ancestor(requested_path, existing_path) {
        return requested_deep;
    }
    false
}

fn lock_path_is_under(parent: &str, child: &str) -> bool {
    parent == child || path_is_ancestor(parent, child)
}

fn unlock_request_targets_lock_scope(lock_path: &str, lock_deep: bool, request_path: &str) -> bool {
    lock_path == request_path || lock_deep && path_is_ancestor(lock_path, request_path)
}

fn path_is_ancestor(parent: &str, child: &str) -> bool {
    if parent == child {
        return false;
    }
    if parent == "/" {
        return child.starts_with('/');
    }
    if parent.ends_with('/') {
        return child.starts_with(parent);
    }
    child
        .strip_prefix(parent)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn same_lock_root(left: &resource_lock::Model, right: &resource_lock::Model) -> bool {
    left.namespace_id == right.namespace_id
        && left.root_kind == right.root_kind
        && left.root_folder_id == right.root_folder_id
        && left.root_file_id == right.root_file_id
}

fn lock_timeout_at(
    now: chrono::DateTime<Utc>,
    timeout: Option<Duration>,
) -> Result<Option<chrono::DateTime<Utc>>, ()> {
    match timeout {
        Some(timeout) => {
            let chrono_timeout = chrono::Duration::from_std(timeout).map_err(|_| ())?;
            Ok(Some(now + chrono_timeout))
        }
        None => Ok(None),
    }
}

const LOCK_DISCOVERY_METADATA_BUDGET: usize = 16 * 1024 * 1024;

fn reserve_lock_metadata(remaining: &mut usize, bytes: usize) -> Result<(), DavBackendError> {
    *remaining = remaining
        .checked_sub(bytes)
        .ok_or_else(|| DavBackendError::new(DavBackendErrorKind::InsufficientStorage))?;
    Ok(())
}

fn dav_lock_allocation_cost(lock: &DavLock) -> usize {
    std::mem::size_of::<DavLock>()
        .saturating_add(lock.token.capacity())
        .saturating_add(lock.path.as_bytes().len())
        .saturating_add(lock.owner.as_deref().map_or(0, dav_xml_allocation_cost))
}

fn dav_xml_allocation_cost(element: &DavXmlElement) -> usize {
    use aster_forge_webdav::DavXmlNode;
    let mut cost = std::mem::size_of::<DavXmlElement>()
        .saturating_add(element.name.capacity())
        .saturating_add(element.prefix.as_ref().map_or(0, String::capacity))
        .saturating_add(element.namespace.as_ref().map_or(0, String::capacity))
        .saturating_add(
            element
                .children
                .capacity()
                .saturating_mul(std::mem::size_of::<DavXmlNode>()),
        );
    // BTreeMap nodes carry pointers and unused entry slots in addition to their strings.
    for (key, value) in element.namespaces.iter().chain(&element.attributes) {
        cost = cost
            .saturating_add(key.capacity())
            .saturating_add(value.capacity())
            .saturating_add(128);
    }
    for child in &element.children {
        cost = cost.saturating_add(match child {
            DavXmlNode::Element(child) => dav_xml_allocation_cost(child),
            DavXmlNode::Text(text) | DavXmlNode::CData(text) | DavXmlNode::Comment(text) => {
                text.capacity()
            }
            DavXmlNode::ProcessingInstruction(name, value) => name
                .capacity()
                .saturating_add(value.as_ref().map_or(0, String::capacity)),
        });
    }
    cost
}

fn model_to_dav_lock(lock: &resource_lock::Model) -> DavLock {
    let dav_path = DavPath::new(&encode_href(lock.path())).unwrap_or_else(|_| DavPath::root());

    DavLock {
        token: lock.token.clone(),
        path: Box::new(dav_path),
        // owner_id 是 AsterDrive 内部 actor，不要作为 WebDAV principal 暴露。
        principal: None,
        owner: lock_owner_xml(lock)
            .as_deref()
            .and_then(deserialize_element)
            .map(Box::new),
        timeout_at: lock.timeout_at.map(|t| {
            let dur = (t - Utc::now()).to_std().unwrap_or(Duration::ZERO);
            SystemTime::now() + dur
        }),
        timeout: lock
            .timeout_at
            .map(|t| (t - Utc::now()).to_std().unwrap_or(Duration::ZERO)),
        shared: lock.shared(),
        deep: lock.deep(),
    }
}

fn serialize_element(elem: &DavXmlElement) -> Result<String, aster_forge_webdav::DavXmlError> {
    String::from_utf8(elem.to_bytes()?).map_err(|_| aster_forge_webdav::DavXmlError::Malformed)
}

fn deserialize_element(xml: &str) -> Option<DavXmlElement> {
    DavXmlElement::parse(xml.as_bytes()).ok()
}

fn lock_owner_xml(lock: &resource_lock::Model) -> Option<String> {
    match crate::services::files::lock::deserialize_resource_lock_owner_info(lock).ok()? {
        Some(crate::services::files::lock::ResourceLockOwnerInfo::Webdav(payload)) => {
            Some(payload.xml)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{LockAcquireTransactionError, serialize_element};
    use aster_forge_webdav::DavXmlElement;

    #[test]
    fn serialize_element_preserves_xml_writer_errors() {
        let element = DavXmlElement::new("invalid element name");

        assert!(serialize_element(&element).is_err());
    }

    #[test]
    fn lock_acquire_transaction_errors_have_stable_diagnostics() {
        assert_eq!(
            LockAcquireTransactionError::TargetBecameMissing.to_string(),
            "WebDAV LOCK target became missing"
        );
        assert_eq!(
            LockAcquireTransactionError::LimitExceeded.to_string(),
            "WebDAV active lock limit exceeded"
        );
        assert_eq!(
            LockAcquireTransactionError::Product(crate::errors::AsterError::internal_error(
                "quota lookup failed"
            ))
            .to_string(),
            "Internal Server Error: quota lookup failed"
        );
    }
}
