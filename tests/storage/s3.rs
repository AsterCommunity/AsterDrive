//! S3 存储驱动集成测试（使用 testcontainers + rustfs）

use aster_drive::storage::drivers::s3::{S3Driver, S3DriverOptions};
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, StoredStoragePolicyConfig,
};
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, PresignedDownloadOptions, PresignedStorageDriver,
    StorageDriver, StoragePolicyBehaviorConfig, StreamUploadDriver, encode_storage_policy_config,
};
use serde::Serialize;
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};

const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";

#[derive(Serialize)]
struct S3ConnectorConfigV1 {
    endpoint: String,
    bucket: String,
    base_path: String,
    object_storage_upload_strategy: ObjectStorageUploadStrategy,
    object_storage_download_strategy: ObjectStorageDownloadStrategy,
    s3_path_style: bool,
    s3_region: String,
    s3_connect_timeout_secs: u64,
    s3_read_timeout_secs: u64,
    s3_operation_timeout_secs: u64,
}

/// 创建 S3 测试用的 storage_policy model
fn s3_policy(endpoint: &str, bucket: &str) -> aster_drive_model::entities::storage_policy::Model {
    use chrono::Utc;
    aster_drive_model::entities::storage_policy::Model {
        id: 999,
        name: "Test S3".to_string(),
        driver_type: aster_drive_model::types::DriverType::S3,
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        access_key: "rustfsadmin".to_string(),
        secret_key: "rustfsadmin123".to_string(),
        base_path: "test-prefix".to_string(),
        remote_node_id: None,
        remote_storage_target_key: None,
        connector_id: "asterdrive.storage.s3".to_string(),
        storage_config: StoredStoragePolicyConfig(
            encode_storage_policy_config(
                ConnectorConfigEnvelope::new(
                    ConnectorId::declared("asterdrive.storage.s3"),
                    1,
                    serde_json::to_value(S3ConnectorConfigV1 {
                        endpoint: endpoint.to_string(),
                        bucket: bucket.to_string(),
                        base_path: "test-prefix".to_string(),
                        object_storage_upload_strategy: ObjectStorageUploadStrategy::RelayStream,
                        object_storage_download_strategy:
                            ObjectStorageDownloadStrategy::RelayStream,
                        s3_path_style: true,
                        s3_region: "us-east-1".to_string(),
                        s3_connect_timeout_secs: 5,
                        s3_read_timeout_secs: 30,
                        s3_operation_timeout_secs: 3_600,
                    })
                    .expect("encode typed S3 connector fixture"),
                ),
                StoragePolicyBehaviorConfig::default(),
            )
            .expect("encode typed S3 policy fixture"),
        ),
        max_file_size: 0,
        allowed_types: aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty(),
        options: aster_drive_model::types::StoredStoragePolicyOptions::empty(),
        is_default: false,
        chunk_size: 5_242_880,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn s3_test_client(endpoint: &str) -> aws_sdk_s3::Client {
    let credentials =
        aws_credential_types::Credentials::new("rustfsadmin", "rustfsadmin123", None, None, "test");
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

async fn try_create_bucket(endpoint: &str, bucket: &str) -> std::result::Result<(), String> {
    use aws_sdk_s3::error::ProvideErrorMetadata;

    let client = s3_test_client(endpoint);
    if let Err(err) = client.create_bucket().bucket(bucket).send().await {
        let code = err
            .as_service_error()
            .and_then(|service_err| service_err.code());
        if matches!(
            code,
            Some("BucketAlreadyOwnedByYou") | Some("BucketAlreadyExists")
        ) {
            return Ok(());
        }
        return Err(err.to_string());
    }
    Ok(())
}

async fn wait_for_s3_bucket(endpoint: &str, bucket: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                try_create_bucket(endpoint, bucket),
            )
            .await
            {
                Ok(Ok(())) => break,
                Ok(Err(err)) => last_err = Some(err),
                Err(_) => {
                    last_err = Some("create_bucket attempt timed out".to_string());
                }
            }
            // 这里只是 readiness probe 的退避间隔；真正的同步条件是上面的 create_bucket 成功。
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for S3 bucket {bucket} at {endpoint}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

#[tokio::test]
async fn test_s3_put_get_delete() {
    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("failed to start rustfs container");

    let port = container.get_host_port_ipv4(9000).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let bucket = "test-bucket";

    wait_for_s3_bucket(&endpoint, bucket).await;

    let policy = s3_policy(&endpoint, bucket);
    let driver = S3Driver::new(&policy, S3DriverOptions::default(), std::convert::identity)
        .expect("failed to create S3Driver");

    // PUT
    let data = b"hello s3 world";
    driver.put("test/hello.txt", data).await.unwrap();

    // EXISTS
    assert!(driver.exists("test/hello.txt").await.unwrap());
    assert!(!driver.exists("test/nonexistent.txt").await.unwrap());

    // GET
    let got = driver.get("test/hello.txt").await.unwrap();
    assert_eq!(got, data);

    // METADATA
    let meta = driver.metadata("test/hello.txt").await.unwrap();
    assert_eq!(meta.size, data.len() as u64);

    // PUT_FILE
    let temp_dir = format!("/tmp/asterdrive-s3-test-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&temp_dir).unwrap();
    let temp_path = format!("{}/upload.bin", temp_dir);
    std::fs::write(&temp_path, b"file upload content").unwrap();
    driver
        .put_file("test/uploaded.bin", &temp_path)
        .await
        .unwrap();
    let got = driver.get("test/uploaded.bin").await.unwrap();
    assert_eq!(got, b"file upload content");

    // GET_STREAM
    use tokio::io::AsyncReadExt;
    let mut stream = driver.get_stream("test/hello.txt").await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, data);

    // DELETE
    driver.delete("test/hello.txt").await.unwrap();
    assert!(!driver.exists("test/hello.txt").await.unwrap());

    // COPY
    driver.put("test/src.txt", b"copy me").await.unwrap();
    driver
        .copy_object("test/src.txt", "test/dst.txt")
        .await
        .unwrap();
    let got = driver.get("test/dst.txt").await.unwrap();
    assert_eq!(got, b"copy me");

    // PRESIGNED URL (just verify it generates without error)
    let url = driver
        .presigned_url(
            "test/dst.txt",
            std::time::Duration::from_secs(300),
            PresignedDownloadOptions::default(),
        )
        .await
        .unwrap();
    assert!(url.is_some());
    assert!(url.unwrap().contains("test-prefix"));

    // PRESIGNED PUT URL
    let url = driver
        .presigned_put_url("test/new.txt", std::time::Duration::from_secs(300))
        .await
        .unwrap();
    assert!(url.is_some());

    // cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
