//! WebDAV 子模块：`db_lock_system`。

use std::collections::HashMap;
use std::io::Cursor;
use std::time::{Duration, SystemTime};

use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection};
use xmltree::Element;

use crate::config::webdav;
use crate::db::repository::{file_repo, folder_repo, lock_repo, user_repo};
use crate::entities::resource_lock;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::audit_service::{self, AuditContext};
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::types::EntityType;
use crate::webdav::dav::{
    DavLock, DavLockError, DavLockPreflightError, DavLockSystem, DavPath, LsFuture,
};
use crate::webdav::path_resolver::{self, ResolvedNode};

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
            (EntityType::File, true) => audit_service::AuditAction::FileLock,
            (EntityType::File, false) => audit_service::AuditAction::FileUnlock,
            (EntityType::Folder, true) => audit_service::AuditAction::FolderLock,
            (EntityType::Folder, false) => audit_service::AuditAction::FolderUnlock,
        };
        match entity_type {
            EntityType::File => match file_repo::find_by_id(&self.db, entity_id).await {
                Ok(file) => {
                    let details = crate::services::files::file::audit_location_details_for_model(
                        state, self.scope, &file,
                    )
                    .await;
                    audit_service::log_with_details(
                        state,
                        &self.audit_ctx,
                        action,
                        audit_service::AuditEntityType::File,
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
                    audit_service::log_with_details(
                        state,
                        &self.audit_ctx,
                        action,
                        audit_service::AuditEntityType::File,
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
                    audit_service::log_with_details(
                        state,
                        &self.audit_ctx,
                        action,
                        audit_service::AuditEntityType::Folder,
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
                    audit_service::log_with_details(
                        state,
                        &self.audit_ctx,
                        action,
                        audit_service::AuditEntityType::Folder,
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
        owner: Option<&Element>,
        timeout: Option<Duration>,
        shared: bool,
        deep: bool,
    ) -> LsFuture<'_, Result<DavLock, DavLockError>> {
        let path_str = normalize_path(path);
        let path_owned = path.clone();
        let principal_owned = principal.map(|s| s.to_string());
        let owner_xml = owner.map(serialize_element);
        let owner_clone = owner.cloned();
        let timeout_dur = timeout;

        Box::pin(async move {
            let txn = crate::db::transaction::begin(&self.db)
                .await
                .map_err(|error| {
                    tracing::warn!(error = %error, path = %path_str, "failed to begin WebDAV lock transaction");
                    DavLockError::Backend
                })?;
            let result = async {
                let now = Utc::now();

                let (entity_type, entity_id) =
                    resolve_path_to_entity(&txn, self.scope, self.root_folder_id, &path_str)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = ?error, path = %path_str, "failed to resolve WebDAV lock target");
                            DavLockError::Backend
                        })?;
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
                        delete_lock_and_sync_flag(&txn, &existing).await;
                        continue;
                    }

                    if !shared || !existing.shared {
                        return Err(DavLockError::Conflict(model_to_dav_lock(&existing)));
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
                crate::services::files::lock::set_entity_locked(
                    &txn,
                    entity_type,
                    entity_id,
                    true,
                )
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
                    crate::db::transaction::commit(txn)
                        .await
                        .map_err(|error| {
                            tracing::warn!(error = %error, path = %path_str, "failed to commit WebDAV lock transaction");
                            DavLockError::Backend
                        })?;
                    self.log_lock_action(entity_type, entity_id, true).await;
                    Ok(lock)
                }
                Err(error) => {
                    if let Err(error) = crate::db::transaction::rollback(txn).await {
                        tracing::warn!(error = %error, "failed to rollback WebDAV lock transaction");
                    }
                    Err(error)
                }
            }
        })
    }

    fn unlock(&self, path: &DavPath, token: &str) -> LsFuture<'_, Result<(), ()>> {
        let token_owned = token.to_string();
        let path_str = normalize_path(path);
        Box::pin(async move {
            // 查锁拿 entity 信息
            let lock = lock_repo::find_by_token(&self.db, &token_owned)
                .await
                .map_err(|_| ())?
                .ok_or(())?;
            if !unlock_request_targets_lock_scope(&lock.path, lock.deep, &path_str) {
                return Err(());
            }

            lock_repo::delete_by_token(&self.db, &token_owned)
                .await
                .map_err(|_| ())?;

            if let Err(e) = crate::services::files::lock::clear_entity_locked_if_unlocked(
                &self.db,
                lock.entity_type,
                lock.entity_id,
            )
            .await
            {
                tracing::warn!("failed to sync is_locked after unlock: {e}");
            }
            self.log_lock_action(lock.entity_type, lock.entity_id, false)
                .await;
            Ok(())
        })
    }

    fn refresh(
        &self,
        path: &DavPath,
        token: &str,
        timeout: Option<Duration>,
    ) -> LsFuture<'_, Result<DavLock, ()>> {
        let token_owned = token.to_string();
        let path_clone = path.clone();
        let path_str = normalize_path(path);
        let timeout_dur = timeout;

        Box::pin(async move {
            let now = Utc::now();

            let current_lock = lock_repo::find_by_token(&self.db, &token_owned)
                .await
                .map_err(|_| ())?
                .ok_or(())?;
            if !unlock_request_targets_lock_scope(&current_lock.path, current_lock.deep, &path_str)
            {
                return Err(());
            }
            let new_timeout_at = lock_timeout_at(now, timeout_dur).map_err(|_| ())?;

            let lock = lock_repo::refresh(&self.db, &token_owned, new_timeout_at)
                .await
                .map_err(|_| ())?
                .ok_or(())?;
            self.log_lock_action(lock.entity_type, lock.entity_id, true)
                .await;
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
    ) -> LsFuture<'_, Result<(), DavLock>> {
        let path_str = normalize_path(path);
        let tokens: Vec<String> = submitted_tokens.to_vec();
        let _ = ignore_principal; // 简化：统一用 token 匹配

        Box::pin(async move {
            let now = Utc::now();

            // 查祖先路径的锁
            let ancestor_paths = path_ancestors(&path_str);
            let mut all_locks = lock_repo::find_ancestors(&self.db, &ancestor_paths)
                .await
                .unwrap_or_default();

            // deep check：查后代路径的锁
            if deep {
                let descendants = lock_repo::find_by_path_prefix(&self.db, &path_str)
                    .await
                    .unwrap_or_default();
                all_locks.extend(descendants);
            }

            all_locks.sort_by_key(|l| l.id);
            all_locks.dedup_by_key(|l| l.id);

            all_locks.retain(|lock| lock_paths_overlap(&lock.path, lock.deep, &path_str, deep));

            let holds_any = all_locks.iter().any(|lock| {
                lock.timeout_at.is_none_or(|t| t >= now) && tokens.contains(&lock.token)
            });

            if holds_any {
                return Ok(());
            }

            // 检查冲突
            for lock in &all_locks {
                if lock.timeout_at.is_some_and(|t| t < now) {
                    continue;
                }

                return Err(model_to_dav_lock(lock));
            }

            Ok(())
        })
    }

    fn discover(&self, path: &DavPath) -> LsFuture<'_, Vec<DavLock>> {
        let path_str = normalize_path(path);

        Box::pin(async move {
            let now = Utc::now();
            let ancestor_paths = path_ancestors(&path_str);
            let locks = lock_repo::find_ancestors(&self.db, &ancestor_paths)
                .await
                .unwrap_or_default();

            locks
                .iter()
                .filter(|l| l.timeout_at.is_none_or(|t| t >= now))
                .map(model_to_dav_lock)
                .collect()
        })
    }

    fn discover_many<'a>(
        &'a self,
        paths: &'a [DavPath],
    ) -> LsFuture<'a, HashMap<DavPath, Vec<DavLock>>> {
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
                locks.extend(
                    lock_repo::find_ancestors(&self.db, chunk)
                        .await
                        .unwrap_or_default(),
                );
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
            result
        })
    }

    fn conflicting_locks(&self, path: &DavPath, deep: bool) -> LsFuture<'_, Vec<DavLock>> {
        let path_str = normalize_path(path);

        Box::pin(async move {
            let now = Utc::now();
            find_overlapping_locks(&self.db, &path_str, deep)
                .await
                .unwrap_or_default()
                .iter()
                .filter(|lock| lock.timeout_at.is_none_or(|timeout_at| timeout_at >= now))
                .map(model_to_dav_lock)
                .collect()
        })
    }

    fn delete(&self, path: &DavPath) -> LsFuture<'_, Result<(), ()>> {
        let path_str = normalize_path(path);
        Box::pin(async move {
            let locks = lock_repo::find_by_path_prefix(&self.db, &path_str)
                .await
                .unwrap_or_default();

            for lock in locks {
                if !lock_path_is_under(&path_str, &lock.path) {
                    continue;
                }
                delete_lock_and_sync_flag(&self.db, &lock).await;
            }

            Ok(())
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn normalize_path(path: &DavPath) -> String {
    let raw = String::from_utf8_lossy(path.as_bytes()).to_string();
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

/// 从 WebDAV 路径解析出 (entity_type, entity_id)
async fn resolve_path_to_entity<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    root_folder_id: Option<i64>,
    path: &str,
) -> Result<(EntityType, i64), ()> {
    let dav_path = DavPath::new(path).map_err(|_| ())?;
    match path_resolver::resolve_path_in_scope(db, scope, &dav_path, root_folder_id).await {
        Ok(ResolvedNode::File(f)) => Ok((EntityType::File, f.id)),
        Ok(ResolvedNode::Folder(f)) => Ok((EntityType::Folder, f.id)),
        _ => Err(()),
    }
}

async fn lock_target_entity<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> crate::errors::Result<()> {
    match entity_type {
        EntityType::File => {
            file_repo::lock_by_id(db, entity_id).await?;
        }
        EntityType::Folder => {
            folder_repo::lock_by_id(db, entity_id).await?;
        }
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

async fn delete_lock_and_sync_flag<C: ConnectionTrait>(db: &C, lock: &resource_lock::Model) {
    if let Err(error) = lock_repo::delete_by_id(db, lock.id).await {
        tracing::warn!(lock_id = lock.id, error = %error, "failed to delete WebDAV lock");
        return;
    }
    if let Err(error) = crate::services::files::lock::clear_entity_locked_if_unlocked(
        db,
        lock.entity_type,
        lock.entity_id,
    )
    .await
    {
        tracing::warn!(
            lock_id = lock.id,
            entity_type = ?lock.entity_type,
            entity_id = lock.entity_id,
            error = %error,
            "failed to sync is_locked after WebDAV lock deletion"
        );
    }
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

fn serialize_element(elem: &Element) -> String {
    let mut buf = Vec::new();
    elem.write(&mut buf).unwrap_or_default();
    String::from_utf8_lossy(&buf).to_string()
}

fn deserialize_element(xml: &str) -> Option<Element> {
    Element::parse(Cursor::new(xml.as_bytes())).ok()
}

fn lock_owner_xml(lock: &resource_lock::Model) -> Option<String> {
    match crate::services::files::lock::deserialize_resource_lock_owner_info(lock).ok()? {
        Some(crate::services::files::lock::ResourceLockOwnerInfo::Webdav(payload)) => {
            Some(payload.xml)
        }
        _ => None,
    }
}
