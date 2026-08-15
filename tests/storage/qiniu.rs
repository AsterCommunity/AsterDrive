//! Qiniu Kodo wrapper integration tests using the shared S3-compatible contract.

use aster_drive::storage::drivers::qiniu::{
    QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials,
};
use std::time::Duration;

use aster_drive_storage::{
    MultipartStorageDriver, PresignedDownloadOptions, StorageDriver, StorageErrorKind,
};
use testcontainers::{GenericImage, ImageExt, runners::AsyncRunner};
use tokio::io::AsyncReadExt;

const RUSTFS_TEST_IMAGE_TAG: &str = "1.0.0-alpha.90";
const REAL_KODO_ENV: &[&str] = &[
    "ASTER_TEST_QINIU_KODO_ENDPOINT",
    "ASTER_TEST_QINIU_KODO_BUCKET",
    "ASTER_TEST_QINIU_KODO_REGION",
    "ASTER_TEST_QINIU_KODO_ACCESS_KEY",
    "ASTER_TEST_QINIU_KODO_SECRET_KEY",
    "ASTER_TEST_QINIU_KODO_PATH_STYLE",
    "ASTER_TEST_QINIU_KODO_CORS_ORIGIN",
];

struct RealKodoConfig {
    endpoint: String,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
    path_style: bool,
    cors_origin: String,
}

impl RealKodoConfig {
    fn from_env() -> Option<Self> {
        let missing = REAL_KODO_ENV
            .iter()
            .filter(|name| std::env::var(name).is_err())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            eprintln!(
                "skipping real Qiniu Kodo smoke test; missing environment variables: {}",
                missing.join(", ")
            );
            return None;
        }

        let read = |name: &str| {
            std::env::var(name).unwrap_or_else(|_| panic!("required environment variable {name}"))
        };
        let region = read("ASTER_TEST_QINIU_KODO_REGION");
        let endpoint = read("ASTER_TEST_QINIU_KODO_ENDPOINT");
        let parsed = url::Url::parse(&endpoint).expect("Kodo endpoint must be a valid URL");
        assert_eq!(
            parsed.host_str(),
            Some(format!("s3.{region}.qiniucs.com").as_str()),
            "Kodo smoke endpoint must be the official service endpoint matching the region"
        );
        let path_style = match read("ASTER_TEST_QINIU_KODO_PATH_STYLE").as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON" => true,
            "0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF" => false,
            _ => panic!("ASTER_TEST_QINIU_KODO_PATH_STYLE must be a boolean"),
        };

        Some(Self {
            endpoint,
            bucket: read("ASTER_TEST_QINIU_KODO_BUCKET"),
            region,
            access_key: read("ASTER_TEST_QINIU_KODO_ACCESS_KEY"),
            secret_key: read("ASTER_TEST_QINIU_KODO_SECRET_KEY"),
            path_style,
            cors_origin: read("ASTER_TEST_QINIU_KODO_CORS_ORIGIN"),
        })
    }

    fn driver(&self, base_path: String) -> QiniuDriver {
        QiniuDriver::new(
            QiniuDriverConfig {
                endpoint: self.endpoint.clone(),
                bucket: self.bucket.clone(),
                base_path,
                region: self.region.clone(),
                path_style: self.path_style,
                connect_timeout: Duration::from_secs(5),
                read_timeout: Duration::from_secs(30),
                operation_timeout: Duration::from_secs(120),
            },
            QiniuStaticCredentials {
                access_key: self.access_key.clone(),
                secret_key: self.secret_key.clone(),
            },
        )
        .expect("real Qiniu Kodo driver should build")
    }
}

fn presigned_request(
    client: &reqwest::Client,
    method: reqwest::Method,
    request: &aster_drive_storage::PresignedUploadRequest,
) -> reqwest::RequestBuilder {
    request.headers.iter().fold(
        client.request(method, &request.url),
        |builder, (name, value)| builder.header(name, value),
    )
}

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

    let client = reqwest::Client::new();
    let presigned = driver
        .extensions()
        .presigned
        .expect("Qiniu presigned extension");
    let put_request = presigned
        .presigned_put_request("folder/presigned.txt", Duration::from_secs(60))
        .await
        .expect("presign RustFS PUT")
        .expect("Qiniu wrapper should expose presigned PUT");
    let response = presigned_request(&client, reqwest::Method::PUT, &put_request)
        .body(b"presigned through Qiniu wrapper".to_vec())
        .send()
        .await
        .expect("execute RustFS presigned PUT");
    assert!(response.status().is_success());
    assert!(response.headers().get(reqwest::header::ETAG).is_some());
    let get_url = presigned
        .presigned_url(
            "folder/presigned.txt",
            Duration::from_secs(60),
            PresignedDownloadOptions::default(),
        )
        .await
        .expect("presign RustFS GET")
        .expect("Qiniu wrapper should expose presigned GET");
    let response = client
        .get(get_url)
        .send()
        .await
        .expect("execute RustFS presigned GET");
    assert!(response.status().is_success());
    assert_eq!(
        response.bytes().await.expect("read presigned GET body"),
        b"presigned through Qiniu wrapper".as_slice()
    );

    let aborted_upload_id = driver
        .create_multipart_upload("folder/aborted.bin")
        .await
        .expect("create abortable multipart upload");
    driver
        .upload_multipart_part(
            "folder/aborted.bin",
            &aborted_upload_id,
            1,
            b"discarded-part",
        )
        .await
        .expect("upload abortable multipart part");
    assert_eq!(
        driver
            .list_uploaded_parts("folder/aborted.bin", &aborted_upload_id)
            .await
            .expect("list uploaded multipart parts"),
        vec![1]
    );
    driver
        .abort_multipart_upload("folder/aborted.bin", &aborted_upload_id)
        .await
        .expect("abort multipart upload");
    assert_eq!(
        driver
            .list_uploaded_parts("folder/aborted.bin", &aborted_upload_id)
            .await
            .expect_err("aborted multipart upload should not remain")
            .kind(),
        StorageErrorKind::NotFound
    );

    for path in [
        "folder/object.txt",
        "folder/multipart.bin",
        "folder/presigned.txt",
    ] {
        driver
            .delete(path)
            .await
            .expect("delete through Qiniu wrapper");
    }
    assert!(
        !driver
            .exists("folder/object.txt")
            .await
            .expect("HEAD deleted object")
    );
}

#[tokio::test]
#[ignore = "requires ASTER_TEST_QINIU_KODO_* and an isolated real Kodo S3 space"]
async fn real_qiniu_kodo_smoke_validates_connection_presigned_cors_and_multipart_contracts() {
    let Some(config) = RealKodoConfig::from_env() else {
        return;
    };
    let test_id = uuid::Uuid::new_v4();
    let driver = config.driver(format!("asterdrive-kodo-smoke/{test_id}"));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("Kodo smoke HTTP client should build");

    // Same write/delete contract used by draft and saved policy connection tests.
    driver
        .put("connection-probe", b"ok")
        .await
        .expect("real Kodo connection probe should write");
    driver
        .delete("connection-probe")
        .await
        .expect("real Kodo connection probe should clean up");

    let object_path = "objects/ordinary.txt";
    let payload = b"hello from AsterDrive real Kodo smoke";
    driver
        .put(object_path, payload)
        .await
        .expect("ordinary Kodo PUT should succeed");
    assert!(
        driver
            .exists(object_path)
            .await
            .expect("ordinary Kodo HEAD should succeed")
    );
    assert_eq!(
        driver
            .metadata(object_path)
            .await
            .expect("ordinary Kodo metadata HEAD should succeed")
            .size,
        payload.len() as u64
    );
    assert_eq!(
        driver
            .get(object_path)
            .await
            .expect("ordinary Kodo GET should succeed"),
        payload
    );
    let mut range = driver
        .get_range(object_path, 6, Some(4))
        .await
        .expect("real Kodo Range should succeed");
    let mut range_body = Vec::new();
    range
        .read_to_end(&mut range_body)
        .await
        .expect("real Kodo Range body should read");
    assert_eq!(range_body, b"from");
    assert!(
        driver
            .extensions()
            .list
            .expect("Qiniu list capability")
            .list_paths(Some("objects"))
            .await
            .expect("real Kodo list should succeed")
            .contains(&object_path.to_string())
    );

    let presigned = driver
        .extensions()
        .presigned
        .expect("Qiniu presigned capability");
    let presigned_path = "objects/presigned.txt";
    let presigned_payload = b"presigned Kodo payload";
    let put_request = presigned
        .presigned_put_request(presigned_path, Duration::from_secs(300))
        .await
        .expect("Kodo presigned PUT should be generated")
        .expect("Qiniu driver should expose presigned PUT");
    let put_response = presigned_request(&client, reqwest::Method::PUT, &put_request)
        .body(presigned_payload.to_vec())
        .send()
        .await
        .unwrap_or_else(|_| panic!("real Kodo presigned PUT request failed"));
    assert!(
        put_response.status().is_success(),
        "real Kodo presigned PUT returned status {}",
        put_response.status()
    );
    assert!(
        put_response.headers().get(reqwest::header::ETAG).is_some(),
        "real Kodo presigned PUT must expose ETag"
    );

    let get_url = presigned
        .presigned_url(
            presigned_path,
            Duration::from_secs(300),
            PresignedDownloadOptions::default(),
        )
        .await
        .expect("Kodo presigned GET should be generated")
        .expect("Qiniu driver should expose presigned GET");
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .unwrap_or_else(|_| panic!("real Kodo presigned GET request failed"));
    assert!(
        get_response.status().is_success(),
        "real Kodo presigned GET returned status {}",
        get_response.status()
    );
    assert_eq!(
        get_response
            .bytes()
            .await
            .unwrap_or_else(|_| panic!("real Kodo presigned GET body failed")),
        presigned_payload.as_slice()
    );

    // Preflight the exact object target without retaining the signed query string.
    let mut cors_url = url::Url::parse(&put_request.url).expect("presigned Kodo URL should parse");
    cors_url.set_query(None);
    let cors_response = client
        .request(reqwest::Method::OPTIONS, cors_url)
        .header(reqwest::header::ORIGIN, &config.cors_origin)
        .header(reqwest::header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
        .header(
            reqwest::header::ACCESS_CONTROL_REQUEST_HEADERS,
            "content-type",
        )
        .send()
        .await
        .unwrap_or_else(|_| panic!("real Kodo CORS preflight failed"));
    assert!(
        cors_response.status().is_success(),
        "real Kodo CORS preflight returned status {}",
        cors_response.status()
    );
    let allow_origin = cors_response
        .headers()
        .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|value| value.to_str().ok())
        .expect("real Kodo CORS must expose Access-Control-Allow-Origin");
    assert!(allow_origin == "*" || allow_origin == config.cors_origin);
    let allow_methods = cors_response
        .headers()
        .get(reqwest::header::ACCESS_CONTROL_ALLOW_METHODS)
        .and_then(|value| value.to_str().ok())
        .expect("real Kodo CORS must expose Access-Control-Allow-Methods");
    assert!(
        allow_methods
            .split(',')
            .any(|method| method.trim() == "PUT")
    );
    assert!(
        cors_response
            .headers()
            .get(reqwest::header::ACCESS_CONTROL_EXPOSE_HEADERS)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|headers| {
                headers
                    .split(',')
                    .any(|header| header.trim().eq_ignore_ascii_case("etag"))
            }),
        "real Kodo CORS must expose ETag to browser uploads"
    );

    let multipart_payload = vec![b'm'; 5 * 1024 * 1024];
    let multipart_path = "objects/multipart.bin";
    let upload_id = driver
        .create_multipart_upload(multipart_path)
        .await
        .expect("real Kodo multipart create should succeed");
    let etag = driver
        .upload_multipart_part(multipart_path, &upload_id, 1, &multipart_payload)
        .await
        .expect("real Kodo multipart part should return ETag");
    assert!(!etag.trim_matches('"').is_empty());
    driver
        .complete_multipart_upload(multipart_path, &upload_id, vec![(1, etag)])
        .await
        .expect("real Kodo multipart complete should succeed");
    assert_eq!(
        driver
            .metadata(multipart_path)
            .await
            .expect("completed Kodo multipart object should exist")
            .size,
        multipart_payload.len() as u64
    );

    let presigned_multipart_path = "objects/presigned-multipart.bin";
    let upload_id = driver
        .create_multipart_upload(presigned_multipart_path)
        .await
        .expect("real Kodo presigned multipart create should succeed");
    let part_request = driver
        .presigned_upload_part_request(
            presigned_multipart_path,
            &upload_id,
            1,
            Duration::from_secs(300),
        )
        .await
        .expect("real Kodo multipart part should be presigned");
    let part_response = presigned_request(&client, reqwest::Method::PUT, &part_request)
        .body(multipart_payload.clone())
        .send()
        .await
        .unwrap_or_else(|_| panic!("real Kodo presigned multipart part request failed"));
    assert!(
        part_response.status().is_success(),
        "real Kodo presigned multipart part returned status {}",
        part_response.status()
    );
    let etag = part_response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim_matches('"').is_empty())
        .expect("real Kodo presigned multipart part must expose ETag")
        .to_string();
    driver
        .complete_multipart_upload(presigned_multipart_path, &upload_id, vec![(1, etag)])
        .await
        .expect("real Kodo presigned multipart complete should succeed");

    let aborted_path = "objects/aborted-multipart.bin";
    let aborted_upload_id = driver
        .create_multipart_upload(aborted_path)
        .await
        .expect("real Kodo abort fixture should be created");
    driver
        .upload_multipart_part(aborted_path, &aborted_upload_id, 1, b"discarded")
        .await
        .expect("real Kodo abort fixture part should upload");
    assert_eq!(
        driver
            .list_uploaded_parts(aborted_path, &aborted_upload_id)
            .await
            .expect("real Kodo should list uploaded multipart parts"),
        vec![1]
    );
    driver
        .abort_multipart_upload(aborted_path, &aborted_upload_id)
        .await
        .expect("real Kodo multipart abort should succeed");
    assert_eq!(
        driver
            .list_uploaded_parts(aborted_path, &aborted_upload_id)
            .await
            .expect_err("aborted Kodo upload should no longer exist")
            .kind(),
        StorageErrorKind::NotFound
    );

    let failed_path = "objects/failed-completion.bin";
    let failed_upload_id = driver
        .create_multipart_upload(failed_path)
        .await
        .expect("real Kodo failed-completion fixture should be created");
    driver
        .upload_multipart_part(failed_path, &failed_upload_id, 1, &multipart_payload)
        .await
        .expect("real Kodo failed-completion part should upload");
    driver
        .complete_multipart_upload(
            failed_path,
            &failed_upload_id,
            vec![(1, "definitely-not-the-provider-etag".to_string())],
        )
        .await
        .expect_err("real Kodo must reject multipart completion with the wrong ETag");
    driver
        .abort_multipart_upload(failed_path, &failed_upload_id)
        .await
        .expect("failed Kodo completion must remain abortable for cleanup");

    for path in [
        object_path,
        presigned_path,
        multipart_path,
        presigned_multipart_path,
    ] {
        driver
            .delete(path)
            .await
            .expect("real Kodo smoke object should clean up");
        assert!(
            !driver
                .exists(path)
                .await
                .expect("deleted real Kodo smoke object should be absent")
        );
    }
}
