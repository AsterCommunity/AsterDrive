use super::*;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadStorageDriver, MultipartStorageDriver,
    ProviderResumableUploadDriver, StorageDriver, StreamUploadDriver,
};
use base64::{Engine, engine::general_purpose::URL_SAFE};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
use tokio::io::AsyncReadExt;

fn config() -> QiniuDriverConfig {
    QiniuDriverConfig {
        bucket: "archive".into(),
        base_path: "tenant".into(),
        download_origin: "https://origin.example.test".into(),
        download_delivery: QiniuDownloadDelivery::Origin,
        download_base_url: None,
        download_auth_validity_seconds: 180,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(5),
        operation_timeout: Duration::from_secs(10),
    }
}
fn driver() -> QiniuDriver {
    QiniuDriver::new(
        config(),
        QiniuStaticCredentials {
            access_key: "MY_ACCESS_KEY".into(),
            secret_key: "MY_SECRET_KEY".into(),
        },
    )
    .unwrap()
}

#[test]
fn signs_independent_official_download_vector_and_redacts_credentials() {
    // developer.qiniu.com/kodo/1202/download-token: an independent published vector.
    assert_eq!(
        driver()
            .sign(b"http://78re52.com1.z0.glb.clouddn.com/resource/flower.jpg?e=1451491200")
            .unwrap(),
        "MY_ACCESS_KEY:438dd8pXocjYuF-6dTcKMtETB2g="
    );
    assert!(!format!("{:?}", driver().credentials).contains("MY_SECRET_KEY"));
}

#[test]
fn private_attachment_url_matches_independent_signature_vector() {
    // Independent Python stdlib HMAC-SHA1 vector using the public test keys.
    let result = driver()
        .private_url(
            "https://origin.example.test",
            "中文/a %2F.txt",
            1451491200,
            &[("attname", "中文 +%*.txt")],
        )
        .unwrap();
    assert_eq!(
        result,
        "https://origin.example.test/tenant/%E4%B8%AD%E6%96%87/a%20%252F.txt?attname=%E4%B8%AD%E6%96%87+%2B%25%2A.txt&e=1451491200&token=MY_ACCESS_KEY:OvAlZtly9lQfN03U-JNPIZ8RHg0="
    );
}

#[test]
fn scopes_upload_capability_and_rejects_invalid_keys_or_domains() {
    let driver = driver();
    let token = driver
        .upload_token("tenant/object", 1234, Some(123), false)
        .unwrap();
    let policy: serde_json::Value =
        serde_json::from_slice(&URL_SAFE.decode(token.split(':').nth(2).unwrap()).unwrap())
            .unwrap();
    assert_eq!(
        policy,
        json!({"scope":"archive:tenant/object","deadline":1234,"fsizeLimit":123})
    );
    for path in ["../escape", "a/../b", "/absolute", "a\\b", "a/./b"] {
        assert!(driver.key(path).is_err());
    }
    for domain in [
        "http://origin.test",
        "https://u:p@origin.test",
        "https://origin.test/prefix",
        "https://origin.test?q=1",
        "https://origin.test/#x",
    ] {
        let mut config = config();
        config.download_origin = domain.into();
        assert!(QiniuDriver::validate_config(&config, &driver.credentials).is_err());
    }
}

#[tokio::test]
async fn private_urls_preserve_final_host_key_encoding_and_exact_expiry() {
    let mut driver = driver();
    driver.config.download_base_url = Some("https://cdn.example.test".into());
    driver.config.download_delivery = QiniuDownloadDelivery::QiniuCdn;
    let result = driver
        .resolve_download_url(
            "中文/a %2F.txt",
            Duration::from_secs(60),
            DirectDownloadOptions {
                allow_provider_response_headers: true,
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let url = Url::parse(&result.url).unwrap();
    assert_eq!(url.host_str(), Some("cdn.example.test"));
    assert_eq!(url.path(), "/tenant/%E4%B8%AD%E6%96%87/a%20%252F.txt");
    let values: BTreeMap<_, _> = url.query_pairs().collect();
    assert_eq!(
        values["e"].parse::<i64>().unwrap(),
        result.expires_at.unwrap().timestamp()
    );
    let signed = result.url.split("&token=").next().unwrap();
    assert_eq!(values["token"], driver.sign(signed.as_bytes()).unwrap());
    assert!(
        driver
            .resolve_download_url("object", Duration::ZERO, Default::default())
            .await
            .is_err()
    );
    assert!(
        driver
            .resolve_download_url("object", Duration::from_secs(8 * 86400), Default::default())
            .await
            .is_err()
    );
    assert!(
        driver
            .resolve_download_url(
                "object",
                Duration::from_secs(60),
                DirectDownloadOptions {
                    response_content_disposition: Some("attachment".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn native_downloads_use_provider_headers_and_signed_current_attachment_names() {
    for delivery in [
        QiniuDownloadDelivery::Origin,
        QiniuDownloadDelivery::QiniuCdn,
    ] {
        let mut driver = driver();
        driver.config.download_delivery = delivery;
        driver.config.download_base_url = Some("https://cdn.example.test".into());
        for (disposition, attachment) in [
            (Some("attachment; filename=old.txt"), true),
            (Some(" ATTACHMENT ; filename=old.txt"), true),
            (Some("inline; filename=old.txt"), false),
            (None, true),
        ] {
            for relaxed in [false, true] {
                let request = driver
                    .resolve_download_url(
                        "objects/old-uuid",
                        Duration::from_secs(60),
                        DirectDownloadOptions {
                            allow_provider_response_headers: relaxed,
                            download_name: Some("改名 +%&?=#*.png".into()),
                            require_download_name_match: true,
                            response_content_disposition: disposition.map(str::to_string),
                            response_content_type: Some("image/png".into()),
                            response_cache_control: Some(
                                "private, max-age=0, must-revalidate".into(),
                            ),
                        },
                    )
                    .await
                    .unwrap()
                    .expect("native headers must not trigger relay");
                let url = Url::parse(&request.url).unwrap();
                let query: BTreeMap<_, _> = url.query_pairs().collect();
                assert!(!query.keys().any(|key| key.starts_with("response-")));
                assert_eq!(query.contains_key("attname"), attachment);
                if attachment {
                    assert_eq!(query["attname"], "改名 +%&?=#*.png");
                }
                assert_eq!(
                    query["e"].parse::<i64>().unwrap(),
                    request.expires_at.unwrap().timestamp()
                );
                let unsigned = request.url.split("&token=").next().unwrap();
                assert_eq!(query["token"], driver.sign(unsigned.as_bytes()).unwrap());
                if attachment {
                    assert_ne!(
                        query["token"],
                        driver
                            .sign(unsigned.replace("attname=", "attname=other").as_bytes())
                            .unwrap()
                    );
                }
            }
        }
    }
    for invalid in ["x\r\nInjected: yes", "x\0y"] {
        for options in [
            DirectDownloadOptions {
                download_name: Some(invalid.into()),
                ..Default::default()
            },
            DirectDownloadOptions {
                response_content_disposition: Some(invalid.into()),
                ..Default::default()
            },
        ] {
            assert!(
                driver()
                    .resolve_download_url("object", Duration::from_secs(60), options)
                    .await
                    .is_err()
            );
        }
    }
}

#[derive(Default)]
struct State {
    objects: BTreeMap<String, Vec<u8>>,
    parts: BTreeMap<i32, Vec<u8>>,
    aborted: usize,
    completed: usize,
    requests: Vec<(String, String)>,
    ignore_range: bool,
    invalid_range: bool,
    malformed_json: bool,
    page_size: Option<usize>,
    repeat_marker: bool,
    upload_completed: bool,
    max_source_ttl: Option<i64>,
}
struct Server {
    base: String,
    state: Arc<std::sync::Mutex<State>>,
    handle: actix_web::dev::ServerHandle,
}
impl Server {
    async fn driver(&self) -> QiniuDriver {
        let mut driver = driver();
        driver.config.download_origin = self.base.clone();
        *driver.endpoints.lock().await = Some(Endpoints {
            upload: Url::parse(&self.base).unwrap(),
            management: Url::parse(&self.base).unwrap(),
            list: Url::parse(&self.base).unwrap(),
            expires_at: Instant::now() + Duration::from_secs(60),
        });
        driver
    }
    async fn stop(self) {
        self.handle.stop(true).await;
    }
}
async fn spawn() -> Server {
    async fn handler(
        req: HttpRequest,
        body: web::Bytes,
        data: web::Data<Arc<std::sync::Mutex<State>>>,
    ) -> HttpResponse {
        let mut state = data.lock().unwrap();
        let path = req.path();
        let method = req.method().as_str();
        state.requests.push((method.into(), path.into()));
        let query: BTreeMap<String, String> =
            url::form_urlencoded::parse(req.query_string().as_bytes())
                .into_owned()
                .collect();
        if path == "/v4/query" {
            assert_eq!(query.get("bucket").map(String::as_str), Some("archive"));
            assert_eq!(query.get("ak").map(String::as_str), Some("MY_ACCESS_KEY"));
            return HttpResponse::Ok().json(json!({"hosts":[{"ttl":600,"up":{"domains":["upload.example.test"]},"rs":{"domains":["rs.example.test"]},"rsf":{"domains":["rsf.example.test"]}}]}));
        }
        if path == "/malformed" {
            return HttpResponse::Ok().body("not-json MY_SECRET_KEY");
        }
        if path == "/expiry-window" {
            return HttpResponse::Forbidden().insert_header(("x-reqid","safe-expiry-request-id")).json(json!({"error":"The difference between the request expires and the server's time is too large"}));
        }
        if path.starts_with("/error/") {
            let status = path.rsplit('/').next().unwrap().parse().unwrap();
            return HttpResponse::build(actix_web::http::StatusCode::from_u16(status).unwrap())
                .insert_header(("x-reqid", "safe-request-id"))
                .json(json!({"error":"MY_SECRET_KEY https://example.test/?token=secret"}));
        }
        if path.starts_with("/buckets/") {
            assert!(
                req.headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("UpToken MY_ACCESS_KEY:")
            );
            let fields: Vec<_> = path.split('/').collect();
            assert_eq!(fields[2], "archive");
            let key = String::from_utf8(URL_SAFE.decode(fields[4]).unwrap()).unwrap();
            if fields.len() == 6 && method == "POST" {
                state.parts.clear();
                state.upload_completed = false;
                return HttpResponse::Ok().json(json!({"uploadId":"native-session","expireAt":chrono::Utc::now().timestamp()+604800}));
            }
            if fields.len() == 8 && method == "PUT" {
                let number: i32 = fields[7].parse().unwrap();
                assert_eq!(
                    req.headers()
                        .get("content-length")
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .parse::<usize>()
                        .unwrap(),
                    body.len()
                );
                state.parts.insert(number, body.to_vec());
                return HttpResponse::Ok()
                    .json(json!({"etag":format!("part-{number}"),"md5":"unused"}));
            }
            if method == "GET" {
                if state.upload_completed {
                    return HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(612).unwrap(),
                    )
                    .finish();
                }
                let marker = query
                    .get("part-number-marker")
                    .and_then(|v| v.parse::<i32>().ok())
                    .unwrap_or(0);
                let remaining = state
                    .parts
                    .iter()
                    .filter(|(n, _)| **n > marker)
                    .collect::<Vec<_>>();
                let limit = state.page_size.unwrap_or(1000);
                let page = remaining
                    .iter()
                    .take(limit)
                    .map(|(n, b)| json!({"partNumber":n,"size":b.len(),"etag":format!("part-{n}")}))
                    .collect::<Vec<_>>();
                let next = if state.repeat_marker {
                    marker.max(1)
                } else if remaining.len() > limit {
                    *remaining[limit - 1].0
                } else {
                    0
                };
                return HttpResponse::Ok().json(json!({"parts":page,"partNumberMarker":next}));
            }
            if method == "DELETE" {
                state.aborted += 1;
                state.parts.clear();
                return HttpResponse::Ok().finish();
            }
            if method == "POST" {
                let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let mut object = Vec::new();
                for part in payload["parts"].as_array().unwrap() {
                    let number = i32::try_from(part["partNumber"].as_i64().unwrap()).unwrap();
                    assert_eq!(part["etag"], format!("part-{number}"));
                    object.extend_from_slice(&state.parts[&number]);
                }
                state.objects.insert(key.clone(), object);
                state.completed += 1;
                state.upload_completed = true;
                return HttpResponse::Ok().json(json!({"key":key,"hash":"native-hash"}));
            }
        }
        if path == "/" && method == "POST" {
            let raw = String::from_utf8_lossy(&body);
            let field = |name: &str| {
                let start = raw.find(&format!("name=\"{name}\"")).unwrap();
                let content = &raw[start..];
                let offset = content.find("\r\n\r\n").unwrap() + 4;
                content[offset..].split("\r\n").next().unwrap().to_string()
            };
            let key = field("key");
            let token = field("token");
            assert!(token.starts_with("MY_ACCESS_KEY:"));
            let start = raw.find("name=\"file\"").unwrap();
            let offset = start + raw[start..].find("\r\n\r\n").unwrap() + 4;
            let tail = &body[offset..];
            let end = tail.windows(4).position(|v| v == b"\r\n--").unwrap();
            state.objects.insert(key.clone(), tail[..end].to_vec());
            return HttpResponse::Ok().json(json!({"key":key,"hash":"native-hash"}));
        }
        if path.starts_with("/stat/")
            || path.starts_with("/delete/")
            || path.starts_with("/copy/")
            || path == "/list"
        {
            if state.malformed_json {
                return HttpResponse::Ok().body("not-json MY_SECRET_KEY");
            }
            assert!(
                req.headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .starts_with("Qiniu MY_ACCESS_KEY:")
            );
            assert!(req.headers().contains_key("x-qiniu-date"));
            let decode = |entry: &str| {
                let raw = String::from_utf8(URL_SAFE.decode(entry).unwrap()).unwrap();
                raw.strip_prefix("archive:").unwrap().to_string()
            };
            if path == "/list" {
                let keys = state
                    .objects
                    .keys()
                    .filter(|k| k.starts_with(query.get("prefix").unwrap()))
                    .collect::<Vec<_>>();
                let marker = query
                    .get("marker")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let limit = state.page_size.unwrap_or(1000);
                let items = keys
                    .iter()
                    .skip(marker)
                    .take(limit)
                    .map(|key| json!({"key":key}))
                    .collect::<Vec<_>>();
                let next = if state.repeat_marker {
                    marker.max(1).to_string()
                } else if keys.len() > marker + limit {
                    (marker + limit).to_string()
                } else {
                    String::new()
                };
                return HttpResponse::Ok().json(json!({"items":items,"marker":next}));
            }
            let key = decode(path.split('/').nth(2).unwrap());
            if path.starts_with("/delete/") {
                state.objects.remove(&key);
                return HttpResponse::Ok().finish();
            }
            if path.starts_with("/copy/") {
                let dest = decode(path.split('/').nth(3).unwrap());
                let bytes = state.objects[&key].clone();
                state.objects.insert(dest, bytes);
                return HttpResponse::Ok().finish();
            }
            return match state.objects.get(&key) {
                Some(bytes) => HttpResponse::Ok()
                    .json(json!({"fsize":bytes.len(),"mimeType":"application/octet-stream"})),
                None => HttpResponse::build(actix_web::http::StatusCode::from_u16(612).unwrap())
                    .finish(),
            };
        }
        let key = percent_encoding::percent_decode_str(path.trim_start_matches('/'))
            .decode_utf8()
            .unwrap()
            .to_string();
        if state.max_source_ttl.is_some_and(|limit| {
            query
                .get("e")
                .and_then(|e| e.parse::<i64>().ok())
                .is_some_and(|deadline| deadline - chrono::Utc::now().timestamp() > limit)
        }) {
            return HttpResponse::Forbidden().json(json!({"error":"The difference between the request expires and the server's time is too large"}));
        }
        let Some(bytes) = state.objects.get(&key) else {
            return HttpResponse::NotFound().finish();
        };
        assert!(query.contains_key("e") && query.contains_key("token"));
        if let Some(range) = req.headers().get("range") {
            if state.ignore_range {
                return HttpResponse::Ok().body(bytes.clone());
            }
            let range = range.to_str().unwrap().strip_prefix("bytes=").unwrap();
            let (start, end) = range.split_once('-').unwrap();
            let start: usize = start.parse().unwrap();
            let end: usize = if end.is_empty() {
                bytes.len() - 1
            } else {
                end.parse().unwrap()
            };
            return HttpResponse::PartialContent()
                .insert_header((
                    "content-range",
                    if state.invalid_range {
                        "bytes 0-invalid/1".to_string()
                    } else {
                        format!("bytes {start}-{end}/{}", bytes.len())
                    },
                ))
                .body(bytes[start..=end].to_vec());
        }
        HttpResponse::Ok().body(bytes.clone())
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let state = Arc::new(std::sync::Mutex::new(State::default()));
    let data = state.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(data.clone()))
            .app_data(web::PayloadConfig::new(16 * 1024 * 1024))
            .default_service(web::to(handler))
    })
    .workers(1)
    .listen(listener)
    .unwrap()
    .run();
    let handle = server.handle();
    tokio::spawn(server);
    Server {
        base: format!("http://{address}"),
        state,
        handle,
    }
}

#[tokio::test]
async fn native_object_io_range_copy_list_and_form_streaming() {
    let server = spawn().await;
    let driver = server.driver().await;
    driver.put("hello.txt", b"hello Kodo").await.unwrap();
    assert_eq!(driver.metadata("hello.txt").await.unwrap().size, 10);
    assert!(driver.exists("hello.txt").await.unwrap());
    assert!(!driver.exists("missing").await.unwrap());
    assert_eq!(driver.get("hello.txt").await.unwrap(), b"hello Kodo");
    let mut range = driver.get_range("hello.txt", 6, Some(4)).await.unwrap();
    let mut bytes = Vec::new();
    range.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"Kodo");
    driver.copy_object("hello.txt", "copy.txt").await.unwrap();
    assert_eq!(
        driver
            .extensions()
            .list
            .unwrap()
            .list_paths(None)
            .await
            .unwrap(),
        vec!["copy.txt", "hello.txt"]
    );
    driver
        .put_reader(
            "stream",
            Box::new(std::io::Cursor::new(b"streaming".to_vec())),
            9,
        )
        .await
        .unwrap();
    assert_eq!(driver.get("stream").await.unwrap(), b"streaming");
    driver.delete("hello.txt").await.unwrap();
    assert!(!driver.exists("hello.txt").await.unwrap());
    assert!(
        !server
            .state
            .lock()
            .unwrap()
            .requests
            .iter()
            .any(|(_, p)| p.contains("X-Amz"))
    );
    server.stop().await;
}

#[tokio::test]
async fn internal_stream_head_and_thumbnail_range_respect_native_origin_expiry_window() {
    let server = spawn().await;
    let driver = server.driver().await;
    driver.put("image.png", b"source-image").await.unwrap();
    driver
        .put("_thumb/test.webp", b"cached-thumbnail")
        .await
        .unwrap();
    server.state.lock().unwrap().max_source_ttl = Some(180);
    let mut stream = driver
        .get_stream("image.png")
        .await
        .expect("server relay must use a short-lived source URL");
    let mut body = Vec::new();
    stream.read_to_end(&mut body).await.unwrap();
    assert_eq!(body, b"source-image");
    assert_eq!(driver.head_metadata("image.png").await.unwrap().size, 12);
    let mut range = driver
        .get_range("_thumb/test.webp", 0, Some(5))
        .await
        .unwrap();
    let mut body = Vec::new();
    range.read_to_end(&mut body).await.unwrap();
    assert_eq!(body, b"cache");
    assert_eq!(
        driver.get("_thumb/test.webp").await.unwrap(),
        b"cached-thumbnail"
    );
    assert!(
        matches!(driver.get_range("_thumb/missing.webp",0,Some(5)).await,Err(error) if error.kind()==StorageErrorKind::NotFound)
    );
    server.stop().await;
}

#[tokio::test]
async fn default_native_origin_direct_link_clamps_actual_expiry_without_changing_custom_delivery() {
    let mut driver = driver();
    driver.config.download_origin = "https://bucket.kodo-cn-south-1.qiniucs.com".into();
    let before = chrono::Utc::now().timestamp();
    let request = driver
        .resolve_download_url("image.png", Duration::from_secs(3600), Default::default())
        .await
        .unwrap()
        .unwrap();
    let expiry = request.expires_at.unwrap().timestamp();
    assert!((before + 179..=before + 181).contains(&expiry));
    let url = Url::parse(&request.url).unwrap();
    assert_eq!(
        url.query_pairs()
            .find(|(k, _)| k == "e")
            .unwrap()
            .1
            .parse::<i64>()
            .unwrap(),
        expiry
    );
    driver.config.download_origin = "https://origin.example.test".into();
    driver.config.download_auth_validity_seconds = 3600;
    let request = driver
        .resolve_download_url("image.png", Duration::from_secs(3600), Default::default())
        .await
        .unwrap()
        .unwrap();
    assert!(request.expires_at.unwrap().timestamp() >= before + 3600);
    driver.config.download_origin = "https://bucket.kodo-cn-east-1.qiniucs.com".into();
    let short = driver
        .resolve_download_url("image.png", Duration::from_secs(60), Default::default())
        .await
        .unwrap()
        .unwrap();
    assert!(short.expires_at.unwrap().timestamp() <= chrono::Utc::now().timestamp() + 60);
    driver.config.download_delivery = QiniuDownloadDelivery::QiniuCdn;
    driver.config.download_base_url = Some("https://cdn.example.test".into());
    let cdn = driver
        .resolve_download_url("image.png", Duration::from_secs(3600), Default::default())
        .await
        .unwrap()
        .unwrap();
    assert!(cdn.expires_at.unwrap().timestamp() >= before + 3600);
}

#[tokio::test]
async fn configured_download_validity_bounds_origin_cdn_and_internal_reads() {
    for validity in [1, 60, 180, 300, 600, 3600] {
        for (domain, provider_limit) in [
            ("https://bucket.kodo-cn-south-1.qiniucs.com", 300),
            ("https://origin.example.test", 3600),
        ] {
            let mut driver = driver();
            driver.config.download_auth_validity_seconds = validity;
            driver.config.download_origin = domain.into();
            for requested in [1, 60, 180, 300, 600, 3600, 604800] {
                let now = chrono::Utc::now().timestamp();
                let request = driver
                    .resolve_download_url(
                        "file",
                        Duration::from_secs(requested),
                        Default::default(),
                    )
                    .await
                    .unwrap()
                    .unwrap();
                let expected = requested.min(u64::from(validity)).min(provider_limit);
                let seconds = request.expires_at.unwrap().timestamp() - now;
                assert!(
                    (i64::try_from(expected).unwrap()..=i64::try_from(expected).unwrap() + 1)
                        .contains(&seconds)
                );
            }
            let now = chrono::Utc::now().timestamp();
            let expected = i64::from(validity).min(i64::try_from(provider_limit).unwrap());
            assert!(
                (now + expected..=now + expected + 1)
                    .contains(&driver.source_read_deadline().unwrap())
            );
            driver.config.download_delivery = QiniuDownloadDelivery::QiniuCdn;
            driver.config.download_base_url = Some("https://cdn.example.test".into());
            let request = driver
                .resolve_download_url("file", Duration::from_secs(3600), Default::default())
                .await
                .unwrap()
                .unwrap();
            assert!(request.expires_at.unwrap().timestamp() >= now + i64::from(validity));
        }
    }
    for validity in [0, 3601, u32::MAX] {
        let mut invalid = config();
        invalid.download_auth_validity_seconds = validity;
        assert!(QiniuDriver::validate_config(&invalid, &driver().credentials).is_err());
    }
}

#[tokio::test]
async fn provider_expiry_window_error_is_not_reported_as_account_permission_failure() {
    let server = spawn().await;
    let driver = server.driver().await;
    let response = driver
        .client
        .get(format!("{}/expiry-window", server.base))
        .send()
        .await
        .unwrap();
    let error = http::checked(response).await.err().unwrap();
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(
        error
            .message()
            .contains("expiry exceeds the provider time window")
    );
    assert!(error.message().contains("safe-expiry-request-id"));
    server.stop().await;
}

#[tokio::test]
async fn native_resumable_parts_recover_complete_validate_and_abort() {
    let server = spawn().await;
    let driver = server.driver().await;
    assert!(
        !driver
            .provider_resumable_upload_capabilities()
            .implicit_completion
    );
    assert!(driver.numbered_parts().is_some());
    let id = driver.create_multipart_upload("large").await.unwrap();
    let native_request = driver
        .browser_part_request("large", &id, 1, Duration::from_secs(60))
        .await
        .unwrap();
    let authorization = native_request.authorization.unwrap();
    let policy: serde_json::Value = serde_json::from_slice(
        &URL_SAFE
            .decode(authorization.token.split(':').nth(2).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(policy["scope"], "archive:tenant/large");
    assert_eq!(
        policy["insertOnly"], 1,
        "browser capability must not overwrite an already published object"
    );
    assert!(!authorization.token.contains("MY_SECRET_KEY"));
    let data = vec![b'a'; 1024 * 1024];
    let etag = driver
        .upload_multipart_part("large", &id, 1, &data)
        .await
        .unwrap();
    driver
        .upload_multipart_part_reader(
            "large",
            &id,
            2,
            Box::new(std::io::Cursor::new(b"end".to_vec())),
            3,
        )
        .await
        .unwrap();
    assert_eq!(
        driver
            .list_uploaded_part_details("large", &id)
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(
        driver
            .complete_multipart_upload("large", &id, vec![(1, "wrong".into()), (2, "".into())])
            .await
            .is_err()
    );
    driver
        .complete_multipart_upload("large", &id, vec![(1, etag), (2, "".into())])
        .await
        .unwrap();
    assert_eq!(
        driver.metadata("large").await.unwrap().size,
        1024 * 1024 + 3
    );
    assert_eq!(server.state.lock().unwrap().completed, 1);
    driver
        .complete_upload_session(&id, 1024 * 1024 + 3)
        .await
        .unwrap();
    assert_eq!(
        server.state.lock().unwrap().completed,
        1,
        "lost completion responses must not merge the object twice"
    );
    assert!(
        driver
            .browser_part_request("other-key", &id, 1, Duration::from_secs(60))
            .await
            .is_err()
    );
    assert!(
        driver
            .browser_part_request("large", &id, 0, Duration::from_secs(60))
            .await
            .is_err()
    );
    driver.abort_multipart_upload("large", &id).await.unwrap();
    assert_eq!(server.state.lock().unwrap().aborted, 1);
    server.stop().await;
}

#[tokio::test]
async fn form_upload_rejects_short_oversized_and_nonempty_zero_length_streams() {
    let server = spawn().await;
    let driver = server.driver().await;
    for (name, data, size) in [
        ("short-form", b"ab".as_slice(), 3),
        ("long-form", b"abcd".as_slice(), 3),
        ("zero-form", b"x".as_slice(), 0),
    ] {
        let result = driver
            .put_reader(name, Box::new(std::io::Cursor::new(data.to_vec())), size)
            .await;
        assert!(result.is_err());
        assert!(!driver.exists(name).await.unwrap());
    }
    driver
        .put_reader("empty", Box::new(std::io::Cursor::new(Vec::<u8>::new())), 0)
        .await
        .unwrap();
    assert_eq!(driver.metadata("empty").await.unwrap().size, 0);
    server.stop().await;
}

#[tokio::test]
async fn native_listing_and_part_recovery_paginate_and_reject_stalled_cursors() {
    let server = spawn().await;
    let driver = server.driver().await;
    {
        let mut state = server.state.lock().unwrap();
        state.page_size = Some(2);
        for i in 0..5 {
            state.objects.insert(format!("tenant/object-{i}"), vec![0]);
        }
        state
            .objects
            .insert("tenant-neighbor/must-not-leak".into(), vec![0]);
    }
    assert_eq!(
        driver
            .extensions()
            .list
            .unwrap()
            .list_paths(None)
            .await
            .unwrap()
            .len(),
        5
    );
    let id = driver.create_multipart_upload("parts").await.unwrap();
    for number in 1..=5 {
        driver
            .upload_multipart_part("parts", &id, number, b"part")
            .await
            .unwrap();
    }
    assert_eq!(
        driver.list_uploaded_parts("parts", &id).await.unwrap(),
        vec![1, 2, 3, 4, 5]
    );
    server.state.lock().unwrap().repeat_marker = true;
    assert!(
        driver
            .extensions()
            .list
            .unwrap()
            .list_paths(None)
            .await
            .is_err()
    );
    assert!(driver.list_uploaded_parts("parts", &id).await.is_err());
    server.stop().await;
}

#[tokio::test]
async fn errors_keep_provider_status_and_request_id_without_secret_echoes() {
    let server = spawn().await;
    let driver = server.driver().await;
    for (code, kind) in [
        (401, StorageErrorKind::Auth),
        (403, StorageErrorKind::Permission),
        (612, StorageErrorKind::NotFound),
        (429, StorageErrorKind::RateLimited),
        (503, StorageErrorKind::Transient),
    ] {
        let response = driver
            .client
            .get(format!("{}/error/{code}", server.base))
            .send()
            .await
            .unwrap();
        let error = http::checked(response).await.err().unwrap();
        assert_eq!(error.kind(), kind);
        assert!(error.message().contains("safe-request-id"));
        assert!(!error.message().contains("MY_SECRET_KEY"));
        assert!(!error.message().contains("token="));
    }
    server.stop().await;
}

#[tokio::test]
async fn discovers_region_once_and_never_falls_back_to_s3() {
    let server = spawn().await;
    let mut driver = driver();
    driver.region_query = Url::parse(&format!("{}/v4/query", server.base)).unwrap();
    let upload = driver.endpoint(http::ServiceKind::Upload).await.unwrap();
    assert_eq!(upload.as_str(), "https://upload.example.test/");
    assert_eq!(
        driver
            .endpoint(http::ServiceKind::Management)
            .await
            .unwrap()
            .host_str(),
        Some("rs.example.test")
    );
    assert_eq!(server.state.lock().unwrap().requests.len(), 1);
    server.stop().await;
}

#[tokio::test]
async fn rejects_malformed_provider_bodies_and_ignored_range() {
    let server = spawn().await;
    let driver = server.driver().await;
    driver.put("object", b"test").await.unwrap();
    server.state.lock().unwrap().ignore_range = true;
    assert!(driver.get_range("object", 1, Some(2)).await.is_err());
    server.state.lock().unwrap().ignore_range = false;
    server.state.lock().unwrap().invalid_range = true;
    assert!(driver.get_range("object", 1, Some(2)).await.is_err());
    server.state.lock().unwrap().malformed_json = true;
    let error = driver.metadata("object").await.unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Precondition);
    assert!(!error.message().contains("MY_SECRET_KEY"));
    server.stop().await;
}

#[tokio::test]
async fn multipart_rejects_duplicate_missing_too_small_and_expired_parts() {
    let server = spawn().await;
    let driver = server.driver().await;
    let id = driver.create_multipart_upload("object").await.unwrap();
    driver
        .upload_multipart_part("object", &id, 1, b"small")
        .await
        .unwrap();
    driver
        .upload_multipart_part("object", &id, 2, b"last")
        .await
        .unwrap();
    for parts in [
        vec![(1, "".into()), (1, "".into())],
        vec![(1, "".into()), (2, "".into())],
        vec![(3, "".into())],
        vec![],
    ] {
        assert!(
            driver
                .complete_multipart_upload("object", &id, parts)
                .await
                .is_err()
        );
    }
    let mut expired: serde_json::Value = serde_json::from_str(&id).unwrap();
    expired["expires_at"] = json!(0);
    assert!(
        driver
            .list_uploaded_parts("object", &expired.to_string())
            .await
            .is_err()
    );
    assert!(
        driver
            .abort_multipart_upload("object", &expired.to_string())
            .await
            .is_ok()
    );
    assert!(
        driver
            .browser_part_request("object", &id, 10001, Duration::from_secs(60))
            .await
            .is_err()
    );
    assert!(
        driver
            .browser_part_request("object", &id, 1, Duration::ZERO)
            .await
            .is_err()
    );
    assert_eq!(server.state.lock().unwrap().completed, 0);
    server.stop().await;
}

#[tokio::test]
async fn streaming_multipart_uses_small_read_buffers_and_cleans_failed_or_canceled_sessions() {
    use std::{
        pin::Pin,
        sync::atomic::{AtomicUsize, Ordering},
        task::{Context, Poll},
    };
    struct Counted<R> {
        source: R,
        largest: Arc<AtomicUsize>,
    }
    impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Counted<R> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            self.largest.fetch_max(buffer.remaining(), Ordering::SeqCst);
            Pin::new(&mut self.source).poll_read(cx, buffer)
        }
    }
    let server = spawn().await;
    let driver = server.driver().await;
    let size = 9 * 1024 * 1024;
    let largest = Arc::new(AtomicUsize::new(0));
    let reader = Counted {
        source: tokio::io::repeat(b'x').take(size),
        largest: largest.clone(),
    };
    driver
        .put_multipart_reader("bounded", Box::new(reader), size)
        .await
        .unwrap();
    assert!(
        largest.load(Ordering::SeqCst) <= 64 * 1024,
        "logical 8 MiB parts must not become 8 MiB read buffers"
    );
    assert_eq!(driver.metadata("bounded").await.unwrap().size, size);
    assert!(
        driver
            .put_multipart_reader("short", Box::new(std::io::Cursor::new(b"ab".to_vec())), 3)
            .await
            .is_err()
    );
    assert!(
        driver
            .put_multipart_reader("long", Box::new(std::io::Cursor::new(b"abcd".to_vec())), 3)
            .await
            .is_err()
    );
    assert!(server.state.lock().unwrap().aborted >= 2);
    struct PendingInput(Arc<tokio::sync::Notify>);
    impl tokio::io::AsyncRead for PendingInput {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            self.0.notify_one();
            Poll::Pending
        }
    }
    let started = Arc::new(tokio::sync::Notify::new());
    let pending = PendingInput(started.clone());
    let cloned = driver.clone();
    let task = tokio::spawn(async move {
        cloned
            .put_multipart_reader("canceled", Box::new(pending), 3)
            .await
    });
    // Wait for actual request-body polling: a fixed sleep can cancel before the
    // provider returns its uploadId on a loaded host, when abort is not yet possible.
    tokio::time::timeout(Duration::from_secs(10), started.notified())
        .await
        .unwrap();
    task.abort();
    let _ = task.await;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if server.state.lock().unwrap().aborted >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!driver.exists("short").await.unwrap());
    assert!(!driver.exists("long").await.unwrap());
    server.stop().await;
}
