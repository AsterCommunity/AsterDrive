//! WebDAV 子模块：`db_lock_system`。

use aster_forge_db::transaction;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use aster_forge_webdav::DavXmlElement;
use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::config::webdav;
use crate::db::repository::{file_repo, folder_repo, lock_repo, team_repo, user_repo};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit::{self, AuditContext};
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::webdav::backend::path_resolver::{self, ResolvedNode};
use aster_drive_model::entities::resource_lock;
use aster_drive_model::types::{EntityType, ResourceLockTargetType};
use aster_forge_webdav::{
    DavBackendError, DavBackendErrorKind, DavLock, DavLockError, DavLockPreflightError,
    DavLockSystem, DavPath, FsError, IfHeader, LsFuture, href_for_dav_path, submitted_lock_tokens,
};

const DISCOVER_MANY_ANCESTOR_CHUNK_SIZE: usize = 500;

/// 数据库支持的 WebDAV 锁系统
///
/// Per-request 创建（需要 user_id 做 path → entity_id 解析）
#[derive(Clone)]
pub struct DbLockSystem {
    db: DatabaseConnection,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    audit_state: Option<PrimaryAppState>,
    audit_ctx: AuditContext,
}

impl DbLockSystem {
    pub fn new(db: DatabaseConnection, user_id: i64, root_folder_id: Option<i64>) -> Box<Self> {
        Box::new(Self {
            db,
            scope: WorkspaceStorageScope::Personal { user_id },
            root_folder_id,
            audit_state: None,
            audit_ctx: AuditContext {
                user_id,
                ip_address: None,
                user_agent: None,
            },
        })
    }

    pub(crate) fn new_with_audit(
        state: PrimaryAppState,
        scope: WorkspaceStorageScope,
        root_folder_id: Option<i64>,
        audit_ctx: AuditContext,
    ) -> Box<Self> {
        Box::new(Self {
            db: state.writer_db().clone(),
            scope,
            root_folder_id,
            audit_state: Some(state),
            audit_ctx,
        })
    }

    async fn log_lock_action(&self, entity_type: EntityType, entity_id: i64, locked: bool) {
        let Some(state) = &self.audit_state else {
            return;
        };
        let action = match (entity_type, locked) {
            (EntityType::File, true) => audit::AuditAction::FileLock,
            (EntityType::File, false) => audit::AuditAction::FileUnlock,
            (EntityType::Folder, true) => audit::AuditAction::FolderLock,
            (EntityType::Folder, false) => audit::AuditAction::FolderUnlock,
        };
        match entity_type {
            EntityType::File => match file_repo::find_by_id(&self.db, entity_id).await {
                Ok(file) => {
                    let details = crate::services::files::file::audit_location_details_for_model(
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
            },
            EntityType::Folder => match folder_repo::find_by_id(&self.db, entity_id).await {
                Ok(folder) => {
                    let details = crate::services::files::folder::audit_location_details_for_model(
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
            },
        }
    }

    fn max_active_locks_per_owner(&self) -> u64 {
        self.audit_state
            .as_ref()
            .map(|state| webdav::max_active_locks_per_user(state.runtime_config()))
            .unwrap_or(crate::config::definitions::DEFAULT_WEBDAV_MAX_ACTIVE_LOCKS_PER_USER)
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
        Box::pin(async move { self.ensure_lock_quota(&self.db, Utc::now()).await })
    }

    fn lock(
        &self,
        path: &DavPath,
        principal: Option<&str>,
        owner: Option<&DavXmlElement>,
        timeout: Option<Duration>,
        shared: bool,
        deep: bool,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
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
            let txn = transaction::begin(&self.db)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to begin WebDAV lock transaction");
                    DavLockError::Backend
                })?;
            let result = async {
                let now = Utc::now();

                let (entity_type, entity_id) = match resolve_path_to_entity(
                    &txn,
                    self.scope,
                    self.root_folder_id,
                    &path_str,
                )
                .await
                .map_err(|error| {
                    tracing::warn!(error = ?error, path = %path_str, "failed to resolve WebDAV lock target");
                    DavLockError::Backend
                })? {
                    LockPathTarget::Entity(entity_type, entity_id) => {
                        (ResourceLockTargetType::from(entity_type), entity_id)
                    }
                    LockPathTarget::Root => root_lock_target(self.scope, self.root_folder_id),
                    LockPathTarget::Missing => return Err(DavLockError::NotFound),
                };
                lock_target_entity(&txn, entity_type, entity_id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            error = %error,
                            entity_type = ?entity_type,
                            entity_id,
                            "failed to lock WebDAV target entity"
                        );
                        DavLockError::Backend
                    })?;

                let mut overlapping = find_overlapping_locks(&txn, &path_str, deep)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to find overlapping WebDAV locks");
                        DavLockError::Backend
                    })?;
                overlapping.sort_by_key(|lock| lock.id);

                for existing in overlapping {
                    if existing
                        .timeout_at
                        .is_some_and(|timeout_at| timeout_at < now)
                    {
                        delete_lock_and_sync_flag(&txn, &existing).await?;
                        continue;
                    }

                    if !shared || !existing.shared {
                        return Err(DavLockError::Conflict(Box::new(model_to_dav_lock(
                            &existing,
                        ))));
                    }
                }

                self.ensure_lock_quota(&txn, now).await.map_err(|error| {
                    if matches!(error, DavLockPreflightError::LimitExceeded) {
                        DavLockError::LimitExceeded
                    } else {
                        DavLockError::Backend
                    }
                })?;

                let token = format!("urn:uuid:{}", uuid::Uuid::new_v4());
                let timeout_at = lock_timeout_at(now, timeout_dur)
                    .map_err(|_| DavLockError::Backend)?;
                let owner_info = owner_xml.clone().map(|xml| {
                    crate::services::files::lock::ResourceLockOwnerInfo::Webdav(
                        crate::services::files::lock::WebdavLockOwnerInfo { xml },
                    )
                });

                let model = resource_lock::ActiveModel {
                    token: sea_orm::Set(token.clone()),
                    entity_type: sea_orm::Set(entity_type),
                    entity_id: sea_orm::Set(entity_id),
                    path: sea_orm::Set(path_str.clone()),
                    // WebDAV 协议层用 token 判定持锁者；业务存储层用 owner_id
                    // 区分“自己的锁”和“其他用户的锁”，否则 Finder 持锁 PUT 会被
                    // workspace::storage 误判为被其他用户锁定。
                    owner_id: sea_orm::Set(Some(self.scope.actor_user_id())),
                    owner_info: sea_orm::Set(
                        crate::services::files::lock::serialize_resource_lock_owner_info(
                            owner_info.as_ref(),
                        )
                        .map_err(|error| {
                            tracing::warn!(error = %error, path = %path_str, "failed to serialize WebDAV lock owner");
                            DavLockError::Backend
                        })?,
                    ),
                    timeout_at: sea_orm::Set(timeout_at),
                    shared: sea_orm::Set(shared),
                    deep: sea_orm::Set(deep),
                    created_at: sea_orm::Set(now),
                    ..Default::default()
                };

                lock_repo::create(&txn, model)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to create WebDAV lock");
                        DavLockError::Backend
                    })?;
                set_lock_target_locked(&txn, entity_type, entity_id, true)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        error = %error,
                        entity_type = ?entity_type,
                        entity_id,
                        "failed to mark WebDAV lock target as locked"
                    );
                    DavLockError::Backend
                })?;

                Ok((
                    DavLock {
                        token,
                        path: Box::new(path_owned.clone()),
                        principal: principal_owned,
                        owner: owner_clone.map(Box::new),
                        timeout_at: timeout_dur.map(|d| SystemTime::now() + d),
                        timeout: timeout_dur,
                        shared,
                        deep,
                    },
                    entity_type,
                    entity_id,
                ))
            }
            .await;

            match result {
                Ok((lock, entity_type, entity_id)) => {
                    transaction::commit(txn)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = %error, path = %path_str, "failed to commit WebDAV lock transaction");
                            DavLockError::Backend
                        })?;
                    if let Some(entity_type) = entity_type.entity_type() {
                        self.log_lock_action(entity_type, entity_id, true).await;
                    }
                    Ok(lock)
                }
                Err(error) => {
                    if let Err(error) = transaction::rollback(txn).await {
                        tracing::warn!(error = %error, "failed to rollback WebDAV lock transaction");
                    }
                    Err(error)
                }
            }
        })
    }

    fn unlock(&self, path: &DavPath, token: &str) -> LsFuture<'_, Result<(), DavLockError>> {
        let token_owned = token.to_string();
        let path_str = normalize_path(path);
        Box::pin(async move {
            let txn = transaction::begin(&self.db).await.map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to begin WebDAV unlock transaction");
                DavLockError::Backend
            })?;
            let result = async {
                let lock = lock_repo::find_by_token_for_update(&txn, &token_owned)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock for unlock");
                        DavLockError::Backend
                    })?
                    .ok_or(DavLockError::TokenMismatch)?;
                if !unlock_request_targets_lock_scope(&lock.path, lock.deep, &path_str) {
                    return Err(DavLockError::TokenMismatch);
                }

                lock_target_entity(&txn, lock.entity_type, lock.entity_id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(
                            error = %error,
                            entity_type = ?lock.entity_type,
                            entity_id = lock.entity_id,
                            "failed to lock WebDAV unlock target entity"
                        );
                        DavLockError::Backend
                    })?;
                lock_repo::delete_by_id(&txn, lock.id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to delete WebDAV lock for unlock");
                        DavLockError::Backend
                    })?;
                clear_lock_target_locked_if_unlocked(&txn, lock.entity_type, lock.entity_id)
                .await
                .map_err(|error| {
                    tracing::warn!(
                        error = %error,
                        entity_type = ?lock.entity_type,
                        entity_id = lock.entity_id,
                        "failed to sync is_locked after WebDAV unlock"
                    );
                    DavLockError::Backend
                })?;
                Ok(lock)
            }
            .await;

            let lock = match result {
                Ok(lock) => {
                    transaction::commit(txn).await.map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to commit WebDAV unlock transaction");
                        DavLockError::Backend
                    })?;
                    lock
                }
                Err(error) => {
                    return Err(error);
                }
            };
            if let Some(entity_type) = lock.entity_type.entity_type() {
                self.log_lock_action(entity_type, lock.entity_id, false)
                    .await;
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

            let current_lock = lock_repo::find_by_token(&self.db, &token_owned)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV lock for refresh");
                    DavLockError::Backend
                })?
                .ok_or(DavLockError::TokenMismatch)?;
            if !unlock_request_targets_lock_scope(&current_lock.path, current_lock.deep, &path_str)
            {
                return Err(DavLockError::TokenMismatch);
            }
            let new_timeout_at =
                lock_timeout_at(now, timeout_dur).map_err(|_| DavLockError::Backend)?;

            let lock = lock_repo::refresh(&self.db, &token_owned, new_timeout_at)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to refresh WebDAV lock");
                    DavLockError::Backend
                })?
                .ok_or(DavLockError::TokenMismatch)?;
            if let Some(entity_type) = lock.entity_type.entity_type() {
                self.log_lock_action(entity_type, lock.entity_id, true)
                    .await;
            }
            let owner = lock_owner_xml(&lock)
                .as_deref()
                .and_then(deserialize_element)
                .map(Box::new);

            Ok(DavLock {
                token: lock.token,
                path: Box::new(path_clone),
                principal: None,
                owner,
                timeout_at: timeout_dur.map(|d| SystemTime::now() + d),
                timeout: timeout_dur,
                shared: lock.shared,
                deep: lock.deep,
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

            // 查祖先路径的锁
            let ancestor_paths = path_ancestors(&path_str);
            let mut all_locks = lock_repo::find_ancestors(&self.db, &ancestor_paths)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query ancestor WebDAV locks");
                    DavLockError::Backend
                })?;

            // deep check：查后代路径的锁
            if deep {
                let descendants = lock_repo::find_by_path_prefix(&self.db, &path_str)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %path_str, "failed to query descendant WebDAV locks");
                        DavLockError::Backend
                    })?;
                all_locks.extend(descendants);
            }

            all_locks.sort_by_key(|l| l.id);
            all_locks.dedup_by_key(|l| l.id);

            all_locks.retain(|lock| lock_paths_overlap(&lock.path, lock.deep, &path_str, deep));

            for (index, lock) in all_locks.iter().enumerate() {
                if lock.timeout_at.is_some_and(|timeout_at| timeout_at < now) {
                    continue;
                }
                if all_locks[..index].iter().any(|previous| {
                    previous.path == lock.path
                        && previous
                            .timeout_at
                            .is_none_or(|timeout_at| timeout_at >= now)
                }) {
                    continue;
                }
                let root_is_satisfied = all_locks.iter().any(|candidate| {
                    candidate.path == lock.path
                        && candidate
                            .timeout_at
                            .is_none_or(|timeout_at| timeout_at >= now)
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
            let ancestor_paths = path_ancestors(&path_str);
            let locks = lock_repo::find_ancestors(&self.db, &ancestor_paths)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to discover WebDAV locks");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?;

            Ok(locks
                .iter()
                .filter(|l| l.timeout_at.is_none_or(|t| t >= now))
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
                locks.extend(lock_repo::find_ancestors(&self.db, chunk).await.map_err(
                    |error| {
                        tracing::warn!(error = %error, "failed to batch-discover WebDAV locks");
                        DavBackendError::new(DavBackendErrorKind::Internal)
                    },
                )?);
            }
            locks.retain(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at >= now));
            locks.sort_by_key(|lock| lock.id);

            let mut locks_by_path: HashMap<String, Vec<DavLock>> = HashMap::new();
            for lock in &locks {
                locks_by_path
                    .entry(lock.path.clone())
                    .or_default()
                    .push(model_to_dav_lock(lock));
            }

            let mut result = HashMap::with_capacity(paths.len());
            for (path, ancestors) in normalized_paths {
                let mut discovered = Vec::new();
                for ancestor in ancestors {
                    if let Some(locks) = locks_by_path.get(&ancestor) {
                        discovered.extend(locks.iter().cloned());
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
            Ok(find_overlapping_locks(&self.db, &path_str, deep)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query conflicting WebDAV locks");
                    DavBackendError::new(DavBackendErrorKind::Internal)
                })?
                .iter()
                .filter(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at >= now))
                .map(model_to_dav_lock)
                .collect())
        })
    }

    fn delete(&self, path: &DavPath) -> LsFuture<'_, Result<(), DavLockError>> {
        let path_str = normalize_path(path);
        Box::pin(async move {
            let txn = transaction::begin(&self.db).await.map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to begin WebDAV lock deletion transaction");
                DavLockError::Backend
            })?;
            let locks = lock_repo::find_by_path_prefix(&txn, &path_str)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to query WebDAV locks for deletion");
                    DavLockError::Backend
                })?;

            for lock in locks {
                if !lock_path_is_under(&path_str, &lock.path) {
                    continue;
                }
                delete_lock_and_sync_flag(&txn, &lock).await?;
            }

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockPathTarget {
    Entity(EntityType, i64),
    Root,
    Missing,
}

/// Resolve a WebDAV path without collapsing virtual roots and missing resources.
async fn resolve_path_to_entity<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    path: &str,
) -> Result<LockPathTarget, FsError> {
    let dav_path = DavPath::new(path).map_err(|_| FsError::GeneralFailure)?;
    match path_resolver::resolve_path_in_scope(db, scope, &dav_path, root_folder_id).await {
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

async fn set_lock_target_locked<C: ConnectionTrait>(
    db: &C,
    entity_type: ResourceLockTargetType,
    entity_id: i64,
    locked: bool,
) -> crate::errors::Result<()> {
    if let Some(entity_type) = entity_type.entity_type() {
        crate::services::files::lock::set_entity_locked(db, entity_type, entity_id, locked).await?;
    }
    Ok(())
}

async fn clear_lock_target_locked_if_unlocked<C: ConnectionTrait>(
    db: &C,
    entity_type: ResourceLockTargetType,
    entity_id: i64,
) -> crate::errors::Result<()> {
    if let Some(entity_type) = entity_type.entity_type() {
        crate::services::files::lock::clear_entity_locked_if_unlocked(db, entity_type, entity_id)
            .await?;
    }
    Ok(())
}

async fn find_overlapping_locks<C: ConnectionTrait>(
    db: &C,
    path: &str,
    deep: bool,
) -> crate::errors::Result<Vec<resource_lock::Model>> {
    let ancestor_paths = path_ancestors(path);
    let mut locks = lock_repo::find_ancestors(db, &ancestor_paths).await?;

    let descendants = lock_repo::find_by_path_prefix(db, path).await?;
    locks.extend(descendants);
    locks.sort_by_key(|lock| lock.id);
    locks.dedup_by_key(|lock| lock.id);
    locks.retain(|lock| lock_paths_overlap(&lock.path, lock.deep, path, deep));
    Ok(locks)
}

/// Revalidates all current locks that overlap one mutation target on the caller's transaction.
///
/// Lock roots sharing the same path are satisfied when any active shared-lock token for that root
/// was submitted. Backend lookup errors remain typed and fail closed.
pub(crate) async fn revalidate_mutation_locks<C: ConnectionTrait>(
    db: &C,
    path: &DavPath,
    deep: bool,
    prefix: &str,
    if_header: Option<&IfHeader>,
    request_scheme: &str,
    request_host: &str,
) -> Result<(), DavLockError> {
    let path_str = normalize_path(path);
    let now = Utc::now();
    let conflicts = find_overlapping_locks(db, &path_str, deep)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to revalidate WebDAV mutation locks");
            DavLockError::Backend
        })?;
    for (index, lock) in conflicts.iter().enumerate() {
        if lock.timeout_at.is_some_and(|timeout_at| timeout_at < now)
            || conflicts[..index].iter().any(|previous| {
                previous.path == lock.path
                    && previous
                        .timeout_at
                        .is_none_or(|timeout_at| timeout_at >= now)
            })
        {
            continue;
        }
        let lock_href = href_for_dav_path(prefix, &DavPath::new(&lock.path).map_err(|_| {
            tracing::warn!(lock_id = lock.id, path = %lock.path, "stored WebDAV lock path is invalid");
            DavLockError::Backend
        })?);
        let submitted = if_header.map_or_else(Vec::new, |header| {
            submitted_lock_tokens(header, &lock_href, request_scheme, request_host)
        });
        let satisfied = conflicts.iter().any(|candidate| {
            candidate.path == lock.path
                && candidate
                    .timeout_at
                    .is_none_or(|timeout_at| timeout_at >= now)
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

pub(crate) async fn lock_mutation_ancestor_entities<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    path: &DavPath,
) -> Result<(), LockMutationAncestorError> {
    let target = normalize_path(path);
    for ancestor in path_ancestors(&target) {
        match resolve_path_to_entity(db, scope, root_folder_id, &ancestor).await {
            Ok(LockPathTarget::Entity(entity_type, entity_id)) => {
                lock_target_entity(db, entity_type.into(), entity_id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %ancestor, "failed to lock WebDAV mutation ancestor");
                        LockMutationAncestorError::Backend
                    })?;
            }
            Ok(LockPathTarget::Root) => {
                let (entity_type, entity_id) = root_lock_target(scope, root_folder_id);
                lock_target_entity(db, entity_type, entity_id)
                    .await
                    .map_err(|error| {
                        tracing::warn!(error = %error, path = %ancestor, "failed to lock WebDAV mutation root");
                        LockMutationAncestorError::Backend
                    })?;
            }
            Ok(LockPathTarget::Missing)
                if ancestor.trim_end_matches('/') == target.trim_end_matches('/') => {}
            Ok(LockPathTarget::Missing) => {
                tracing::warn!(path = %ancestor, "WebDAV mutation ancestor is missing");
                return Err(LockMutationAncestorError::Conflict);
            }
            Err(error) => {
                tracing::warn!(error = %error, path = %ancestor, "failed to resolve WebDAV mutation ancestor");
                return Err(LockMutationAncestorError::Backend);
            }
        }
    }
    Ok(())
}

/// Destroys every rooted lock under `path` and synchronizes entity lock flags on the caller's
/// transaction.
pub(crate) async fn delete_rooted_locks_on<C: ConnectionTrait>(
    db: &C,
    path: &DavPath,
) -> Result<(), DavLockError> {
    let path_str = normalize_path(path);
    let locks = lock_repo::find_by_path_prefix(db, &path_str)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to query rooted WebDAV locks");
            DavLockError::Backend
        })?;
    for lock in locks {
        if lock_path_is_under(&path_str, &lock.path) {
            delete_lock_and_sync_flag(db, &lock).await?;
        }
    }
    Ok(())
}

/// Deletes descendant-rooted locks while retaining locks rooted exactly on `path` for destination
/// overwrite rebinding.
pub(crate) async fn delete_descendant_rooted_locks_on<C: ConnectionTrait>(
    db: &C,
    path: &DavPath,
) -> Result<(), DavLockError> {
    let path_str = normalize_path(path);
    let locks = lock_repo::find_by_path_prefix(db, &path_str)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to query descendant WebDAV locks");
            DavLockError::Backend
        })?;
    for lock in locks {
        if lock.path != path_str && lock_path_is_under(&path_str, &lock.path) {
            delete_lock_and_sync_flag(db, &lock).await?;
        }
    }
    Ok(())
}

/// Rebinds locks rooted at a destination path to the replacement entity and synchronizes both
/// sides of the denormalized lock flag.
pub(crate) async fn rebind_destination_root_locks_on<C: ConnectionTrait>(
    db: &C,
    path: &DavPath,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<(), DavLockError> {
    let path_str = normalize_path(path);
    let previous = lock_repo::find_by_path(db, &path_str)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to query destination WebDAV locks");
            DavLockError::Backend
        })?;
    let rows_affected = lock_repo::rebind_path(db, &path_str, entity_type, entity_id)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to rebind destination WebDAV locks");
            DavLockError::Backend
        })?;
    for lock in previous {
        if lock.entity_type != entity_type.into() || lock.entity_id != entity_id {
            clear_lock_target_locked_if_unlocked(db, lock.entity_type, lock.entity_id)
            .await
            .map_err(|error| {
                tracing::warn!(error = %error, path = %path_str, "failed to clear replaced WebDAV lock flag");
                DavLockError::Backend
            })?;
        }
    }
    if rows_affected == 0 {
        return Ok(());
    }
    set_lock_target_locked(db, entity_type.into(), entity_id, true)
        .await
        .map_err(|error| {
            tracing::warn!(error = %error, path = %path_str, "failed to set rebound WebDAV lock flag");
            DavLockError::Backend
        })
}

async fn delete_lock_and_sync_flag<C: ConnectionTrait>(
    db: &C,
    lock: &resource_lock::Model,
) -> Result<(), DavLockError> {
    lock_repo::delete_by_id(db, lock.id)
        .await
        .map_err(|error| {
            tracing::warn!(lock_id = lock.id, error = %error, "failed to delete WebDAV lock");
            DavLockError::Backend
        })?;
    clear_lock_target_locked_if_unlocked(db, lock.entity_type, lock.entity_id)
        .await
        .map_err(|error| {
            tracing::warn!(
                lock_id = lock.id,
                entity_type = ?lock.entity_type,
                entity_id = lock.entity_id,
                error = %error,
                "failed to sync is_locked after WebDAV lock deletion"
            );
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
    let dav_path = DavPath::new(&lock.path).unwrap_or_else(|_| DavPath::root());

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
        shared: lock.shared,
        deep: lock.deep,
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
    use super::serialize_element;
    use aster_forge_webdav::DavXmlElement;

    #[test]
    fn serialize_element_preserves_xml_writer_errors() {
        let element = DavXmlElement::new("invalid element name");

        assert!(serialize_element(&element).is_err());
    }
}
