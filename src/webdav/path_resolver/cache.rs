//! WebDAV 路径解析缓存。

use aster_forge_cache::CacheExt;
use crate::runtime::SharedRuntimeState;

use super::{CachedResolvedNode, CachedResolvedParent};

const WEBDAV_PATH_CACHE_TTL: u64 = 30;
pub(crate) const WEBDAV_PATH_CACHE_PREFIX: &str = "webdav_path:";
pub(crate) const WEBDAV_PARENT_CACHE_PREFIX: &str = "webdav_parent:";

pub(super) async fn load_resolved_node_by_key(
    state: &impl SharedRuntimeState,
    cache_key: &str,
) -> Option<CachedResolvedNode> {
    state.cache().get::<CachedResolvedNode>(cache_key).await
}

pub(super) async fn store_resolved_node_by_key(
    state: &impl SharedRuntimeState,
    cache_key: &str,
    node: &CachedResolvedNode,
) {
    state
        .cache()
        .set(cache_key, node, Some(WEBDAV_PATH_CACHE_TTL))
        .await;
}

pub(super) async fn load_resolved_parent_by_key(
    state: &impl SharedRuntimeState,
    cache_key: &str,
) -> Option<CachedResolvedParent> {
    state.cache().get::<CachedResolvedParent>(cache_key).await
}

pub(super) async fn store_resolved_parent_by_key(
    state: &impl SharedRuntimeState,
    cache_key: &str,
    parent_id: Option<i64>,
) {
    state
        .cache()
        .set(
            cache_key,
            &CachedResolvedParent { parent_id },
            Some(WEBDAV_PATH_CACHE_TTL),
        )
        .await;
}

pub(super) async fn delete_by_key(state: &impl SharedRuntimeState, cache_key: &str) {
    state.cache().delete(cache_key).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;

    #[tokio::test]
    async fn resolved_node_cache_roundtrips_root_folder_and_file_variants() {
        let state = CacheOnlyState::new().await;

        let root_key = "webdav_path:test:root";
        let folder_key = "webdav_path:test:folder";
        let file_key = "webdav_path:test:file";

        store_resolved_node_by_key(&state, root_key, &CachedResolvedNode::Root).await;
        store_resolved_node_by_key(
            &state,
            folder_key,
            &CachedResolvedNode::Folder {
                id: 10,
                parent_id: Some(1),
                name: "docs".to_string(),
            },
        )
        .await;
        store_resolved_node_by_key(
            &state,
            file_key,
            &CachedResolvedNode::File {
                id: 20,
                folder_id: Some(10),
                name: "deck.pptx".to_string(),
            },
        )
        .await;

        assert!(matches!(
            load_resolved_node_by_key(&state, root_key).await,
            Some(CachedResolvedNode::Root)
        ));
        assert!(matches!(
            load_resolved_node_by_key(&state, folder_key).await,
            Some(CachedResolvedNode::Folder { id: 10, .. })
        ));
        assert!(matches!(
            load_resolved_node_by_key(&state, file_key).await,
            Some(CachedResolvedNode::File { id: 20, .. })
        ));
    }

    #[tokio::test]
    async fn resolved_parent_cache_roundtrips_none_and_some_parent_id() {
        let state = CacheOnlyState::new().await;

        store_resolved_parent_by_key(&state, "webdav_parent:test:none", None).await;
        store_resolved_parent_by_key(&state, "webdav_parent:test:some", Some(42)).await;

        assert_eq!(
            load_resolved_parent_by_key(&state, "webdav_parent:test:none")
                .await
                .map(|cached| cached.parent_id),
            Some(None)
        );
        assert_eq!(
            load_resolved_parent_by_key(&state, "webdav_parent:test:some")
                .await
                .map(|cached| cached.parent_id),
            Some(Some(42))
        );
    }

    #[tokio::test]
    async fn delete_by_key_removes_node_and_parent_entries() {
        let state = CacheOnlyState::new().await;

        store_resolved_node_by_key(&state, "webdav_path:test:delete", &CachedResolvedNode::Root)
            .await;
        store_resolved_parent_by_key(&state, "webdav_parent:test:delete", Some(42)).await;

        delete_by_key(&state, "webdav_path:test:delete").await;
        delete_by_key(&state, "webdav_parent:test:delete").await;

        assert!(
            load_resolved_node_by_key(&state, "webdav_path:test:delete")
                .await
                .is_none()
        );
        assert!(
            load_resolved_parent_by_key(&state, "webdav_parent:test:delete")
                .await
                .is_none()
        );
    }
}
