//! 服务模块：`audit_service`。

use actix_web::HttpRequest;
use chrono::{DateTime, Duration, Utc};
use sea_orm::{DatabaseConnection, Set};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::api::pagination::{AdminAuditLogSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::audit_log_repo;
use crate::entities::audit_log;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{auth_service::Claims, profile_service, user_service};
pub use crate::types::AuditAction;
use crate::types::{TeamMemberRole, UserRole, UserStatus};
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const DEFAULT_RETENTION_DAYS: i64 = 90;
const MAX_AUDIT_IP_ADDRESS_LEN: usize = 45;
const MAX_AUDIT_USER_AGENT_LEN: usize = 512;
const AUDIT_LOG_QUEUE_CAPACITY: usize = 4096;
const AUDIT_LOG_BATCH_SIZE: usize = 100;
const AUDIT_LOG_FLUSH_INTERVAL: StdDuration = StdDuration::from_millis(500);

static GLOBAL_AUDIT_LOG_MANAGER: OnceLock<Arc<AuditLogManager>> = OnceLock::new();

struct AuditLogManager {
    db: DatabaseConnection,
    buffer: parking_lot::Mutex<Vec<audit_log::ActiveModel>>,
    flush_lock: Mutex<()>,
    flush_pending: AtomicBool,
    shutdown_token: CancellationToken,
}

struct FlushPendingReset {
    manager: Arc<AuditLogManager>,
}

impl Drop for FlushPendingReset {
    fn drop(&mut self) {
        self.manager.flush_pending.store(false, Ordering::Release);
    }
}

pub fn init_global_audit_log_manager(db: DatabaseConnection) {
    let manager = Arc::new(AuditLogManager::new(db));
    match GLOBAL_AUDIT_LOG_MANAGER.set(manager.clone()) {
        Ok(()) => {
            drop(tokio::spawn(manager.start_background_task()));
        }
        Err(_) => {
            tracing::warn!("global audit log manager is already initialized; ignoring");
        }
    }
}

pub async fn flush_global_audit_log_manager() {
    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.flush().await;
    }
}

pub async fn shutdown_global_audit_log_manager() {
    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.cancel();
        manager.flush().await;
    }
}

/// 从 HttpRequest 提取的审计上下文
#[derive(Clone)]
pub struct AuditContext {
    pub user_id: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// 从 HttpRequest 提取的请求级审计元信息。
#[derive(Clone)]
pub struct AuditRequestInfo {
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(IntoParams))]
pub struct AuditLogFilterQuery {
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub after: Option<String>,
    pub before: Option<String>,
}

pub struct AuditLogFilters {
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
}

impl AuditLogFilters {
    pub fn from_query(query: &AuditLogFilterQuery) -> Self {
        Self {
            user_id: query.user_id,
            action: query.action.clone(),
            entity_type: query.entity_type.clone(),
            entity_id: query.entity_id,
            after: query
                .after
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            before: query
                .before
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditLogEntry {
    pub id: i64,
    pub user: Option<user_service::UserSummary>,
    pub action: AuditAction,
    pub entity_type: Option<String>,
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct ConfigUpdateDetails<'a> {
    pub value: &'a str,
}

#[derive(Serialize)]
pub struct ConfigActionDetails<'a> {
    pub action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_email: Option<&'a str>,
}

#[derive(Serialize)]
pub struct AdminCreateUserDetails<'a> {
    pub email: &'a str,
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AdminUpdateUserDetails {
    pub email_verified: bool,
    pub role: UserRole,
    pub status: UserStatus,
    pub storage_quota: i64,
    pub policy_group_id: Option<i64>,
}

#[derive(Serialize)]
pub struct AdminForceDeleteUserDetails {
    pub file_count: usize,
    pub folder_count: usize,
    pub share_count: u64,
    pub webdav_account_count: u64,
    pub upload_session_count: u64,
    pub lock_count: u64,
}

#[derive(Serialize)]
pub struct PolicyGroupAuditDetails {
    pub is_default: bool,
    pub is_enabled: bool,
    pub item_count: usize,
}

#[derive(Serialize)]
pub struct PolicyGroupMigrationDetails<'a> {
    pub source_group_id: i64,
    pub source_group_name: &'a str,
    pub target_group_id: i64,
    pub target_group_name: &'a str,
    pub affected_users: u64,
    pub migrated_assignments: u64,
}

#[derive(Serialize)]
pub struct StoragePolicyAuditDetails<'a> {
    pub driver_type: &'a str,
    pub remote_node_id: Option<i64>,
    pub max_file_size: i64,
    pub chunk_size: i64,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct BatchDeleteDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct BatchTransferDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    pub target_folder_id: Option<i64>,
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct ArchiveSelectionAuditDetails<'a> {
    pub file_ids: &'a [i64],
    pub folder_ids: &'a [i64],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_folder_id: Option<i64>,
}

#[derive(Serialize)]
pub struct UploadCancelAuditDetails<'a> {
    pub upload_id: &'a str,
}

#[derive(Serialize)]
pub struct FileAccessTokenAuditDetails<'a> {
    pub source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_key: Option<&'a str>,
}

#[derive(Serialize)]
pub struct PropertyAuditDetails<'a> {
    pub entity_type: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
}

#[derive(Serialize)]
pub struct FileVersionAuditDetails {
    pub version_id: i64,
}

#[derive(Serialize)]
pub struct TrashPurgeAllAuditDetails {
    pub purged: u32,
}

#[derive(Serialize)]
pub struct TaskRetryAuditDetails {
    pub kind: String,
    pub previous_attempt_count: i32,
}

#[derive(Serialize)]
pub struct AdminTaskCleanupAuditDetails {
    pub removed: u64,
    pub finished_before: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<crate::types::BackgroundTaskKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<crate::types::BackgroundTaskStatus>,
}

#[derive(Serialize)]
pub struct ShareBatchDeleteDetails<'a> {
    pub share_ids: &'a [i64],
    pub succeeded: u32,
    pub failed: u32,
}

#[derive(Serialize)]
pub struct ShareUpdateDetails {
    pub has_password: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub max_downloads: i64,
}

#[derive(Serialize)]
pub struct AuthSessionAuditDetails<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removed: Option<u64>,
    pub revoked_current: bool,
}

#[derive(Serialize)]
pub struct UserProfileAuditDetails<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'a str>,
}

#[derive(Serialize)]
pub struct UserAvatarSourceAuditDetails<'a> {
    pub source: &'a str,
}

#[derive(Serialize)]
pub struct RemoteNodeAuditDetails<'a> {
    pub base_url: &'a str,
    pub is_enabled: bool,
    pub enrollment_status: &'a str,
}

#[derive(Serialize)]
pub struct RemoteIngressProfileAuditDetails<'a> {
    pub profile_key: &'a str,
    pub driver_type: &'a str,
    pub is_default: bool,
}

#[derive(Serialize)]
pub struct LockAuditDetails {
    pub entity_type: crate::types::EntityType,
    pub entity_id: i64,
}

#[derive(Serialize)]
pub struct LockCleanupAuditDetails {
    pub removed: u64,
}

#[derive(Serialize)]
pub struct TeamAuditDetails<'a> {
    #[serde(skip_serializing_if = "str::is_empty")]
    pub description: &'a str,
    pub member_count: u64,
    pub storage_quota: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_group_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamCleanupAuditDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    pub retention_days: i64,
}

#[derive(Serialize)]
pub struct TeamMemberAddAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamMemberUpdateAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub previous_role: TeamMemberRole,
    pub next_role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Serialize)]
pub struct TeamMemberRemoveAuditDetails<'a> {
    pub member_user_id: i64,
    pub member_username: &'a str,
    pub removed_role: TeamMemberRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_role: Option<TeamMemberRole>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct TeamAuditEntryInfo {
    pub id: i64,
    pub action: AuditAction,
    pub actor: Option<user_service::UserSummary>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<user_service::UserSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_role: Option<TeamMemberRole>,
}

pub fn details<T: Serialize>(value: T) -> Option<serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(value) => Some(value),
        Err(e) => {
            tracing::warn!("failed to serialize audit details: {e}");
            None
        }
    }
}

async fn write_audit_model(db: &DatabaseConnection, model: audit_log::ActiveModel) {
    if let Err(e) = audit_log_repo::create(db, model).await {
        tracing::warn!("failed to write audit log: {e}");
    }
}

async fn write_audit_batch(db: &DatabaseConnection, batch: &mut Vec<audit_log::ActiveModel>) {
    if batch.is_empty() {
        return;
    }

    let total = batch.len();
    let mut models = std::mem::take(batch).into_iter();
    loop {
        let chunk = models
            .by_ref()
            .take(AUDIT_LOG_BATCH_SIZE)
            .collect::<Vec<_>>();
        if chunk.is_empty() {
            break;
        }

        let count = chunk.len();
        if let Err(e) = audit_log_repo::create_many(db, chunk).await {
            tracing::warn!(count, total, "failed to write audit log batch: {e}");
        }
    }
}

impl AuditLogManager {
    fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            buffer: parking_lot::Mutex::new(Vec::with_capacity(AUDIT_LOG_BATCH_SIZE)),
            flush_lock: Mutex::new(()),
            flush_pending: AtomicBool::new(false),
            shutdown_token: CancellationToken::new(),
        }
    }

    async fn record(self: &Arc<Self>, model: audit_log::ActiveModel) {
        let mut overflow_model = None;
        let should_flush;
        {
            let mut buffer = self.buffer.lock();
            if buffer.len() >= AUDIT_LOG_QUEUE_CAPACITY {
                overflow_model = Some(model);
                should_flush = false;
            } else {
                buffer.push(model);
                should_flush = buffer.len() >= AUDIT_LOG_BATCH_SIZE;
            }
        }

        if let Some(model) = overflow_model {
            tracing::warn!(
                capacity = AUDIT_LOG_QUEUE_CAPACITY,
                "audit log buffer is full; falling back to direct write"
            );
            self.schedule_flush();
            write_audit_model(&self.db, model).await;
            return;
        }

        if should_flush {
            self.schedule_flush();
        }
    }

    fn schedule_flush(self: &Arc<Self>) {
        if self
            .flush_pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let manager = Arc::clone(self);
        drop(tokio::spawn(async move {
            let _pending_reset = FlushPendingReset {
                manager: Arc::clone(&manager),
            };
            let _guard = manager.flush_lock.lock().await;
            manager.flush_buffer().await;
        }));
    }

    async fn start_background_task(self: Arc<Self>) {
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown_token.cancelled() => break,
                _ = tokio::time::sleep(AUDIT_LOG_FLUSH_INTERVAL) => {
                    if let Ok(_guard) = self.flush_lock.try_lock() {
                        self.flush_buffer().await;
                    }
                }
            }
        }
    }

    async fn flush(&self) {
        let _guard = self.flush_lock.lock().await;
        self.flush_buffer().await;
    }

    fn cancel(&self) {
        self.shutdown_token.cancel();
    }

    async fn flush_buffer(&self) {
        let mut models = {
            let mut buffer = self.buffer.lock();
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };
        write_audit_batch(&self.db, &mut models).await;
    }
}

impl AuditContext {
    pub fn system() -> Self {
        Self {
            user_id: 0,
            ip_address: None,
            user_agent: None,
        }
    }

    pub fn from_request(req: &HttpRequest, claims: &Claims) -> Self {
        AuditRequestInfo::from_request(req).to_context(claims.user_id)
    }
}

impl AuditRequestInfo {
    pub fn from_request(req: &HttpRequest) -> Self {
        Self {
            ip_address: req
                .connection_info()
                .realip_remote_addr()
                .map(|s| bounded_audit_value(s, MAX_AUDIT_IP_ADDRESS_LEN)),
            user_agent: req
                .headers()
                .get("user-agent")
                .and_then(|v| v.to_str().ok())
                .map(|s| bounded_audit_value(s, MAX_AUDIT_USER_AGENT_LEN)),
        }
    }

    pub fn to_context(&self, user_id: i64) -> AuditContext {
        AuditContext {
            user_id,
            ip_address: self.ip_address.clone(),
            user_agent: self.user_agent.clone(),
        }
    }
}

fn bounded_audit_value(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }

    let mut end = max_len;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

/// Best-effort 审计日志。默认入队批量写入，DB 错误只 warn 不传播。
pub async fn log(
    state: &PrimaryAppState,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: Option<&str>,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) {
    // 检查运行时配置
    if matches!(
        state.runtime_config.get_bool("audit_log_enabled"),
        Some(false)
    ) {
        return;
    }

    let model = audit_log::ActiveModel {
        id: Default::default(),
        user_id: Set(ctx.user_id),
        action: Set(action),
        entity_type: Set(entity_type.map(|s| s.to_string())),
        entity_id: Set(entity_id),
        entity_name: Set(entity_name.map(|s| s.to_string())),
        details: Set(details.map(|v| v.to_string())),
        ip_address: Set(ctx.ip_address.clone()),
        user_agent: Set(ctx.user_agent.clone()),
        created_at: Set(Utc::now()),
    };

    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.record(model).await;
    } else {
        write_audit_model(&state.db, model).await;
    }
}

async fn query_models(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<audit_log::Model>> {
    flush_global_audit_log_manager().await;
    load_offset_page(limit, offset, 200, |limit, offset| async move {
        audit_log_repo::find_with_filters(
            &state.db,
            audit_log_repo::AuditLogQuery {
                user_id: filters.user_id,
                action: filters.action.as_deref(),
                entity_type: filters.entity_type.as_deref(),
                entity_id: filters.entity_id,
                after: filters.after,
                before: filters.before,
                limit,
                offset,
                sort_by,
                sort_order,
            },
        )
        .await
    })
    .await
}

async fn build_audit_entries(
    state: &PrimaryAppState,
    entries: Vec<audit_log::Model>,
) -> Result<Vec<AuditLogEntry>> {
    let user_ids: Vec<i64> = entries
        .iter()
        .map(|entry| entry.user_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let users = user_service::user_summaries_by_ids(
        state,
        &user_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    Ok(entries
        .into_iter()
        .map(|model| AuditLogEntry {
            id: model.id,
            user: users.get(&model.user_id).cloned(),
            action: model.action,
            entity_type: model.entity_type,
            entity_id: model.entity_id,
            entity_name: model.entity_name,
            details: model.details,
            ip_address: model.ip_address,
            user_agent: model.user_agent,
            created_at: model.created_at,
        })
        .collect())
}

pub async fn query(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<AuditLogEntry>> {
    let page = query_models(state, filters, limit, offset, sort_by, sort_order).await?;
    let items = build_audit_entries(state, page.items).await?;
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

fn parse_team_member_role(value: Option<&serde_json::Value>) -> Option<TeamMemberRole> {
    serde_json::from_value(value?.clone()).ok()
}

fn parse_i64_field(details: &serde_json::Value, key: &str) -> Option<i64> {
    details.get(key)?.as_i64()
}

fn build_team_audit_entry(
    entry: audit_log::Model,
    users: &HashMap<i64, user_service::UserSummary>,
) -> TeamAuditEntryInfo {
    let parsed_details = entry
        .details
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());

    let member_user_id = parsed_details
        .as_ref()
        .and_then(|details| parse_i64_field(details, "member_user_id"));
    let role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("role")))
        .or_else(|| {
            parsed_details
                .as_ref()
                .and_then(|details| parse_team_member_role(details.get("removed_role")))
        });
    let previous_role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("previous_role")));
    let next_role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("next_role")));

    TeamAuditEntryInfo {
        id: entry.id,
        action: entry.action,
        actor: users.get(&entry.user_id).cloned(),
        created_at: entry.created_at,
        member: member_user_id.and_then(|member_user_id| users.get(&member_user_id).cloned()),
        role,
        previous_role,
        next_role,
    }
}

pub async fn query_team_entries(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<TeamAuditEntryInfo>> {
    let page = query_models(
        state,
        filters,
        limit,
        offset,
        AdminAuditLogSortBy::CreatedAt,
        SortOrder::Desc,
    )
    .await?;
    let mut user_ids = HashSet::new();
    for entry in &page.items {
        user_ids.insert(entry.user_id);
        if let Some(member_user_id) = entry
            .details
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .as_ref()
            .and_then(|details| parse_i64_field(details, "member_user_id"))
        {
            user_ids.insert(member_user_id);
        }
    }
    let user_ids: Vec<i64> = user_ids.into_iter().collect();
    let users = user_service::user_summaries_by_ids(
        state,
        &user_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;
    let items = page
        .items
        .into_iter()
        .map(|entry| build_team_audit_entry(entry, &users))
        .collect();

    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

/// 清理过期审计日志
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    flush_global_audit_log_manager().await;
    let retention_days = state
        .runtime_config
        .get_i64("audit_log_retention_days")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config.get("audit_log_retention_days") {
                tracing::warn!(
                    "invalid audit_log_retention_days value '{}', using default",
                    raw
                );
            }
            DEFAULT_RETENTION_DAYS
        });

    let cutoff = Utc::now() - Duration::days(retention_days);
    let deleted = audit_log_repo::delete_before(&state.db, cutoff).await?;
    if deleted > 0 {
        tracing::info!("cleaned up {deleted} expired audit log entries");
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use actix_web::test as actix_test;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
    use std::sync::Arc;

    use super::{
        AuditAction, AuditContext, AuditRequestInfo, MAX_AUDIT_IP_ADDRESS_LEN,
        MAX_AUDIT_USER_AGENT_LEN, bounded_audit_value,
    };
    use crate::entities::audit_log;

    async fn in_memory_db() -> sea_orm::DatabaseConnection {
        let db = sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("in-memory db should connect");
        migration::Migrator::up(&db, None)
            .await
            .expect("migrations should run");
        db
    }

    fn audit_model(index: i64) -> audit_log::ActiveModel {
        audit_log::ActiveModel {
            id: Default::default(),
            user_id: Set(42),
            action: Set(AuditAction::FileUpload),
            entity_type: Set(Some("file".to_string())),
            entity_id: Set(Some(index)),
            entity_name: Set(Some(format!("file-{index}.txt"))),
            details: Set(None),
            ip_address: Set(None),
            user_agent: Set(None),
            created_at: Set(chrono::Utc::now()),
        }
    }

    #[test]
    fn bounded_audit_value_truncates_without_splitting_utf8() {
        assert_eq!(bounded_audit_value("abcdef", 3), "abc");
        assert_eq!(bounded_audit_value("猫猫猫", 4), "猫");
    }

    #[test]
    fn request_audit_info_truncates_user_controlled_headers() {
        let long_ip = "1".repeat(MAX_AUDIT_IP_ADDRESS_LEN + 32);
        let long_user_agent = "a".repeat(MAX_AUDIT_USER_AGENT_LEN + 32);
        let req = actix_test::TestRequest::default()
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .insert_header(("X-Forwarded-For", long_ip.as_str()))
            .insert_header(("User-Agent", long_user_agent.as_str()))
            .to_http_request();

        let info = AuditRequestInfo::from_request(&req);

        assert_eq!(
            info.ip_address.as_deref(),
            Some(&long_ip[..MAX_AUDIT_IP_ADDRESS_LEN])
        );
        assert_eq!(
            info.user_agent.as_deref(),
            Some(&long_user_agent[..MAX_AUDIT_USER_AGENT_LEN])
        );
    }

    #[test]
    fn audit_action_strings_match_existing_contract() {
        let cases = [
            (AuditAction::AdminCreateUser, "admin_create_user"),
            (AuditAction::AdminForceDeleteUser, "admin_force_delete_user"),
            (AuditAction::AdminCreateTeam, "admin_create_team"),
            (AuditAction::AdminCreatePolicy, "admin_create_policy"),
            (AuditAction::AdminUpdatePolicy, "admin_update_policy"),
            (AuditAction::AdminDeletePolicy, "admin_delete_policy"),
            (
                AuditAction::AdminCreatePolicyGroup,
                "admin_create_policy_group",
            ),
            (AuditAction::AdminArchiveTeam, "admin_archive_team"),
            (AuditAction::AdminRestoreTeam, "admin_restore_team"),
            (
                AuditAction::AdminDeletePolicyGroup,
                "admin_delete_policy_group",
            ),
            (
                AuditAction::AdminMigratePolicyGroupUsers,
                "admin_migrate_policy_group_users",
            ),
            (
                AuditAction::AdminRevokeUserSessions,
                "admin_revoke_user_sessions",
            ),
            (
                AuditAction::AdminResetUserPassword,
                "admin_reset_user_password",
            ),
            (AuditAction::AdminUpdateTeam, "admin_update_team"),
            (
                AuditAction::AdminUpdatePolicyGroup,
                "admin_update_policy_group",
            ),
            (AuditAction::AdminUpdateUser, "admin_update_user"),
            (AuditAction::AdminDeleteConfig, "admin_delete_config"),
            (AuditAction::AdminDeleteShare, "admin_delete_share"),
            (AuditAction::AdminForceUnlock, "admin_force_unlock"),
            (
                AuditAction::AdminCleanupExpiredLocks,
                "admin_cleanup_expired_locks",
            ),
            (AuditAction::AdminCleanupTasks, "admin_cleanup_tasks"),
            (
                AuditAction::AdminCreateRemoteNode,
                "admin_create_remote_node",
            ),
            (
                AuditAction::AdminUpdateRemoteNode,
                "admin_update_remote_node",
            ),
            (
                AuditAction::AdminDeleteRemoteNode,
                "admin_delete_remote_node",
            ),
            (AuditAction::AdminTestRemoteNode, "admin_test_remote_node"),
            (
                AuditAction::AdminCreateRemoteNodeEnrollmentToken,
                "admin_create_remote_node_enrollment_token",
            ),
            (
                AuditAction::AdminCreateRemoteIngressProfile,
                "admin_create_remote_ingress_profile",
            ),
            (
                AuditAction::AdminUpdateRemoteIngressProfile,
                "admin_update_remote_ingress_profile",
            ),
            (
                AuditAction::AdminDeleteRemoteIngressProfile,
                "admin_delete_remote_ingress_profile",
            ),
            (AuditAction::BatchCopy, "batch_copy"),
            (AuditAction::BatchDelete, "batch_delete"),
            (AuditAction::BatchMove, "batch_move"),
            (AuditAction::ConfigActionExecute, "config_action_execute"),
            (AuditAction::ConfigUpdate, "config_update"),
            (AuditAction::FileCopy, "file_copy"),
            (AuditAction::FileCreate, "file_create"),
            (AuditAction::FileDelete, "file_delete"),
            (AuditAction::FileDownload, "file_download"),
            (AuditAction::FileDirectLinkCreate, "file_direct_link_create"),
            (AuditAction::FileEdit, "file_edit"),
            (AuditAction::FileMove, "file_move"),
            (AuditAction::FileRename, "file_rename"),
            (AuditAction::FileUpload, "file_upload"),
            (
                AuditAction::FilePreviewLinkCreate,
                "file_preview_link_create",
            ),
            (AuditAction::FileWopiOpen, "file_wopi_open"),
            (AuditAction::FileUploadCancel, "file_upload_cancel"),
            (AuditAction::FileRestore, "file_restore"),
            (AuditAction::FilePurge, "file_purge"),
            (AuditAction::FileLock, "file_lock"),
            (AuditAction::FileUnlock, "file_unlock"),
            (AuditAction::FileVersionRestore, "file_version_restore"),
            (AuditAction::FileVersionDelete, "file_version_delete"),
            (AuditAction::FolderCopy, "folder_copy"),
            (AuditAction::FolderCreate, "folder_create"),
            (AuditAction::FolderDelete, "folder_delete"),
            (AuditAction::FolderMove, "folder_move"),
            (AuditAction::FolderPolicyChange, "folder_policy_change"),
            (AuditAction::FolderRename, "folder_rename"),
            (AuditAction::FolderRestore, "folder_restore"),
            (AuditAction::FolderPurge, "folder_purge"),
            (AuditAction::FolderLock, "folder_lock"),
            (AuditAction::FolderUnlock, "folder_unlock"),
            (AuditAction::PropertySet, "property_set"),
            (AuditAction::PropertyDelete, "property_delete"),
            (AuditAction::ShareBatchDelete, "share_batch_delete"),
            (AuditAction::ShareCreate, "share_create"),
            (AuditAction::ShareDelete, "share_delete"),
            (AuditAction::ShareUpdate, "share_update"),
            (AuditAction::SystemSetup, "system_setup"),
            (AuditAction::TeamArchive, "team_archive"),
            (AuditAction::TeamCleanupExpired, "team_cleanup_expired"),
            (AuditAction::TeamCreate, "team_create"),
            (AuditAction::TeamMemberAdd, "team_member_add"),
            (AuditAction::TeamMemberRemove, "team_member_remove"),
            (AuditAction::TeamMemberUpdate, "team_member_update"),
            (AuditAction::TeamRestore, "team_restore"),
            (AuditAction::TeamUpdate, "team_update"),
            (AuditAction::TaskRetry, "task_retry"),
            (AuditAction::ArchiveCompress, "archive_compress"),
            (AuditAction::ArchiveExtract, "archive_extract"),
            (AuditAction::ArchiveDownload, "archive_download"),
            (AuditAction::TrashPurgeAll, "trash_purge_all"),
            (
                AuditAction::RemoteEnrollmentRedeem,
                "remote_enrollment_redeem",
            ),
            (AuditAction::RemoteEnrollmentAck, "remote_enrollment_ack"),
            (
                AuditAction::UserRevokeOtherSessions,
                "user_revoke_other_sessions",
            ),
            (AuditAction::UserRevokeSession, "user_revoke_session"),
            (
                AuditAction::UserUpdatePreferences,
                "user_update_preferences",
            ),
            (AuditAction::UserUpdateProfile, "user_update_profile"),
            (AuditAction::UserUploadAvatar, "user_upload_avatar"),
            (AuditAction::UserSetAvatarSource, "user_set_avatar_source"),
            (AuditAction::UserUpdateWopiInfo, "user_update_wopi_info"),
            (AuditAction::WebdavAccountCreate, "webdav_account_create"),
            (AuditAction::WebdavAccountDelete, "webdav_account_delete"),
            (AuditAction::WebdavAccountToggle, "webdav_account_toggle"),
            (AuditAction::UserChangePassword, "user_change_password"),
            (
                AuditAction::UserConfirmPasswordReset,
                "user_confirm_password_reset",
            ),
            (
                AuditAction::UserConfirmEmailChange,
                "user_confirm_email_change",
            ),
            (
                AuditAction::UserConfirmRegistration,
                "user_confirm_registration",
            ),
            (AuditAction::UserLogin, "user_login"),
            (AuditAction::UserLogout, "user_logout"),
            (
                AuditAction::UserRefreshTokenReuseDetected,
                "user_refresh_token_reuse_detected",
            ),
            (
                AuditAction::UserRequestEmailChange,
                "user_request_email_change",
            ),
            (
                AuditAction::UserRequestPasswordReset,
                "user_request_password_reset",
            ),
            (AuditAction::UserRegister, "user_register"),
            (
                AuditAction::UserResendEmailChange,
                "user_resend_email_change",
            ),
            (
                AuditAction::UserResendRegistration,
                "user_resend_registration",
            ),
        ];

        for (action, expected) in cases {
            assert_eq!(action.as_str(), expected);
            assert_eq!(action.as_ref(), expected);
            assert_eq!(action.to_string(), expected);
            assert_eq!(AuditAction::from_str_name(expected), Some(action));
        }
    }

    #[tokio::test]
    async fn log_writes_synchronously_without_global_manager() {
        // This test assumes GLOBAL_AUDIT_LOG_MANAGER has not been initialized in this test binary.
        let db = in_memory_db().await;

        let runtime_config = std::sync::Arc::new(crate::config::RuntimeConfig::new());
        runtime_config
            .reload(&db)
            .await
            .expect("runtime config should load");
        let cache = crate::cache::create_cache(&crate::config::CacheConfig {
            enabled: false,
            ..Default::default()
        })
        .await;
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY,
            );
        let state = crate::runtime::PrimaryAppState {
            db: db.clone(),
            driver_registry: std::sync::Arc::new(crate::storage::DriverRegistry::new()),
            runtime_config,
            policy_snapshot: std::sync::Arc::new(crate::storage::PolicySnapshot::new()),
            config: std::sync::Arc::new(crate::config::Config::default()),
            cache,
            mail_sender: crate::services::mail_service::memory_sender(),
            storage_change_tx,
            share_download_rollback,
        };

        super::log(
            &state,
            &AuditContext {
                user_id: 42,
                ip_address: None,
                user_agent: None,
            },
            AuditAction::FileUpload,
            Some("file"),
            Some(7),
            Some("report.txt"),
            None,
        )
        .await;

        let count = audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq(AuditAction::FileUpload))
            .count(&db)
            .await
            .expect("audit query should succeed");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn audit_log_manager_flushes_threshold_batch() {
        let db = in_memory_db().await;
        let manager = Arc::new(super::AuditLogManager::new(db.clone()));

        for index in 0..super::AUDIT_LOG_BATCH_SIZE {
            let index = i64::try_from(index).expect("audit batch test index fits i64");
            manager.record(audit_model(index)).await;
        }
        manager.flush().await;
        manager.cancel();

        let count = audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq(AuditAction::FileUpload))
            .count(&db)
            .await
            .expect("audit query should succeed");
        let expected =
            u64::try_from(super::AUDIT_LOG_BATCH_SIZE).expect("audit batch size fits u64");
        assert_eq!(count, expected);
    }
}
