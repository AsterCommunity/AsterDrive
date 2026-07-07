//! 上传服务子模块：`scope`。

use crate::db::repository::upload_session_repo;
use crate::entities::upload_session;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::storage::{self, WorkspaceStorageScope};
use sea_orm::ConnectionTrait;

pub(super) fn personal_scope(user_id: i64) -> WorkspaceStorageScope {
    WorkspaceStorageScope::Personal { user_id }
}

pub(super) fn team_scope(team_id: i64, actor_user_id: i64) -> WorkspaceStorageScope {
    WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    }
}

fn ensure_personal_upload_session_scope(session: &upload_session::Model) -> Result<()> {
    if session.team_id.is_some() {
        return Err(AsterError::auth_forbidden(
            "upload session belongs to a team workspace",
        ));
    }
    Ok(())
}

fn ensure_team_upload_session_scope(session: &upload_session::Model, team_id: i64) -> Result<()> {
    if session.team_id != Some(team_id) {
        return Err(AsterError::auth_forbidden(
            "upload session is outside team workspace",
        ));
    }
    Ok(())
}

async fn load_upload_session_with_db<C: ConnectionTrait>(
    state: &PrimaryAppState,
    db: &C,
    scope: WorkspaceStorageScope,
    upload_id: &str,
) -> Result<upload_session::Model> {
    let session = upload_session_repo::find_by_id(db, upload_id).await?;
    // 上传 session 始终绑定发起人：complete/cancel 只能由原作者操作。
    // 多写入者会打破分片顺序校验、blob 去重、配额扣减等单写入语义——
    // 即使团队成员之间也不共享 session。团队 scope 额外再校验成员身份，
    // 防止跨团队劫持。
    //
    // 发起人掉线后的残留资源（chunk 临时文件 / S3 temp 对象 / session DB 行）
    // 由 `cleanup_expired`（expires_at 到期，默认 24h）兜底。注意 storage_used
    // 配额并不会在 init 时预占——只在 complete 时写入，所以这里不会泄漏配额。
    crate::utils::verify_owner(session.user_id, scope.actor_user_id(), "upload session")?;
    if let Some(team_id) = scope.team_id() {
        storage::require_team_access_with_db(state, db, team_id, scope.actor_user_id()).await?;
        ensure_team_upload_session_scope(&session, team_id)?;
    } else {
        ensure_personal_upload_session_scope(&session)?;
    }
    Ok(session)
}

pub(super) async fn load_upload_session(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    upload_id: &str,
) -> Result<upload_session::Model> {
    load_upload_session_with_db(state, state.writer_db(), scope, upload_id).await
}

pub(super) async fn load_upload_session_for_read(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    upload_id: &str,
) -> Result<upload_session::Model> {
    load_upload_session_with_db(state, state.reader_db(), scope, upload_id).await
}
