//! 文件夹层级缓存。

use serde::{Deserialize, Serialize};

use aster_forge_cache::CacheExt;
use crate::runtime::SharedRuntimeState;

const FOLDER_PATH_CACHE_TTL: u64 = 300;
pub(crate) const FOLDER_PATH_CACHE_PREFIX: &str = "folder_path:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CachedFolderPathChain {
    pub(super) chain_ids: Vec<i64>,
}

pub(crate) fn folder_path_cache_key(folder_id: i64) -> String {
    format!("{FOLDER_PATH_CACHE_PREFIX}{folder_id}")
}

pub(super) async fn load_folder_path_chain(
    state: &impl SharedRuntimeState,
    folder_id: i64,
) -> Option<CachedFolderPathChain> {
    state
        .cache()
        .get::<CachedFolderPathChain>(&folder_path_cache_key(folder_id))
        .await
}

pub(super) async fn store_folder_path_chain(
    state: &impl SharedRuntimeState,
    folder_id: i64,
    chain_ids: Vec<i64>,
) {
    state
        .cache()
        .set(
            &folder_path_cache_key(folder_id),
            &CachedFolderPathChain { chain_ids },
            Some(FOLDER_PATH_CACHE_TTL),
        )
        .await;
}

pub(super) async fn invalidate_folder_path_chain(state: &impl SharedRuntimeState, folder_id: i64) {
    state
        .cache()
        .delete(&folder_path_cache_key(folder_id))
        .await;
}

pub(super) async fn invalidate_folder_path_chains(
    state: &impl SharedRuntimeState,
    folder_ids: &[i64],
) {
    let keys = folder_ids
        .iter()
        .copied()
        .map(folder_path_cache_key)
        .collect::<Vec<_>>();
    state.cache().delete_many(&keys).await;
}

pub(crate) async fn invalidate_all_folder_path_chains(state: &impl SharedRuntimeState) {
    state
        .cache()
        .invalidate_prefix(FOLDER_PATH_CACHE_PREFIX)
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;

    #[tokio::test]
    async fn folder_path_chain_roundtrips_and_is_scoped_by_folder_id() {
        let state = CacheOnlyState::new().await;

        store_folder_path_chain(&state, 10, vec![1, 5, 10]).await;
        store_folder_path_chain(&state, 11, vec![1, 11]).await;

        assert_eq!(
            load_folder_path_chain(&state, 10)
                .await
                .map(|cached| cached.chain_ids),
            Some(vec![1, 5, 10])
        );
        assert_eq!(
            load_folder_path_chain(&state, 11)
                .await
                .map(|cached| cached.chain_ids),
            Some(vec![1, 11])
        );
    }

    #[tokio::test]
    async fn folder_path_chain_supports_single_and_global_invalidation() {
        let state = CacheOnlyState::new().await;

        store_folder_path_chain(&state, 10, vec![10]).await;
        store_folder_path_chain(&state, 11, vec![11]).await;

        invalidate_folder_path_chain(&state, 10).await;

        assert!(load_folder_path_chain(&state, 10).await.is_none());
        assert!(load_folder_path_chain(&state, 11).await.is_some());

        invalidate_all_folder_path_chains(&state).await;

        assert!(load_folder_path_chain(&state, 11).await.is_none());
    }

    #[tokio::test]
    async fn folder_path_chain_supports_batch_invalidation() {
        let state = CacheOnlyState::new().await;

        store_folder_path_chain(&state, 10, vec![10]).await;
        store_folder_path_chain(&state, 11, vec![11]).await;
        store_folder_path_chain(&state, 12, vec![12]).await;

        invalidate_folder_path_chains(&state, &[10, 12]).await;

        assert!(load_folder_path_chain(&state, 10).await.is_none());
        assert!(load_folder_path_chain(&state, 11).await.is_some());
        assert!(load_folder_path_chain(&state, 12).await.is_none());
    }
}
