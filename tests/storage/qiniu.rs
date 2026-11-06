//! Real native Kodo acceptance. No S3 mock can establish provider acceptance.
use aster_drive::storage::drivers::qiniu::{
    QiniuDownloadDelivery, QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials,
};
use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadStorageDriver, MultipartStorageDriver, StorageDriver,
    StreamUploadDriver,
};
use std::time::Duration;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn native_kodo_cloud_acceptance_and_read_only_existing_object_takeover() {
    let fields = [
        "ASTER_TEST_QINIU_KODO_BUCKET",
        "ASTER_TEST_QINIU_KODO_ORIGIN",
        "ASTER_TEST_QINIU_KODO_ACCESS_KEY",
        "ASTER_TEST_QINIU_KODO_SECRET_KEY",
    ];
    if fields.iter().any(|name| std::env::var(name).is_err()) {
        eprintln!(
            "Kodo native cloud acceptance SKIPPED: configure native bucket, HTTPS origin and credentials; no provider acceptance was performed"
        );
        return;
    }
    let read = |name: &str| std::env::var(name).expect("required native Kodo test environment");
    let driver = QiniuDriver::new(
        QiniuDriverConfig {
            bucket: read(fields[0]),
            base_path: std::env::var("ASTER_TEST_QINIU_KODO_BASE_PATH").unwrap_or_default(),
            download_origin: read(fields[1]),
            download_delivery: QiniuDownloadDelivery::Origin,
            download_base_url: None,
            download_auth_validity_seconds: 180,
            connect_timeout: Duration::from_secs(10),
            read_timeout: Duration::from_secs(60),
            operation_timeout: Duration::from_secs(120),
        },
        QiniuStaticCredentials {
            access_key: read(fields[2]),
            secret_key: read(fields[3]),
        },
    )
    .expect("native configuration");
    // This is a read-only probe: it never renames, copies, deletes or reuploads the supplied object.
    if let Ok(key) = std::env::var("ASTER_TEST_QINIU_KODO_EXISTING_KEY") {
        let metadata = driver.metadata(&key).await.expect("existing object stat");
        assert!(
            metadata.size > 0,
            "choose a nonempty existing object for takeover acceptance"
        );
        let mut range = driver
            .get_range(&key, 0, Some(metadata.size.min(64)))
            .await
            .expect("existing object Range");
        let mut bytes = Vec::new();
        range
            .read_to_end(&mut bytes)
            .await
            .expect("existing object stream");
        assert_eq!(u64::try_from(bytes.len()).unwrap(), metadata.size.min(64));
    }
    let prefix = format!("_aster-native-acceptance-{}", uuid::Uuid::new_v4());
    let object = format!("{prefix}/中文 %2F.txt");
    driver
        .put_reader(
            &object,
            Box::new(std::io::Cursor::new(b"hello native Kodo".to_vec())),
            17,
        )
        .await
        .expect("streaming form upload");
    assert_eq!(
        driver.get(&object).await.expect("private origin GET"),
        b"hello native Kodo"
    );
    let request = driver
        .resolve_download_url(
            &object,
            Duration::from_secs(60),
            DirectDownloadOptions::default(),
        )
        .await
        .unwrap()
        .unwrap();
    let client = aster_forge_http::HttpClient::builder()
        .build()
        .expect("test HTTP client");
    let attachment_name = "中文 +%.txt";
    let attachment = driver
        .resolve_download_url(
            &object,
            Duration::from_secs(60),
            DirectDownloadOptions {
                download_name: Some(attachment_name.into()),
                response_content_disposition: Some("attachment".into()),
                response_content_type: Some("application/x-requested-override".into()),
                response_cache_control: Some("private, max-age=0, must-revalidate".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    for range in [false, true] {
        let mut request = client.get(&attachment.url);
        if range {
            request = request.header("range", "bytes=0-4");
        }
        let response = request
            .send()
            .await
            .unwrap_or_else(|_| panic!("native attachment transport failed; signed URL redacted"));
        assert_eq!(
            response.status(),
            if range {
                reqwest::StatusCode::PARTIAL_CONTENT
            } else {
                reqwest::StatusCode::OK
            }
        );
        let disposition =
            std::str::from_utf8(response.headers()["content-disposition"].as_bytes()).unwrap();
        assert!(disposition.starts_with("attachment;"));
        let encoded = disposition
            .split(';')
            .find_map(|item| item.trim().strip_prefix("filename*="))
            .unwrap()
            .trim_matches('"');
        let (charset, encoded) = encoded.split_once("''").unwrap();
        assert!(charset.eq_ignore_ascii_case("utf-8"));
        assert_eq!(
            percent_encoding::percent_decode_str(encoded)
                .decode_utf8()
                .unwrap(),
            attachment_name
        );
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            if range {
                b"hello".as_slice()
            } else {
                b"hello native Kodo".as_slice()
            }
        );
    }
    assert!(
        client
            .head(&request.url)
            .send()
            .await
            .unwrap_or_else(|_| panic!("native private HEAD transport failed; signed URL redacted"))
            .status()
            .is_success()
    );
    assert!(
        client
            .get(&request.url)
            .header("range", "bytes=0-4")
            .send()
            .await
            .unwrap_or_else(|_| panic!(
                "native private Range transport failed; signed URL redacted"
            ))
            .status()
            == reqwest::StatusCode::PARTIAL_CONTENT
    );
    let copy = format!("{prefix}/copy");
    driver
        .copy_object(&object, &copy)
        .await
        .expect("native management copy");
    assert!(
        driver
            .extensions()
            .list
            .unwrap()
            .list_paths(Some(&prefix))
            .await
            .expect("native list")
            .contains(&object)
    );
    let multipart_key = format!("{prefix}/multipart");
    let id = driver
        .create_multipart_upload(&multipart_key)
        .await
        .expect("native initiate");
    let bytes = vec![b'x'; 1024 * 1024];
    let etag = driver
        .upload_multipart_part(&multipart_key, &id, 1, &bytes)
        .await
        .expect("native part");
    assert_eq!(
        driver
            .list_uploaded_part_details(&multipart_key, &id)
            .await
            .unwrap()[0]
            .size,
        1024 * 1024
    );
    driver
        .complete_multipart_upload(&multipart_key, &id, vec![(1, etag)])
        .await
        .expect("native complete");
    let abort_key = format!("{prefix}/abort");
    let id = driver.create_multipart_upload(&abort_key).await.unwrap();
    driver
        .upload_multipart_part(&abort_key, &id, 1, b"abort")
        .await
        .unwrap();
    driver
        .abort_multipart_upload(&abort_key, &id)
        .await
        .expect("native abort");
    for key in [object, copy, multipart_key, abort_key] {
        driver
            .delete(&key)
            .await
            .expect("delete only test-owned objects");
    }
}

#[tokio::test]
async fn native_kodo_schema_only_upgrade_preserves_unverified_configuration() {
    use aster_drive::db::repository::policy_repo;
    use aster_drive_model::types::StoredStoragePolicyConfig;
    use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};
    let state = crate::common::setup().await;
    let db = state.writer_db();
    let policy = policy_repo::find_default(db).await.unwrap().unwrap();
    let id = policy.id;
    let mut active: aster_drive_model::entities::storage_policy::ActiveModel = policy.into();
    active.connector_id = Set("asterdrive.storage.qiniu".into());
    active.storage_config=Set(StoredStoragePolicyConfig(serde_json::json!({
        "format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.qiniu","schema_version":1,"values":{
            "endpoint":"https://s3.cn-east-1.qiniucs.com","bucket":"legacy-s3-alias","s3_region":"cn-east-1","base_path":"retained-prefix",
            "object_storage_upload_strategy":"presigned","object_storage_download_strategy":"relay_stream"
        }},"behavior":{"format_version":1,"schema_version":1,"values":{}}
    }).to_string()));
    active.update(db).await.unwrap();
    db.execute_unprepared(
        "DELETE FROM seaql_migrations WHERE version='m20261104_000001_qiniu_native'",
    )
    .await
    .unwrap();
    let original = policy_repo::find_by_id(db, id)
        .await
        .unwrap()
        .storage_config;
    assert!(
        aster_drive_migration::apply_database_migrations(db)
            .await
            .is_err()
    );
    assert_eq!(
        policy_repo::find_by_id(db, id)
            .await
            .unwrap()
            .storage_config,
        original
    );
}
