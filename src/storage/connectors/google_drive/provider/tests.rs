//! HTTP + encrypted SQLite tests for refresh classification, rotation and races.
use super::*;
use crate::storage::{
    connectors::test_support,
    test_support::{HttpFixture, HttpReply, ResponseGate},
};
use sea_orm::ActiveModelTrait;
use serde_json::json;

const KEY: &str = "google-drive-test-master-key-32bytes";
async fn database() -> (sea_orm::DatabaseConnection, i64) {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    test_support::migrate_current_storage_test_schema(&db).await;
    let policy = test_support::insertable_policy(test_support::policy(
        GoogleDriveConnector::ID,
        1,
        json!({}),
        aster_drive_storage::StoragePolicyBehaviorConfig::default(),
    ))
    .insert(&db)
    .await
    .unwrap();
    (db, policy.id)
}
async fn store(db: &sea_orm::DatabaseConnection, id: i64, payload: &GoogleDriveCredentialV1) {
    crate::storage::connectors::persist_connector_credential_payload(
        db,
        KEY,
        id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        payload,
    )
    .await
    .unwrap();
}
async fn save(db: &sea_orm::DatabaseConnection, id: i64) -> GoogleDriveCredentialV1 {
    let mut payload = super::super::tests::payload();
    payload.authorization.as_mut().unwrap().expires_at =
        Some(Utc::now() - chrono::Duration::minutes(1));
    store(db, id, &payload).await;
    payload
}
fn token() -> serde_json::Value {
    json!({"access_token":"rotated-access","refresh_token":"rotated-refresh","expires_in":3600,"token_type":"Bearer"})
}
fn new_provider(
    db: &sea_orm::DatabaseConnection,
    id: i64,
    payload: GoogleDriveCredentialV1,
    peer: &HttpFixture,
) -> Arc<GoogleTokenProvider> {
    let mut provider = GoogleTokenProvider::new(
        Some(db.clone()),
        KEY.into(),
        id,
        payload,
        CredentialUse::Runtime,
    )
    .unwrap();
    Arc::get_mut(&mut provider).unwrap().token_url = format!("{}/token", peer.base);
    provider
}
fn assert_form(peer: &HttpFixture) {
    for req in peer.requests() {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/token");
        assert!(req.headers["content-type"].starts_with("application/x-www-form-urlencoded"));
        let values: std::collections::BTreeMap<_, _> = url::form_urlencoded::parse(&req.body)
            .into_owned()
            .collect();
        assert_eq!(values["grant_type"], "refresh_token");
        assert_eq!(values["refresh_token"], "private-refresh");
        assert_eq!(values["client_id"], "client");
        assert_eq!(values["client_secret"], "private-secret");
    }
}

#[tokio::test]
async fn refresh_is_single_flight_and_persists_rotated_tokens_encrypted() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let peer = HttpFixture::new([HttpReply::json(200, token())]).await;
    let provider = new_provider(&db, id, payload, &peer);
    let (a, b) = tokio::join!(provider.access_token(), provider.access_token());
    assert_eq!(a.unwrap(), "rotated-access");
    assert_eq!(b.unwrap(), "rotated-access");
    assert_eq!(peer.requests().len(), 1);
    assert_form(&peer);
    let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(row.revision, 2);
    assert!(!row.ciphertext.contains("rotated-"));
    let auth = super::super::decode(KEY, &row)
        .unwrap()
        .authorization
        .unwrap();
    assert_eq!(auth.refresh_token.as_deref(), Some("rotated-refresh"));
    assert!(auth.last_refreshed_at.is_some());
    assert!(!format!("{provider:?}").contains("private-"));
    peer.stop().await;
}

#[tokio::test]
async fn oauth_error_codes_and_http_failures_have_distinct_durable_effects() {
    for (status, code, kind, next) in [
        (
            400,
            Some("invalid_grant"),
            StorageErrorKind::Auth,
            Some(StorageCredentialStatus::ReauthRequired),
        ),
        (
            401,
            Some("invalid_grant"),
            StorageErrorKind::Auth,
            Some(StorageCredentialStatus::ReauthRequired),
        ),
        (
            400,
            Some("invalid_client"),
            StorageErrorKind::Misconfigured,
            Some(StorageCredentialStatus::Invalid),
        ),
        (
            401,
            Some("invalid_client"),
            StorageErrorKind::Misconfigured,
            Some(StorageCredentialStatus::Invalid),
        ),
        (
            400,
            Some("unauthorized_client"),
            StorageErrorKind::Misconfigured,
            Some(StorageCredentialStatus::Invalid),
        ),
        (
            400,
            Some("invalid_request"),
            StorageErrorKind::Transient,
            None,
        ),
        (
            400,
            Some("unknown_error"),
            StorageErrorKind::Transient,
            None,
        ),
        (401, None, StorageErrorKind::Transient, None),
        (403, None, StorageErrorKind::Transient, None),
        (
            404,
            Some("invalid_grant"),
            StorageErrorKind::Transient,
            None,
        ),
        (
            408,
            Some("invalid_grant"),
            StorageErrorKind::Transient,
            None,
        ),
        (
            429,
            Some("invalid_grant"),
            StorageErrorKind::RateLimited,
            None,
        ),
        (
            500,
            Some("invalid_client"),
            StorageErrorKind::Transient,
            None,
        ),
        (
            503,
            Some("invalid_grant"),
            StorageErrorKind::Transient,
            None,
        ),
        (302, None, StorageErrorKind::Transient, None),
    ] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let before = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        let reply = if let Some(code) = code {
            HttpReply::json(
                status,
                json!({"error":code,"error_description":"private-secret"}),
            )
        } else {
            HttpReply::text(status, "Temporary proxy rejection private-secret")
        };
        let peer =
            HttpFixture::new([reply.header("location", "https://attacker.invalid/token")]).await;
        let provider = new_provider(&db, id, payload, &peer);
        let error = provider.access_token().await.unwrap_err();
        assert_eq!(error.kind(), kind, "HTTP {status} {code:?}");
        assert!(!error.to_string().contains("private-secret"));
        let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        let auth = super::super::decode(KEY, &row)
            .unwrap()
            .authorization
            .unwrap();
        assert_eq!(
            auth.status,
            next.unwrap_or(StorageCredentialStatus::Authorized)
        );
        assert!(
            auth.status_reason
                .as_deref()
                .is_none_or(|s| !s.contains("private-secret"))
        );
        if next.is_none() {
            assert_eq!(row.revision, before.revision);
            assert_eq!(row.ciphertext, before.ciphertext);
        } else {
            assert_eq!(row.revision, before.revision + 1);
            assert_eq!(provider.access_token().await.unwrap_err().kind(), kind);
        }
        assert_eq!(peer.requests().len(), 1);
        peer.stop().await;
    }
}

#[tokio::test]
async fn unknown_gateway_errors_remain_retryable_without_reauthorization() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let peer = HttpFixture::new([
        HttpReply::text(403, "proxy rejection"),
        HttpReply::json(200, token()),
    ])
    .await;
    let provider = new_provider(&db, id, payload, &peer);
    assert_eq!(
        provider.access_token().await.unwrap_err().kind(),
        StorageErrorKind::Transient
    );
    assert_eq!(provider.access_token().await.unwrap(), "rotated-access");
    let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(row.revision, 2);
    assert_eq!(
        super::super::decode(KEY, &row)
            .unwrap()
            .authorization
            .unwrap()
            .status,
        StorageCredentialStatus::Authorized
    );
    peer.stop().await;
}

#[tokio::test]
async fn missing_rotation_fields_preserve_existing_refresh_token_and_scopes() {
    for refresh in [None, Some(""), Some("   ")] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let mut response =
            json!({"access_token":"new-access","expires_in":3600,"token_type":"bearer"});
        if let Some(refresh) = refresh {
            response["refresh_token"] = json!(refresh);
        }
        let peer = HttpFixture::new([HttpReply::json(200, response)]).await;
        assert_eq!(
            new_provider(&db, id, payload, &peer)
                .access_token()
                .await
                .unwrap(),
            "new-access"
        );
        let auth =
            super::super::decode(KEY, &repo::find_by_policy(&db, id).await.unwrap().unwrap())
                .unwrap()
                .authorization
                .unwrap();
        assert_eq!(auth.refresh_token.as_deref(), Some("private-refresh"));
        assert_eq!(auth.scopes, ["https://www.googleapis.com/auth/drive"]);
        peer.stop().await;
    }
}

#[tokio::test]
async fn malformed_success_responses_do_not_replace_or_invalidate_saved_tokens() {
    for response in [
        HttpReply::text(200, "not json private-secret"),
        HttpReply::json(
            200,
            json!({"access_token":"","expires_in":3600,"token_type":"Bearer"}),
        ),
        HttpReply::json(
            200,
            json!({"access_token":"with\nnewline","expires_in":3600,"token_type":"Bearer"}),
        ),
        HttpReply::json(
            200,
            json!({"access_token":"x","expires_in":0,"token_type":"Bearer"}),
        ),
        HttpReply::json(
            200,
            json!({"access_token":"x","expires_in":-1,"token_type":"Bearer"}),
        ),
        HttpReply::json(
            200,
            json!({"access_token":"x","expires_in":i64::MAX,"token_type":"Bearer"}),
        ),
        HttpReply::json(200, json!({"access_token":"x","token_type":"Bearer"})),
        HttpReply::json(
            200,
            json!({"access_token":"x","expires_in":3600,"token_type":"Basic"}),
        ),
        HttpReply::json(200, json!({"access_token":"x","expires_in":3600})),
    ] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let before = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        let peer = HttpFixture::new([response]).await;
        let error = new_provider(&db, id, payload, &peer)
            .access_token()
            .await
            .unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Transient);
        assert!(!error.to_string().contains("private-secret"));
        let after = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.ciphertext, before.ciphertext);
        peer.stop().await;
    }
}

#[tokio::test]
async fn in_flight_refresh_cannot_overwrite_a_new_application_or_grant() {
    // Both success and permanent-error responses can lose the CAS to a new grant.
    for (reply, authorized) in [
        (HttpReply::json(200, token()), true),
        (HttpReply::json(400, json!({"error":"invalid_grant"})), true),
        (HttpReply::json(200, token()), false),
    ] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let gate = Arc::new(ResponseGate::default());
        let peer = HttpFixture::new([reply.gated(gate.clone())]).await;
        let provider = new_provider(&db, id, payload.clone(), &peer);
        let edit = async {
            gate.entered.notified().await;
            let mut changed = super::super::tests::payload();
            changed.application.client_id = "new-application".into();
            if authorized {
                changed.authorization.as_mut().unwrap().access_token = "winner-access".into();
            } else {
                changed.authorization = None;
            }
            store(&db, id, &changed).await;
            gate.release.notify_one();
        };
        let (result, ()) = tokio::join!(provider.access_token(), edit);
        if authorized {
            assert_eq!(result.unwrap(), "winner-access");
        } else {
            assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Auth);
        }
        let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        assert_eq!(row.revision, 2);
        assert_eq!(
            super::super::decode(KEY, &row)
                .unwrap()
                .application
                .client_id,
            "new-application"
        );
        peer.stop().await;
    }
}

#[tokio::test]
async fn deleted_credentials_and_bounded_cas_conflicts_do_not_publish_stale_tokens() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let gate = Arc::new(ResponseGate::default());
    let peer = HttpFixture::new([HttpReply::json(200, token()).gated(gate.clone())]).await;
    let provider = new_provider(&db, id, payload, &peer);
    let (result, ()) = tokio::join!(provider.access_token(), async {
        gate.entered.notified().await;
        repo::delete_by_policy(&db, id).await.unwrap();
        gate.release.notify_one();
    });
    assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Auth);
    assert!(repo::find_by_policy(&db, id).await.unwrap().is_none());
    peer.stop().await;

    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let gates: Vec<_> = (0..3).map(|_| Arc::new(ResponseGate::default())).collect();
    let peer = HttpFixture::new(
        gates
            .iter()
            .map(|gate| HttpReply::json(200, token()).gated(gate.clone())),
    )
    .await;
    let provider = new_provider(&db, id, payload.clone(), &peer);
    let (result, ()) = tokio::join!(provider.access_token(), async {
        for gate in &gates {
            gate.entered.notified().await;
            store(&db, id, &payload).await;
            gate.release.notify_one();
        }
    });
    assert_eq!(result.unwrap_err().kind(), StorageErrorKind::Transient);
    assert_eq!(peer.requests().len(), 3);
    assert_eq!(
        repo::find_by_policy(&db, id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        4
    );
    peer.stop().await;
}

#[tokio::test]
async fn independent_providers_converge_on_the_one_persisted_refresh() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let a = Arc::new(ResponseGate::default());
    let b = Arc::new(ResponseGate::default());
    let peer = HttpFixture::new([
        HttpReply::json(200, token()).gated(a.clone()),
        HttpReply::json(200, token()).gated(b.clone()),
    ])
    .await;
    let one = new_provider(&db, id, payload.clone(), &peer);
    let two = new_provider(&db, id, payload, &peer);
    let (one, two, ()) = tokio::join!(one.access_token(), two.access_token(), async {
        a.entered.notified().await;
        b.entered.notified().await;
        a.release.notify_one();
        b.release.notify_one();
    });
    assert_eq!(one.unwrap(), "rotated-access");
    assert_eq!(two.unwrap(), "rotated-access");
    assert_eq!(
        repo::find_by_policy(&db, id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        2
    );
    peer.stop().await;
}

#[tokio::test]
async fn cleanup_snapshot_refreshes_without_a_live_policy_row() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    repo::delete_by_policy(&db, id).await.unwrap();
    let peer = HttpFixture::new([HttpReply::json(200, token())]).await;
    let mut provider =
        GoogleTokenProvider::new(None, KEY.into(), id, payload, CredentialUse::Runtime).unwrap();
    Arc::get_mut(&mut provider).unwrap().token_url = format!("{}/token", peer.base);
    assert_eq!(provider.access_token().await.unwrap(), "rotated-access");
    assert_eq!(provider.access_token().await.unwrap(), "rotated-access");
    assert_eq!(peer.requests().len(), 1);
    assert!(repo::find_by_policy(&db, id).await.unwrap().is_none());
    peer.stop().await;
}

#[tokio::test]
async fn unusable_statuses_and_missing_refresh_tokens_fail_before_network_io() {
    for (status, kind) in [
        (
            StorageCredentialStatus::ReauthRequired,
            StorageErrorKind::Auth,
        ),
        (StorageCredentialStatus::Revoked, StorageErrorKind::Auth),
        (
            StorageCredentialStatus::Invalid,
            StorageErrorKind::Misconfigured,
        ),
        (
            StorageCredentialStatus::PermissionDenied,
            StorageErrorKind::Permission,
        ),
    ] {
        let (db, id) = database().await;
        let mut payload = save(&db, id).await;
        payload.authorization.as_mut().unwrap().status = status;
        store(&db, id, &payload).await;
        let peer = HttpFixture::new([]).await;
        assert_eq!(
            new_provider(&db, id, payload, &peer)
                .access_token()
                .await
                .unwrap_err()
                .kind(),
            kind
        );
        assert!(peer.requests().is_empty());
        peer.stop().await;
    }
    let (db, id) = database().await;
    let mut payload = save(&db, id).await;
    payload.authorization.as_mut().unwrap().refresh_token = None;
    store(&db, id, &payload).await;
    let peer = HttpFixture::new([]).await;
    assert_eq!(
        new_provider(&db, id, payload, &peer)
            .access_token()
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Auth
    );
    assert!(peer.requests().is_empty());
    peer.stop().await;
}

#[tokio::test]
async fn transport_disconnects_and_timeouts_preserve_saved_authorization() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let before = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/token", listener.local_addr().unwrap());
    drop(listener);
    let mut provider = GoogleTokenProvider::new(
        Some(db.clone()),
        KEY.into(),
        id,
        payload.clone(),
        CredentialUse::Runtime,
    )
    .unwrap();
    Arc::get_mut(&mut provider).unwrap().token_url = url;
    assert_eq!(
        provider.access_token().await.unwrap_err().kind(),
        StorageErrorKind::Transient
    );
    let gate = Arc::new(ResponseGate::default());
    let peer = HttpFixture::new([HttpReply::json(200, token()).gated(gate.clone())]).await;
    let mut provider = GoogleTokenProvider::new(
        Some(db.clone()),
        KEY.into(),
        id,
        payload,
        CredentialUse::Runtime,
    )
    .unwrap();
    let inner = Arc::get_mut(&mut provider).unwrap();
    inner.token_url = format!("{}/token", peer.base);
    inner.http = reqwest::Client::builder()
        .read_timeout(std::time::Duration::from_millis(50))
        .build()
        .unwrap();
    assert_eq!(
        provider.access_token().await.unwrap_err().kind(),
        StorageErrorKind::Transient
    );
    gate.release.notify_one();
    let after = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(after.ciphertext, before.ciphertext);
    assert_eq!(after.revision, before.revision);
    peer.stop().await;
}

#[test]
fn token_expiry_handles_overflow_and_uses_the_provider_duration() {
    for seconds in [i64::MIN, -1, 0, i64::MAX] {
        let token = TokenResponse {
            access_token: "x".into(),
            refresh_token: None,
            expires_in: Some(seconds),
            scope: None,
            token_type: Some("Bearer".into()),
        };
        assert!(token.validate().is_err());
        assert!(token.expires_at(Utc::now()).is_none());
    }
    let now = Utc::now();
    let token = TokenResponse {
        access_token: "x".into(),
        refresh_token: None,
        expires_in: Some(172800),
        scope: None,
        token_type: Some("Bearer".into()),
    };
    token.validate().unwrap();
    assert_eq!(token.expires_at(now), Some(now + chrono::Duration::days(2)));
}

#[tokio::test]
async fn explicit_validation_can_probe_repaired_permissions_without_enabling_runtime() {
    for status in [
        StorageCredentialStatus::PermissionDenied,
        StorageCredentialStatus::Invalid,
    ] {
        let (db, id) = database().await;
        let mut payload = super::super::tests::payload();
        payload.authorization.as_mut().unwrap().status = status;
        store(&db, id, &payload).await;
        let peer = HttpFixture::new([]).await;
        let runtime = new_provider(&db, id, payload.clone(), &peer);
        assert!(runtime.access_token().await.is_err());
        let mut probe = GoogleTokenProvider::new(
            Some(db.clone()),
            KEY.into(),
            id,
            payload,
            CredentialUse::Validation,
        )
        .unwrap();
        Arc::get_mut(&mut probe).unwrap().token_url = format!("{}/token", peer.base);
        assert_eq!(probe.access_token().await.unwrap(), "private-access");
        assert!(runtime.access_token().await.is_err());
        let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        assert_eq!(
            super::super::decode(KEY, &row)
                .unwrap()
                .authorization
                .unwrap()
                .status,
            status
        );
        assert!(peer.requests().is_empty());
        peer.stop().await;
    }
}

#[tokio::test]
async fn token_endpoint_redirects_never_forward_refresh_credentials() {
    for status in [301, 302, 307, 308] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let untrusted = HttpFixture::new([]).await;
        let peer =
            HttpFixture::new([HttpReply::text(status, "")
                .header("location", format!("{}/capture", untrusted.base))])
            .await;
        let provider = new_provider(&db, id, payload, &peer);
        assert_eq!(
            provider.access_token().await.unwrap_err().kind(),
            StorageErrorKind::Transient
        );
        assert_eq!(peer.requests().len(), 1);
        assert!(untrusted.requests().is_empty());
        peer.stop().await;
        untrusted.stop().await;
    }
}
