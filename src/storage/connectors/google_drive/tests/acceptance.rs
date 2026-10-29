//! Explicitly opted-in provider acceptance. Local HTTP mocks cannot establish
//! Google permissions, indexing behavior, session cancellation or OAuth consent.
use super::*;
use aster_drive_storage::StreamUploadAttempt;
use futures::FutureExt;
use serde_json::json;
use tokio::io::AsyncReadExt;

fn required(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("set {name} to run Google Drive acceptance"))
}
fn optional(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[tokio::test]
#[ignore = "requires Google OAuth credentials and an explicitly writable isolated pool"]
async fn google_drive_real_provider_acceptance() {
    assert_eq!(required("ASTER_TEST_GOOGLE_DRIVE_ALLOW_WRITE"), "1");
    let appdata = optional("ASTER_TEST_GOOGLE_DRIVE_APP_DATA_FOLDER").as_deref() == Some("1");
    let root = optional("ASTER_TEST_GOOGLE_DRIVE_ROOT_FOLDER_ID");
    let shared = optional("ASTER_TEST_GOOGLE_DRIVE_SHARED_DRIVE_ID");
    assert!(
        appdata || root.is_some() || shared.is_some(),
        "select an isolated folder/shared drive or appDataFolder"
    );
    let mut credential = payload();
    credential.application.client_id = required("ASTER_TEST_GOOGLE_DRIVE_CLIENT_ID");
    credential.application.client_secret = required("ASTER_TEST_GOOGLE_DRIVE_CLIENT_SECRET")
        .as_str()
        .into();
    let auth = credential.authorization.as_mut().unwrap();
    auth.access_token.clear();
    auth.refresh_token = Some(required("ASTER_TEST_GOOGLE_DRIVE_REFRESH_TOKEN"));
    auth.expires_at = Some(Utc::now());
    auth.scopes = vec![
        if appdata {
            "https://www.googleapis.com/auth/drive.appdata"
        } else {
            "https://www.googleapis.com/auth/drive"
        }
        .into(),
    ];
    let context = TestContext::new(
        json!({"root_folder_id":root,"shared_drive_id":shared,"app_data_folder":appdata}),
        Some(credential),
    )
    .await;
    let row = repo::find_by_policy(&context.db, context.policy.id)
        .await
        .unwrap()
        .unwrap();
    let validated = GoogleDriveConnector
        .validate_credential(&context.db, &context.config, &context.policy, &row)
        .await
        .unwrap();
    let saved = crate::storage::connectors::persist_connector_credential_value(
        &context.db,
        KEY,
        &validated.credential,
        validated.credential_payload,
    )
    .await
    .unwrap();
    let runtime = GoogleDriveConnector
        .load_runtime_credential(&context.db, &context.config, &context.policy, &saved)
        .await
        .unwrap()
        .unwrap();
    let driver = GoogleDriveConnector
        .build_authorized_driver(&context.policy, runtime)
        .unwrap();
    let path = format!("acceptance/{}", uuid::Uuid::new_v4());
    let cancel_path = format!("acceptance/{}", uuid::Uuid::new_v4());
    let resumable = driver.extensions().provider_resumable.unwrap();
    let session = resumable.create_upload_session(&cancel_path).await.unwrap();
    let result = std::panic::AssertUnwindSafe(async {
        driver.put(&path, b"").await?;
        assert_eq!(driver.metadata(&path).await?.size, 0);
        driver.put(&path, b"abcdef").await?;
        assert_eq!(driver.get(&path).await?, b"abcdef");
        let mut range = driver.get_range(&path, 2, Some(3)).await?;
        let mut data = Vec::new();
        range.read_to_end(&mut data).await.unwrap();
        assert_eq!(data, b"cde");
        let stream = driver.extensions().stream_upload.unwrap();
        let size = 8 * 1024 * 1024 + 1;
        stream
            .put_reader(
                &path,
                Box::new(tokio::io::repeat(7).take(size)),
                i64::try_from(size).unwrap(),
            )
            .await?;
        assert_eq!(driver.metadata(&path).await?.size, size);
        let mut content = driver.get_stream(&path).await?;
        let mut read = 0_u64;
        let mut chunk = [0; 65536];
        loop {
            let count = content.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            assert!(chunk[..count].iter().all(|byte| *byte == 7));
            read += u64::try_from(count).unwrap();
        }
        assert_eq!(read, size);
        let attempt = StreamUploadAttempt::new(&path, 4)?;
        assert!(
            stream
                .stage_attempt(&attempt, Box::new(std::io::Cursor::new(b"bad".to_vec())))
                .await
                .is_err()
        );
        stream.abort_attempt(&attempt).await?;
        assert_eq!(driver.metadata(&path).await?.size, size);
        let fragment_size = 256 * 1024;
        let outcome = resumable
            .upload_session_fragment_reader(
                &session.upload_url,
                0,
                fragment_size + 1,
                Box::new(tokio::io::repeat(3).take(fragment_size)),
                i64::try_from(fragment_size).unwrap(),
            )
            .await?;
        assert!(!outcome.completed);
        assert_eq!(
            resumable
                .query_upload_session(&session.upload_url)
                .await?
                .next_expected_ranges,
            [format!("{fragment_size}-")]
        );
        Ok::<(), aster_drive_storage::StorageError>(())
    })
    .catch_unwind()
    .await;
    // Clean only this test's unique keys/session, even after a protocol failure.
    let abort = resumable.abort_upload_session(&session.upload_url).await;
    let remove_cancel = driver.delete(&cancel_path).await;
    let remove = driver.delete(&path).await;
    match result {
        Ok(result) => result.unwrap(),
        Err(panic) => std::panic::resume_unwind(panic),
    }
    abort.unwrap();
    remove_cancel.unwrap();
    remove.unwrap();
    assert!(!driver.exists(&path).await.unwrap());
    assert!(!driver.exists(&cancel_path).await.unwrap());
}
