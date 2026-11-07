use crate::db::repository::file_repo;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{events::storage_change, workspace::storage::WorkspaceStorageScope};
use aster_drive_model::entities::file;
use aster_forge_db::transaction;
use sea_orm::ConnectionTrait;

pub(crate) async fn delete_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
    source: crate::services::workspace::activity::ActivitySource,
) -> Result<()> {
    tracing::debug!(scope = ?scope, file_id = id, "soft deleting file");
    let txn = transaction::begin(state.writer_db()).await?;
    let file = delete_in_scope_on(
        &txn,
        scope,
        id,
        &crate::services::files::lock::SubmittedLockCredentials::none(),
        source,
    )
    .await?;
    transaction::commit(txn).await?;
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileTrashed,
            scope,
            vec![file.id],
            vec![],
            vec![file.folder_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        file_id = file.id,
        folder_id = file.folder_id,
        "soft deleted file"
    );
    Ok(())
}

/// Soft-deletes one locked file row on the caller's transaction.
///
/// 所有入口传递实际锁凭据；协议不能用裸 boolean 绕过产品层锁校验。
/// 父集合成员移除与目标内容修改是两个独立约束，都在同一个 writer transaction 内验证。
pub(crate) async fn delete_in_scope_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    id: i64,
    credentials: &crate::services::files::lock::SubmittedLockCredentials<'_>,
    source: crate::services::workspace::activity::ActivitySource,
) -> Result<file::Model> {
    crate::services::files::trash::coordination::admit_mutation_on(db, scope).await?;
    let access = crate::services::workspace::scope::authorize_workspace_on(
        db,
        scope,
        crate::services::workspace::scope::WorkspaceOperation::Delete,
        &crate::services::workspace::scope::WorkspaceAccessConstraints::default(),
    )
    .await?;
    let snapshot = file_repo::find_by_id(db, id).await?;
    access.file(&snapshot).await?;
    let file =
        crate::services::files::lock::enforce_file_mutation_on(db, &snapshot, credentials).await?;
    let workspace = crate::services::files::lock::LockWorkspace::from_file(&file)?;
    crate::services::files::lock::enforce_collection_membership_mutation_on(
        db,
        workspace,
        file.folder_id,
        credentials,
    )
    .await?;
    file_repo::soft_delete(db, id).await?;
    let ctx = crate::services::workspace::activity::ActivityContext::actor(
        db,
        scope.actor_user_id(),
        source,
    )
    .await?;
    let mut after = file.clone();
    after.deleted_at = Some(chrono::Utc::now());
    crate::services::workspace::activity::file_changed(
        db,
        &ctx,
        &file,
        &mut after,
        aster_drive_model::types::AuditAction::FileDelete,
    )
    .await?;
    Ok(file)
}

/// 删除文件（软删除 → 回收站）
pub async fn delete(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    delete_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        id,
        crate::services::workspace::activity::ActivitySource::Rest,
    )
    .await
}
