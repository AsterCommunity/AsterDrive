//! Final authorization boundaries for credentials, optional identities and streaming responses.
use crate::common;
use actix_web::{body::MessageBody, test};
use aster_drive::{
    db::repository::{config_repo, user_repo},
    services::auth::local,
};
use aster_drive_model::entities::{contact_verification_token, user};
use aster_drive_model::types::{VerificationChannel, VerificationPurpose};
use aster_forge_config::ConfigVisibility;
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};
use std::{future::poll_fn, pin::Pin};

#[actix_web::test]
async fn restricted_optional_identity_never_receives_authenticated_custom_config() {
    let state = common::setup().await;
    let account = common::create_test_account(
        &state,
        "optional-user",
        "optional@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user = user_repo::find_by_id(state.writer_db(), account.id)
        .await
        .unwrap();
    config_repo::upsert_with_options(
        state.writer_db(),
        "custom.member_hint",
        "members-only",
        Some(ConfigVisibility::Authenticated),
        None,
    )
    .await
    .unwrap();
    let mut active: user::ActiveModel = user.into();
    active.must_change_password = Set(true);
    let user = active.update(state.writer_db()).await.unwrap();
    let (access, _) = local::issue_tokens_for_user(state.writer_db(), &state, &user, None, None)
        .await
        .unwrap();
    assert!(
        local::verify_token(&access, &state.config.auth.jwt_secret)
            .unwrap()
            .password_change
    );
    let app = create_test_app!(state.clone());
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/public/custom-config")
            .insert_header(("Authorization", format!("Bearer {access}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert!(body["data"]["entries"]["custom.member_hint"].is_null());
    let mut active: user::ActiveModel = user.into();
    active.must_change_password = Set(false);
    let user = active.update(state.writer_db()).await.unwrap();
    let (access, _) = local::issue_tokens_for_user(state.writer_db(), &state, &user, None, None)
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/public/custom-config")
            .insert_header(("Authorization", format!("Bearer {access}")))
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(
        body["data"]["entries"]["custom.member_hint"],
        "members-only"
    );
}

#[actix_web::test]
async fn password_change_rejects_revoked_generation_and_revoked_session() {
    for revoke_all in [true, false] {
        let state = common::setup().await;
        let account = common::create_test_account(
            &state,
            "password-user",
            "password@example.com",
            "password123",
        )
        .await
        .unwrap();
        let user = user_repo::find_by_id(state.writer_db(), account.id)
            .await
            .unwrap();
        let (access, _) =
            local::issue_tokens_for_user(state.writer_db(), &state, &user, None, None)
                .await
                .unwrap();
        let claims = local::verify_token(&access, &state.config.auth.jwt_secret).unwrap();
        match revoke_all {
            true => {
                local::revoke_user_sessions(&state, user.id).await.unwrap();
            }
            false => {
                local::revoke_auth_session(&state, user.id, &claims.sid, None)
                    .await
                    .unwrap();
            }
        }
        assert!(
            local::change_password(&state, &claims, "password123", "new-password-123")
                .await
                .is_err()
        );
        assert_eq!(
            user_repo::find_by_id(state.writer_db(), user.id)
                .await
                .unwrap()
                .password_hash,
            user.password_hash
        );
    }
}

#[actix_web::test]
async fn concurrent_password_changes_cannot_both_replace_the_verified_password() {
    let state = common::setup_with_pool_size(4).await;
    let account = common::create_test_account(
        &state,
        "change-race",
        "change-race@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user = user_repo::find_by_id(state.writer_db(), account.id)
        .await
        .unwrap();
    let (access, _) = local::issue_tokens_for_user(state.writer_db(), &state, &user, None, None)
        .await
        .unwrap();
    let claims = local::verify_token(&access, &state.config.auth.jwt_secret).unwrap();
    let (first, second) = tokio::join!(
        local::change_password(&state, &claims, "password123", "first-new-password-123"),
        local::change_password(&state, &claims, "password123", "second-new-password-123"),
    );
    assert_ne!(
        first.is_ok(),
        second.is_ok(),
        "one credential generation can authorize exactly one replacement"
    );
    let updated = user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .unwrap();
    assert_eq!(updated.session_version, user.session_version + 1);
    let expected = match first {
        Ok(_) => "first-new-password-123",
        Err(_) => "second-new-password-123",
    };
    assert!(
        state
            .runtime_config
            .password_hash_runtime()
            .verify_password(expected, &updated.password_hash)
            .await
            .unwrap()
            .is_valid
    );
}

#[actix_web::test]
async fn email_confirmation_rechecks_blocklist_without_reapplying_allowlist() {
    for blocked in [true, false] {
        let state = common::setup().await;
        let account = common::create_test_account(
            &state,
            "contact-user",
            "contact@example.com",
            "password123",
        )
        .await
        .unwrap();
        let user = user_repo::find_by_id(state.writer_db(), account.id)
            .await
            .unwrap();
        let target = "changed@example.net";
        let mut active: user::ActiveModel = user.clone().into();
        active.pending_email = Set(Some(target.to_string()));
        active.update(state.writer_db()).await.unwrap();
        let record = contact_verification_token::ActiveModel {
            user_id: Set(user.id),
            channel: Set(VerificationChannel::Email),
            purpose: Set(VerificationPurpose::ContactChange),
            target: Set(target.to_string()),
            token_hash: Set(aster_forge_crypto::sha256_hex(b"contact-proof")),
            expires_at: Set(Utc::now() + Duration::minutes(10)),
            consumed_at: Set(None),
            created_at: Set(Utc::now()),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .unwrap();
        for (key, value) in [
            (
                aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
                "[\"new-policy.example\"]",
            ),
            (
                aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
                match blocked {
                    true => "[\"example.net\"]",
                    false => "[]",
                },
            ),
        ] {
            let value = config_repo::upsert_with_options(state.writer_db(), key, value, None, None)
                .await
                .unwrap();
            state.runtime_config.apply(value);
        }
        let result = local::confirm_contact_verification(&state, "contact-proof").await;
        match blocked {
            true => {
                assert_eq!(
                    result.unwrap_err().api_error_code(),
                    aster_drive::api::api_error_code::ApiErrorCode::AuthEmailBlocked
                );
                assert_eq!(
                    user_repo::find_by_id(state.writer_db(), user.id)
                        .await
                        .unwrap()
                        .email,
                    user.email
                );
                use sea_orm::EntityTrait;
                assert!(
                    contact_verification_token::Entity::find_by_id(record.id)
                        .one(state.writer_db())
                        .await
                        .unwrap()
                        .unwrap()
                        .consumed_at
                        .is_none()
                );
            }
            false => {
                result.unwrap();
                assert_eq!(
                    user_repo::find_by_id(state.writer_db(), user.id)
                        .await
                        .unwrap()
                        .email,
                    target
                );
            }
        }
    }
}

#[actix_web::test]
async fn established_storage_stream_closes_at_the_verified_access_expiry() {
    let state = common::setup().await;
    let account =
        common::create_test_account(&state, "stream-user", "stream@example.com", "password123")
            .await
            .unwrap();
    let user = user_repo::find_by_id(state.writer_db(), account.id)
        .await
        .unwrap();
    let (access, _) = local::issue_tokens_for_user(state.writer_db(), &state, &user, None, None)
        .await
        .unwrap();
    let claims = local::verify_token(&access, &state.config.auth.jwt_secret).unwrap();
    let app = create_test_app!(state.clone());
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/auth/events/storage")
            .insert_header(("Authorization", format!("Bearer {access}")))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let mut body = response.into_body();
    assert!(
        poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
            .await
            .unwrap()
            .is_ok()
    );
    tokio::time::pause();
    let remaining = u64::try_from(claims.exp)
        .unwrap()
        .saturating_sub(u64::try_from(Utc::now().timestamp()).unwrap());
    tokio::time::advance(std::time::Duration::from_secs(remaining + 1)).await;
    aster_drive::services::events::storage_change::publish(
        &state,
        aster_drive::services::events::storage_change::StorageChangeEvent::sync_required(),
    );
    assert!(
        poll_fn(|cx| Pin::new(&mut body).poll_next(cx))
            .await
            .is_none(),
        "expiry must win over pending event/heartbeat delivery"
    );
    tokio::time::resume();
}
