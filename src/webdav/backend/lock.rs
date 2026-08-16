//! WebDAV 子模块：`db_lock_system`。

use aster_forge_db::transaction;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use aster_forge_webdav::DavXmlElement;
use chrono::Utc;
use sea_orm::ConnectionTrait;

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

const DISCOVER_MANY_ANCESTOR_CHUNK_SIZE: usize = 500;

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
            root_folder_id,
            audit_ctx,
            #[cfg(test)]
            lock_transaction_test_barrier: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
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
                                .create_file_on(txn, &blob)
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
                            let conflict =
                                match find_lock_namespace(self.state.writer_db(), self.scope).await
                                {
                                    Ok(Some(namespace)) => find_overlapping_locks(
                                        self.state.writer_db(),
                                        namespace.id,
                                        &path_str,
                                        deep,
                                    )
                                    .await
                                    .ok()
                                    .and_then(|locks| {
                                        locks.into_iter().find(|lock| {
                                            lock.timeout_at
                                                .is_none_or(|expires_at| expires_at > Utc::now())
                                        })
                                    })
                                    .map(|lock| model_to_dav_lock(&lock)),
                                    Ok(None) | Err(_) => None,
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
        let token_owned = token.to_string();
        let path_str = normalize_path(path);
        Box::pin(async move {
            let snapshot = lock_repo::find_by_token(self.state.writer_db(), &token_owned)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock for unlock");
                    DavLockError::Backend
                })?
                .ok_or(DavLockError::TokenMismatch)?;
            if !unlock_request_targets_lock_scope(snapshot.path(), snapshot.deep(), &path_str) {
                return Err(DavLockError::TokenMismatch);
            }
            let lock = crate::services::files::lock::unlock_by_token_on(self.state.writer_db(), &token_owned)
                .await
                .map_err(|error| {
                    if matches!(error, crate::errors::AsterError::RecordNotFound(_)) {
                        DavLockError::TokenMismatch
                    } else {
                        tracing::warn!(error = %error, path = %path_str, "failed to release WebDAV lock");
                        DavLockError::Backend
                    }
                })?;
            if let (Some(entity_type), Some(entity_id)) = (lock.entity_type(), lock.entity_id()) {
                self.log_lock_action(entity_type, entity_id, false).await;
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
        let token_owned = token.to_string();
        let path_clone = path.clone();
        let path_str = normalize_path(path);
        let timeout_dur = timeout;

        Box::pin(async move {
            let now = Utc::now();

            let current_lock = lock_repo::find_by_token(self.state.writer_db(), &token_owned)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock for refresh");
                    DavLockError::Backend
                })?
                .ok_or(DavLockError::TokenMismatch)?;
            if !unlock_request_targets_lock_scope(
                current_lock.path(),
                current_lock.deep(),
                &path_str,
            ) {
                return Err(DavLockError::TokenMismatch);
            }
            let new_timeout_at =
                lock_timeout_at(now, timeout_dur).map_err(|_| DavLockError::Backend)?;

            let lock = crate::services::files::lock::refresh_by_token_on(
                self.state.writer_db(),
                &token_owned,
                new_timeout_at,
            )
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to refresh WebDAV lock");
                DavLockError::Backend
            })?;
            if let (Some(entity_type), Some(entity_id)) = (lock.entity_type(), lock.entity_id()) {
                self.log_lock_action(entity_type, entity_id, true).await;
            }
            let owner = lock_owner_xml(&lock)
                .as_deref()
                .and_then(deserialize_element)
                .map(Box::new);

            let shared = lock.shared();
            let deep = lock.deep();
            Ok(DavLock {
                token: lock.token,
                path: Box::new(path_clone),
                principal: None,
                owner,
                timeout_at: timeout_dur.map(|d| SystemTime::now() + d),
                timeout: timeout_dur,
                shared,
                deep,
            })
        })
    }

    fn check(
        &self,
        path: &DavPath,
        _principal: Option<&str>,
        ignore_principal: bool,
        deep: bool,
        submitted_tokens: &[String],
    ) -> LsFuture<'_, Result<(), DavLockError>> {
        let path_str = normalize_path(path);
        let tokens: Vec<String> = submitted_tokens.to_vec();
        let _ = ignore_principal; // 简化：统一用 token 匹配

        Box::pin(async move {
            let now = Utc::now();
            let Some(namespace) = find_lock_namespace(self.state.writer_db(), self.scope)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock namespace");
                    DavLockError::Backend
                })?
            else {
                return Ok(());
            };

            // 查祖先路径的锁
            let ancestor_paths = path_ancestors(&path_str);
            let mut all_locks = lock_repo::find_ancestors_in_namespace(
                self.state.writer_db(),
                namespace.id,
                &ancestor_paths,
            )
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query ancestor WebDAV locks");
                    DavLockError::Backend
                })?;

            // deep check：查后代路径的锁
            if deep {
                let descendants = lock_repo::find_by_path_prefix_in_namespace(
                    self.state.writer_db(),
                    namespace.id,
                    &path_str,
                )
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to query descendant WebDAV locks");
                        DavLockError::Backend
                    })?;
                all_locks.extend(descendants);
            }

            all_locks.sort_by_key(|l| l.id);
            all_locks.dedup_by_key(|l| l.id);

            all_locks.retain(|lock| lock_paths_overlap(lock.path(), lock.deep(), &path_str, deep));

            for (index, lock) in all_locks.iter().enumerate() {
                if lock.timeout_at.is_some_and(|timeout_at| timeout_at <= now) {
                    continue;
                }
                if all_locks[..index].iter().any(|previous| {
                    previous.path() == lock.path()
                        && previous
                            .timeout_at
                            .is_none_or(|timeout_at| timeout_at > now)
                }) {
                    continue;
                }
                let root_is_satisfied = all_locks.iter().any(|candidate| {
                    candidate.path() == lock.path()
                        && candidate
                            .timeout_at
                            .is_none_or(|timeout_at| timeout_at > now)
                        && tokens.contains(&candidate.token)
                });
                if !root_is_satisfied {
                    return Err(DavLockError::Conflict(Box::new(model_to_dav_lock(lock))));
                }
            }

            Ok(())
        })
    }

    fn discover(&self, path: &DavPath) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        let path_str = normalize_path(path);

        Box::pin(async move {
            let now = Utc::now();
            let Some(namespace) = find_lock_namespace(self.state.writer_db(), self.scope)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock namespace");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?
            else {
                return Ok(vec![]);
            };
            let ancestor_paths = path_ancestors(&path_str);
            let locks = lock_repo::find_ancestors_in_namespace(
                self.state.writer_db(),
                namespace.id,
                &ancestor_paths,
            )
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to discover WebDAV locks");
                DavBackendError::new(DavBackendErrorKind::Internal)
            })?;

            Ok(locks
                .iter()
                .filter(|lock| {
                    lock.timeout_at.is_none_or(|timeout_at| timeout_at > now)
                        && lock_paths_overlap(lock.path(), lock.deep(), &path_str, false)
                })
                .map(model_to_dav_lock)
                .collect())
        })
    }

    fn discover_many<'a>(
        &'a self,
        paths: &'a [DavPath],
    ) -> LsFuture<'a, Result<HashMap<DavPath, Vec<DavLock>>, DavBackendError>> {
        Box::pin(async move {
            let now = Utc::now();
            let Some(namespace) = find_lock_namespace(self.state.writer_db(), self.scope)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, "failed to query WebDAV lock namespace");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?
            else {
                return Ok(paths
                    .iter()
                    .cloned()
                    .map(|path| (path, Vec::new()))
                    .collect());
            };
            let mut normalized_paths = Vec::with_capacity(paths.len());
            let mut all_ancestors = Vec::new();
            for path in paths {
                let normalized = normalize_path(path);
                let ancestors = path_ancestors(&normalized);
                all_ancestors.extend(ancestors.iter().cloned());
                normalized_paths.push((path.clone(), ancestors));
            }
            all_ancestors.sort();
            all_ancestors.dedup();

            let mut locks = Vec::new();
            for chunk in all_ancestors.chunks(DISCOVER_MANY_ANCESTOR_CHUNK_SIZE) {
                locks.extend(
                    lock_repo::find_ancestors_in_namespace(
                        self.state.writer_db(),
                        namespace.id,
                        chunk,
                    )
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, "failed to batch-discover WebDAV locks");
                        DavBackendError::new(DavBackendErrorKind::Internal)
                    })?,
                );
            }
            locks.retain(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at > now));
            locks.sort_by_key(|lock| lock.id);

            let mut locks_by_path: HashMap<String, Vec<DavLock>> = HashMap::new();
            for lock in &locks {
                locks_by_path
                    .entry(lock.path().to_string())
                    .or_default()
                    .push(model_to_dav_lock(lock));
            }

            let mut result = HashMap::with_capacity(paths.len());
            for (path, ancestors) in normalized_paths {
                let mut discovered = Vec::new();
                for ancestor in ancestors {
                    if let Some(locks) = locks_by_path.get(&ancestor) {
                        discovered.extend(
                            locks
                                .iter()
                                .filter(|lock| {
                                    lock_paths_overlap(
                                        lock.path.as_str(),
                                        lock.deep,
                                        path.as_str(),
                                        false,
                                    )
                                })
                                .cloned(),
                        );
                    }
                }
                result.insert(path, discovered);
            }
            Ok(result)
        })
    }

    fn conflicting_locks(
        &self,
        path: &DavPath,
        deep: bool,
    ) -> LsFuture<'_, Result<Vec<DavLock>, DavBackendError>> {
        let path_str = normalize_path(path);

        Box::pin(async move {
            let now = Utc::now();
            let Some(namespace) = find_lock_namespace(self.state.writer_db(), self.scope)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock namespace");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?
            else {
                return Ok(vec![]);
            };
            Ok(find_overlapping_locks(self.state.writer_db(), namespace.id, &path_str, deep)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query conflicting WebDAV locks");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?
                .iter()
                .filter(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at > now))
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

fn path_ancestors(path: &str) -> Vec<String> {
    let mut ancestors = vec!["/".to_string()];
    let trimmed = path.trim_start_matches('/');
    let mut current = String::from("/");
    for seg in trimmed.split('/') {
        if seg.is_empty() {
            continue;
        }
        current.push_str(seg);
        current.push('/');
        if current != "/" {
            ancestors.push(current.clone());
        }
    }
    if path != "/" && !path.ends_with('/') {
        ancestors.push(path.to_string());
    }
    ancestors.dedup();
    ancestors
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

async fn find_overlapping_locks<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    path: &str,
    deep: bool,
) -> crate::errors::Result<Vec<resource_lock::Model>> {
    let ancestor_paths = path_ancestors(path);
    let mut locks =
        lock_repo::find_ancestors_in_namespace(db, namespace_id, &ancestor_paths).await?;

    let descendants = lock_repo::find_by_path_prefix_in_namespace(db, namespace_id, path).await?;
    locks.extend(descendants);
    locks.sort_by_key(|lock| lock.id);
    locks.dedup_by_key(|lock| lock.id);
    locks.retain(|lock| lock_paths_overlap(lock.path(), lock.deep(), path, deep));
    Ok(locks)
}

/// Revalidates all current locks that overlap one mutation target on the caller's transaction.
///
/// Lock roots sharing the same path are satisfied when any active shared-lock token for that root
/// was submitted. Backend lookup errors remain typed and fail closed.
pub(crate) async fn revalidate_mutation_locks<C: ConnectionTrait>(
    db: &C,
    namespace_id: i64,
    path: &DavPath,
    deep: bool,
    conditions: &super::DavMutationConditions<'_>,
) -> Result<(), DavLockError> {
    let path_str = normalize_path(path);
    let now = Utc::now();
    let conflicts = find_overlapping_locks(db, namespace_id, &path_str, deep)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to revalidate WebDAV mutation locks");
            DavLockError::Backend
        })?;
    for (index, lock) in conflicts.iter().enumerate() {
        if lock.timeout_at.is_some_and(|timeout_at| timeout_at <= now)
            || conflicts[..index].iter().any(|previous| {
                previous.path() == lock.path()
                    && previous
                        .timeout_at
                        .is_none_or(|timeout_at| timeout_at > now)
            })
        {
            continue;
        }
        let lock_href = href_for_relative(conditions.prefix, lock.path());
        let submitted = conditions.if_header.map_or_else(Vec::new, |header| {
            submitted_lock_tokens(
                header,
                &lock_href,
                conditions.request_scheme,
                conditions.request_host,
            )
        });
        let satisfied = conflicts.iter().any(|candidate| {
            candidate.path() == lock.path()
                && candidate
                    .timeout_at
                    .is_none_or(|timeout_at| timeout_at > now)
                && submitted.iter().any(|token| token == &candidate.token)
        });
        if !satisfied {
            return Err(DavLockError::Conflict(Box::new(model_to_dav_lock(lock))));
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
        })
    }

    pub(crate) fn namespace_id(&self) -> i64 {
        self.namespace.id
    }

    pub(crate) async fn lock_ancestor_entities(
        &self,
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        path: &DavPath,
    ) -> Result<(), LockMutationAncestorError> {
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

    /// Deletes every lock whose RFC lock-root is at or below `path`.
    pub(crate) async fn delete_rooted_locks(&mut self, path: &DavPath) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let locks = lock_repo::find_by_path_prefix_in_namespace(
            self.db,
            self.namespace.id,
            &path_str,
        )
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to query rooted WebDAV locks");
            DavLockError::Backend
        })?;
        for lock in locks {
            if lock_path_is_under(&path_str, lock.path()) {
                delete_lock(self.db, &lock).await?;
                self.projection_changed = true;
            }
        }
        Ok(())
    }

    /// Deletes descendant lock-roots while retaining a lock rooted exactly on `path`.
    pub(crate) async fn delete_descendant_rooted_locks(
        &mut self,
        path: &DavPath,
    ) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let locks = lock_repo::find_by_path_prefix_in_namespace(
            self.db,
            self.namespace.id,
            &path_str,
        )
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to query descendant WebDAV locks");
            DavLockError::Backend
        })?;
        for lock in locks {
            if lock.path() != path_str && lock_path_is_under(&path_str, lock.path()) {
                delete_lock(self.db, &lock).await?;
                self.projection_changed = true;
            }
        }
        Ok(())
    }

    /// Rebinds destination lock-roots to the replacement Drive entity.
    pub(crate) async fn rebind_destination_root_locks(
        &mut self,
        path: &DavPath,
        entity_type: EntityType,
        entity_id: i64,
    ) -> Result<(), DavLockError> {
        let path_str = normalize_path(path);
        let rows_affected = lock_repo::rebind_path_in_namespace(
            self.db,
            self.namespace.id,
            &path_str,
            entity_type,
            entity_id,
        )
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to rebind destination WebDAV locks");
            DavLockError::Backend
        })?;
        self.projection_changed |= rows_affected != 0;
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
