//! Authentication one-time state never depends on cache availability or fallback copies.
use crate::common;
use aster_drive::db::repository::auth_once_state_repo::{consume, reserve};
use aster_drive_model::entities::auth_once_state;
use chrono::{Duration, Utc};
use sea_orm::{EntityTrait, PaginatorTrait, QueryOrder};

#[tokio::test]
async fn auth_once_state_concurrent_consumers_and_reservations_have_one_winner() {
    let state = common::setup_with_pool_size(4).await;
    let expiry = Utc::now() + Duration::minutes(10);
    let outcomes = futures::future::join_all(
        (0..12).map(|_| reserve(state.writer_db(), "nonce:node:123", "", expiry)),
    )
    .await;
    assert_eq!(
        outcomes
            .into_iter()
            .filter(|value| matches!(value, Ok(true)))
            .count(),
        1
    );
    assert!(
        reserve(state.writer_db(), "passkey:login:123", "challenge", expiry)
            .await
            .unwrap()
    );
    let outcomes =
        futures::future::join_all((0..12).map(|_| consume(state.writer_db(), "passkey:login:123")))
            .await;
    assert_eq!(
        outcomes
            .into_iter()
            .filter(|value| matches!(value, Ok(Some(payload)) if payload == "challenge"))
            .count(),
        1
    );
    assert!(
        !reserve(state.writer_db(), "nonce:node:123", "", expiry)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn auth_once_state_expiry_collection_is_indexed_and_bounded() {
    let state = common::setup().await;
    let expired = Utc::now() - Duration::minutes(1);
    for i in 0..300 {
        // Insert fixtures directly: production reservation also collects expired rows.
        auth_once_state::Entity::insert(auth_once_state::ActiveModel {
            key_hash: sea_orm::Set(format!("{i:064x}")),
            payload: sea_orm::Set("expired".into()),
            expires_at: sea_orm::Set(expired),
        })
        .exec(state.writer_db())
        .await
        .unwrap();
    }
    reserve(
        state.writer_db(),
        "fresh",
        "value",
        Utc::now() + Duration::minutes(5),
    )
    .await
    .unwrap();
    assert_eq!(
        auth_once_state::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        173
    );
    assert_eq!(
        consume(state.writer_db(), "fresh")
            .await
            .unwrap()
            .as_deref(),
        Some("value")
    );
    assert!(
        consume(state.writer_db(), "missing")
            .await
            .unwrap()
            .is_none()
    );
    let row = auth_once_state::Entity::find()
        .order_by_asc(auth_once_state::Column::ExpiresAt)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert!(row.expires_at < Utc::now());
}

#[tokio::test]
async fn auth_once_state_backend_errors_cannot_authorize_or_consume_again() {
    let state = common::setup().await;
    let expiry = Utc::now() + Duration::minutes(5);
    assert!(
        reserve(state.writer_db(), "challenge", "payload", expiry)
            .await
            .unwrap()
    );
    use sea_orm::ConnectionTrait;
    state
        .writer_db()
        .execute_unprepared("DROP TABLE auth_once_states")
        .await
        .unwrap();
    assert!(consume(state.writer_db(), "challenge").await.is_err());
    assert!(
        reserve(state.writer_db(), "nonce", "", expiry)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn auth_once_state_payload_budget_preserves_large_valid_challenges() {
    let state = common::setup().await;
    let expiry = Utc::now() + Duration::minutes(5);
    let payload = "x".repeat(256 * 1024);
    assert!(
        reserve(state.writer_db(), "large", &payload, expiry)
            .await
            .unwrap()
    );
    assert_eq!(
        consume(state.writer_db(), "large")
            .await
            .unwrap()
            .as_deref(),
        Some(payload.as_str())
    );
    assert!(
        reserve(state.writer_db(), "oversized", &(payload + "x"), expiry)
            .await
            .is_err()
    );
}
