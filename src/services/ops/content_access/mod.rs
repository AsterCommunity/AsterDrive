//! Administrator authorization is separate from resource ownership and storage delivery.
mod download;
mod types;
mod workspace;

pub use download::{download, init_download};
pub use types::*;
pub use workspace::{
    AdminFolderInfo, AdminWorkspaceContents, AdminWorkspaceQuery, get_file, get_folder,
    get_thumbnail, list_trash, list_versions, list_workspace,
};

use crate::{
    api::api_error_code::ApiErrorCode,
    db::repository::{admin_content_access_repo, config_repo, team_repo, user_repo},
    errors::{Result, auth_forbidden_with_code},
    runtime::SharedRuntimeState,
    services::{auth::local::Claims, ops::audit, workspace::scope::WorkspaceResourceScope},
};
use aster_drive_model::entities::admin_content_access;
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

async fn require_target(
    state: &impl SharedRuntimeState,
    scope: &AccessScope,
) -> Result<WorkspaceResourceScope> {
    let resource = match scope.scope_kind {
        AdminScopeKind::Personal => WorkspaceResourceScope::Personal {
            user_id: scope.target_id,
        },
        AdminScopeKind::Team => WorkspaceResourceScope::Team {
            team_id: scope.target_id,
        },
    };
    // 管理端的错误契约隐藏目标不存在/失效细节；生命周期决策由统一授权模块拥有。
    crate::services::workspace::scope::authorize_resource_lifecycle_on(state.writer_db(), resource)
        .await
        .map_err(|error| match error {
            crate::errors::AsterError::RecordNotFound(_)
            | crate::errors::AsterError::AuthForbidden(_) => {
                denied(ApiErrorCode::AdminContentTargetUnavailable)
            }
            error => error,
        })?;
    Ok(resource)
}

fn validate_request(request: &CreateAdminContentAccess) -> Result<WorkspaceAccess> {
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
    Ok(WorkspaceAccess {
        scope: AccessScope {
            scope_kind: request.scope_kind,
            target_id: request.target_id,
        },
        reason,
        remark: request
            .remark
            .as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty()),
    })
}

pub async fn create(
    state: &impl SharedRuntimeState,
    claims: &Claims,
    ctx: &audit::AuditContext,
    request: &CreateAdminContentAccess,
) -> Result<AdminContentAccessInfo> {
    crate::services::workspace::scope::authorize_admin_actor(
        state,
        claims,
        crate::services::workspace::scope::AdminContentOperation::Inspect,
    )
    .await?;
    let payload = validate_request(request)?;
    require_target(state, &payload.scope).await?;
    let txn = state.writer_db().begin().await?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let info = persist(
        &txn,
        claims,
        &AccessPayload::Workspace(payload.clone()),
        &id,
    )
    .await?;
    record(
        &txn,
        ctx,
        &payload.scope,
        Some(&payload),
        audit::AuditAction::AdminWorkspaceInspect,
        (None, None),
        "authorized",
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

async fn record<C: sea_orm::ConnectionTrait>(
    db: &C,
    ctx: &audit::AuditContext,
    scope: &AccessScope,
    inspection: Option<&WorkspaceAccess>,
    action: audit::AuditAction,
    target: (Option<i64>, Option<i64>),
    result: &str,
) -> Result<()> {
    let (entity_type, entity_id) = match scope.scope_kind {
        AdminScopeKind::Team => (audit::AuditEntityType::Team, scope.target_id),
        AdminScopeKind::Personal => (audit::AuditEntityType::User, scope.target_id),
    };
    // Snapshot target identity with the evidence; later account renames must not rewrite history.
    let entity_name = match scope.scope_kind {
        AdminScopeKind::Personal => user_repo::find_by_id(db, scope.target_id).await?.username,
        AdminScopeKind::Team => team_repo::find_by_id(db, scope.target_id).await?.name,
    };
    let mut details = serde_json::json!({
        "scope_kind": scope.scope_kind, "target_id": scope.target_id,
        "file_id": target.0, "revision_id": target.1,
        "result": result, "source": "admin",
    });
    if let Some(inspection) = inspection {
        details["reason"] = serde_json::json!(inspection.reason);
        if let Some(remark) = &inspection.remark {
            details["remark"] = serde_json::json!(remark);
        }
    }
    audit::log_required(
        db,
        audit::AuditLogInput {
            ctx,
            action,
            entity_type,
            entity_id: Some(entity_id),
            entity_name: Some(&entity_name),
        },
        details,
    )
    .await
}
