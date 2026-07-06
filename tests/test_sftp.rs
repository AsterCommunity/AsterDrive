//! SFTP storage driver integration test using testcontainers.

use std::time::Duration;

use aster_drive::storage::drivers::sftp::SftpDriver;
use aster_drive::storage::{StorageDriver, StorageErrorKind, StreamUploadDriver};
use testcontainers::{GenericImage, ImageExt, core::IntoContainerPort, runners::AsyncRunner};
use tokio::io::AsyncReadExt as _;

const SFTP_IMAGE: &str = "atmoz/sftp";
const SFTP_TAG: &str = "alpine";
const SFTP_PORT: u16 = 22;
const SFTP_USERNAME: &str = "aster";
const SFTP_PASSWORD: &str = "asterpass";

fn sftp_policy(
    endpoint: &str,
    base_path: &str,
    host_key_fingerprint: Option<&str>,
) -> aster_drive::entities::storage_policy::Model {
    use chrono::Utc;

    let options = host_key_fingerprint
        .map(|fingerprint| {
            aster_drive::types::serialize_storage_policy_options(
                &aster_drive::types::StoragePolicyOptions {
                    sftp_host_key_fingerprint: Some(fingerprint.to_string()),
                    ..Default::default()
                },
            )
            .expect("serialize SFTP host key options")
        })
        .unwrap_or_else(aster_drive::types::StoredStoragePolicyOptions::empty);

    aster_drive::entities::storage_policy::Model {
        id: 997,
        name: "Test SFTP".to_string(),
        driver_type: aster_drive::types::DriverType::Sftp,
        endpoint: endpoint.to_string(),
        bucket: String::new(),
        access_key: SFTP_USERNAME.to_string(),
        secret_key: SFTP_PASSWORD.to_string(),
        base_path: base_path.to_string(),
        remote_node_id: None,
        remote_storage_target_key: None,
        max_file_size: 0,
        allowed_types: aster_drive::types::StoredStoragePolicyAllowedTypes::empty(),
        options,
        is_default: false,
        chunk_size: 1024 * 1024,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn extract_sftp_host_key_fingerprint(error: &aster_drive::errors::AsterError) -> String {
    let message = error.message();
    let marker = "Confirm fingerprint ";
    let (_, rest) = message
        .split_once(marker)
        .expect("untrusted host key error should include fingerprint");
    rest.split_whitespace()
        .next()
        .expect("fingerprint should be present")
        .to_string()
}

async fn wait_for_sftp_host_key_fingerprint(driver: &SftpDriver) -> String {
    let mut last_error = None;
    let fingerprint = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), driver.exists("readiness/probe.txt"))
                .await
            {
                Ok(Ok(_)) => last_error = Some("untrusted host key was accepted".to_string()),
                Ok(Err(error))
                    if error.storage_error_kind() == Some(StorageErrorKind::Precondition) =>
                {
                    break extract_sftp_host_key_fingerprint(&error);
                }
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => last_error = Some("host key probe attempt timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    fingerprint.unwrap_or_else(|_| {
        panic!(
            "timed out waiting for SFTP host key fingerprint: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )
    })
}

async fn wait_for_sftp(driver: &SftpDriver) {
    let mut last_error = None;
    let ready = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                Duration::from_secs(5),
                driver.put("readiness/probe.txt", b"ready"),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let _ = driver.delete("readiness/probe.txt").await;
                    break;
                }
                Ok(Err(error)) => last_error = Some(error.to_string()),
                Err(_) => last_error = Some("readiness upload attempt timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for SFTP test server: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

#[tokio::test]
async fn test_sftp_driver_upload_download_round_trip() {
    let container = GenericImage::new(SFTP_IMAGE, SFTP_TAG)
        .with_exposed_port(IntoContainerPort::tcp(SFTP_PORT))
        .with_cmd(vec![format!("{SFTP_USERNAME}:{SFTP_PASSWORD}:::upload")])
        .start()
        .await
        .expect("failed to start sftp container");

    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(SFTP_PORT))
        .await
        .expect("resolve mapped sftp port");
    let endpoint = format!("sftp://127.0.0.1:{port}");
    let base_path = format!("/upload/asterdrive-itest-{}", uuid::Uuid::new_v4());
    let untrusted_driver =
        SftpDriver::new(&sftp_policy(&endpoint, &base_path, None)).expect("create SftpDriver");
    let host_key_fingerprint = wait_for_sftp_host_key_fingerprint(&untrusted_driver).await;
    SftpDriver::validate_host_key_fingerprint(&host_key_fingerprint)
        .expect("reported host key fingerprint should be valid");

    let driver = SftpDriver::new(&sftp_policy(
        &endpoint,
        &base_path,
        Some(&host_key_fingerprint),
    ))
    .expect("create SftpDriver");
    wait_for_sftp(&driver).await;

    let data = b"hello sftp world";
    driver.put("docs/hello.txt", data).await.unwrap();
    assert!(driver.exists("docs/hello.txt").await.unwrap());
    assert!(!driver.exists("docs/missing.txt").await.unwrap());
    assert_eq!(driver.get("docs/hello.txt").await.unwrap(), data);

    let missing_meta = driver
        .metadata("docs/missing.txt")
        .await
        .expect_err("missing sftp object metadata should fail");
    assert_eq!(
        missing_meta.storage_error_kind(),
        Some(StorageErrorKind::NotFound)
    );

    let meta = driver.metadata("docs/hello.txt").await.unwrap();
    assert_eq!(meta.size, u64::try_from(data.len()).unwrap());

    let unicode_path = "docs/space dir/中文+plus.txt";
    driver.put(unicode_path, b"encoded path").await.unwrap();
    assert_eq!(driver.get(unicode_path).await.unwrap(), b"encoded path");

    let mut full_stream = driver.get_stream("docs/hello.txt").await.unwrap();
    let mut full_body = Vec::new();
    full_stream.read_to_end(&mut full_body).await.unwrap();
    assert_eq!(full_body, data);

    let mut empty_range = driver
        .get_range("docs/hello.txt", 0, Some(0))
        .await
        .unwrap();
    let mut empty_range_body = Vec::new();
    empty_range
        .read_to_end(&mut empty_range_body)
        .await
        .unwrap();
    assert!(empty_range_body.is_empty());

    let mut range = driver
        .get_range("docs/hello.txt", 6, Some(4))
        .await
        .unwrap();
    let mut range_body = Vec::new();
    range.read_to_end(&mut range_body).await.unwrap();
    assert_eq!(range_body, b"sftp");

    let mut tail = driver.get_range("docs/hello.txt", 11, None).await.unwrap();
    let mut tail_body = Vec::new();
    tail.read_to_end(&mut tail_body).await.unwrap();
    assert_eq!(tail_body, b"world");

    driver
        .copy_object("docs/hello.txt", "docs/copied.txt")
        .await
        .unwrap();
    assert_eq!(driver.get("docs/copied.txt").await.unwrap(), data);

    driver
        .put_reader(
            "stream/reader.bin",
            Box::new(std::io::Cursor::new(b"stream upload".to_vec())),
            13,
        )
        .await
        .unwrap();
    assert_eq!(
        driver.get("stream/reader.bin").await.unwrap(),
        b"stream upload"
    );

    let temp_dir = std::env::temp_dir().join(format!("asterdrive-sftp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let local_upload = temp_dir.join("upload.bin");
    std::fs::write(&local_upload, b"file upload").expect("write local upload");
    driver
        .put_file(
            "stream/from-file.bin",
            local_upload
                .to_str()
                .expect("temp upload path should be valid utf-8"),
        )
        .await
        .unwrap();
    assert_eq!(
        driver.get("stream/from-file.bin").await.unwrap(),
        b"file upload"
    );
    let _ = std::fs::remove_dir_all(&temp_dir);

    driver.delete("docs/hello.txt").await.unwrap();
    assert!(!driver.exists("docs/hello.txt").await.unwrap());
}
