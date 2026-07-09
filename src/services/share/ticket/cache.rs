//! 临时下载 ticket 缓存。

use std::sync::LazyLock;
use std::time::Duration as StdDuration;

use moka::future::Cache;

use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use aster_forge_cache::CacheExt;

use super::{STREAM_TICKET_TTL_SECS, StreamTicketPayload};

const STREAM_TICKET_CACHE_PREFIX: &str = "stream_ticket:";

static FALLBACK_STREAM_TICKETS: LazyLock<Cache<String, StreamTicketPayload>> =
    LazyLock::new(|| {
        Cache::builder()
            .max_capacity(10_000)
            .time_to_live(StdDuration::from_secs(
                u64::try_from(STREAM_TICKET_TTL_SECS).unwrap_or(300),
            ))
            .build()
    });

fn ticket_cache_key(token: &str) -> String {
    format!("{STREAM_TICKET_CACHE_PREFIX}{token}")
}

pub(super) async fn store_ticket(
    state: &impl SharedRuntimeState,
    token: &str,
    payload: &StreamTicketPayload,
    ttl_secs: u64,
) -> Result<()> {
    let cache_key = ticket_cache_key(token);
    state.cache().set(&cache_key, payload, Some(ttl_secs)).await;
    if state
        .cache()
        .get::<StreamTicketPayload>(&cache_key)
        .await
        .is_some()
    {
        return Ok(());
    }

    tracing::warn!(
        "stream ticket cache backend did not persist entry; falling back to local cache"
    );
    FALLBACK_STREAM_TICKETS
        .insert(cache_key, payload.clone())
        .await;
    Ok(())
}

pub(super) async fn load_ticket(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Option<StreamTicketPayload> {
    let cache_key = ticket_cache_key(token);
    if let Some(payload) = state.cache().get::<StreamTicketPayload>(&cache_key).await {
        return Some(payload);
    }

    FALLBACK_STREAM_TICKETS.get(&cache_key).await
}

pub(super) async fn delete_ticket(state: &impl SharedRuntimeState, token: &str) {
    let cache_key = ticket_cache_key(token);
    state.cache().delete(&cache_key).await;
    FALLBACK_STREAM_TICKETS.remove(&cache_key).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;
    use aster_forge_cache::CacheExt;

    fn payload(exp: i64) -> StreamTicketPayload {
        StreamTicketPayload {
            actor_user_id: 42,
            team_id: None,
            exp,
            kind: super::super::StreamTicketKind::ArchiveDownload {
                file_ids: vec![1, 2],
                folder_ids: vec![3],
                archive_name: "archive.zip".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn ticket_roundtrips_through_primary_cache_and_delete_removes_it() {
        let state = CacheOnlyState::new().await;

        store_ticket(&state, "token-a", &payload(100), 60)
            .await
            .unwrap();

        assert_eq!(
            load_ticket(&state, "token-a")
                .await
                .map(|cached| cached.actor_user_id),
            Some(42)
        );

        delete_ticket(&state, "token-a").await;

        assert!(load_ticket(&state, "token-a").await.is_none());
    }

    #[tokio::test]
    async fn ticket_delete_removes_fallback_entry_even_if_primary_is_missing() {
        let state = CacheOnlyState::new().await;
        let key = ticket_cache_key("token-fallback");
        FALLBACK_STREAM_TICKETS
            .insert(key.clone(), payload(100))
            .await;

        assert!(load_ticket(&state, "token-fallback").await.is_some());

        delete_ticket(&state, "token-fallback").await;

        assert!(load_ticket(&state, "token-fallback").await.is_none());
    }

    #[tokio::test]
    async fn ticket_primary_cache_wins_over_fallback() {
        let state = CacheOnlyState::new().await;
        let key = ticket_cache_key("token-priority");
        FALLBACK_STREAM_TICKETS
            .insert(key.clone(), payload(10))
            .await;
        state.cache().set(&key, &payload(20), Some(60)).await;

        assert_eq!(
            load_ticket(&state, "token-priority")
                .await
                .map(|cached| cached.exp),
            Some(20)
        );
    }
}
