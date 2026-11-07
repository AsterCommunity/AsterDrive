//! Replacements retain their placement but obey the current profile admission.
use crate::common;
use actix_web::test;
use aster_drive::db::repository::{file_repo, policy_group_repo, user_repo};
use aster_drive::services::files::file::{self, StoreFromTempRequest};
use aster_drive::services::storage_policy::policy::placement::{
    PlacementPayloadEnvelope, StorageAdmissionConstraints,
};
use aster_drive_model::entities::storage_policy_group;
use sea_orm::{ActiveModelTrait, Set};

async fn set_admission(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
    admission: StorageAdmissionConstraints,
) {
    let user = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    let id = state
        .policy_snapshot
        .require_user_policy_group_id(user.id)
        .unwrap();
    let group = policy_group_repo::find_group_by_id(state.writer_db(), id)
        .await
        .unwrap();
    let mut active: storage_policy_group::ActiveModel = group.into();
    active.admission_config =
        Set(serde_json::to_string(&PlacementPayloadEnvelope::new(admission)).unwrap());
    active.update(state.writer_db()).await.unwrap();
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();
}

#[actix_web::test]
async fn replacement_profile_size_and_type_limits_reject_without_publishing() {
    let state = common::setup().await;
    let user = common::create_test_account(
        &state,
        "replacegroup",
        "replacegroup@example.com",
        "pass1234",
    )
    .await
    .unwrap();
    let source = std::path::Path::new(&state.config.server.temp_dir).join("admission-source.txt");
    tokio::fs::create_dir_all(source.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&source, b"old").await.unwrap();
    let first = file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "report.txt", source.to_str().unwrap(), 3),
    )
    .await
    .unwrap();
    let original = file_repo::find_by_id(state.writer_db(), first.id)
        .await
        .unwrap();
    let original_blob = file_repo::find_blob_by_id(state.writer_db(), original.blob_id)
        .await
        .unwrap();
    set_admission(
        &state,
        user.id,
        StorageAdmissionConstraints {
            max_file_size: 4,
            ..Default::default()
        },
    )
    .await;
    tokio::fs::write(&source, b"oversized").await.unwrap();
    let error = file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "report.txt", source.to_str().unwrap(), 9)
            .overwrite(first.id),
    )
    .await
    .unwrap_err();
    assert!(error.message().contains("limit"));
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), first.id)
            .await
            .unwrap()
            .blob_id,
        original.blob_id
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        3
    );

    set_admission(
        &state,
        user.id,
        StorageAdmissionConstraints {
            denied_extensions: vec!["txt".into()],
            ..Default::default()
        },
    )
    .await;
    tokio::fs::write(&source, b"four").await.unwrap();
    let error = file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "report.txt", source.to_str().unwrap(), 4)
            .overwrite(first.id),
    )
    .await
    .unwrap_err();
    assert!(error.message().contains("placement_extension_denied"));
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), first.id)
            .await
            .unwrap()
            .blob_id,
        original.blob_id
    );

    set_admission(
        &state,
        user.id,
        StorageAdmissionConstraints {
            max_file_size: 4,
            ..Default::default()
        },
    )
    .await;
    let replacement = file::store_from_temp(
        &state,
        user.id,
        StoreFromTempRequest::new(None, "report.txt", source.to_str().unwrap(), 4)
            .overwrite(first.id),
    )
    .await
    .unwrap();
    let blob = file_repo::find_blob_by_id(state.writer_db(), replacement.blob_id)
        .await
        .unwrap();
    assert_eq!(
        blob.policy_id, original_blob.policy_id,
        "replacement must not reroute storage"
    );
    assert_eq!(replacement.size, 4);
}

#[actix_web::test]
async fn unknown_length_content_stream_obeys_profile_ceiling_and_keeps_old_revision() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let before = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    set_admission(
        &state,
        user.id,
        StorageAdmissionConstraints {
            max_file_size: 4,
            ..Default::default()
        },
    )
    .await;
    let request = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("oversized")
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["code"], "file.too_large");
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), file_id)
            .await
            .unwrap()
            .blob_id,
        before.blob_id
    );
}
