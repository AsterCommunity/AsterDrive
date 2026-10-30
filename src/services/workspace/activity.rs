//! Workspace business facts and attribution. Persistence errors roll back the caller's mutation.
mod batch;
use crate::{
    db::repository::{user_repo, workspace_activity_repo},
    errors::Result,
};
use aster_drive_model::entities::{file, folder, workspace_activity};
use aster_drive_model::types::AuditAction;
pub(crate) use batch::ActivityBatch;
use chrono::Utc;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ActivityIdentity {
    pub user_id: Option<i64>,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WorkspaceActivity {
    pub id: i64,
    pub event_id: String,
    pub operation_id: String,
    pub entity_type: String,
    pub entity_id: i64,
    pub entity_name: String,
    pub actor: ActivityIdentity,
    pub action: AuditAction,
    pub source: ActivitySource,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, ToSchema)
)]
pub struct ActivityListQuery {
    pub before_id: Option<i64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WorkspaceActivityPage {
    pub items: Vec<WorkspaceActivity>,
    pub next_before_id: Option<i64>,
}

pub(crate) async fn list_activities(
    state: &impl crate::runtime::SharedRuntimeState,
    scope: super::scope::WorkspaceStorageScope,
    entity: Option<(&str, i64)>,
    query: &ActivityListQuery,
) -> Result<WorkspaceActivityPage> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) || query.before_id.is_some_and(|id| id <= 0) {
        return Err(crate::errors::AsterError::validation_error(
            "invalid activity page",
        ));
    }
    match entity {
        Some(("file", id)) => {
            super::scope::verify_file_access_for_read(state, scope, id).await?;
        }
        Some(("folder", id)) => {
            super::scope::verify_folder_access_for_read(state, scope, id).await?;
        }
        Some(_) => {
            return Err(crate::errors::AsterError::validation_error(
                "invalid activity resource",
            ));
        }
        None => {
            let team_id = scope.team_id().ok_or_else(|| {
                crate::errors::AsterError::auth_forbidden("team scope is required")
            })?;
            let team = super::team::get_team(state, team_id, scope.actor_user_id()).await?;
            if !team.my_role.can_manage_team() {
                return Err(crate::errors::AsterError::auth_forbidden(
                    "team manager is required",
                ));
            }
        }
    }
    list_records(state, scope.team_id(), scope.owner_user_id(), entity, query).await
}

/// The admin transport guard owns global administrator authorization.
pub(crate) async fn list_admin_team(
    state: &impl crate::runtime::SharedRuntimeState,
    team_id: i64,
    query: &ActivityListQuery,
) -> Result<WorkspaceActivityPage> {
    crate::db::repository::team_repo::find_by_id(state.writer_db(), team_id).await?;
    list_records(state, Some(team_id), None, None, query).await
}

async fn list_records(
    state: &impl crate::runtime::SharedRuntimeState,
    team_id: Option<i64>,
    owner_user_id: Option<i64>,
    entity: Option<(&str, i64)>,
    query: &ActivityListQuery,
) -> Result<WorkspaceActivityPage> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) || query.before_id.is_some_and(|id| id <= 0) {
        return Err(crate::errors::AsterError::validation_error(
            "invalid activity page",
        ));
    }
    let mut models = workspace_activity_repo::list(
        state.writer_db(),
        workspace_activity_repo::ActivityQuery {
            team_id,
            owner_user_id,
            entity,
            before_id: query.before_id,
            at_most_id: None,
            limit: limit + 1,
        },
    )
    .await?;
    let limit = usize::try_from(limit)
        .map_err(|_| crate::errors::AsterError::validation_error("invalid activity limit"))?;
    let has_more = models.len() > limit;
    models.truncate(limit);
    let next_before_id = if has_more {
        models.last().map(|m| m.id)
    } else {
        None
    };
    Ok(WorkspaceActivityPage {
        items: models
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>>>()?,
        next_before_id,
    })
}

impl TryFrom<workspace_activity::Model> for WorkspaceActivity {
    type Error = crate::errors::AsterError;
    fn try_from(m: workspace_activity::Model) -> Result<Self> {
        let action = AuditAction::from_str_name(&m.action).ok_or_else(|| {
            crate::errors::AsterError::internal_error("unsupported workspace activity action")
        })?;
        Ok(Self {
            id: m.id,
            event_id: m.event_id,
            operation_id: m.operation_id,
            entity_type: m.entity_type,
            entity_id: m.entity_id,
            entity_name: m.entity_name,
            actor: ActivityIdentity {
                user_id: m.actor_user_id,
                name: m.actor_name,
            },
            action,
            source: ActivitySource::parse(&m.source)?,
            details: serde_json::from_str(&m.details).map_err(|_| {
                crate::errors::AsterError::internal_error("invalid workspace activity details")
            })?,
            created_at: m.created_at.to_rfc3339(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ActivityContext {
    pub actor_user_id: Option<i64>,
    pub actor_name: String,
    pub source: ActivitySource,
    pub operation_id: String,
}

#[derive(Serialize)]
pub(crate) struct FileChangeDetails<'a> {
    pub previous_name: &'a str,
    pub next_name: &'a str,
    pub source_folder_id: Option<i64>,
    pub target_folder_id: Option<i64>,
    pub previous_size: i64,
    pub size: i64,
    pub previous_mime_type: &'a str,
    pub mime_type: &'a str,
}

#[derive(Serialize)]
pub(crate) struct FolderChangeDetails<'a> {
    pub previous_name: &'a str,
    pub next_name: &'a str,
    pub source_parent_id: Option<i64>,
    pub target_parent_id: Option<i64>,
    pub previous_kind: &'a aster_drive_model::types::FolderIconKind,
    pub kind: &'a aster_drive_model::types::FolderIconKind,
    pub previous_key: Option<&'a str>,
    pub key: Option<&'a str>,
}

impl ActivityContext {
    pub async fn actor<C: ConnectionTrait>(
        db: &C,
        user_id: i64,
        source: ActivitySource,
    ) -> Result<Self> {
        let user = user_repo::find_by_id(db, user_id).await?;
        Ok(Self {
            actor_user_id: Some(user_id),
            actor_name: user.username,
            source,
            operation_id: uuid::Uuid::new_v4().to_string(),
        })
    }
    pub fn creator(user_id: Option<i64>, name: &str) -> Self {
        Self {
            actor_user_id: user_id,
            actor_name: name.to_owned(),
            source: ActivitySource::Service,
            operation_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

pub fn snapshot_identity(user_id: Option<i64>, name: Option<&str>) -> Option<ActivityIdentity> {
    let name = name.filter(|name| !name.trim().is_empty());
    (user_id.is_some() || name.is_some()).then(|| ActivityIdentity {
        user_id,
        name: name.unwrap_or_default().to_owned(),
    })
}

/// Resolve only public display names in a bounded batch, without leaking user administration data.
pub async fn display_names(
    state: &impl crate::runtime::SharedRuntimeState,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, String>> {
    user_repo::find_public_display_names(state.reader_db(), ids).await
}

pub fn resolve_identity(
    identity: &mut Option<ActivityIdentity>,
    names: &std::collections::HashMap<i64, String>,
) {
    if let Some(identity) = identity
        && let Some(name) = identity.user_id.and_then(|id| names.get(&id))
    {
        identity.name = name.clone();
    }
    if identity
        .as_ref()
        .is_some_and(|identity| identity.name.trim().is_empty())
    {
        *identity = None;
    }
}

enum ActivityResource<'a> {
    File(&'a file::Model),
    Folder(&'a folder::Model),
}

fn activity_model(
    ctx: &ActivityContext,
    resource: ActivityResource<'_>,
    action: AuditAction,
    details: serde_json::Value,
    now: chrono::DateTime<Utc>,
) -> workspace_activity::ActiveModel {
    let (team_id, owner_user_id, kind, id, name) = match resource {
        ActivityResource::File(file) => (
            file.team_id,
            file.owner_user_id,
            "file",
            file.id,
            &file.name,
        ),
        ActivityResource::Folder(folder) => (
            folder.team_id,
            folder.owner_user_id,
            "folder",
            folder.id,
            &folder.name,
        ),
    };
    workspace_activity::ActiveModel {
        event_id: Set(uuid::Uuid::new_v4().to_string()),
        operation_id: Set(ctx.operation_id.clone()),
        team_id: Set(team_id),
        owner_user_id: Set(owner_user_id),
        entity_type: Set(kind.to_owned()),
        entity_id: Set(id),
        entity_name: Set(name.to_owned()),
        actor_user_id: Set(ctx.actor_user_id),
        actor_name: Set(ctx.actor_name.clone()),
        action: Set(action.as_str().to_owned()),
        source: Set(ctx.source.as_str().to_owned()),
        details: Set(details.to_string()),
        created_at: Set(now),
        ..Default::default()
    }
}

pub(crate) async fn file_created<C: ConnectionTrait>(
    db: &C,
    f: &file::Model,
    action: AuditAction,
    source: ActivitySource,
) -> Result<()> {
    let mut ctx = ActivityContext::creator(f.created_by_user_id, &f.created_by_username);
    ctx.source = source;
    workspace_activity_repo::append(
        db,
        activity_model(
            &ctx,
            ActivityResource::File(f),
            action,
            serde_json::json!({"folder_id":f.folder_id}),
            Utc::now(),
        ),
    )
    .await
}

pub(crate) async fn folder_created<C: ConnectionTrait>(
    db: &C,
    f: &folder::Model,
    source: ActivitySource,
) -> Result<()> {
    let mut ctx = ActivityContext::creator(f.created_by_user_id, &f.created_by_username);
    ctx.source = source;
    workspace_activity_repo::append(
        db,
        activity_model(
            &ctx,
            ActivityResource::Folder(f),
            AuditAction::FolderCreate,
            serde_json::json!({"parent_id":f.parent_id}),
            Utc::now(),
        ),
    )
    .await
}

pub(crate) async fn file_changed<C: ConnectionTrait>(
    db: &C,
    ctx: &ActivityContext,
    before: &file::Model,
    after: &mut file::Model,
    change: impl Into<ActivityChange>,
) -> Result<()> {
    let mut change = change.into();
    if before.name != after.name || before.folder_id != after.folder_id {
        change.details = merge_details(
            change.details,
            location_changes(
                db,
                before.folder_id,
                &before.name,
                after.folder_id,
                &after.name,
            )
            .await?,
        );
    }
    let now = Utc::now();
    file::Entity::update_many()
        .col_expr(
            file::Column::ModifiedByUserId,
            Expr::value(ctx.actor_user_id),
        )
        .col_expr(
            file::Column::ModifiedByUsername,
            Expr::value(Some(ctx.actor_name.clone())),
        )
        .col_expr(file::Column::ModifiedAt, Expr::value(Some(now)))
        .filter(file::Column::Id.eq(after.id))
        .exec(db)
        .await?;
    after.modified_by_user_id = ctx.actor_user_id;
    after.modified_by_username = Some(ctx.actor_name.clone());
    after.modified_at = Some(now);
    workspace_activity_repo::append(
        db,
        activity_model(
            ctx,
            ActivityResource::File(after),
            change.action,
            merge_details(
                serde_json::to_value(FileChangeDetails {
                    previous_name: &before.name,
                    next_name: &after.name,
                    source_folder_id: before.folder_id,
                    target_folder_id: after.folder_id,
                    previous_size: before.size,
                    size: after.size,
                    previous_mime_type: &before.mime_type,
                    mime_type: &after.mime_type,
                })
                .map_err(|_| {
                    crate::errors::AsterError::internal_error(
                        "failed to serialize file activity details",
                    )
                })?,
                change.details,
            ),
            Utc::now(),
        ),
    )
    .await
}

pub(crate) async fn folder_changed<C: ConnectionTrait>(
    db: &C,
    ctx: &ActivityContext,
    before: &folder::Model,
    after: &mut folder::Model,
    change: impl Into<ActivityChange>,
) -> Result<()> {
    let mut change = change.into();
    if before.name != after.name || before.parent_id != after.parent_id {
        change.details = merge_details(
            change.details,
            location_changes(
                db,
                before.parent_id,
                &before.name,
                after.parent_id,
                &after.name,
            )
            .await?,
        );
    }
    let now = Utc::now();
    folder::Entity::update_many()
        .col_expr(
            folder::Column::ModifiedByUserId,
            Expr::value(ctx.actor_user_id),
        )
        .col_expr(
            folder::Column::ModifiedByUsername,
            Expr::value(Some(ctx.actor_name.clone())),
        )
        .col_expr(folder::Column::ModifiedAt, Expr::value(Some(now)))
        .filter(folder::Column::Id.eq(after.id))
        .exec(db)
        .await?;
    after.modified_by_user_id = ctx.actor_user_id;
    after.modified_by_username = Some(ctx.actor_name.clone());
    after.modified_at = Some(now);
    workspace_activity_repo::append(
        db,
        activity_model(
            ctx,
            ActivityResource::Folder(after),
            change.action,
            merge_details(
                serde_json::to_value(FolderChangeDetails {
                    previous_name: &before.name,
                    next_name: &after.name,
                    source_parent_id: before.parent_id,
                    target_parent_id: after.parent_id,
                    previous_kind: &before.icon_kind,
                    kind: &after.icon_kind,
                    previous_key: before.icon_value.as_deref(),
                    key: after.icon_value.as_deref(),
                })
                .map_err(|_| {
                    crate::errors::AsterError::internal_error(
                        "failed to serialize folder activity details",
                    )
                })?,
                change.details,
            ),
            Utc::now(),
        ),
    )
    .await
}

/// Create a directory and its activity on the same caller-owned transaction.
pub(crate) async fn create_folder<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    source: ActivitySource,
) -> Result<folder::Model> {
    let folder = crate::db::repository::folder_repo::create(db, model).await?;
    folder_created(db, &folder, source).await?;
    Ok(folder)
}

/// Typed action with action-specific details, shared by all mutation entry points.
pub struct ActivityChange {
    action: AuditAction,
    details: serde_json::Value,
}
impl From<AuditAction> for ActivityChange {
    fn from(action: AuditAction) -> Self {
        Self {
            action,
            details: serde_json::json!({}),
        }
    }
}
impl ActivityChange {
    pub fn with_details(action: AuditAction, details: serde_json::Value) -> Self {
        Self { action, details }
    }
}
fn merge_details(mut base: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    if let (Some(base), serde_json::Value::Object(extra)) = (base.as_object_mut(), extra) {
        base.extend(extra);
    }
    base
}

/// Retention is separate from optional audit settings; zero retains all activity by default.
pub async fn cleanup_expired(state: &impl crate::runtime::SharedRuntimeState) -> Result<u64> {
    let days = state
        .runtime_config()
        .get_i64("workspace_activity_retention_days")
        .unwrap_or(0);
    if days <= 0 {
        return Ok(0);
    }
    let Some(duration) = chrono::Duration::try_days(days) else {
        return Err(crate::errors::AsterError::validation_error(
            "activity retention days out of range",
        ));
    };
    let cutoff = Utc::now().checked_sub_signed(duration).ok_or_else(|| {
        crate::errors::AsterError::validation_error("activity retention cutoff out of range")
    })?;
    workspace_activity_repo::delete_before(state.writer_db(), cutoff).await
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ActivitySource {
    #[default]
    Rest,
    Webdav,
    Wopi,
    Task,
    Service,
}
impl ActivitySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
            Self::Webdav => "webdav",
            Self::Wopi => "wopi",
            Self::Task => "task",
            Self::Service => "service",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "rest" => Ok(Self::Rest),
            "webdav" => Ok(Self::Webdav),
            "wopi" => Ok(Self::Wopi),
            "task" => Ok(Self::Task),
            "service" => Ok(Self::Service),
            _ => Err(crate::errors::AsterError::internal_error(
                "invalid activity source",
            )),
        }
    }
}

/// Locked pre-mutation resource for metadata adapters that mutate a separate table.
pub(crate) enum ResourceSnapshot {
    File(file::Model),
    Folder(folder::Model),
}
impl ResourceSnapshot {
    pub(crate) async fn lock<C: ConnectionTrait>(
        db: &C,
        scope: super::scope::WorkspaceStorageScope,
        kind: aster_drive_model::types::EntityType,
        id: i64,
    ) -> Result<Self> {
        use crate::services::files::lock::{self, SubmittedLockCredentials};
        match kind {
            aster_drive_model::types::EntityType::File => {
                let file = crate::db::repository::file_repo::lock_by_id(db, id).await?;
                super::scope::ensure_active_file_scope(&file, scope)?;
                lock::enforce_file_mutation_on(db, &file, &SubmittedLockCredentials::none())
                    .await?;
                Ok(Self::File(file))
            }
            aster_drive_model::types::EntityType::Folder => {
                let folder = crate::db::repository::folder_repo::lock_by_id(db, id).await?;
                super::scope::ensure_active_folder_scope(&folder, scope)?;
                lock::enforce_folder_mutation_on(
                    db,
                    &folder,
                    aster_drive_model::types::LockDepth::Resource,
                    &SubmittedLockCredentials::none(),
                )
                .await?;
                Ok(Self::Folder(folder))
            }
        }
    }
    /// Protocol adapters already validate their submitted lock credentials.
    pub(crate) async fn lock_authorized<C: ConnectionTrait>(
        db: &C,
        scope: super::scope::WorkspaceStorageScope,
        kind: aster_drive_model::types::EntityType,
        id: i64,
    ) -> Result<Self> {
        match kind {
            aster_drive_model::types::EntityType::File => {
                let file = crate::db::repository::file_repo::lock_by_id(db, id).await?;
                super::scope::ensure_active_file_scope(&file, scope)?;
                Ok(Self::File(file))
            }
            aster_drive_model::types::EntityType::Folder => {
                let folder = crate::db::repository::folder_repo::lock_by_id(db, id).await?;
                super::scope::ensure_active_folder_scope(&folder, scope)?;
                Ok(Self::Folder(folder))
            }
        }
    }
    pub(crate) async fn record<C: ConnectionTrait>(
        &self,
        db: &C,
        ctx: &ActivityContext,
        change: ActivityChange,
    ) -> Result<()> {
        match self {
            Self::File(before) => file_changed(db, ctx, before, &mut before.clone(), change).await,
            Self::Folder(before) => {
                folder_changed(db, ctx, before, &mut before.clone(), change).await
            }
        }
    }
}

async fn location_changes<C: ConnectionTrait>(
    db: &C,
    source_parent: Option<i64>,
    source_name: &str,
    target_parent: Option<i64>,
    target_name: &str,
) -> Result<serde_json::Value> {
    let ids = [source_parent, target_parent]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let paths = crate::services::files::folder::build_folder_paths(db, &ids).await?;
    location_details(
        &paths,
        source_parent,
        source_name,
        target_parent,
        target_name,
    )
}

fn location_details(
    paths: &std::collections::HashMap<i64, String>,
    source_parent: Option<i64>,
    source_name: &str,
    target_parent: Option<i64>,
    target_name: &str,
) -> Result<serde_json::Value> {
    let path = |parent: Option<i64>, name: &str| -> Result<String> {
        let parent = match parent {
            Some(id) => paths
                .get(&id)
                .ok_or_else(|| {
                    crate::errors::AsterError::internal_error("missing activity parent path")
                })?
                .as_str(),
            None => "/",
        };
        Ok(format!("{}/{name}", parent.trim_end_matches('/')))
    };
    Ok(
        serde_json::json!({"source_path":path(source_parent,source_name)?,"target_path":path(target_parent,target_name)?}),
    )
}

/// Copy facts share one operation ID; cross-workspace readers see only their side.
pub(crate) async fn file_copied<C: ConnectionTrait>(
    db: &C,
    ctx: &ActivityContext,
    source: &file::Model,
    target: &file::Model,
) -> Result<()> {
    let mut records = Vec::with_capacity(2);
    file_copy_records(&mut records, ctx, source, target, Utc::now());
    workspace_activity_repo::append_many(db, records).await
}
fn file_copy_records(
    records: &mut Vec<workspace_activity::ActiveModel>,
    ctx: &ActivityContext,
    source: &file::Model,
    target: &file::Model,
    now: chrono::DateTime<Utc>,
) {
    let same = source.team_id == target.team_id && source.owner_user_id == target.owner_user_id;
    let details = if same {
        serde_json::json!({"source_file_id":source.id,"source_name":source.name,"source_folder_id":source.folder_id,"target_folder_id":target.folder_id})
    } else {
        records.push(activity_model(
            ctx,
            ActivityResource::File(source),
            AuditAction::FileCopy,
            serde_json::json!({"direction":"outgoing","destination_workspace":"external"}),
            now,
        ));
        serde_json::json!({"direction":"incoming","source_workspace":"external","target_folder_id":target.folder_id})
    };
    records.push(activity_model(
        ctx,
        ActivityResource::File(target),
        AuditAction::FileCopy,
        details,
        now,
    ));
}

pub(crate) async fn folder_copied<C: ConnectionTrait>(
    db: &C,
    ctx: &ActivityContext,
    source: &folder::Model,
    target: &folder::Model,
) -> Result<()> {
    let mut records = Vec::with_capacity(2);
    folder_copy_records(&mut records, ctx, source, target, Utc::now());
    workspace_activity_repo::append_many(db, records).await
}
fn folder_copy_records(
    records: &mut Vec<workspace_activity::ActiveModel>,
    ctx: &ActivityContext,
    source: &folder::Model,
    target: &folder::Model,
    now: chrono::DateTime<Utc>,
) {
    let same = source.team_id == target.team_id && source.owner_user_id == target.owner_user_id;
    let details = if same {
        serde_json::json!({"source_folder_id":source.id,"source_name":source.name,"source_parent_id":source.parent_id,"target_parent_id":target.parent_id})
    } else {
        records.push(activity_model(
            ctx,
            ActivityResource::Folder(source),
            AuditAction::FolderCopy,
            serde_json::json!({"direction":"outgoing","destination_workspace":"external"}),
            now,
        ));
        serde_json::json!({"direction":"incoming","source_workspace":"external","target_parent_id":target.parent_id})
    };
    records.push(activity_model(
        ctx,
        ActivityResource::Folder(target),
        AuditAction::FolderCopy,
        details,
        now,
    ));
}
