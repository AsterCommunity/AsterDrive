//! CLI doctor integration contracts.
use super::*;

#[tokio::test]
async fn test_root_binary_doctor_defaults_to_json_output() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["ok"], true);
    let redacted_database_url = report["data"]["database_url"]
        .as_str()
        .expect("doctor database url should be a string");
    assert!(redacted_database_url.starts_with("sqlite:///.../asterdrive-cli-ready-test-"));
    assert!(redacted_database_url.ends_with(".db?mode=rwc"));
    assert_eq!(report["data"]["status"], "warn");
    assert_eq!(report["data"]["summary"]["fail"], 0);
    assert!(
        report["data"]["summary"]["warn"]
            .as_u64()
            .expect("warn count should exist")
            >= 1
    );
}

#[tokio::test]
async fn test_root_binary_doctor_redacts_database_url_query_credentials() {
    let database_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-redaction-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!(
        "sqlite://{}?mode=rwc&password=secret%2Fvalue&DB_TOKEN=doctor-query-token-credential",
        database_path.display()
    );

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["data"]["status"], "fail");
    assert!(stdout.contains("password=***"));
    assert!(stdout.contains("DB_TOKEN=***"));
    assert!(!stdout.contains("secret%2Fvalue"));
    assert!(!stdout.contains("doctor-query-token-credential"));
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_redacts_database_url_query_credentials() {
    let database_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-redaction-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!(
        "sqlite://{}?mode=rwc&password=human-secret%2Fvalue&ACCESS_TOKEN=human-token",
        database_path.display()
    );

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
    ]);
    let stdout = String::from_utf8(output.stdout).expect("doctor human stdout should be utf-8");
    assert!(stdout.contains("Database:"));
    assert!(stdout.contains("password=***"));
    assert!(stdout.contains("ACCESS_TOKEN=***"));
    assert!(!stdout.contains("human-secret%2Fvalue"));
    assert!(!stdout.contains("human-token"));
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_is_readable() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
    ]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor human stdout should be utf-8");
    assert!(stdout.contains("System doctor"));
    assert!(stdout.contains("Database:"));
    assert!(stdout.contains("Mode:"));
    assert!(stdout.contains("Status:"));
    assert!(stdout.contains("Checks:"));
    assert!(stdout.contains("Database connection"));
    assert!(stdout.contains("Mail configuration"));
    assert!(stdout.contains("hint:"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_supports_forced_color() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive_with_env(
        &[
            "doctor",
            "--database-url",
            &database_url,
            "--output-format",
            "human",
        ],
        &[("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout =
        String::from_utf8(output.stdout).expect("doctor forced-color stdout should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stdout.contains("System doctor"));
}

#[tokio::test]
async fn test_root_binary_doctor_strict_turns_warnings_into_nonzero_exit() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url, "--strict"]);
    assert!(
        !output.status.success(),
        "doctor --strict should fail when warnings are present"
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should stay json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["strict"], true);
    assert_eq!(report["data"]["status"], "fail");
    assert_eq!(report["data"]["summary"]["fail"], 0);
    assert!(
        report["data"]["summary"]["warn"]
            .as_u64()
            .expect("warn count should exist")
            >= 1
    );
}

#[tokio::test]
async fn test_root_binary_doctor_warns_when_public_site_url_uses_http() {
    let database_url = setup_ready_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"["http://drive.example.com"]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let public_site_url_check = checks
        .iter()
        .find(|check| check["name"] == "public_site_url")
        .expect("public_site_url check should exist");

    assert_eq!(public_site_url_check["status"], "warn");
    assert_eq!(
        public_site_url_check["summary"],
        "public_site_url uses insecure HTTP"
    );
    assert!(
        public_site_url_check["details"]
            .as_array()
            .is_some_and(|details| details.iter().any(
                |detail| detail.as_str() == Some(r#"configured=["http://drive.example.com"]"#)
            ))
    );
}

#[tokio::test]
async fn test_root_binary_doctor_reports_sqlite_search_acceleration_ready() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let sqlite_search_check = checks
        .iter()
        .find(|check| check["name"] == "sqlite_search_acceleration")
        .expect("sqlite_search_acceleration check should exist");

    assert_eq!(sqlite_search_check["status"], "ok");
    assert_eq!(
        sqlite_search_check["summary"],
        "FTS5 trigram search acceleration ready"
    );
    assert!(
        sqlite_search_check["details"]
            .as_array()
            .is_some_and(|details| details.iter().any(|detail| detail
                .as_str()
                .is_some_and(|detail| detail.starts_with("sqlite_version="))))
    );
}

#[tokio::test]
async fn test_root_binary_doctor_deep_fix_repairs_counters() {
    let database_url = setup_database_url().await;
    let state = common::setup_with_database_url(&database_url).await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("test user should exist");
    let user_id = user.id;
    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();

    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.storage_used = Set(0);
    user_active.update(&db).await.unwrap();

    let mut blob_active: aster_drive_model::entities::file_blob::ActiveModel = blob.into();
    blob_active.ref_count = Set(0);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(&db).await.unwrap();

    let preview = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--fix",
        "--dry-run",
    ]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["data"]["dry_run"], true);
    let unchanged_user = user_repo::find_by_id(&db, user_id).await.unwrap();
    let unchanged_blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    assert_eq!(unchanged_user.storage_used, 0);
    assert_eq!(unchanged_blob.ref_count, 0);

    let output = run_aster_drive(&["doctor", "--database-url", &database_url, "--deep", "--fix"]);
    assert!(
        output.status.success(),
        "doctor --deep --fix stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["deep"], true);
    assert_eq!(report["data"]["fix"], true);

    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let usage_check = checks
        .iter()
        .find(|check| check["name"] == "storage_usage_consistency")
        .expect("storage usage check should exist");
    assert_eq!(usage_check["status"], "ok");
    assert!(
        usage_check["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("fixed 1"))
    );

    let ref_count_check = checks
        .iter()
        .find(|check| check["name"] == "blob_ref_counts")
        .expect("blob ref_count check should exist");
    assert_eq!(ref_count_check["status"], "ok");
    assert!(
        ref_count_check["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("fixed 1"))
    );
}

#[tokio::test]
async fn test_root_binary_doctor_scope_and_policy_filter_limit_deep_checks() {
    let database_url = setup_database_url().await;
    let state = common::setup_with_database_url(&database_url).await;
    let db = state.writer_db().clone();
    let policy = policy_repo::find_default(&db)
        .await
        .unwrap()
        .expect("default policy should exist");
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("test user should exist");
    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();

    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.storage_used = Set(0);
    user_active.update(&db).await.unwrap();

    let mut blob_active: aster_drive_model::entities::file_blob::ActiveModel = blob.into();
    blob_active.ref_count = Set(0);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(&db).await.unwrap();

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--scope",
        "blob-ref-counts",
        "--policy-id",
        &policy.id.to_string(),
    ]);
    assert!(
        output.status.success(),
        "doctor scoped stdout: {}\ndoctor scoped stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["data"]["deep"], true);
    assert_eq!(report["data"]["policy_id"], policy.id);
    assert!(
        report["data"]["scopes"]
            .as_array()
            .is_some_and(|scopes| scopes.len() == 1 && scopes[0] == "blob_ref_counts")
    );

    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "blob_ref_counts")
    );
    assert!(checks.iter().any(|check| check["name"] == "policy_filter"));
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "storage_usage_consistency")
    );
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "tracked_blob_objects")
    );
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "folder_tree_integrity")
    );
}

#[tokio::test]
async fn test_root_binary_doctor_exits_nonzero_when_storage_policy_setup_is_missing() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        !output.status.success(),
        "doctor should fail on incomplete setup"
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should stay json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["status"], "fail");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    assert!(
        checks
            .iter()
            .any(|check| { check["name"] == "storage_policies" && check["status"] == "fail" })
    );
}
