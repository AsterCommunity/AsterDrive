use super::*;
use crate::storage::drivers::huawei_obs::tests::{
    config as sample_driver_config, credentials as sample_credentials,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn selects_current_bucket_origin_and_cname_without_using_cdn() {
    let mut config = sample_driver_config("https://obs.cn-north-4.myhuaweicloud.com", false);
    let credentials = sample_credentials();
    let client = ObsCorsClient::new(&config, &credentials).unwrap();
    assert_eq!(client.bucket, "archive-bucket");
    assert_eq!(
        client.url.as_str(),
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/?cors"
    );

    config.endpoint = "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com".into();
    let client = ObsCorsClient::new(&config, &credentials).unwrap();
    assert_eq!(
        client.url.as_str(),
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/?cors"
    );

    config.cdn_domain = Some("https://cdn.example.test".into());
    config.endpoint = "https://files.example.test".into();
    config.use_cname = true;
    let client = ObsCorsClient::new(&config, &credentials).unwrap();
    assert_eq!(client.url.as_str(), "https://files.example.test/?cors");
}

fn sample_config() -> CorsConfiguration {
    CorsConfiguration {
        rules: vec![CorsRule {
            id: Some("user-rule".into()),
            origins: vec!["https://other.example".into()],
            methods: vec!["GET".into()],
            headers: vec!["Range".into()],
            expose_headers: vec!["ETag".into()],
            max_age_seconds: Some(30),
        }],
    }
}

#[test]
fn preserves_existing_rules_and_order_on_xml_roundtrip() {
    let config = sample_config();
    assert_eq!(
        parse(serialize(&config).unwrap().as_bytes()).unwrap(),
        config
    );
    let mut merged = config.clone();
    merged
        .rules
        .push(desired_rule(&["https://drive.example".into()]));
    let xml = serialize(&merged).unwrap();
    assert_eq!(parse(xml.as_bytes()).unwrap().rules[0], config.rules[0]);
    assert!(xml.find("other.example").unwrap() < xml.find("drive.example").unwrap());
}

#[test]
fn equivalent_requests_ignore_field_order_without_claiming_partial_origins() {
    let origins = vec!["https://drive.example".into()];
    let mut reordered = desired_rule(&origins);
    reordered.methods.reverse();
    reordered.expose_headers.reverse();
    assert!(!needs_rule(&[reordered.clone()], &origins).unwrap());
    reordered.origins = vec!["https://different.example".into()];
    assert!(needs_rule(&[reordered], &origins).unwrap());
}

#[test]
fn rejects_unknown_fields_attributes_and_capacity_before_write() {
    let valid = serialize(&sample_config()).unwrap();
    assert!(
        parse(
            valid
                .replace("<CORSRule>", "<CORSRule><Unknown>x</Unknown>")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        parse(
            valid
                .replace("<CORSRule>", "<CORSRule unknown=\"x\">")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        parse(
            valid
                .replace("<AllowedMethod>", "<AllowedMethod unknown=\"x\">")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        parse(
            valid
                .replace(
                    "<AllowedOrigin>",
                    "<future:AllowedOrigin xmlns:future=\"urn:unknown\">"
                )
                .replace("</AllowedOrigin>", "</future:AllowedOrigin>")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        parse(
            valid
                .replace("<CORSConfiguration>", "<CORSConfiguration>unknown")
                .as_bytes()
        )
        .is_err()
    );
    assert!(parse(&vec![b'x'; CORS_XML_MAX_BYTES + 1]).is_err());
    let mut config = sample_config();
    config.rules = vec![config.rules[0].clone(); CORS_MAX_RULES + 1];
    assert!(serialize(&config).is_err());
    config.rules.truncate(CORS_MAX_RULES);
    config.rules[0].origins = vec!["x".repeat(CORS_XML_MAX_BYTES)];
    assert!(serialize(&config).is_err());
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn get_then_put_uses_bucket_cors_endpoint_and_preserves_rules() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let mut url = Url::parse(&endpoint).unwrap();
    url.set_query(Some("cors"));
    let client = ObsCorsClient {
        client: aster_forge_http::HttpClient::builder()
            .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
            .build()
            .unwrap(),
        url,
        bucket: "sample-bucket".into(),
        use_cname: false,
        credentials: HuaweiObsStaticCredentials {
            access_key: "ak".into(),
            secret_key: "sk".into(),
            cdn_auth_key: None,
        },
    };
    let server = tokio::spawn(async move {
        let (mut get, _) = listener.accept().await.unwrap();
        let request = read_request(&mut get).await;
        assert!(request.starts_with("GET /?cors HTTP/1.1"));
        assert!(request.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("user-agent")
                    && value.trim() == crate::config::OUTBOUND_HTTP_USER_AGENT
            })
        }));
        assert!(request.to_ascii_lowercase().contains("authorization: obs "));
        let body = serialize(&sample_config()).unwrap().replace(
            "<CORSConfiguration>",
            "<CORSConfiguration xmlns=\"http://obs.cn-north-4.myhuaweicloud.com/doc/2015-06-30/\">",
        );
        get.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nx-obs-request-id: GET123\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        let (mut put, _) = listener.accept().await.unwrap();
        let request = read_request(&mut put).await;
        assert!(request.starts_with("PUT /?cors HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("content-md5:"));
        assert!(request.to_ascii_lowercase().contains("authorization: obs "));
        assert!(request.contains(crate::config::OUTBOUND_HTTP_USER_AGENT));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/xml")
        );
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let md5 = BASE64_STANDARD.encode(Md5::digest(body.as_bytes()));
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("content-md5: {}", md5.to_ascii_lowercase()))
        );
        let config = parse(body.as_bytes()).unwrap();
        assert_eq!(config.rules[0], sample_config().rules[0]);
        assert_eq!(
            config.rules[1],
            desired_rule(&["https://drive.example".into()])
        );
        put.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nx-obs-request-id: PUT123\r\nConnection: close\r\n\r\n").await.unwrap();
    });
    let result = client
        .configure(&["https://drive.example".into()])
        .await
        .unwrap();
    assert!(result.changed);
    assert_eq!(result.preserved_rule_count, 1);
    assert_eq!(result.request_id.as_deref(), Some("PUT123"));
    server.await.unwrap();
}

#[tokio::test]
async fn equivalent_rule_skips_put_and_read_failure_never_overwrites() {
    let origins = vec!["https://drive.example".into()];
    let mut shadow = desired_rule(&origins);
    shadow.methods = vec!["PUT".into()];
    shadow.headers = vec!["Content-*".into()];
    shadow.expose_headers.clear();
    for (body, status, expected_error) in [
        (
            serialize(&CorsConfiguration {
                rules: vec![desired_rule(&["https://drive.example".into()])],
            })
            .unwrap(),
            "200 OK",
            None,
        ),
        (
            "<Error><Code>AccessDenied</Code><Message>private</Message></Error>".into(),
            "403 Forbidden",
            Some("obs:bucket:GetBucketCORS"),
        ),
        (
            serialize(&CorsConfiguration {
                rules: vec![shadow, desired_rule(&origins)],
            })
            .unwrap(),
            "200 OK",
            Some("no rules were written"),
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        url.set_query(Some("cors"));
        let client = ObsCorsClient {
            client: aster_forge_http::HttpClient::builder()
                .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
                .build()
                .unwrap(),
            url,
            bucket: "sample-bucket".into(),
            use_cname: false,
            credentials: HuaweiObsStaticCredentials {
                access_key: "ak".into(),
                secret_key: "sk".into(),
                cdn_auth_key: None,
            },
        };
        let server = tokio::spawn(async move {
            let (mut get, _) = listener.accept().await.unwrap();
            assert!(
                read_request(&mut get)
                    .await
                    .starts_with("GET /?cors HTTP/1.1")
            );
            get.write_all(
                format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err(),
                "GET must not be followed by PUT"
            );
        });
        let result = client.configure(&["https://drive.example".into()]).await;
        match expected_error {
            None => assert!(!result.unwrap().changed),
            Some(expected) => {
                let error = result.err().expect("configuration must fail").to_string();
                assert!(error.contains(expected), "{error}");
                assert!(!error.contains("private"));
            }
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn no_configuration_creates_rule_and_write_failure_is_redacted() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    url.set_query(Some("cors"));
    let client = ObsCorsClient {
        client: aster_forge_http::HttpClient::builder()
            .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
            .build()
            .unwrap(),
        url,
        bucket: "sample-bucket".into(),
        use_cname: false,
        credentials: HuaweiObsStaticCredentials {
            access_key: "ak".into(),
            secret_key: "sk".into(),
            cdn_auth_key: None,
        },
    };
    let server = tokio::spawn(async move {
        let (mut get, _) = listener.accept().await.unwrap();
        read_request(&mut get).await;
        let body = "<Error><Code>NoSuchCORSConfiguration</Code></Error>";
        get.write_all(
            format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let (mut put, _) = listener.accept().await.unwrap();
        let request = read_request(&mut put).await;
        let config = parse(request.split("\r\n\r\n").nth(1).unwrap().as_bytes()).unwrap();
        assert_eq!(config.rules.len(), 1);
        assert_eq!(
            config.rules[0],
            desired_rule(&["https://drive.example".into()])
        );
        let body = "<Error><Code>AccessDenied</Code><Message>private</Message></Error>";
        put.write_all(
            format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    });
    let error = client
        .configure(&["https://drive.example".into()])
        .await
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("obs:bucket:PutBucketCORS"));
    assert!(!error.contains("private"));
    server.await.unwrap();
}

#[test]
fn rejects_rules_that_shadow_required_reads_before_appending() {
    let shadowing = CorsRule {
        id: Some("user-rule".into()),
        origins: vec!["*".into()],
        methods: vec!["GET".into(), "HEAD".into()],
        headers: vec!["*".into()],
        expose_headers: vec!["Content-Length".into()],
        max_age_seconds: Some(60),
    };
    let error = needs_rule(
        std::slice::from_ref(&shadowing),
        &["https://drive.example".into()],
    )
    .expect_err("a first matching rule without ETag must block the write");
    assert!(error.to_string().contains("earlier matching OBS CORS rule"));

    let desired = desired_rule(&["https://drive.example".into()]);
    let error = needs_rule(&[shadowing, desired], &["https://drive.example".into()])
        .expect_err("a later equivalent rule cannot bypass an earlier shadowing rule");
    assert!(error.to_string().contains("no rules were written"));

    let mut put_shadow = desired_rule(&["https://drive.example".into()]);
    put_shadow.expose_headers.retain(|header| header != "ETag");
    put_shadow.methods = vec!["PUT".into()];
    let error = needs_rule(&[put_shadow], &["https://drive.example".into()])
        .expect_err("a matching PUT rule without ETag must block multipart uploads");
    assert!(error.to_string().contains("no rules were written"));
}

#[test]
fn partial_header_wildcards_cannot_hide_an_earlier_put_rule() {
    let origins = vec!["https://drive.example".into()];
    let desired = desired_rule(&origins);
    for pattern in ["Content-*", "CONTENT-*", "*-Type", "cOn*TyPe", "*"] {
        let mut shadow = desired.clone();
        shadow.methods = vec!["PUT".into()];
        shadow.headers = vec![pattern.into()];
        shadow.expose_headers.clear();
        assert!(rule_matches(&shadow, &origins[0], "PUT", &["content-type"]));
        assert!(
            needs_rule(&[shadow, desired.clone()], &origins).is_err(),
            "{pattern}"
        );
    }
    let mut put = desired.clone();
    put.methods = vec!["PUT".into()];
    put.headers = vec!["Content-*".into()];
    put.expose_headers = vec!["etag".into()];
    assert!(!needs_rule(&[put.clone(), desired], &origins).unwrap());
    put.headers = vec!["X-Custom-*".into()];
    assert!(!rule_matches(&put, &origins[0], "PUT", &["content-type"]));
    put.headers = vec!["con**type".into()];
    assert!(!rule_matches(&put, &origins[0], "PUT", &["content-type"]));
}

#[test]
fn appends_when_existing_rules_leave_a_request_unmatched() {
    let existing = CorsRule {
        id: Some("user-rule".into()),
        origins: vec!["https://drive.example".into()],
        methods: vec!["GET".into(), "HEAD".into()],
        headers: vec!["*".into()],
        expose_headers: REQUIRED_EXPOSE_HEADERS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        max_age_seconds: Some(60),
    };
    assert!(needs_rule(&[existing], &["https://drive.example".into()]).unwrap());
}

#[test]
fn response_errors_are_redacted_and_actionable() {
    let error = response_error(
        StatusCode::FORBIDDEN,
        b"<Error><Code>AccessDenied</Code><Message>secret</Message></Error>",
        "PutBucketCors",
        None,
    );
    assert!(error.to_string().contains("obs:bucket:PutBucketCORS"));
    assert!(!error.to_string().contains("secret"));
    assert!(!error.to_string().contains("<Error>"));

    for provider_code in [
        "InvalidAccessKeyId",
        "SignatureDoesNotMatch",
        "RequestTimeTooSkewed",
    ] {
        let body = format!("<Error><Code>{provider_code}</Code></Error>");
        let error = response_error(
            StatusCode::FORBIDDEN,
            body.as_bytes(),
            "GetBucketCors",
            None,
        );
        let message = error.to_string();
        assert!(message.contains(provider_code));
        assert!(!message.contains("verify obs:bucket:GetBucketCORS"));
    }
}

#[test]
fn native_signatures_match_independent_python_hmac_vectors() {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let credentials = sample_credentials();
    for (cname, endpoint, expected_get, expected_put) in [
        (
            false,
            "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/?cors",
            "pWHwARJ6hz+RjvkkGBtMIQ+5JO8=",
            "1NBzQ9tTr6ngyq7fFAuSNeSO8YQ=",
        ),
        (
            true,
            "https://files.example.test/?cors",
            "wYewf0Go1hFOOW2a2LG9UNbWf9E=",
            "JEt/LdLWl5Yqcp30pUKAeelpSEY=",
        ),
    ] {
        for (method, md5, expected) in [
            ("GET", None, expected_get),
            ("PUT", Some("abc="), expected_put),
        ] {
            let headers = signing::signed_cors_request_headers(
                method,
                &Url::parse(endpoint).unwrap(),
                "archive-bucket",
                cname,
                &credentials,
                md5,
                now,
            )
            .unwrap();
            assert_eq!(
                headers.iter().find(|(name, _)| *name == "date").unwrap().1,
                "Tue, 14 Nov 2023 22:13:20 GMT"
            );
            assert_eq!(
                headers
                    .iter()
                    .find(|(name, _)| *name == "authorization")
                    .unwrap()
                    .1,
                format!("OBS access-key:{expected}")
            );
        }
    }
}

#[test]
fn rejects_invalid_origins_and_preserves_optional_ids() {
    for origins in [
        vec![],
        vec!["*".into()],
        vec!["https://drive.example/path".into()],
        vec!["https://user:pass@drive.example".into()],
    ] {
        assert!(validate_origins(&origins).is_err());
    }
    let xml = "<CORSConfiguration><CORSRule><ID> custom &amp; owned </ID><AllowedOrigin>https://other.example</AllowedOrigin><AllowedMethod>GET</AllowedMethod></CORSRule></CORSConfiguration>";
    let config = parse(xml.as_bytes()).unwrap();
    assert_eq!(config.rules[0].id.as_deref(), Some(" custom & owned "));
    assert_eq!(
        parse(serialize(&config).unwrap().as_bytes()).unwrap(),
        config
    );
    assert!(parse(xml.replace("</ID>", "</ID><ID>second</ID>").as_bytes()).is_err());
    assert!(
        parse(
            xml.replace(
                "</CORSConfiguration>",
                "<ResponseVary>true</ResponseVary></CORSConfiguration>"
            )
            .as_bytes()
        )
        .is_err()
    );
}

#[tokio::test]
async fn unsafe_reads_and_capacity_never_write_or_follow_redirects() {
    let full = CorsConfiguration {
        rules: vec![sample_config().rules[0].clone(); CORS_MAX_RULES],
    };
    for (status, body) in [
        (
            "200 OK",
            "<CORSConfiguration><FutureField>keep</FutureField></CORSConfiguration>".to_string(),
        ),
        ("200 OK", "not XML".into()),
        ("200 OK", "x".repeat(CORS_XML_MAX_BYTES + 1)),
        ("200 OK", serialize(&full).unwrap()),
        (
            "404 Not Found",
            "<Error><Code>NoSuchBucket</Code><Message>private</Message></Error>".into(),
        ),
        ("301 Moved Permanently", String::new()),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        // Use the production constructor, including its redirect/UA policies.
        let client = ObsCorsClient::new(
            &sample_driver_config(&endpoint, true),
            &sample_credentials(),
        )
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            assert!(request.starts_with("GET /?cors HTTP/1.1"));
            assert!(request.contains(crate::config::OUTBOUND_HTTP_USER_AGENT));
            stream.write_all(format!("HTTP/1.1 {status}\r\nLocation: {endpoint}/redirected\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                    .await
                    .is_err()
            );
        });
        let error = client
            .configure(&["https://drive.example".into()])
            .await
            .err()
            .expect("must refuse unsafe write")
            .to_string();
        assert!(!error.contains("private"));
        server.await.unwrap();
    }
}

#[test]
fn parses_official_namespaced_response_and_preserves_negative_max_age() {
    let xml = r#"<CORSConfiguration xmlns="http://obs.cn-north-4.myhuaweicloud.com/doc/2015-06-30/"><CORSRule><ID>existing</ID><AllowedMethod>GET</AllowedMethod><AllowedOrigin>https://other.example</AllowedOrigin><MaxAgeSeconds>-1</MaxAgeSeconds></CORSRule></CORSConfiguration>"#;
    let config = parse(xml.as_bytes()).unwrap();
    assert_eq!(config.rules[0].id.as_deref(), Some("existing"));
    assert_eq!(config.rules[0].max_age_seconds, Some(-1));
    assert_eq!(
        parse(serialize(&config).unwrap().as_bytes()).unwrap(),
        config
    );
    assert!(
        parse(
            xml.replace("obs.cn-north-4.myhuaweicloud.com", "evil.example")
                .as_bytes()
        )
        .is_err()
    );
    assert!(
        parse(
            xml.replace("<AllowedMethod>", "<AllowedMethod xmlns=\"urn:unknown\">")
                .as_bytes()
        )
        .is_err()
    );
}

#[test]
fn unsupported_patterns_refuse_a_write_instead_of_missing_a_shadowing_rule() {
    let mut rule = sample_config().rules[0].clone();
    for pattern in ["https://(drive|other).example", "https://**.example"] {
        rule.origins = vec![pattern.into()];
        let error = needs_rule(&[rule.clone()], &["https://drive.example".into()])
            .expect_err("unknown provider matching semantics must not allow overwrites");
        assert!(error.to_string().contains("no rules were written"));
    }
}

#[tokio::test]
async fn total_deadline_caps_body_reads_and_preserves_shorter_operation_timeout() {
    use std::task::Poll;
    use std::time::Duration;

    for (operation_timeout, expected_deadline) in [(3_600, 30), (10, 10)] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut config = sample_driver_config(&endpoint, true);
        config.operation_timeout = Duration::from_secs(operation_timeout);
        // Isolate the total deadline from the timeout between individual reads.
        config.read_timeout = Duration::from_secs(120);
        let client = ObsCorsClient::new(&config, &sample_credentials()).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            assert!(
                read_request(&mut stream)
                    .await
                    .starts_with("GET /?cors HTTP/1.1")
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1024\r\nConnection: close\r\n\r\n<")
                .await
                .unwrap();
            // Keep a partial response open; the per-read timeout is deliberately longer.
            std::future::pending::<()>().await;
        });
        let mut response = client.request("GET", None).await.unwrap();
        assert_eq!(response.chunk().await.unwrap().unwrap().as_ref(), b"<");

        // Only pause after real socket I/O has completed, avoiding virtual-time
        // auto-advancement while the OS is still delivering the response headers.
        tokio::time::pause();
        let mut next_chunk = std::pin::pin!(response.chunk());
        assert!(futures::poll!(next_chunk.as_mut()).is_pending());
        tokio::time::advance(Duration::from_secs(expected_deadline - 1)).await;
        assert!(futures::poll!(next_chunk.as_mut()).is_pending());
        tokio::time::advance(Duration::from_secs(2)).await;
        match futures::poll!(next_chunk.as_mut()) {
            Poll::Ready(Err(error)) => assert!(error.is_timeout(), "{error}"),
            result => {
                panic!("response body exceeded the {expected_deadline}s deadline: {result:?}")
            }
        }
        tokio::time::resume();
        server.abort();
    }
}
