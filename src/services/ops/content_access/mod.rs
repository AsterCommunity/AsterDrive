//! Administrator authorization is separate from resource ownership and storage delivery.
mod download;
mod types;
mod workspace;

pub use download::{download, init_download};
pub use types::*;
pub use workspace::{AdminWorkspaceContents, AdminWorkspaceQuery, list_workspace};

use crate::{
    api::api_error_code::ApiErrorCode,
    db::repository::{
        admin_content_access_repo, auth_session_repo, config_repo, team_repo, user_repo,
    },
    errors::{Result, auth_forbidden_with_code},
    runtime::SharedRuntimeState,
    services::{auth::local::Claims, ops::audit, workspace::scope::WorkspaceResourceScope},
};
use aster_drive_model::{entities::admin_content_access, types::UserStatus};
use chrono::{Duration, Utc};
use sea_orm::{Set, TransactionTrait};

use crate::config::definitions::ADMIN_WORKSPACE_CONTENT_ACCESS_ENABLED_KEY;
const ACCESS_TTL_SECONDS: i64 = 10 * 60;

fn denied(code: ApiErrorCode) -> crate::errors::AsterError {
    auth_forbidden_with_code(code, "administrator content access denied")
}

/// Read the authority, not a potentially stale per-instance configuration projection.
pub async fn is_enabled(state: &impl SharedRuntimeState) -> Result<bool> {
    Ok(config_repo::find_by_key(
        state.writer_db(),
        ADMIN_WORKSPACE_CONTENT_ACCESS_ENABLED_KEY,
    )
    .await?
    .and_then(|m| crate::config::bool_like::parse_bool_like(&m.value))
    .unwrap_or(false))
}

async fn require_actor(state: &impl SharedRuntimeState, claims: &Claims) -> Result<()> {
    if !is_enabled(state).await? {
        return Err(denied(ApiErrorCode::AdminContentDisabled));
    }
    let actor = user_repo::find_by_id(state.writer_db(), claims.user_id).await?;
    if !actor.role.is_admin()
        || !actor.status.is_active()
        || actor.session_version != claims.session_version
        || claims.password_change
    {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let session =
        auth_session_repo::find_by_id_for_user(state.writer_db(), claims.user_id, &claims.sid)
            .await?
            .ok_or_else(|| denied(ApiErrorCode::AdminContentContextInvalid))?;
    if session.revoked_at.is_some() || session.refresh_expires_at <= Utc::now() {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    Ok(())
}

async fn require_target(
    state: &impl SharedRuntimeState,
    payload: &AccessPayload,
) -> Result<WorkspaceResourceScope> {
    match payload.scope_kind {
        AdminScopeKind::Personal => {
            let user = user_repo::find_by_id(state.writer_db(), payload.target_id)
                .await
                .map_err(|e| match e {
                    crate::errors::AsterError::RecordNotFound(_) => {
                        denied(ApiErrorCode::AdminContentTargetUnavailable)
                    }
                    e => e,
                })?;
            if user.status != UserStatus::Active {
                return Err(denied(ApiErrorCode::AdminContentTargetUnavailable));
            }
            Ok(WorkspaceResourceScope::Personal { user_id: user.id })
        }
        AdminScopeKind::Team => {
            let team = team_repo::find_by_id(state.writer_db(), payload.target_id)
                .await
                .map_err(|e| match e {
                    crate::errors::AsterError::RecordNotFound(_) => {
                        denied(ApiErrorCode::AdminContentTargetUnavailable)
                    }
                    e => e,
                })?;
            if team.archived_at.is_some() {
                return Err(denied(ApiErrorCode::AdminContentTargetUnavailable));
            }
            Ok(WorkspaceResourceScope::Team { team_id: team.id })
        }
    }
}

fn validate_request(request: &CreateAdminContentAccess) -> Result<AccessPayload> {
    let reason = request
        .reason
        .ok_or_else(|| denied(ApiErrorCode::AdminContentReasonRequired))?;
    if request.target_id <= 0
        || request
            .remark
            .as_ref()
            .is_some_and(|s| s.chars().count() > 500 || s.chars().any(|c| c.is_control()))
    {
        return Err(crate::errors::AsterError::validation_error(
            "invalid content access request",
        ));
    }
    Ok(AccessPayload {
        scope_kind: request.scope_kind,
        target_id: request.target_id,
        reason,
        remark: request
            .remark
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
        download: None,
    })
}

pub async fn create(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    ctx: &audit::AuditContext,
    request: &CreateAdminContentAccess,
) -> Result<AdminContentAccessInfo> {
    require_actor(state, claims).await?;
    let payload = validate_request(request)?;
    require_target(state, &payload).await?;
    let txn = state.writer_db().begin().await?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let info = persist(&txn, claims, &payload, &id).await?;
    record(
        &txn,
        ctx,
        &payload,
        audit::AuditAction::AdminWorkspaceInspect,
        (None, None),
        "authorized",
        false,
    )
    .await?;
    txn.commit().await?;
    Ok(info)
}

async fn persist<C: sea_orm::ConnectionTrait>(
    db: &C,
    claims: &Claims,
    payload: &AccessPayload,
    id: &str,
) -> Result<AdminContentAccessInfo> {
    let expires_at = Utc::now() + Duration::seconds(ACCESS_TTL_SECONDS);
    let id = id.to_owned();
    let encoded = serde_json::to_string(payload)
        .map_err(|_| crate::errors::AsterError::internal_error("cannot encode content access"))?;
    admin_content_access_repo::insert(
        db,
        admin_content_access::ActiveModel {
            id: Set(id.clone()),
            actor_user_id: Set(claims.user_id),
            session_id: Set(claims.sid.clone()),
            payload: Set(encoded),
            expires_at: Set(expires_at),
        },
    )
    .await?;
    Ok(AdminContentAccessInfo { id, expires_at })
}

async fn load(state: &impl SharedRuntimeState, claims: &Claims, id: &str) -> Result<AccessPayload> {
    require_actor(state, claims).await?;
    if id.is_empty() {
        return Err(denied(ApiErrorCode::AdminContentContextRequired));
    }
    if id.len() > 64 {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let grant = admin_content_access_repo::find(state.writer_db(), id)
        .await?
        .ok_or_else(|| denied(ApiErrorCode::AdminContentContextInvalid))?;
    if grant.actor_user_id != claims.user_id
        || grant.session_id != claims.sid
        || grant.expires_at <= Utc::now()
    {
        return Err(denied(ApiErrorCode::AdminContentContextInvalid));
    }
    let payload: AccessPayload = serde_json::from_str(&grant.payload)
        .map_err(|_| denied(ApiErrorCode::AdminContentContextInvalid))?;
    require_target(state, &payload).await?;
    Ok(payload)
}

async fn record<C: sea_orm::ConnectionTrait>(
    db: &C,
    ctx: &audit::AuditContext,
    payload: &AccessPayload,
    action: audit::AuditAction,
    target: (Option<i64>, Option<i64>),
    result: &str,
    has_range: bool,
) -> Result<()> {
    let (entity_type, entity_id) = match payload.scope_kind {
        AdminScopeKind::Team => (audit::AuditEntityType::Team, payload.target_id),
        AdminScopeKind::Personal => (audit::AuditEntityType::User, payload.target_id),
    };
    audit::log_required(
        db,
        audit::AuditLogInput {
            ctx,
            action,
            entity_type,
            entity_id: Some(entity_id),
            entity_name: None,
        },
        serde_json::json!({
            "scope_kind": payload.scope_kind, "target_id": payload.target_id,
            "file_id": target.0, "revision_id": target.1, "reason": payload.reason,
            "remark": payload.remark, "result": result, "has_range": has_range, "source": "admin",
        }),
    )
    .await
}
