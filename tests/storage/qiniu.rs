//! Qiniu Kodo wrapper integration tests using the shared S3-compatible contract.

use aster_drive::storage::drivers::qiniu::{
    QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials,
};
use aster_drive_storage::{MultipartStorageDriver, StorageDriver};
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};
use tokio::io::AsyncReadExt;

const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";

fn qiniu_driver(endpoint: &str, bucket: &str) -> QiniuDriver {
    QiniuDriver::new(
        QiniuDriverConfig {
            endpoint: endpoint.to_string(),
            bucket: bucket.to_string(),
            base_path: "qiniu-contract".to_string(),
            region: "cn-east-1".to_string(),
            path_style: true,
            connect_timeout: std::time::Duration::from_secs(5),
            read_timeout: std::time::Duration::from_secs(30),
            operation_timeout: std::time::Duration::from_secs(3_600),
        },
        QiniuStaticCredentials {
            access_key: "rustfsadmin".to_string(),
            secret_key: "rustfsadmin123".to_string(),
        },
    )
    .expect("Qiniu S3-compatible driver should build")
}

async fn wait_for_bucket(endpoint: &str, bucket: &str) {
    let credentials = aws_credential_types::Credentials::new(
        "rustfsadmin",
        "rustfsadmin123",
        None,
        None,
        "qiniu-wrapper-test",
    );
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("cn-east-1"))
        .credentials_provider(credentials)
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();
    let client = aws_sdk_s3::Client::from_conf(config);

    tokio::time::timeout(std::time::Duration::from_secs(45), async {
        loop {
            if client.create_bucket().bucket(bucket).send().await.is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await
    .expect("S3-compatible test bucket should become ready");
}

#[tokio::test]
async fn qiniu_wrapper_performs_standard_s3_object_range_list_and_multipart_operations() {
    let container = GenericImage::new("rustfs/rustfs", RUSTFS_TEST_IMAGE_TAG)
        .with_exposed_port(testcontainers::core::IntoContainerPort::tcp(9000))
        .with_env_var("RUSTFS_ACCESS_KEY", "rustfsadmin")
        .with_env_var("RUSTFS_SECRET_KEY", "rustfsadmin123")
        .start()
        .await
        .expect("S3-compatible test server should start");
    let endpoint = format!(
        "http://127.0.0.1:{}",
        container
            .get_host_port_ipv4(9000)
            .await
            .expect("mapped port")
    );
    let bucket = "qiniu-contract-test";
    wait_for_bucket(&endpoint, bucket).await;

    let driver = qiniu_driver(&endpoint, bucket);
    driver
        .put("folder/object.txt", b"hello Kodo-compatible storage")
        .await
        .expect("PUT through Qiniu wrapper");
    assert!(
        driver
            .exists("folder/object.txt")
            .await
            .expect("HEAD object")
    );
    assert_eq!(
        driver.get("folder/object.txt").await.expect("GET object"),
        b"hello Kodo-compatible storage"
    );
    assert_eq!(
        driver
            .metadata("folder/object.txt")
            .await
            .expect("HEAD metadata")
            .size,
        29
    );

    let mut range = driver
        .get_range("folder/object.txt", 6, Some(4))
        .await
        .expect("native Range read");
    let mut range_body = Vec::new();
    range
        .read_to_end(&mut range_body)
        .await
        .expect("read range");
    assert_eq!(range_body, b"Kodo");

    let list = driver.extensions().list.expect("S3 list extension");
    assert_eq!(
        list.list_paths(Some("folder")).await.expect("list paths"),
        vec!["folder/object.txt"]
    );

    let upload_id = driver
        .create_multipart_upload("folder/multipart.bin")
        .await
        .expect("create multipart upload");
    let etag = driver
        .upload_multipart_part("folder/multipart.bin", &upload_id, 1, b"part-data")
        .await
        .expect("upload multipart part");
    driver
        .complete_multipart_upload("folder/multipart.bin", &upload_id, vec![(1, etag)])
        .await
        .expect("complete multipart upload");
    assert_eq!(
        driver
            .get("folder/multipart.bin")
            .await
            .expect("read completed multipart object"),
        b"part-data"
    );

    driver
        .delete("folder/object.txt")
        .await
        .expect("delete through Qiniu wrapper");
    assert!(
        !driver
            .exists("folder/object.txt")
            .await
            .expect("HEAD deleted object")
    );
}
