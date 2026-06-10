//! 服务模块：`tag_service`。

use serde::Serialize;
use std::collections::{HashMap, HashSet};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::pagination::{MAX_PAGE_SIZE, OffsetPage, load_offset_page};
use crate::db::repository::{file_repo, folder_repo, property_repo, tag_repo};
use crate::entities::tag;
use crate::errors::{AsterError, Result};
use crate::runtime::{SharedRuntimeState, StorageChangeRuntimeState};
use crate::services::{
    storage_change_service,
    workspace_storage_service::{
        WorkspaceResourceScope, WorkspaceStorageScope, require_scope_access,
        require_team_management_access, verify_file_access_for_read, verify_folder_access_for_read,
    },
};
use crate::types::{EntityType, TagScopeType};
use crate::utils::char_count;

pub const TAG_PROPERTY_NAMESPACE: &str = "system.tags";
pub const TAG_NAME_MAX_CHARS: usize = 64;
const MAX_TAGS_PER_ENTITY: usize = 64;
const BATCH_ENTITY_VERIFY_CHUNK_SIZE: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TagSummary {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TagInfo {
    pub id: i64,
    pub scope_type: TagScopeType,
    pub owner_user_id: Option<i64>,
    pub team_id: Option<i64>,
    pub name: String,
    pub normalized_name: String,
    pub color: String,
    pub sort_order: i32,
    pub usage_count: u64,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct EntityTags {
    pub tags: Vec<TagSummary>,
}

#[derive(Debug, Clone, Copy)]
struct TagScope {
    scope_type: TagScopeType,
    owner_user_id: Option<i64>,
    team_id: Option<i64>,
}

impl From<WorkspaceResourceScope> for TagScope {
    fn from(scope: WorkspaceResourceScope) -> Self {
        match scope {
            WorkspaceResourceScope::Personal { user_id } => Self {
                scope_type: TagScopeType::Personal,
                owner_user_id: Some(user_id),
                team_id: None,
            },
            WorkspaceResourceScope::Team { team_id } => Self {
                scope_type: TagScopeType::Team,
                owner_user_id: None,
                team_id: Some(team_id),
            },
        }
    }
}

impl From<tag::Model> for TagSummary {
    fn from(tag: tag::Model) -> Self {
        Self {
            id: tag.id,
            name: tag.name,
            color: tag.color,
        }
    }
}

impl TagInfo {
    fn from_model(tag: tag::Model, usage_count: u64) -> Self {
        Self {
            id: tag.id,
            scope_type: tag.scope_type,
            owner_user_id: tag.owner_user_id,
            team_id: tag.team_id,
            name: tag.name,
            normalized_name: tag.normalized_name,
            color: tag.color,
            sort_order: tag.sort_order,
            usage_count,
            created_at: tag.created_at,
            updated_at: tag.updated_at,
        }
    }
}

fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}

fn clean_name(name: &str) -> String {
    name.trim().to_string()
}

fn clean_color(color: &str) -> String {
    color.trim().to_lowercase()
}

fn validate_tag_name(name: &str) -> Result<String> {
    let name = clean_name(name);
    if name.is_empty() {
        return Err(AsterError::validation_error("tag name cannot be empty"));
    }
    if char_count(&name) > TAG_NAME_MAX_CHARS {
        return Err(AsterError::validation_error(format!(
            "tag name too long (max {TAG_NAME_MAX_CHARS})"
        )));
    }
    Ok(name)
}

fn validate_tag_color(color: &str) -> Result<String> {
    let color = clean_color(color);
    let valid = color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(AsterError::validation_error(
            "tag color must be a hex color like #3b82f6",
        ));
    }
    Ok(color)
}

fn tag_matches_scope(tag: &tag::Model, scope: TagScope) -> bool {
    tag.scope_type == scope.scope_type
        && tag.owner_user_id == scope.owner_user_id
        && tag.team_id == scope.team_id
}

async fn ensure_unique_name(
    state: &impl SharedRuntimeState,
    scope: TagScope,
    normalized_name: &str,
    exclude_tag_id: Option<i64>,
) -> Result<()> {
    let existing = match scope.scope_type {
        TagScopeType::Personal => {
            tag_repo::find_personal_by_normalized_name(
                state.reader_db(),
                scope
                    .owner_user_id
                    .ok_or_else(|| AsterError::internal_error("missing personal tag owner"))?,
                normalized_name,
            )
            .await?
        }
        TagScopeType::Team => {
            tag_repo::find_team_by_normalized_name(
                state.reader_db(),
                scope
                    .team_id
                    .ok_or_else(|| AsterError::internal_error("missing team tag scope"))?,
                normalized_name,
            )
            .await?
        }
    };

    if let Some(existing) = existing
        && Some(existing.id) != exclude_tag_id
    {
        return Err(AsterError::validation_error("tag name already exists"));
    }
    Ok(())
}

async fn require_write_access(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { .. } => require_scope_access(state, scope).await,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => require_team_management_access(state, team_id, actor_user_id).await,
    }
}

async fn verify_entity_read(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            verify_file_access_for_read(state, scope, entity_id).await?;
        }
        EntityType::Folder => {
            verify_folder_access_for_read(state, scope, entity_id).await?;
        }
    }
    Ok(())
}

async fn verify_entity_write(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    require_write_access(state, scope).await?;
    verify_entity_read(state, scope, entity_type, entity_id).await
}

async fn load_tag_for_write_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
) -> Result<tag::Model> {
    require_write_access(state, scope).await?;
    let tag = tag_repo::find_by_id(state.reader_db(), tag_id).await?;
    let expected_scope: TagScope = WorkspaceResourceScope::from(scope).into();
    if !tag_matches_scope(&tag, expected_scope) {
        return Err(AsterError::record_not_found("tag not found"));
    }
    Ok(tag)
}

pub(crate) async fn list_page_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    limit: u64,
    offset: u64,
    search: Option<&str>,
) -> Result<OffsetPage<TagInfo>> {
    require_scope_access(state, scope).await?;
    let workspace_scope = WorkspaceResourceScope::from(scope);

    load_offset_page(limit, offset, MAX_PAGE_SIZE, |limit, offset| async move {
        let (tags, total) = match workspace_scope {
            WorkspaceResourceScope::Personal { user_id } => {
                tag_repo::list_personal_page(state.reader_db(), user_id, limit, offset, search)
                    .await?
            }
            WorkspaceResourceScope::Team { team_id } => {
                tag_repo::list_team_page(state.reader_db(), team_id, limit, offset, search).await?
            }
        };
        let tag_ids = tags.iter().map(|tag| tag.id).collect::<Vec<_>>();
        let counts = property_repo::count_entities_by_tag_ids(
            state.reader_db(),
            TAG_PROPERTY_NAMESPACE,
            &tag_ids,
        )
        .await?;
        Ok((
            tags.into_iter()
                .map(|tag| {
                    let count = counts.get(&tag.id).copied().unwrap_or_default();
                    TagInfo::from_model(tag, count)
                })
                .collect(),
            total,
        ))
    })
    .await
}

pub(crate) async fn create_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    name: &str,
    color: &str,
) -> Result<TagInfo> {
    require_write_access(state, scope).await?;
    let tag_scope: TagScope = WorkspaceResourceScope::from(scope).into();
    let name = validate_tag_name(name)?;
    let color = validate_tag_color(color)?;
    let normalized_name = normalize_name(&name);
    ensure_unique_name(state, tag_scope, &normalized_name, None).await?;

    let tag = tag_repo::create(
        state.writer_db(),
        tag_scope.scope_type,
        tag_scope.owner_user_id,
        tag_scope.team_id,
        &name,
        &normalized_name,
        &color,
    )
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::TagCreated,
            scope,
            vec![],
            vec![],
            vec![],
        ),
    );
    Ok(TagInfo::from_model(tag, 0))
}

pub(crate) async fn update_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<TagInfo> {
    let tag = load_tag_for_write_scope(state, scope, tag_id).await?;
    let tag_scope: TagScope = WorkspaceResourceScope::from(scope).into();
    let new_name = name.map(validate_tag_name).transpose()?;
    let new_color = color.map(validate_tag_color).transpose()?;
    let normalized_name = new_name.as_deref().map(normalize_name);

    if let Some(normalized_name) = normalized_name.as_deref()
        && normalized_name != tag.normalized_name
    {
        ensure_unique_name(state, tag_scope, normalized_name, Some(tag.id)).await?;
    }

    let updated = tag_repo::update(
        state.writer_db(),
        tag,
        new_name.as_deref(),
        normalized_name.as_deref(),
        new_color.as_deref(),
    )
    .await?;
    let counts = property_repo::count_entities_by_tag_ids(
        state.reader_db(),
        TAG_PROPERTY_NAMESPACE,
        &[tag_id],
    )
    .await?;
    let info = TagInfo::from_model(updated, counts.get(&tag_id).copied().unwrap_or_default());
    publish_tag_bound_entities_change(
        state,
        scope,
        storage_change_service::StorageChangeKind::TagUpdated,
        tag_id,
    )
    .await?;
    Ok(info)
}

pub(crate) async fn delete_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
) -> Result<()> {
    load_tag_for_write_scope(state, scope, tag_id).await?;
    let (file_ids, folder_ids) = entity_ids_bound_to_tag(state, tag_id).await?;
    let affected_parent_ids =
        affected_parent_ids_for_entities(state, scope, &file_ids, &folder_ids).await?;
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        property_repo::delete_by_namespace_and_name(
            txn,
            TAG_PROPERTY_NAMESPACE,
            &tag_id.to_string(),
        )
        .await?;
        tag_repo::delete(txn, tag_id).await
    })
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::TagDeleted,
            scope,
            file_ids,
            folder_ids,
            affected_parent_ids,
        ),
    );
    Ok(())
}

pub(crate) async fn list_entity_tags_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<EntityTags> {
    verify_entity_read(state, scope, entity_type, entity_id).await?;
    let file_ids;
    let folder_ids;
    let (file_ids_ref, folder_ids_ref) = match entity_type {
        EntityType::File => {
            file_ids = vec![entity_id];
            folder_ids = Vec::new();
            (file_ids.as_slice(), folder_ids.as_slice())
        }
        EntityType::Folder => {
            file_ids = Vec::new();
            folder_ids = vec![entity_id];
            (file_ids.as_slice(), folder_ids.as_slice())
        }
    };
    let map = load_entity_tag_map(
        state,
        WorkspaceResourceScope::from(scope),
        file_ids_ref,
        folder_ids_ref,
    )
    .await?;
    Ok(EntityTags {
        tags: map
            .get(&(entity_type, entity_id))
            .cloned()
            .unwrap_or_default(),
    })
}

pub(crate) async fn attach_to_entity_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<EntityTags> {
    load_tag_for_write_scope(state, scope, tag_id).await?;
    verify_entity_write(state, scope, entity_type, entity_id).await?;
    property_repo::upsert(
        state.writer_db(),
        entity_type,
        entity_id,
        TAG_PROPERTY_NAMESPACE,
        &tag_id.to_string(),
        None,
    )
    .await?;
    let tags = list_entity_tags_in_scope(state, scope, entity_type, entity_id).await?;
    publish_entity_tag_assignment_change(state, scope, entity_type, entity_id).await?;
    Ok(tags)
}

pub(crate) async fn detach_from_entity_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<EntityTags> {
    load_tag_for_write_scope(state, scope, tag_id).await?;
    verify_entity_write(state, scope, entity_type, entity_id).await?;
    property_repo::delete_prop(
        state.writer_db(),
        entity_type,
        entity_id,
        TAG_PROPERTY_NAMESPACE,
        &tag_id.to_string(),
    )
    .await?;
    let tags = list_entity_tags_in_scope(state, scope, entity_type, entity_id).await?;
    publish_entity_tag_assignment_change(state, scope, entity_type, entity_id).await?;
    Ok(tags)
}

pub(crate) async fn replace_entity_tags_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    entity_type: EntityType,
    entity_id: i64,
    tag_ids: &[i64],
) -> Result<EntityTags> {
    verify_entity_write(state, scope, entity_type, entity_id).await?;
    let unique_tag_ids = unique_ids(tag_ids)?;
    ensure_tags_belong_to_scope(state, scope, &unique_tag_ids).await?;
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        property_repo::delete_namespace_for_entity(
            txn,
            entity_type,
            entity_id,
            TAG_PROPERTY_NAMESPACE,
        )
        .await?;
        for tag_id in &unique_tag_ids {
            property_repo::upsert(
                txn,
                entity_type,
                entity_id,
                TAG_PROPERTY_NAMESPACE,
                &tag_id.to_string(),
                None,
            )
            .await?;
        }
        Ok(())
    })
    .await?;
    let tags = list_entity_tags_in_scope(state, scope, entity_type, entity_id).await?;
    publish_entity_tag_assignment_change(state, scope, entity_type, entity_id).await?;
    Ok(tags)
}

pub(crate) async fn batch_attach_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<()> {
    load_tag_for_write_scope(state, scope, tag_id).await?;
    let file_ids = unique_entity_ids(file_ids);
    let folder_ids = unique_entity_ids(folder_ids);
    verify_entities_for_batch_write(state, scope, &file_ids, &folder_ids).await?;
    let tag_id = tag_id.to_string();
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        property_repo::insert_many_for_entities(
            txn,
            EntityType::File,
            &file_ids,
            TAG_PROPERTY_NAMESPACE,
            &tag_id,
            None,
        )
        .await?;
        property_repo::insert_many_for_entities(
            txn,
            EntityType::Folder,
            &folder_ids,
            TAG_PROPERTY_NAMESPACE,
            &tag_id,
            None,
        )
        .await?;
        Ok(())
    })
    .await?;
    publish_tag_assignment_change(state, scope, &file_ids, &folder_ids).await
}

pub(crate) async fn batch_detach_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    tag_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<()> {
    load_tag_for_write_scope(state, scope, tag_id).await?;
    let file_ids = unique_entity_ids(file_ids);
    let folder_ids = unique_entity_ids(folder_ids);
    verify_entities_for_batch_write(state, scope, &file_ids, &folder_ids).await?;
    let tag_id = tag_id.to_string();
    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        property_repo::delete_many_for_entities(
            txn,
            EntityType::File,
            &file_ids,
            TAG_PROPERTY_NAMESPACE,
            &tag_id,
        )
        .await?;
        property_repo::delete_many_for_entities(
            txn,
            EntityType::Folder,
            &folder_ids,
            TAG_PROPERTY_NAMESPACE,
            &tag_id,
        )
        .await?;
        Ok(())
    })
    .await?;
    publish_tag_assignment_change(state, scope, &file_ids, &folder_ids).await
}

fn unique_ids(ids: &[i64]) -> Result<Vec<i64>> {
    let mut seen = HashSet::with_capacity(ids.len());
    let mut unique = Vec::with_capacity(ids.len());
    for id in ids {
        if *id <= 0 {
            return Err(AsterError::validation_error(
                "tag_ids must contain positive ids",
            ));
        }
        if seen.insert(*id) {
            unique.push(*id);
        }
    }
    if unique.len() > MAX_TAGS_PER_ENTITY {
        return Err(AsterError::validation_error(
            "tag_ids cannot contain more than 64 items",
        ));
    }
    Ok(unique)
}

fn unique_entity_ids(ids: &[i64]) -> Vec<i64> {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

async fn ensure_tags_belong_to_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    tag_ids: &[i64],
) -> Result<()> {
    if tag_ids.is_empty() {
        return Ok(());
    }
    let tags = tag_repo::find_by_ids(state.reader_db(), tag_ids).await?;
    if tags.len() != tag_ids.len() {
        return Err(AsterError::record_not_found("tag not found"));
    }
    let expected_scope: TagScope = WorkspaceResourceScope::from(scope).into();
    if tags
        .iter()
        .any(|tag| !tag_matches_scope(tag, expected_scope))
    {
        return Err(AsterError::record_not_found("tag not found"));
    }
    Ok(())
}

pub(crate) async fn ensure_tags_readable_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    tag_ids: &[i64],
) -> Result<()> {
    require_scope_access(state, scope).await?;
    ensure_tags_belong_to_scope(state, scope, tag_ids).await
}

async fn verify_files_for_batch_write(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
) -> Result<()> {
    if file_ids.is_empty() {
        return Ok(());
    }

    for chunk in file_ids.chunks(BATCH_ENTITY_VERIFY_CHUNK_SIZE) {
        let files = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                file_repo::find_by_ids_in_personal_scope(state.reader_db(), user_id, chunk).await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                file_repo::find_by_ids_in_team_scope(state.reader_db(), team_id, chunk).await?
            }
        };
        if files.len() != chunk.len() || files.iter().any(|file| file.deleted_at.is_some()) {
            return Err(AsterError::file_not_found("file not found"));
        }
    }

    Ok(())
}

async fn verify_folders_for_batch_write(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_ids: &[i64],
) -> Result<()> {
    if folder_ids.is_empty() {
        return Ok(());
    }

    for chunk in folder_ids.chunks(BATCH_ENTITY_VERIFY_CHUNK_SIZE) {
        let folders = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                folder_repo::find_by_ids_in_personal_scope(state.reader_db(), user_id, chunk)
                    .await?
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                folder_repo::find_by_ids_in_team_scope(state.reader_db(), team_id, chunk).await?
            }
        };
        if folders.len() != chunk.len() || folders.iter().any(|folder| folder.deleted_at.is_some())
        {
            return Err(AsterError::record_not_found("folder not found"));
        }
    }

    Ok(())
}

async fn verify_entities_for_batch_write(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<()> {
    require_write_access(state, scope).await?;
    verify_files_for_batch_write(state, scope, file_ids).await?;
    verify_folders_for_batch_write(state, scope, folder_ids).await?;
    Ok(())
}

async fn entity_ids_bound_to_tag(
    state: &impl SharedRuntimeState,
    tag_id: i64,
) -> Result<(Vec<i64>, Vec<i64>)> {
    let file_ids = property_repo::find_entity_ids_by_tag_ids(
        state.reader_db(),
        TAG_PROPERTY_NAMESPACE,
        EntityType::File,
        &[tag_id],
    )
    .await?;
    let folder_ids = property_repo::find_entity_ids_by_tag_ids(
        state.reader_db(),
        TAG_PROPERTY_NAMESPACE,
        EntityType::Folder,
        &[tag_id],
    )
    .await?;
    Ok((unique_entity_ids(&file_ids), unique_entity_ids(&folder_ids)))
}

async fn affected_parent_ids_for_entities(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<Vec<Option<i64>>> {
    storage_change_service::affected_parent_ids_for_entities(state, scope, file_ids, folder_ids)
        .await
}

async fn publish_tag_bound_entities_change(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    kind: storage_change_service::StorageChangeKind,
    tag_id: i64,
) -> Result<()> {
    let (file_ids, folder_ids) = entity_ids_bound_to_tag(state, tag_id).await?;
    publish_tag_entity_change(state, scope, kind, file_ids, folder_ids).await
}

async fn publish_entity_tag_assignment_change(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    let (file_ids, folder_ids) = match entity_type {
        EntityType::File => (vec![entity_id], Vec::new()),
        EntityType::Folder => (Vec::new(), vec![entity_id]),
    };
    publish_tag_assignment_change(state, scope, &file_ids, &folder_ids).await
}

async fn publish_tag_assignment_change(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<()> {
    publish_tag_entity_change(
        state,
        scope,
        storage_change_service::StorageChangeKind::TagAssignmentChanged,
        file_ids.to_vec(),
        folder_ids.to_vec(),
    )
    .await
}

async fn publish_tag_entity_change(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    kind: storage_change_service::StorageChangeKind,
    file_ids: Vec<i64>,
    folder_ids: Vec<i64>,
) -> Result<()> {
    let affected_parent_ids =
        affected_parent_ids_for_entities(state, scope, &file_ids, &folder_ids).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            kind,
            scope,
            file_ids,
            folder_ids,
            affected_parent_ids,
        ),
    );
    Ok(())
}

pub(crate) async fn load_entity_tag_map(
    state: &impl SharedRuntimeState,
    scope: WorkspaceResourceScope,
    file_ids: &[i64],
    folder_ids: &[i64],
) -> Result<HashMap<(EntityType, i64), Vec<TagSummary>>> {
    let bindings = property_repo::find_tag_bindings_for_entities(
        state.reader_db(),
        TAG_PROPERTY_NAMESPACE,
        file_ids,
        folder_ids,
    )
    .await?;
    if bindings.is_empty() {
        return Ok(HashMap::new());
    }

    let mut tag_ids = bindings
        .iter()
        .filter_map(|binding| binding.tag_id.parse::<i64>().ok())
        .collect::<Vec<_>>();
    tag_ids.sort_unstable();
    tag_ids.dedup();

    let expected_scope: TagScope = scope.into();
    let tags = tag_repo::find_by_ids(state.reader_db(), &tag_ids).await?;
    let tags_by_id = tags
        .into_iter()
        .filter(|tag| tag_matches_scope(tag, expected_scope))
        .map(|tag| (tag.id, TagSummary::from(tag)))
        .collect::<HashMap<_, _>>();

    let mut map: HashMap<(EntityType, i64), Vec<TagSummary>> = HashMap::new();
    for binding in bindings {
        let Ok(tag_id) = binding.tag_id.parse::<i64>() else {
            continue;
        };
        if let Some(tag) = tags_by_id.get(&tag_id) {
            map.entry((binding.entity_type, binding.entity_id))
                .or_default()
                .push(tag.clone());
        }
    }
    for tags in map.values_mut() {
        tags.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    }
    Ok(map)
}
