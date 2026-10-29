use super::*;
use crate::services::storage_policy::policy::placement::{
    PlacementMatcher, PlacementPayloadEnvelope,
};
use crate::services::workspace::storage::WorkspaceStorageScope;
use aster_drive_model::entities::{
    folder, storage_policy_group_rule as rule, storage_policy_group_rule_target as target, team,
    team_member,
};
use sea_orm::{ActiveValue::NotSet, ColumnTrait, EntityTrait, QueryFilter};

async fn reload(state: &PrimaryAppState) {
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();
}

async fn set_policy_limit(state: &PrimaryAppState, policy: storage_policy::Model, size: i64) {
    let mut active = policy.into_active_model();
    active.max_file_size = Set(size);
    active.update(state.writer_db()).await.unwrap();
    reload(state).await;
}

fn context(user_id: i64, name: &str) -> DavWriteOpenContext {
    DavWriteOpenContext {
        scope: WorkspaceStorageScope::Personal { user_id },
        folder_id: None,
        filename: name.into(),
        existing_file_id: None,
        declared_size: None,
        payload_limit: Config::default().webdav.payload_limit,
        submitted_lock_tokens: vec![],
        file_precondition: None,
        audit_ctx: crate::services::ops::audit::AuditContext {
            user_id,
            ip_address: None,
            user_agent: None,
        },
    }
}

#[tokio::test]
async fn exact_policy_and_remaining_quota_limit_commits_and_charges_once() {
    let (state, user, policy, driver) = build_s3_direct_test_state().await;
    set_policy_limit(&state, policy, 8).await;
    let mut active = user.clone().into_active_model();
    active.storage_used = Set(2);
    active.storage_quota = Set(10);
    active.update(state.writer_db()).await.unwrap();
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "exact.txt"))
            .await
            .unwrap();
    for chunk in [b"1234".as_slice(), b"5678", b""] {
        handle
            .write_bytes(Bytes::copy_from_slice(chunk))
            .await
            .unwrap();
    }
    handle.finish().await.unwrap();
    let stored = file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "exact.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.size, 8);
    assert_eq!(
        crate::db::repository::user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        10
    );
    assert_eq!(driver.put_file_calls(), 1);
}

#[tokio::test]
async fn rejected_chunk_cleans_immediately_and_poisoned_handle_cannot_commit() {
    let (state, user, policy, driver) = build_s3_direct_test_state().await;
    set_policy_limit(&state, policy, 8).await;
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "reject.txt"))
            .await
            .unwrap();
    let path = match &handle.mode {
        super::super::WriteMode::Write { temp_path, .. } => temp_path.clone(),
        _ => panic!("expected temporary upload"),
    };
    handle
        .write_bytes(Bytes::from_static(b"12345678"))
        .await
        .unwrap();
    let error = handle
        .write_bytes(Bytes::from_static(b"9"))
        .await
        .unwrap_err();
    assert_eq!(error.kind, DavBackendErrorKind::PayloadTooLarge);
    assert!(
        !Path::new(&path).exists(),
        "rejection must clean even if caller retains the handle"
    );
    assert_eq!(handle.write_bytes(Bytes::new()).await.unwrap_err(), error);
    assert_eq!(handle.finish().await.unwrap_err(), error);
    assert!(
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "reject.txt")
            .await
            .unwrap()
            .is_none()
    );
    assert!(driver.objects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn direct_declared_overflow_cleans_partial_remote_object_and_abort_is_idempotent() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let mut handle = AsterDavWriteHandle::for_write(
        state.clone(),
        user.id,
        None,
        "direct-over.txt".into(),
        None,
        Some(8),
    )
    .await
    .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"1234"))
        .await
        .unwrap();
    assert_eq!(
        handle
            .write_bytes(Bytes::from_static(b"56789"))
            .await
            .unwrap_err()
            .kind,
        DavBackendErrorKind::InvalidInput
    );
    assert!(driver.objects.lock().unwrap().is_empty());
    handle.abort().await.unwrap();
    assert!(
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct-over.txt")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn cancelling_error_cleanup_still_removes_the_partial_remote_object() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let pause = Arc::new(tokio::sync::Semaphore::new(0));
    *driver.upload_pause.lock().unwrap() = Some(pause.clone());
    let mut handle = AsterDavWriteHandle::for_write(
        state,
        user.id,
        None,
        "cancel-direct.txt".into(),
        None,
        Some(8),
    )
    .await
    .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"1234"))
        .await
        .unwrap();
    // The driver finishes after EOF but delays committing its partial object. Cancel the
    // error response while it waits for cleanup, then let the driver publish that object.
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(25),
            handle.write_bytes(Bytes::from_static(b"56789"))
        )
        .await
        .is_err()
    );
    drop(handle);
    pause.add_permits(1);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while driver.delete_calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cleanup must outlive the cancelled response future");
    assert!(driver.objects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn known_length_short_body_is_rejected_at_finish() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let mut handle =
        AsterDavWriteHandle::for_write(state, user.id, None, "short.txt".into(), None, Some(8))
            .await
            .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"1234567"))
        .await
        .unwrap();
    assert_eq!(
        handle.finish().await.unwrap_err().kind,
        DavBackendErrorKind::InvalidInput
    );
    assert!(driver.objects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn dropping_unfinished_upload_removes_temp_without_persisting() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "cancelled.txt"))
            .await
            .unwrap();
    let path = match &handle.mode {
        super::super::WriteMode::Write { temp_path, .. } => temp_path.clone(),
        _ => panic!("expected temporary upload"),
    };
    handle
        .write_bytes(Bytes::from_static(b"partial"))
        .await
        .unwrap();
    drop(handle);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while Path::new(&path).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cancelled write must clean its staging file");
    assert!(driver.objects.lock().unwrap().is_empty());
    assert!(
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "cancelled.txt")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn quota_changed_after_open_is_rechecked_at_finish() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "race.txt"))
            .await
            .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"12345678"))
        .await
        .unwrap();
    let mut active = user.clone().into_active_model();
    active.storage_used = Set(9);
    active.storage_quota = Set(10);
    active.update(state.writer_db()).await.unwrap();
    assert_eq!(
        handle.finish().await.unwrap_err().kind,
        DavBackendErrorKind::InsufficientStorage
    );
    assert!(
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "race.txt")
            .await
            .unwrap()
            .is_none()
    );
    assert!(driver.objects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn overwrite_retains_old_version_and_charges_full_new_version() {
    let (state, user, policy, driver) = build_s3_direct_test_state().await;
    set_policy_limit(&state, policy, 8).await;
    let mut active = user.clone().into_active_model();
    active.storage_quota = Set(12);
    active.update(state.writer_db()).await.unwrap();
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "version.txt"))
            .await
            .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"1234"))
        .await
        .unwrap();
    handle.finish().await.unwrap();
    let stored = file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "version.txt")
        .await
        .unwrap()
        .unwrap();
    let mut request = context(user.id, "version.txt");
    request.existing_file_id = Some(stored.id);
    let mut handle = AsterDavWriteHandle::for_write_with_audit(state.clone(), request)
        .await
        .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"12345678"))
        .await
        .unwrap();
    handle.finish().await.unwrap();
    assert_eq!(
        crate::db::repository::user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        12
    );
    assert_eq!(
        driver.objects.lock().unwrap().len(),
        2,
        "old revision must remain stored"
    );
    let mut request = context(user.id, "version.txt");
    request.existing_file_id = Some(stored.id);
    let mut handle = AsterDavWriteHandle::for_write_with_audit(state.clone(), request)
        .await
        .unwrap();
    assert_eq!(
        handle
            .write_bytes(Bytes::from_static(b"x"))
            .await
            .unwrap_err()
            .kind,
        DavBackendErrorKind::InsufficientStorage
    );
    handle.abort().await.unwrap();
    assert_eq!(
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "version.txt")
            .await
            .unwrap()
            .unwrap()
            .size,
        8
    );
}

#[tokio::test]
async fn unknown_length_crosses_routing_gap_and_uses_final_size_policy() {
    let (state, user, policy, driver) = build_s3_direct_test_state().await;
    let mut large = policy.clone();
    large.name = "Large files".into();
    large.is_default = false;
    large.max_file_size = 32;
    let large = crate::storage::connectors::test_support::insertable_policy(large)
        .insert(state.writer_db())
        .await
        .unwrap();
    state
        .driver_registry
        .insert_for_test(large.id, Arc::new(driver.clone()));
    let group_id = state
        .policy_snapshot
        .require_user_policy_group_id(user.id)
        .unwrap();
    let small_rule = rule::Entity::find()
        .filter(rule::Column::GroupId.eq(group_id))
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let small_target = target::Entity::find()
        .filter(target::Column::RuleId.eq(small_rule.id))
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let mut active = small_rule.clone().into_active_model();
    active.matcher = Set(
        serde_json::to_string(&PlacementPayloadEnvelope::new(PlacementMatcher {
            max_file_size: 8,
            ..Default::default()
        }))
        .unwrap(),
    );
    active.update(state.writer_db()).await.unwrap();
    let mut active = small_rule.into_active_model();
    active.id = NotSet;
    active.name = Set("Large files".into());
    active.priority = Set(100);
    active.matcher = Set(
        serde_json::to_string(&PlacementPayloadEnvelope::new(PlacementMatcher {
            min_file_size: 16,
            ..Default::default()
        }))
        .unwrap(),
    );
    let large_rule = active.insert(state.writer_db()).await.unwrap();
    let mut active = small_target.into_active_model();
    active.id = NotSet;
    active.rule_id = Set(large_rule.id);
    active.policy_id = Set(large.id);
    active.insert(state.writer_db()).await.unwrap();
    set_policy_limit(&state, policy, 7).await;
    let mut handle =
        AsterDavWriteHandle::for_write_with_audit(state.clone(), context(user.id, "routed.txt"))
            .await
            .unwrap();
    for chunk in [b"1234".as_slice(), b"5678", b"12345678"] {
        handle
            .write_bytes(Bytes::copy_from_slice(chunk))
            .await
            .unwrap();
    }
    handle.finish().await.unwrap();
    let stored = file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "routed.txt")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.size, 16);
    assert_eq!(
        file_repo::find_blob_by_id(state.writer_db(), stored.blob_id)
            .await
            .unwrap()
            .policy_id,
        large.id
    );
}

#[tokio::test]
async fn inherited_folder_policy_bounds_unknown_length_upload() {
    let (state, user, policy, _) = build_s3_direct_test_state().await;
    let mut limited = policy;
    limited.name = "Folder policy".into();
    limited.is_default = false;
    limited.max_file_size = 8;
    let limited = crate::storage::connectors::test_support::insertable_policy(limited)
        .insert(state.writer_db())
        .await
        .unwrap();
    let mut parent_id = None;
    for name in ["parent", "child"] {
        let now = Utc::now();
        let folder = folder::ActiveModel {
            name: Set(name.into()),
            parent_id: Set(parent_id),
            team_id: Set(None),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            policy_id: Set(parent_id.is_none().then_some(limited.id)),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .unwrap();
        parent_id = Some(folder.id);
    }
    reload(&state).await;
    let mut request = context(user.id, "folder.txt");
    request.folder_id = parent_id;
    let mut handle = AsterDavWriteHandle::for_write_with_audit(state, request)
        .await
        .unwrap();
    assert_eq!(
        handle
            .write_bytes(Bytes::from_static(b"123456789"))
            .await
            .unwrap_err()
            .kind,
        DavBackendErrorKind::PayloadTooLarge
    );
    handle.abort().await.unwrap();
}

#[tokio::test]
async fn team_upload_uses_team_quota_instead_of_actor_quota() {
    let (state, user, _, _) = build_s3_direct_test_state().await;
    let group_id = state
        .policy_snapshot
        .require_user_policy_group_id(user.id)
        .unwrap();
    let now = Utc::now();
    let team = team::ActiveModel {
        name: Set("Quota team".into()),
        description: Set(String::new()),
        created_by: Set(user.id),
        storage_used: Set(2),
        storage_quota: Set(10),
        policy_group_id: Set(Some(group_id)),
        created_at: Set(now),
        updated_at: Set(now),
        archived_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    team_member::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(user.id),
        role: Set(aster_drive_model::types::TeamMemberRole::Owner),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let mut active = user.clone().into_active_model();
    active.storage_quota = Set(1);
    active.update(state.writer_db()).await.unwrap();
    reload(&state).await;
    let scope = WorkspaceStorageScope::Team {
        team_id: team.id,
        actor_user_id: user.id,
    };
    let mut request = context(user.id, "team.txt");
    request.scope = scope;
    let mut handle = AsterDavWriteHandle::for_write_with_audit(state.clone(), request)
        .await
        .unwrap();
    handle
        .write_bytes(Bytes::from_static(b"12345678"))
        .await
        .unwrap();
    handle.finish().await.unwrap();
    assert_eq!(
        crate::db::repository::team_repo::find_active_by_id(state.writer_db(), team.id)
            .await
            .unwrap()
            .storage_used,
        10
    );
    assert_eq!(
        crate::db::repository::user_repo::find_by_id(state.writer_db(), user.id)
            .await
            .unwrap()
            .storage_used,
        0
    );
    let mut request = context(user.id, "team-overflow.txt");
    request.scope = scope;
    let mut handle = AsterDavWriteHandle::for_write_with_audit(state, request)
        .await
        .unwrap();
    assert_eq!(
        handle
            .write_bytes(Bytes::from_static(b"x"))
            .await
            .unwrap_err()
            .kind,
        DavBackendErrorKind::InsufficientStorage
    );
    handle.abort().await.unwrap();
}
