use super::*;
use crate::storage::drivers::azure_blob::{
    AZURE_STORAGE_VERSION, AzureBlobDriverConfig, AzureBlobStaticCredentials,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

fn driver(endpoint: &str) -> AzureBlobDriver {
    AzureBlobDriver::new(
        AzureBlobDriverConfig {
            endpoint: endpoint.into(),
            container: "container-not-service".into(),
            base_path: "prefix-not-service".into(),
            chunk_size: 0,
        },
        AzureBlobStaticCredentials {
            account_name: "account".into(),
            account_key: "c2VjcmV0".into(),
        },
    )
    .unwrap()
}

fn origins() -> Vec<String> {
    vec!["https://drive.example.test".into()]
}

#[test]
fn shared_key_signatures_match_independent_python_vectors() {
    let date = "Tue, 03 Nov 2026 12:00:00 GMT";
    let key = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        b"azure-cors-vector-key",
    );
    for (account, endpoint, method, content_type, length, expected) in [
        (
            "account",
            "https://account.blob.core.windows.net/?restype=service&comp=properties",
            "GET",
            "",
            0,
            "CUlqhIyC11SB28f0Ar4aSrgZQa69Mo1gKDprc4MPW7c=",
        ),
        (
            "account",
            "https://account.blob.core.windows.net/?restype=service&comp=properties",
            "PUT",
            "application/xml",
            256,
            "SO7oBq0IbfHSQ/Kb06c92DHuilgfA7VipotL3TfTXYI=",
        ),
        (
            "devstoreaccount1",
            "http://localhost:10000/devstoreaccount1/?restype=service&comp=properties",
            "GET",
            "",
            0,
            "+B7/Y5SUi9+LVLlH9M5yC9d75Il76rs3PNgEy516iRU=",
        ),
    ] {
        let mut storage = driver(endpoint);
        storage.account_name = account.to_owned();
        storage.account_key = key.clone();
        let mut request = reqwest::Client::new().request(method.parse().unwrap(), endpoint);
        request = match length {
            0 => request,
            length => request
                .header("content-type", content_type)
                .body(vec![b'x'; length]),
        };
        let mut request = request.build().unwrap();
        storage
            .sign_blob_service_properties_request(&mut request, date)
            .unwrap();
        assert_eq!(
            request.headers()["authorization"],
            format!("SharedKey {account}:{expected}")
        );
        assert_eq!(request.headers()["x-ms-date"], date);
        assert_eq!(request.headers()["x-ms-version"], AZURE_STORAGE_VERSION);
    }
}

#[test]
fn management_target_rejects_cdn_container_sas_and_wrong_account() {
    for endpoint in [
        "https://cdn.example.test",
        "https://other.blob.core.windows.net",
        "https://account.blob.core.windows.net/container",
        "https://account.blob.core.windows.net/?sig=secret",
        "https://account.blob.core.windows.net/#fragment",
        "http://account.blob.core.windows.net",
        "https://account.blob.core.windows.net:8443",
        "https://user:secret@account.blob.core.windows.net",
    ] {
        assert!(
            driver(endpoint).blob_service_properties_url().is_err(),
            "{endpoint}"
        );
    }
    for suffix in [
        "windows.net",
        "chinacloudapi.cn",
        "usgovcloudapi.net",
        "cloudapi.de",
    ] {
        let endpoint = format!("https://account.blob.core.{suffix}");
        let url = driver(&endpoint).blob_service_properties_url().unwrap();
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), Some("restype=service&comp=properties"));
    }
    assert_eq!(
        driver("http://127.0.0.1:10000/account")
            .blob_service_properties_url()
            .unwrap()
            .path(),
        "/account/"
    );
}

#[test]
fn xml_updates_only_cors_and_preserves_rules_and_order() {
    let mut unrelated = asterdrive_cors_rule(&["https://other.example.test".into()]);
    unrelated
        .0
        .insert("AllowedHeaders".into(), "x-custom,content-type".into());
    let rules = vec![unrelated, asterdrive_cors_rule(&origins())];
    let xml = build_cors_configuration_xml(&rules).unwrap();
    assert_eq!(parse_cors_configuration(&xml).unwrap(), rules);
    for property in [
        "Logging",
        "HourMetrics",
        "MinuteMetrics",
        "DefaultServiceVersion",
        "DeleteRetentionPolicy",
        "StaticWebsite",
    ] {
        assert!(!xml.contains(property));
    }
    let properties = xml.replace("<Cors>", "<Logging><Version>1.0</Version></Logging><FutureProperty>retained by omission</FutureProperty><Cors>");
    assert_eq!(parse_cors_configuration(&properties).unwrap(), rules);
    let escaped = asterdrive_cors_rule(&["https://example.test/?a=1&b=2".into()]);
    assert_eq!(
        parse_cors_configuration(
            &build_cors_configuration_xml(std::slice::from_ref(&escaped)).unwrap()
        )
        .unwrap(),
        vec![escaped]
    );
}

#[test]
fn merging_is_idempotent_without_claiming_other_rules() {
    let desired = asterdrive_cors_rule(&origins());
    assert!(needs_asterdrive_cors_rule(&[], &desired).unwrap());
    let unrelated = asterdrive_cors_rule(&["https://another.example.test".into()]);
    assert!(needs_asterdrive_cors_rule(std::slice::from_ref(&unrelated), &desired).unwrap());
    assert!(!needs_asterdrive_cors_rule(&[unrelated, desired.clone()], &desired).unwrap());
    let mut reordered = desired.clone();
    reordered
        .0
        .insert("AllowedMethods".into(), "HEAD, PUT, GET".into());
    reordered.0.insert(
        "AllowedHeaders".into(),
        "X-MS-VERSION, RANGE, CONTENT-TYPE, X-MS-BLOB-TYPE".into(),
    );
    assert!(!needs_asterdrive_cors_rule(&[reordered], &desired).unwrap());
    for origin in ["*", "https://*.example.test", "https://drive.example.test"] {
        let mut conflicting = asterdrive_cors_rule(&[origin.into()]);
        conflicting
            .0
            .insert("AllowedHeaders".into(), "content-type".into());
        assert!(needs_asterdrive_cors_rule(&[conflicting, desired.clone()], &desired).is_err());
    }
}

#[test]
fn parsing_rejects_unknown_duplicate_and_malformed_cors_without_dropping_them() {
    let xml = build_cors_configuration_xml(&[asterdrive_cors_rule(&origins())]).unwrap();
    for invalid_xml in [
        xml.replace("</CorsRule>", "<FutureField>keep</FutureField></CorsRule>"),
        xml.replace("<CorsRule>", "<CorsRule extra=\"keep\">"),
        xml.replace("<AllowedMethods>", "<AllowedMethods extra=\"keep\">"),
        xml.replace("</AllowedMethods>", "</AllowedMethods><AllowedMethods>GET</AllowedMethods>"),
        xml.replace("600", "invalid"),
        xml.replace("<CorsRule>", "<CorsRule><Unknown/>"),
        xml.replace("</Cors>", "</Cors><Cors/>"),
        "<Error><Code>Denied</Code></Error>".into(),
        "<StorageServiceProperties/>".into(),
        "<!DOCTYPE x [<!ENTITY x 'boom'>]><StorageServiceProperties><Cors>&x;</Cors></StorageServiceProperties>".into(),
    ] { assert!(parse_cors_configuration(&invalid_xml).is_err(), "{invalid_xml}"); }
    assert!(
        parse_cors_configuration("<StorageServiceProperties><Cors/></StorageServiceProperties>")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn rule_count_size_and_entry_boundaries_are_checked_before_put() {
    let rule = asterdrive_cors_rule(&origins());
    assert!(build_cors_configuration_xml(&vec![rule.clone(); 5]).is_ok());
    assert!(build_cors_configuration_xml(&vec![rule.clone(); 6]).is_err());
    let mut boundary = rule.clone();
    boundary
        .0
        .insert("AllowedOrigins".into(), vec!["a".repeat(200); 8].join(","));
    let other_size: usize = boundary
        .0
        .iter()
        .filter(|(key, _)| key.as_str() != "AllowedHeaders")
        .map(|(_, value)| value.len())
        .sum();
    let remaining = 2048 - other_size;
    let headers = (0..remaining).map(|_| "a").collect::<String>();
    let chunks: Vec<_> = headers
        .as_bytes()
        .chunks(200)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();
    boundary.0.insert("AllowedHeaders".into(), chunks.join(","));
    while boundary.0.values().map(String::len).sum::<usize>() > 2048 {
        boundary.0.get_mut("AllowedHeaders").unwrap().pop();
    }
    assert!(validate_cors_configuration_limits(&[boundary.clone()]).is_ok());
    boundary.0.get_mut("AllowedHeaders").unwrap().push('a');
    assert!(validate_cors_configuration_limits(&[boundary]).is_err());
    let mut oversized = rule;
    oversized.0.insert("AllowedOrigins".into(), "a".repeat(257));
    assert!(build_cors_configuration_xml(&[oversized]).is_err());
}

#[test]
fn provider_error_does_not_expose_messages_or_signatures() {
    for status in [
        StatusCode::FORBIDDEN,
        StatusCode::BAD_REQUEST,
        StatusCode::INTERNAL_SERVER_ERROR,
    ] {
        let error = azure_cors_response_error(
            status,
            "<Error><Code>KeyBasedAuthenticationNotPermitted</Code><Message>secret-key sig=secret</Message></Error>",
            Some("request-123"),
        );
        assert!(
            error
                .message()
                .contains("KeyBasedAuthenticationNotPermitted")
        );
        assert!(error.message().contains("request-123"));
        assert!(!error.message().contains("secret"));
        if status == StatusCode::FORBIDDEN {
            assert!(error.message().contains("Shared Key"));
        }
    }
}

#[test]
fn management_and_object_requests_share_azure_provider_error_classification() {
    for (status, code, expected) in [
        (
            StatusCode::FORBIDDEN,
            "AuthenticationFailed",
            StorageErrorKind::Auth,
        ),
        (
            StatusCode::FORBIDDEN,
            "AuthorizationFailure",
            StorageErrorKind::Permission,
        ),
        (
            StatusCode::FORBIDDEN,
            "KeyBasedAuthenticationNotPermitted",
            StorageErrorKind::Permission,
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "FutureServiceFailure",
            StorageErrorKind::Transient,
        ),
    ] {
        let sdk_error = azure_core::Error::with_message(
            azure_core::error::ErrorKind::HttpResponse {
                status: status.as_u16().into(),
                error_code: Some(code.to_owned()),
                raw_response: None,
            },
            "provider request failed",
        );
        let cors_error =
            azure_cors_response_error(status, &format!("<Error><Code>{code}</Code></Error>"), None);
        assert_eq!(cors_error.kind(), expected);
        assert_eq!(AzureBlobDriver::classify_azure_error(&sdk_error), expected);
    }
}

struct RecordedRequest {
    method: String,
    uri: String,
    headers: BTreeMap<String, String>,
    body: String,
}

async fn mock_service(
    responses: Vec<(u16, Vec<u8>)>,
) -> (String, tokio::task::JoinHandle<Vec<RecordedRequest>>) {
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/account", listener.local_addr().unwrap());
    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let result = recorded.clone();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let finished = Arc::new(Mutex::new(Some(finished_tx)));
    let (handle_tx, handle_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        actix_web::rt::System::new().block_on(async move {
            let server = HttpServer::new(move || {
                let responses = responses.clone();
                let recorded = recorded.clone();
                let finished = finished.clone();
                App::new().default_service(web::to(
                    move |request: HttpRequest, body: web::Bytes| {
                        let responses = responses.clone();
                        let recorded = recorded.clone();
                        let finished = finished.clone();
                        async move {
                            recorded.lock().unwrap().push(RecordedRequest {
                                method: request.method().to_string(),
                                uri: request.uri().to_string(),
                                headers: request
                                    .headers()
                                    .iter()
                                    .map(|(name, value)| {
                                        (name.to_string(), value.to_str().unwrap().to_owned())
                                    })
                                    .collect(),
                                body: String::from_utf8(body.to_vec()).unwrap(),
                            });
                            let mut responses = responses.lock().unwrap();
                            let (status, body) =
                                responses.pop_front().expect("unexpected extra request");
                            if responses.is_empty() {
                                finished.lock().unwrap().take().unwrap().send(()).unwrap();
                            }
                            HttpResponse::build(
                                actix_web::http::StatusCode::from_u16(status).unwrap(),
                            )
                            .insert_header(("x-ms-request-id", "request-123"))
                            .body(body)
                        }
                    },
                ))
            })
            .workers(1)
            .listen(listener)
            .unwrap()
            .run();
            handle_tx.send(server.handle()).unwrap();
            server.await.unwrap();
        });
    });
    let handle = handle_rx.recv().unwrap();
    let task = tokio::spawn(async move {
        finished_rx.await.unwrap();
        handle.stop(true).await;
        tokio::task::spawn_blocking(move || thread.join().unwrap())
            .await
            .unwrap();
        std::mem::take(&mut *result.lock().unwrap())
    });
    (endpoint, task)
}

#[tokio::test]
async fn request_contract_reads_merges_writes_only_cors_and_verifies() {
    let unrelated = asterdrive_cors_rule(&["https://other.example.test".into()]);
    let existing = build_cors_configuration_xml(std::slice::from_ref(&unrelated)).unwrap();
    let updated =
        build_cors_configuration_xml(&[unrelated, asterdrive_cors_rule(&origins())]).unwrap();
    let (endpoint, task) = mock_service(vec![
        (200, existing.into_bytes()),
        (202, Vec::new()),
        (200, updated.clone().into_bytes()),
    ])
    .await;
    let result = driver(&endpoint)
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap();
    assert!(result.changed);
    assert_eq!(result.preserved_rule_count, 1);
    assert_eq!(result.request_id.as_deref(), Some("request-123"));
    let requests = task.await.unwrap();
    for (index, request) in requests.iter().enumerate() {
        let method = if index == 1 { "PUT" } else { "GET" };
        assert_eq!(request.method, method);
        assert_eq!(request.uri, "/account/?restype=service&comp=properties");
        assert!(!request.uri.contains("container-not-service"));
        assert!(!request.uri.contains("prefix-not-service"));
        let body = &request.body;
        let headers = &request.headers;
        assert_eq!(headers["x-ms-version"], AZURE_STORAGE_VERSION);
        let mut signed_request = reqwest::Client::new()
            .request(
                method.parse().unwrap(),
                driver(&endpoint).blob_service_properties_url().unwrap(),
            )
            .body(body.clone());
        signed_request = match headers.get("content-type") {
            Some(content_type) => signed_request.header("content-type", content_type),
            None => signed_request,
        };
        let mut signed_request = signed_request.build().unwrap();
        driver(&endpoint)
            .sign_blob_service_properties_request(&mut signed_request, &headers["x-ms-date"])
            .unwrap();
        assert_eq!(
            headers["authorization"],
            signed_request.headers()["authorization"].to_str().unwrap()
        );
        if index == 1 {
            assert_eq!(body, &updated);
        }
    }
}

#[tokio::test]
async fn equivalent_rules_skip_put_and_failed_reads_never_write() {
    let xml = build_cors_configuration_xml(&[asterdrive_cors_rule(&origins())]).unwrap();
    let (endpoint, task) = mock_service(vec![(200, xml.into_bytes())]).await;
    assert!(
        !driver(&endpoint)
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap()
            .changed
    );
    assert_eq!(task.await.unwrap().len(), 1);
    for (status, body) in [
        (403, "<Error><Code>AuthorizationFailure</Code></Error>"),
        (404, ""),
        (200, "broken XML"),
    ] {
        let (endpoint, task) = mock_service(vec![(status, body.as_bytes().to_vec())]).await;
        assert!(
            driver(&endpoint)
                .configure_asterdrive_cors(&origins())
                .await
                .is_err()
        );
        assert_eq!(task.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn invalid_utf8_errors_preserve_http_status_and_request_id() {
    let body = b"<Error><Code>AuthorizationFailure</Code><Message>\xff</Message></Error>".to_vec();
    let (endpoint, task) = mock_service(vec![(403, body)]).await;
    let error = driver(&endpoint)
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Permission);
    assert!(error.message().contains("403"));
    assert!(error.message().contains("AuthorizationFailure"));
    assert!(error.message().contains("request-123"));
    assert!(!error.message().contains("encoding"));
    assert_eq!(task.await.unwrap().len(), 1);

    let (endpoint, task) = mock_service(vec![(200, vec![0xff])]).await;
    let error = driver(&endpoint)
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert!(
        error
            .message()
            .contains("invalid service properties encoding")
    );
    assert_eq!(task.await.unwrap().len(), 1);
}

#[tokio::test]
async fn failed_put_and_conflicting_verification_are_reported() {
    let empty = "<StorageServiceProperties><Cors/></StorageServiceProperties>".to_owned();
    let (endpoint, task) = mock_service(vec![
        (200, empty.clone().into_bytes()),
        (
            403,
            b"<Error><Code>KeyBasedAuthenticationNotPermitted</Code></Error>".to_vec(),
        ),
    ])
    .await;
    assert!(
        driver(&endpoint)
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap_err()
            .message()
            .contains("Shared Key")
    );
    assert_eq!(task.await.unwrap().len(), 2);
    let (endpoint, task) = mock_service(vec![
        (200, empty.clone().into_bytes()),
        (202, Vec::new()),
        (200, empty.into_bytes()),
    ])
    .await;
    assert!(
        driver(&endpoint)
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap_err()
            .message()
            .contains("verification")
    );
    assert_eq!(task.await.unwrap().len(), 3);
}

#[tokio::test]
async fn failed_error_body_reads_preserve_status_and_request_id() {
    let (endpoint, task) =
        mock_service(vec![(403, vec![b'x'; AZURE_CORS_XML_MAX_BYTES + 1])]).await;
    let error = driver(&endpoint)
        .configure_asterdrive_cors(&origins())
        .await
        .unwrap_err();
    assert_eq!(task.await.unwrap().len(), 1);
    assert_eq!(error.kind(), StorageErrorKind::Permission);
    assert!(error.message().contains("403"));
    assert!(error.message().contains("request-123"));
}

#[tokio::test]
async fn failed_readback_reports_that_the_write_was_accepted() {
    for (status, body) in [
        (503, b"service unavailable".to_vec()),
        (200, b"invalid service properties".to_vec()),
    ] {
        let (endpoint, task) = mock_service(vec![
            (
                200,
                b"<StorageServiceProperties><Cors/></StorageServiceProperties>".to_vec(),
            ),
            (202, Vec::new()),
            (status, body),
        ])
        .await;
        let error = driver(&endpoint)
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap_err();
        assert_eq!(task.await.unwrap().len(), 3);
        assert!(error.message().contains("accepted the CORS write"));
        assert!(error.message().contains("request-123"));
        assert!(!error.message().contains("existing rules were not replaced"));
    }
}

#[tokio::test]
async fn readback_compares_rule_semantics_while_preserving_rule_order() {
    let unrelated = asterdrive_cors_rule(&["https://other.example.test".into()]);
    let existing = build_cors_configuration_xml(std::slice::from_ref(&unrelated)).unwrap();
    let mut desired = asterdrive_cors_rule(&origins());
    desired
        .0
        .insert("AllowedMethods".into(), "HEAD, PUT, GET".into());
    desired.0.insert(
        "AllowedHeaders".into(),
        "X-MS-VERSION, RANGE, CONTENT-TYPE, X-MS-BLOB-TYPE".into(),
    );
    desired.0.insert("MaxAgeInSeconds".into(), "0600".into());
    for (rules, success) in [
        (vec![unrelated.clone(), desired.clone()], true),
        (vec![desired, unrelated], false),
    ] {
        let (endpoint, task) = mock_service(vec![
            (200, existing.clone().into_bytes()),
            (202, Vec::new()),
            (
                200,
                build_cors_configuration_xml(&rules).unwrap().into_bytes(),
            ),
        ])
        .await;
        let result = driver(&endpoint)
            .configure_asterdrive_cors(&origins())
            .await;
        assert_eq!(task.await.unwrap().len(), 3);
        assert_eq!(result.is_ok(), success);
    }
}

#[tokio::test]
async fn azurite_cors_preserves_service_properties_and_browser_headers() {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };
    let container = GenericImage::new("mcr.microsoft.com/azure-storage/azurite", "3.35.0")
        .with_exposed_port(IntoContainerPort::tcp(10000))
        .with_wait_for(WaitFor::message_on_stdout(
            "Azurite Blob service successfully listens",
        ))
        .with_cmd(vec![
            "azurite-blob",
            "--blobHost",
            "0.0.0.0",
            "--blobPort",
            "10000",
        ])
        .start()
        .await
        .unwrap();
    let port = container
        .get_host_port_ipv4(IntoContainerPort::tcp(10000))
        .await
        .unwrap();
    let endpoint = format!("http://127.0.0.1:{port}/devstoreaccount1");
    let mut storage = driver(&endpoint);
    storage.account_name = "devstoreaccount1".into();
    storage.account_key =
        "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw=="
            .into();
    let initial = "<StorageServiceProperties><Logging><Version>1.0</Version><Delete>true</Delete><Read>true</Read><Write>false</Write><RetentionPolicy><Enabled>true</Enabled><Days>7</Days></RetentionPolicy></Logging><DefaultServiceVersion>2023-11-03</DefaultServiceVersion><Cors/></StorageServiceProperties>";
    let client = reqwest::Client::new();
    storage
        .request_blob_service_properties(&client, Method::PUT, Some(initial.into()))
        .await
        .unwrap();
    let (before, _) = storage
        .request_blob_service_properties(&client, Method::GET, None)
        .await
        .unwrap();
    let other = asterdrive_cors_rule(&["https://other.example.test".into()]);
    storage
        .request_blob_service_properties(
            &client,
            Method::PUT,
            Some(build_cors_configuration_xml(std::slice::from_ref(&other)).unwrap()),
        )
        .await
        .unwrap();
    let result = storage.configure_asterdrive_cors(&origins()).await.unwrap();
    assert!(result.changed);
    assert_eq!(result.preserved_rule_count, 1);
    assert!(
        !storage
            .configure_asterdrive_cors(&origins())
            .await
            .unwrap()
            .changed
    );
    let (after, _) = storage
        .request_blob_service_properties(&client, Method::GET, None)
        .await
        .unwrap();
    assert_eq!(
        parse_cors_configuration(&after).unwrap(),
        vec![other, asterdrive_cors_rule(&origins())]
    );
    let non_cors = |xml: &str| {
        let doc =
            BorrowedDocument::parse_with_options(xml.as_bytes(), &ParseOptions::new()).unwrap();
        doc.root()
            .child_elements()
            .filter(|e| e.name() != "Cors")
            .map(|e| (e.name().to_owned(), e.raw_xml().to_vec()))
            .collect::<Vec<_>>()
    };
    assert_eq!(non_cors(&before), non_cors(&after));
    let response = reqwest::Client::new()
        .request(
            Method::OPTIONS,
            format!("{endpoint}/any-container/any-blob"),
        )
        .header("Origin", &origins()[0])
        .header("Access-Control-Request-Method", "PUT")
        .header(
            "Access-Control-Request-Headers",
            "content-type,x-ms-blob-type",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["access-control-allow-origin"],
        origins()[0]
    );
}
