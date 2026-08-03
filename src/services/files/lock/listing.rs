use crate::api::pagination::{AdminLockSortBy, load_offset_page};
use crate::db::repository::lock_repo;
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::{user::account, user::profile};
use aster_drive_model::entities::resource_lock;
use aster_forge_api::{OffsetPage, SortOrder};

use super::models::ResourceLock;
use super::owner_info::deserialize_resource_lock_owner_info;

pub async fn list_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    sort_by: AdminLockSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<ResourceLock>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) =
            lock_repo::find_paginated(state.writer_db(), limit, offset, sort_by, sort_order)
                .await?;
        let items = build_resource_locks(state, items).await?;
        Ok((items, total))
    })
    .await
}

async fn build_resource_locks(
    state: &impl SharedRuntimeState,
    locks: Vec<resource_lock::Model>,
) -> Result<Vec<ResourceLock>> {
    let owner_ids: Vec<i64> = locks.iter().filter_map(|lock| lock.owner_id()).collect();
    let owners =
        account::user_summaries_by_ids(state, &owner_ids, profile::AvatarAudience::AdminUser)
            .await?;

    locks
        .into_iter()
        .map(|model| {
            let owner_info = deserialize_resource_lock_owner_info(&model)?;
            let owner = model
                .owner_id()
                .and_then(|owner_id| owners.get(&owner_id).cloned());
            Ok(ResourceLock {
                id: model.id,
                token: model.token,
                namespace_id: model.namespace_id,
                root_kind: model.root_kind,
                root_folder_id: model.root_folder_id,
                root_file_id: model.root_file_id,
                depth: model.depth,
                mode: model.mode,
                origin: model.origin,
                lockroot_path: model.lockroot_path,
                owner,
                owner_info,
                timeout_at: model.timeout_at,
                created_at: model.created_at,
            })
        })
        .collect()
}
