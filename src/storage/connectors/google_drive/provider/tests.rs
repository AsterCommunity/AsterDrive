use super::*;
use crate::storage::connectors::test_support;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use sea_orm::ActiveModelTrait;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn database() -> (sea_orm::DatabaseConnection, i64) {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    test_support::migrate_current_storage_test_schema(&db).await;
    let policy = test_support::policy(
        GoogleDriveConnector::ID,
        1,
        serde_json::json!({}),
        aster_drive_storage::StoragePolicyBehaviorConfig::default(),
    );
    let policy = test_support::insertable_policy(policy)
        .insert(&db)
        .await
        .unwrap();
    (db, policy.id)
}
const KEY: &str = "google-drive-test-master-key-32bytes";
async fn save(db: &sea_orm::DatabaseConnection, id: i64) -> GoogleDriveCredentialV1 {
    let mut payload = super::super::tests::payload();
    payload.authorization.as_mut().unwrap().expires_at =
        Some(Utc::now() - chrono::Duration::minutes(1));
    crate::storage::connectors::persist_connector_credential_payload(
        db,
        KEY,
        id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &payload,
    )
    .await
    .unwrap();
    payload
}
async fn server(status: u16) -> (String, Arc<AtomicUsize>, actix_web::dev::ServerHandle) {
    let count = Arc::new(AtomicUsize::new(0));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/token", listener.local_addr().unwrap());
    let calls = count.clone();
    let server = HttpServer::new(move || {
        let calls = calls.clone();
        App::new().app_data(web::Data::new(calls)).default_service(web::to(move |req: HttpRequest, body: web::Bytes, calls: web::Data<Arc<AtomicUsize>>| async move {
            calls.fetch_add(1,Ordering::SeqCst);
            assert_eq!(req.method(),"POST");
            let form: std::collections::BTreeMap<String,String> = url::form_urlencoded::parse(&body).into_owned().collect();
            assert_eq!(form["grant_type"], "refresh_token");
            assert_eq!(form["refresh_token"], "private-refresh");
            if status == 200 { HttpResponse::Ok().json(serde_json::json!({"access_token":"rotated-access","refresh_token":"rotated-refresh","expires_in":3600,"token_type":"Bearer"})) }
            else { HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap()).json(serde_json::json!({"error":"invalid_grant","error_description":"private-secret"})) }
        }))
    }).workers(1).listen(listener).unwrap().run();
    let handle = server.handle();
    tokio::spawn(server);
    (base, count, handle)
}
#[tokio::test]
async fn refresh_is_single_flight_and_persists_rotated_tokens_encrypted() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let (url, calls, handle) = server(200).await;
    let mut provider = GoogleTokenProvider::new(Some(db.clone()), KEY.into(), id, payload);
    Arc::get_mut(&mut provider).unwrap().token_url = url;
    let (a, b) = tokio::join!(provider.access_token(), provider.access_token());
    assert_eq!(a.unwrap(), "rotated-access");
    assert_eq!(b.unwrap(), "rotated-access");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    assert_eq!(row.revision, 2);
    assert!(!row.ciphertext.contains("rotated-"));
    let payload = super::super::decode(KEY, &row).unwrap();
    let auth = payload.authorization.unwrap();
    assert_eq!(auth.refresh_token.as_deref(), Some("rotated-refresh"));
    assert!(auth.last_refreshed_at.is_some());
    handle.stop(true).await;
}
#[tokio::test]
async fn rejected_refresh_requires_reauthorization_but_transient_failures_keep_authorization() {
    for (status, kind, expected) in [
        (
            400,
            StorageErrorKind::Auth,
            StorageCredentialStatus::ReauthRequired,
        ),
        (
            429,
            StorageErrorKind::RateLimited,
            StorageCredentialStatus::Authorized,
        ),
        (
            503,
            StorageErrorKind::Transient,
            StorageCredentialStatus::Authorized,
        ),
    ] {
        let (db, id) = database().await;
        let payload = save(&db, id).await;
        let (url, _, handle) = server(status).await;
        let mut provider = GoogleTokenProvider::new(Some(db.clone()), KEY.into(), id, payload);
        Arc::get_mut(&mut provider).unwrap().token_url = url;
        let error = provider.access_token().await.unwrap_err();
        assert_eq!(error.kind(), kind);
        assert!(!error.to_string().contains("private-secret"));
        let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
        assert_eq!(
            super::super::decode(KEY, &row)
                .unwrap()
                .authorization
                .unwrap()
                .status,
            expected
        );
        handle.stop(true).await;
    }
}
#[tokio::test]
async fn stale_refresh_cannot_overwrite_new_application_credentials() {
    let (db, id) = database().await;
    let payload = save(&db, id).await;
    let row = repo::find_by_policy(&db, id).await.unwrap().unwrap();
    let mut changed = payload.clone();
    changed.application.client_id = "new-application".into();
    changed.authorization = None;
    crate::storage::connectors::persist_connector_credential_payload(
        &db,
        KEY,
        id,
        &ConnectorId::declared(GoogleDriveConnector::ID),
        1,
        &changed,
    )
    .await
    .unwrap();
    assert!(!persist(&db, KEY, &row, &payload).await.unwrap());
    assert_eq!(
        super::super::decode(KEY, &repo::find_by_policy(&db, id).await.unwrap().unwrap())
            .unwrap()
            .application
            .client_id,
        "new-application"
    );
}
#[test]
fn malformed_token_metadata_is_rejected() {
    for token in [
        serde_json::json!({"access_token":"","expires_in":3600,"token_type":"Bearer"}),
        serde_json::json!({"access_token":"x","expires_in":-1,"token_type":"Bearer"}),
        serde_json::json!({"access_token":"x","expires_in":i64::MAX,"token_type":"Bearer"}),
        serde_json::json!({"access_token":"x","expires_in":3600,"token_type":"Basic"}),
    ] {
        assert!(
            serde_json::from_value::<TokenResponse>(token)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
