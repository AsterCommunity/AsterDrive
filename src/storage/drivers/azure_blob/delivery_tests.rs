use std::{collections::BTreeMap, time::Duration};

use super::{
    AzureBlobDownloadDelivery, AzureBlobDriver, AzureBlobDriverConfig, AzureBlobStaticCredentials,
};
use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadStorageDriver, PresignedUploadStorageDriver,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

fn config() -> AzureBlobDriverConfig {
    AzureBlobDriverConfig {
        endpoint: "https://account.blob.core.windows.net".into(),
        container: "archive".into(),
        base_path: "base path".into(),
        chunk_size: 0,
        download_delivery: AzureBlobDownloadDelivery::FrontDoorClientSas,
        download_base_url: Some("https://files.example.com".into()),
    }
}

fn driver() -> AzureBlobDriver {
    AzureBlobDriver::new(
        config(),
        AzureBlobStaticCredentials {
            account_name: "account".into(),
            account_key: "c2VjcmV0".into(),
        },
    )
    .unwrap()
}

// An independent HMAC implementation and explicit REST field order verify the
// canonical resource and all response overrides, rather than comparing two
// invocations of the production signer.
fn expected_signature(query: &BTreeMap<String, String>, resource: &str) -> String {
    let get = |name: &str| query.get(name).map(String::as_str).unwrap_or("");
    let input = [
        get("sp"),
        get("st"),
        get("se"),
        resource,
        "",
        "",
        get("spr"),
        get("sv"),
        get("sr"),
        "",
        "",
        get("rscc"),
        get("rscd"),
        "",
        "",
        get("rsct"),
    ]
    .join("\n");
    let key = STANDARD.decode("c2VjcmV0").unwrap();
    STANDARD.encode(
        ring::hmac::sign(
            &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &key),
            input.as_bytes(),
        )
        .as_ref(),
    )
}

#[tokio::test]
async fn signs_canonical_blob_and_headers_without_routing_uploads_to_front_door() {
    let driver = driver();
    let name = "中文 +%2F%25.txt";
    let options = DirectDownloadOptions {
        response_content_disposition: Some("attachment; filename*=UTF-8''%E4%B8%AD.txt".into()),
        response_content_type: Some("application/octet-stream".into()),
        response_cache_control: Some("public, max-age=3600".into()),
        ..Default::default()
    };
    let request = driver
        .resolve_download_url(name, Duration::from_secs(300), options)
        .await
        .unwrap()
        .unwrap();
    let url = url::Url::parse(&request.url).unwrap();
    assert_eq!(url.host_str(), Some("files.example.com"));
    assert_eq!(
        url.path(),
        "/archive/base%20path/%E4%B8%AD%E6%96%87%20%2B%252F%2525%2Etxt"
    );
    let mut query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(query["sp"], "r");
    assert_eq!(query["sr"], "b");
    assert_eq!(query["spr"], "https");
    assert_eq!(query["rscc"], "private, no-store");
    let resource = format!("/blob/account/archive/base path/{name}");
    assert_eq!(query["sig"], expected_signature(&query, &resource));
    assert_eq!(
        request
            .expires_at
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        query["se"]
    );
    assert_ne!(
        query["sig"],
        expected_signature(&query, "/blob/account/other/blob")
    );
    query.insert("rscd".into(), "inline".into());
    assert_ne!(query["sig"], expected_signature(&query, &resource));
    let upload = driver
        .presigned_put_request(name, Duration::from_secs(300))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        url::Url::parse(&upload.url).unwrap().host_str(),
        Some("account.blob.core.windows.net")
    );
    assert!(upload.url.contains("sp=cw"));
    assert_eq!(
        driver
            .container_url("rl", Duration::from_secs(300))
            .unwrap()
            .host_str(),
        Some("account.blob.core.windows.net")
    );
    assert_eq!(
        driver
            .block_blob_url(name, "w", Duration::from_secs(300))
            .unwrap()
            .host_str(),
        Some("account.blob.core.windows.net")
    );
}

#[test]
fn rejects_unsafe_delivery_origins_without_echoing_secrets() {
    for value in [
        "http://files.example.com",
        "https:files.example.com",
        "https:/files.example.com",
        "https://user:secret@files.example.com",
        "https://files.example.com?sig=SECRET",
        "https://files.example.com#SECRET",
        "https://files.example.com/prefix",
        "https://files.example.com/a/../",
        "https://files.example.com/%2e%2e",
        "https://files.example.com:8443",
        "https://files.example.com\\evil",
        "https://files.example.com\n",
        "https://account.z1.web.core.windows.net",
    ] {
        let mut config = config();
        config.download_base_url = Some(value.into());
        let error = AzureBlobDriver::validate_delivery_config(&config).unwrap_err();
        assert!(!error.to_string().contains("SECRET"));
        assert!(!error.to_string().contains("user:secret"));
    }
}

#[test]
fn rejects_unsafe_blob_origins_and_redacts_credentials() {
    for endpoint in [
        "https://user:PRIVATE_MARKER@account.blob.core.windows.net",
        "https://account.blob.core.windows.net?sig=PRIVATE_MARKER",
        "https://account.blob.core.windows.net#PRIVATE_MARKER",
        "https://account.blob.core.windows.net/a/../PRIVATE_MARKER",
        "https://account.blob.core.windows.net/%2e%2e/PRIVATE_MARKER",
        "https://account.blob.core.windows.net/PRIVATE_MARKER\n",
        "https://account.z1.web.core.windows.net",
    ] {
        let error =
            AzureBlobDriver::normalize_endpoint_and_container(endpoint, "archive").unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_MARKER"));
    }
    let credentials = AzureBlobStaticCredentials {
        account_name: "PRIVATE_ACCOUNT".into(),
        account_key: "PRIVATE_KEY".into(),
    };
    let debug = format!("{credentials:?}");
    assert!(!debug.contains("PRIVATE_ACCOUNT"));
    assert!(!debug.contains("PRIVATE_KEY"));

    for endpoint in [
        "http://127.0.0.1:10000/devstoreaccount1",
        "https://account.blob.core.windows.net/container",
    ] {
        let mut config = config();
        config.endpoint = endpoint.into();
        assert!(AzureBlobDriver::validate_delivery_config(&config).is_err());
    }
}

#[tokio::test]
async fn default_delivery_keeps_origin_and_signs_response_options() {
    let driver = AzureBlobDriver::new(
        AzureBlobDriverConfig {
            download_delivery: AzureBlobDownloadDelivery::BlobOrigin,
            // The hidden Front Door field is retained when switching modes.
            download_base_url: Some("https://files.example.com/prefix".into()),
            ..config()
        },
        AzureBlobStaticCredentials {
            account_name: "account".into(),
            account_key: "c2VjcmV0".into(),
        },
    )
    .unwrap();
    let request = driver
        .resolve_download_url(
            "file",
            Duration::from_secs(300),
            DirectDownloadOptions {
                response_cache_control: Some("private, max-age=60".into()),
                response_content_disposition: Some("attachment; filename=report.txt".into()),
                response_content_type: Some("text/plain".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let url = url::Url::parse(&request.url).unwrap();
    assert_eq!(url.host_str(), Some("account.blob.core.windows.net"));
    let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(query["rscc"], "private, max-age=60");
    assert_eq!(
        query["sig"],
        expected_signature(&query, "/blob/account/archive/base path/file")
    );
}

#[tokio::test]
async fn rejects_traversal_header_injection_and_invalid_expiry() {
    let driver = driver();
    for path in [
        "../secret",
        "folder/./file",
        "folder/../file",
        "folder\\file",
        "file\n",
    ] {
        assert!(
            driver
                .resolve_download_url(path, Duration::from_secs(300), Default::default())
                .await
                .is_err()
        );
    }
    for expiry in [Duration::ZERO, Duration::from_millis(999), Duration::MAX] {
        assert!(
            driver
                .resolve_download_url("file", expiry, Default::default())
                .await
                .is_err()
        );
    }
    assert!(
        driver
            .resolve_download_url(
                "file",
                Duration::from_secs(1),
                DirectDownloadOptions {
                    response_content_type: Some("text/plain\r\nx-evil: true".into()),
                    ..Default::default()
                }
            )
            .await
            .is_err()
    );
}

#[actix_web::test]
async fn private_download_request_contract_rejects_tampering_and_warm_url_after_expiry() {
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use std::sync::{
        Arc,
        atomic::{AtomicI64, AtomicUsize, Ordering},
    };
    let clock = Arc::new(AtomicI64::new(chrono::Utc::now().timestamp()));
    let hits = Arc::new(AtomicUsize::new(0));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server_clock = clock.clone();
    let server_hits = hits.clone();
    let server = HttpServer::new(move || {
        let clock = server_clock.clone();
        let hits = server_hits.clone();
        App::new().default_service(web::to(move |req: HttpRequest| {
            let clock = clock.clone();
            let hits = hits.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                if req.method() == "OPTIONS" {
                    return HttpResponse::NoContent()
                        .insert_header(("access-control-allow-origin", "https://drive.example.com"))
                        .insert_header(("access-control-allow-methods", "GET,HEAD,OPTIONS"))
                        .insert_header(("access-control-allow-headers", "Range"))
                        .finish();
                }
                let query: BTreeMap<String, String> =
                    url::form_urlencoded::parse(req.query_string().as_bytes())
                        .into_owned()
                        .collect();
                let resource = format!(
                    "/blob/account{}",
                    percent_encoding::percent_decode_str(req.path())
                        .decode_utf8()
                        .unwrap()
                );
                let expiry = query
                    .get("se")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
                if query.get("sig") != Some(&expected_signature(&query, &resource))
                    || query.get("sp").map(String::as_str) != Some("r")
                    || expiry.is_none_or(|se| se.timestamp() <= clock.load(Ordering::SeqCst))
                {
                    return HttpResponse::Forbidden().finish();
                }
                if req.path().ends_with("missing") {
                    return HttpResponse::NotFound().finish();
                }
                let range = req.headers().get("range").is_some();
                let mut response = if range {
                    HttpResponse::PartialContent()
                } else {
                    HttpResponse::Ok()
                };
                response
                    .insert_header(("cache-control", query["rscc"].as_str()))
                    .insert_header(("content-disposition", query["rscd"].as_str()))
                    .insert_header(("content-type", query["rsct"].as_str()))
                    .insert_header(("access-control-allow-origin", "https://drive.example.com"));
                if range {
                    response.insert_header(("content-range", "bytes 1-2/5"));
                }
                if req.method() == "HEAD" {
                    response.body("hello")
                } else {
                    response.body(if range { "el" } else { "hello" })
                }
            }
        }))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    let signed = driver()
        .resolve_download_url(
            "中文 %2F.txt",
            Duration::from_secs(300),
            DirectDownloadOptions {
                response_content_disposition: Some(
                    "attachment; filename*=UTF-8''%E4%B8%AD.txt".into(),
                ),
                response_content_type: Some("application/octet-stream".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    // Exercise the emitted wire path/query against a local provider emulator;
    // production configuration continues to require HTTPS.
    let signed_url = url::Url::parse(&signed.url).unwrap();
    let local = format!(
        "http://{address}{}?{}",
        signed_url.path(),
        signed_url.query().unwrap()
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client.get(&local).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(
        response.headers()["content-disposition"],
        "attachment; filename*=UTF-8''%E4%B8%AD.txt"
    );
    assert_eq!(response.text().await.unwrap(), "hello");
    assert_eq!(
        client.head(&local).send().await.unwrap().headers()["content-length"],
        "5"
    );
    let response = client
        .get(&local)
        .header("Range", "bytes=1-2")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["content-range"], "bytes 1-2/5");
    assert_eq!(response.text().await.unwrap(), "el");
    assert_eq!(
        client
            .request(reqwest::Method::OPTIONS, &local)
            .header("Origin", "https://drive.example.com")
            .header("Access-Control-Request-Headers", "Range")
            .send()
            .await
            .unwrap()
            .status(),
        204
    );
    assert_eq!(
        client
            .get(format!("http://{address}{}", signed_url.path()))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(local.replace("sp=r", "sp=w"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(local.replace("rscc=private", "rscc=public"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    // The exact URL already returned 200: an expired repeat must reach origin,
    // not be served from an emulator cache. Real Front Door needs cloud proof.
    clock.store(signed.expires_at.unwrap().timestamp() + 1, Ordering::SeqCst);
    assert_eq!(client.get(&local).send().await.unwrap().status(), 403);
    assert_eq!(hits.load(Ordering::SeqCst), 8);
    handle.stop(true).await;
}

#[actix_web::test]
async fn sdk_operations_keep_blob_endpoint_and_do_not_follow_redirects() {
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use aster_drive_storage::{MultipartStorageDriver, StorageDriver};
    use std::sync::{Arc, Mutex};
    let captured = Arc::new(Mutex::new(Vec::new()));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let records = captured.clone();
    let server = HttpServer::new(move || {
        let records = records.clone();
        App::new().default_service(web::to(move |req: HttpRequest, _body: web::Bytes| {
            let records = records.clone();
            async move {
                records.lock().unwrap().push((
                    req.method().to_string(),
                    req.path().to_string(),
                    req.query_string().to_string(),
                    req.headers()
                        .get("range")
                        .map(|v| v.to_str().unwrap().to_string()),
                ));
                if req.path().ends_with("redirect") {
                    return HttpResponse::TemporaryRedirect()
                        .insert_header(("location", format!("http://{address}/leaked")))
                        .finish();
                }
                if req.path().ends_with("missing") {
                    return HttpResponse::NotFound().insert_header(("x-ms-error-code", "BlobNotFound")).finish();
                }
                if req.path().ends_with("forbidden") {
                    return HttpResponse::Forbidden().insert_header(("x-ms-error-code", "AuthorizationPermissionMismatch")).finish();
                }
                if req.query_string().contains("comp=list") {
                    return HttpResponse::Ok().content_type("application/xml").body("<?xml version=\"1.0\" encoding=\"utf-8\"?><EnumerationResults><Blobs/><NextMarker/></EnumerationResults>");
                }
                if req.query_string().contains("comp=blocklist") || req.method() == "PUT" {
                    return HttpResponse::Created()
                        .insert_header(("etag", "\"test\""))
                        .insert_header(("last-modified", "Mon, 02 Nov 2026 00:00:00 GMT"))
                        .finish();
                }
                if req.method() == "DELETE" {
                    return HttpResponse::Accepted().finish();
                }
                if req.method() == "HEAD" {
                    return HttpResponse::Ok()
                        .insert_header(("content-type", "text/plain"))
                        .body("hello");
                }
                if req.headers().get("range").is_some_and(|value| value == "bytes=1-2") {
                    return HttpResponse::PartialContent()
                        .insert_header(("content-range", "bytes 1-2/5"))
                        .body("el");
                }
                if req.headers().contains_key("range") {
                    return HttpResponse::PartialContent()
                        .insert_header(("content-range", "bytes 0-4/5"))
                        .body("hello");
                }
                HttpResponse::Ok().body("hello")
            }
        }))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    let mut driver = driver();
    // Test-only transport substitution for real Azure SDK requests. Config
    // validation does not allow HTTP Front Door origins in production.
    driver.endpoint = format!("http://{address}");
    assert_eq!(driver.get("file").await.unwrap(), b"hello");
    assert_eq!(driver.metadata("file").await.unwrap().size, 5);
    let mut range = driver.get_range("file", 1, Some(2)).await.unwrap();
    let mut bytes = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut range, &mut bytes)
        .await
        .unwrap();
    assert_eq!(bytes, b"el");
    driver.put("file", b"hello").await.unwrap();
    driver.delete("file").await.unwrap();
    driver
        .upload_multipart_part("file", "upload", 1, b"hello")
        .await
        .unwrap();
    driver
        .complete_multipart_upload("file", "upload", vec![(1, "ignored".into())])
        .await
        .unwrap();
    use aster_drive_storage::{ListStorageDriver, StorageErrorKind};
    assert!(driver.list_paths(None).await.unwrap().is_empty());
    assert_eq!(
        driver.get("missing").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    assert_eq!(
        driver.get("forbidden").await.unwrap_err().kind(),
        StorageErrorKind::Permission
    );
    assert!(driver.get("redirect").await.is_err());
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 11);
    assert!(requests.iter().all(
        |(_, path, query, _)| (path.starts_with("/archive/base%20path/") || path == "/archive")
            && query.contains("sig=")
    ));
    assert!(!requests.iter().any(|(_, path, _, _)| path == "/leaked"));
    assert!(
        requests
            .iter()
            .any(|(_, _, query, _)| query.contains("comp=block&"))
    );
    assert!(
        requests
            .iter()
            .any(|(_, _, _, range)| range.as_deref() == Some("bytes=1-2"))
    );
    drop(requests);
    handle.stop(true).await;
}
