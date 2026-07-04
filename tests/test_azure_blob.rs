//! Azure Blob 存储驱动集成测试（使用 testcontainers + Azurite）

use std::time::Duration;

use aster_drive::storage::drivers::azure_blob::AzureBlobDriver;
use aster_drive::storage::{
    ListStorageDriver, MultipartStorageDriver, PresignedDownloadOptions, PresignedStorageDriver,
    StorageDriver, StorageErrorKind, StreamUploadDriver,
};
use base64::Engine as _;
use chrono::Utc;
use reqwest::StatusCode;
use testcontainers::{GenericImage, ImageExt, core::IntoContainerPort, runners::AsyncRunner};
use tokio::io::AsyncReadExt as _;

const AZURITE_IMAGE: &str = "mcr.microsoft.com/azure-storage/azurite";
const AZURITE_TAG: &str = "3.35.0";
const AZURITE_BLOB_PORT: u16 = 10000;
const AZURITE_ACCOUNT: &str = "devstoreaccount1";
const AZURITE_ACCOUNT_KEY: &str =
    "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";
const AZURE_STORAGE_VERSION: &str = "2023-11-03";

fn azure_policy(
    endpoint: &str,
    container: &str,
    base_path: &str,
) -> aster_drive::entities::storage_policy::Model {
    use chrono::Utc;

    aster_drive::entities::storage_policy::Model {
        id: 998,
        name: "Test Azure Blob".to_string(),
        driver_type: aster_drive::types::DriverType::AzureBlob,
        endpoint: endpoint.to_string(),
        bucket: container.to_string(),
        access_key: AZURITE_ACCOUNT.to_string(),
        secret_key: AZURITE_ACCOUNT_KEY.to_string(),
        base_path: base_path.to_string(),
        remote_node_id: None,
        remote_storage_target_key: None,
        max_file_size: 0,
        allowed_types: aster_drive::types::StoredStoragePolicyAllowedTypes::empty(),
        options: aster_drive::types::StoredStoragePolicyOptions::empty(),
        is_default: false,
        chunk_size: 5_242_880,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn shared_key_signature(string_to_sign: &str) -> String {
    use azure_core::credentials::Secret;
    use azure_core::hmac::hmac_sha256;

    hmac_sha256(
        string_to_sign,
        &Secret::new(AZURITE_ACCOUNT_KEY.to_string()),
    )
    .expect("sign Azurite shared key request")
}

async fn create_container_if_needed(endpoint: &str, container: &str) -> Result<(), String> {
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let canonicalized_headers = format!("x-ms-date:{date}\nx-ms-version:{AZURE_STORAGE_VERSION}\n");
    let canonicalized_resource =
        format!("/{AZURITE_ACCOUNT}/{AZURITE_ACCOUNT}/{container}\nrestype:container");
    let string_to_sign =
        format!("PUT\n\n\n\n\n\n\n\n\n\n\n\n{canonicalized_headers}{canonicalized_resource}");
    let signature = shared_key_signature(&string_to_sign);
    let url = format!(
        "{}/{container}?restype=container",
        endpoint.trim_end_matches('/')
    );

    let response = reqwest::Client::new()
        .put(url)
        .header(
            "Authorization",
            format!("SharedKey {AZURITE_ACCOUNT}:{signature}"),
        )
        .header("x-ms-date", date)
        .header("x-ms-version", AZURE_STORAGE_VERSION)
        .send()
        .await
        .map_err(|error| error.to_string())?;

    match response.status() {
        StatusCode::CREATED | StatusCode::CONFLICT => Ok(()),
        status => Err(format!(
            "create Azurite container failed with {status}: {}",
            response.text().await.unwrap_or_default()
        )),
    }
}

async fn wait_for_azurite_container(endpoint: &str, container: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            match tokio::time::timeout(
                Duration::from_secs(3),
                create_container_if_needed(endpoint, container),
            )
            .await
            {
                Ok(Ok(())) => break,
                Ok(Err(error)) => last_err = Some(error),
                Err(_) => last_err = Some("create container attempt timed out".to_string()),
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for Azurite container {container} at {endpoint}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

fn unique_container_name() -> String {
    format!("az{:024}", &uuid::Uuid::new_v4().simple().to_string()[..24])
}

#[tokio::test]
async fn test_azure_blob_driver_e2e_with_azurite() {
    let container = GenericImage::new(AZURITE_IMAGE, AZURITE_TAG)
        .with_exposed_port(IntoContainerPort::tcp(AZURITE_BLOB_PORT))
        .with_cmd(vec![
            "azurite-blob",
            "--blobHost",
            "0.0.0.0",
            "--blobPort",
            "10000",
        ])
        .start()
        .await
        .expect("failed to start azurite container");

    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(AZURITE_BLOB_PORT))
        .await
        .expect("resolve mapped azurite port");
    let endpoint = format!("http://127.0.0.1:{port}/{AZURITE_ACCOUNT}");
    let container_name = unique_container_name();
    wait_for_azurite_container(&endpoint, &container_name).await;

    let driver = AzureBlobDriver::new(&azure_policy(&endpoint, &container_name, "itest/prefix"))
        .expect("create AzureBlobDriver");
    assert_eq!(
        driver.presigned_put_headers(),
        std::collections::BTreeMap::from([("x-ms-blob-type".to_string(), "BlockBlob".to_string())])
    );
    assert!(!driver.presigned_put_requires_etag());

    let data = b"hello azure blob";
    driver.put("docs/hello.txt", data).await.unwrap();
    assert!(driver.exists("docs/hello.txt").await.unwrap());
    assert!(!driver.exists("docs/missing.txt").await.unwrap());
    assert_eq!(driver.get("docs/hello.txt").await.unwrap(), data);
    let missing_meta = driver
        .metadata("docs/missing.txt")
        .await
        .expect_err("missing blob metadata should fail");
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
        .get_range("docs/hello.txt", 6, Some(5))
        .await
        .unwrap();
    let mut range_body = Vec::new();
    range.read_to_end(&mut range_body).await.unwrap();
    assert_eq!(range_body, b"azure");

    let mut tail = driver.get_range("docs/hello.txt", 12, None).await.unwrap();
    let mut tail_body = Vec::new();
    tail.read_to_end(&mut tail_body).await.unwrap();
    assert_eq!(tail_body, b"blob");

    driver
        .put("docs/copy-src.txt", b"copy payload")
        .await
        .unwrap();
    driver
        .copy_object("docs/copy-src.txt", "docs/copy-dst.txt")
        .await
        .unwrap();
    assert_eq!(
        driver.get("docs/copy-dst.txt").await.unwrap(),
        b"copy payload"
    );

    let listed = driver.list_paths(Some("docs")).await.unwrap();
    assert!(listed.contains(&"docs/hello.txt".to_string()));
    assert!(listed.contains(&unicode_path.to_string()));
    assert!(listed.contains(&"docs/copy-src.txt".to_string()));
    assert!(listed.contains(&"docs/copy-dst.txt".to_string()));

    let missing_header_put = driver
        .presigned_put_url("direct/missing-header.bin", Duration::from_secs(300))
        .await
        .unwrap()
        .expect("azure presigned put");
    let missing_header_resp = reqwest::Client::new()
        .put(&missing_header_put)
        .body("missing blob type".as_bytes().to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(missing_header_resp.status(), StatusCode::BAD_REQUEST);
    assert!(!driver.exists("direct/missing-header.bin").await.unwrap());

    let presigned_put = driver
        .presigned_put_url("direct/presigned.bin", Duration::from_secs(300))
        .await
        .unwrap()
        .expect("azure presigned put");
    let presigned_put_query: std::collections::HashMap<_, _> = url::Url::parse(&presigned_put)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(
        presigned_put_query.get("spr").map(String::as_str),
        Some("https,http")
    );
    let put_resp = reqwest::Client::new()
        .put(&presigned_put)
        .header("x-ms-blob-type", "BlockBlob")
        .body("presigned body".as_bytes().to_vec())
        .send()
        .await
        .unwrap();
    assert!(
        put_resp.status().is_success(),
        "azure presigned put failed: {}",
        put_resp.status()
    );
    assert_eq!(
        driver.get("direct/presigned.bin").await.unwrap(),
        b"presigned body"
    );

    let presigned_get = driver
        .presigned_url(
            "direct/presigned.bin",
            Duration::from_secs(300),
            PresignedDownloadOptions::default(),
        )
        .await
        .unwrap()
        .expect("azure presigned get");
    let get_resp = reqwest::get(&presigned_get).await.unwrap();
    assert!(get_resp.status().is_success());
    assert_eq!(get_resp.bytes().await.unwrap().as_ref(), b"presigned body");

    let upload_id = driver
        .create_multipart_upload("multipart/assembled.bin")
        .await
        .unwrap();
    assert!(
        driver
            .list_uploaded_part_details("multipart/assembled.bin", &upload_id)
            .await
            .unwrap()
            .is_empty()
    );
    let part1_url = driver
        .presigned_upload_part_url(
            "multipart/assembled.bin",
            &upload_id,
            1,
            Duration::from_secs(300),
        )
        .await
        .unwrap();
    let part1_resp = reqwest::Client::new()
        .put(&part1_url)
        .body(b"hello ".to_vec())
        .send()
        .await
        .unwrap();
    assert!(
        part1_resp.status().is_success(),
        "azure presigned part upload failed: {}",
        part1_resp.status()
    );
    let part1_marker = url::Url::parse(&part1_url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "blockid")
        .map(|(_, value)| value.into_owned())
        .expect("blockid query parameter");
    let part2_marker = driver
        .upload_multipart_part("multipart/assembled.bin", &upload_id, 2, b"world")
        .await
        .unwrap();

    let uploaded_parts = driver
        .list_uploaded_part_details("multipart/assembled.bin", &upload_id)
        .await
        .unwrap();
    assert_eq!(
        uploaded_parts,
        vec![
            aster_drive::storage::UploadedMultipartPart {
                part_number: 1,
                size: 6,
            },
            aster_drive::storage::UploadedMultipartPart {
                part_number: 2,
                size: 5,
            },
        ]
    );

    driver
        .complete_multipart_upload(
            "multipart/assembled.bin",
            &upload_id,
            vec![(2, part2_marker), (1, part1_marker)],
        )
        .await
        .unwrap();
    assert_eq!(
        driver.get("multipart/assembled.bin").await.unwrap(),
        b"hello world"
    );
    assert!(
        driver
            .list_uploaded_part_details("multipart/assembled.bin", &upload_id)
            .await
            .unwrap()
            .is_empty()
    );

    let reader_path = "stream/reader.bin";
    let reader_size = 5_242_880_i64 + 17;
    driver
        .put_reader(
            reader_path,
            Box::new(tokio::io::repeat(b'Z').take(u64::try_from(reader_size).unwrap())),
            reader_size,
        )
        .await
        .unwrap();
    let reader_meta = driver.metadata(reader_path).await.unwrap();
    assert_eq!(reader_meta.size, u64::try_from(reader_size).unwrap());
    let reader_body = driver.get(reader_path).await.unwrap();
    assert_eq!(reader_body.len(), usize::try_from(reader_size).unwrap());
    assert!(reader_body.iter().all(|byte| *byte == b'Z'));

    driver.delete("docs/hello.txt").await.unwrap();
    driver.delete("docs/hello.txt").await.unwrap();
    assert!(!driver.exists("docs/hello.txt").await.unwrap());
}

#[tokio::test]
async fn test_azure_blob_put_reader_length_boundaries_with_azurite() {
    let container = GenericImage::new(AZURITE_IMAGE, AZURITE_TAG)
        .with_exposed_port(IntoContainerPort::tcp(AZURITE_BLOB_PORT))
        .with_cmd(vec![
            "azurite-blob",
            "--blobHost",
            "0.0.0.0",
            "--blobPort",
            "10000",
        ])
        .start()
        .await
        .expect("failed to start azurite container");

    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(AZURITE_BLOB_PORT))
        .await
        .expect("resolve mapped azurite port");
    let endpoint = format!("http://127.0.0.1:{port}/{AZURITE_ACCOUNT}");
    let container_name = unique_container_name();
    wait_for_azurite_container(&endpoint, &container_name).await;

    let driver = AzureBlobDriver::new(&azure_policy(&endpoint, &container_name, "boundaries"))
        .expect("create AzureBlobDriver");

    driver
        .put_reader("empty.bin", Box::new(tokio::io::empty()), 0)
        .await
        .unwrap();
    assert_eq!(driver.metadata("empty.bin").await.unwrap().size, 0);
    assert!(driver.get("empty.bin").await.unwrap().is_empty());

    let short_error = driver
        .put_reader("short.bin", Box::new(tokio::io::empty()), 1)
        .await
        .expect_err("short stream should fail");
    assert_eq!(
        short_error.storage_error_kind(),
        Some(StorageErrorKind::Precondition)
    );

    let long_error = driver
        .put_reader(
            "long.bin",
            Box::new(std::io::Cursor::new(b"XYZ".to_vec())),
            2,
        )
        .await
        .expect_err("long stream should fail");
    assert!(
        matches!(
            long_error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured | StorageErrorKind::Precondition)
        ),
        "unexpected long reader error kind: {:?}",
        long_error.storage_error_kind()
    );

    let upload_id = driver
        .create_multipart_upload("multipart/reader.bin")
        .await
        .unwrap();
    let part_error = driver
        .upload_multipart_part_reader(
            "multipart/reader.bin",
            &upload_id,
            1,
            Box::new(tokio::io::empty()),
            2,
        )
        .await
        .expect_err("short multipart reader should fail");
    assert!(
        matches!(
            part_error.storage_error_kind(),
            Some(StorageErrorKind::Transient | StorageErrorKind::Misconfigured)
        ),
        "unexpected multipart reader error kind: {:?}",
        part_error.storage_error_kind()
    );

    let marker = driver
        .upload_multipart_part_reader(
            "multipart/reader.bin",
            &upload_id,
            2,
            Box::new(tokio::io::repeat(b'Q').take(2)),
            2,
        )
        .await
        .expect("exact-size multipart reader should succeed");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(marker)
            .unwrap(),
        b"aster-part-0000000002"
    );
}
