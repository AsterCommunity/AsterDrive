use reqwest::Method;

use crate::errors::Result;
use aster_drive_model::entities::master_binding;
use aster_drive_storage::StorageErrorKind;

use super::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, body_sha256_hex, empty_body_sha256, path_and_query_for_url,
    sign_internal_request,
};

pub async fn send_signed_master_request(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    method: Method,
    url: &str,
    body: Option<Vec<u8>>,
) -> Result<reqwest::Response> {
    let content_length = body
        .as_ref()
        .map(|body| {
            u64::try_from(body.len()).map_err(|_| {
                crate::errors::storage_driver_error(
                    StorageErrorKind::Precondition,
                    "remote master request body length overflow",
                )
            })
        })
        .transpose()?;
    // 控制面 body 是有界 JSON，整体入签；None 视为空 body。
    let body_sha256 = body
        .as_deref()
        .map(body_sha256_hex)
        .unwrap_or_else(|| empty_body_sha256().to_string());
    // 签名输入从 URL 归一化得出，与 master 侧 `req.uri().path_and_query()`
    // 的验签输入必然一致；调用方手拼路径的写法在未来加 query 参数时会静默
    // 脱离签名，这里收口为单一事实源。
    let parsed_url = reqwest::Url::parse(url).map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("parse remote master request url: {error}"),
        )
    })?;
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = aster_forge_utils::id::new_uuid();
    let signature = sign_internal_request(
        &binding.secret_key,
        method.as_str(),
        &path_and_query_for_url(&parsed_url),
        timestamp,
        &nonce,
        content_length,
        &body_sha256,
    );

    let mut builder = client
        .request(method, url)
        .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, &binding.access_key)
        .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
        .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
        .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
        .header(
            crate::storage::remote_protocol::INTERNAL_CONTENT_SHA256_HEADER,
            &body_sha256,
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    if let Some(content_length) = content_length {
        builder = builder.header(reqwest::header::CONTENT_LENGTH, content_length);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }

    builder.send().await.map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Transient,
            format!("send signed remote master request: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use aster_drive_model::types::ResolvedRemoteTransport;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct LoggedRequest {
        method: String,
        path_and_query: String,
        access_key: String,
        timestamp: i64,
        nonce: String,
        signature: String,
        content_length: Option<u64>,
        body: Vec<u8>,
    }

    #[tokio::test]
    async fn signed_master_requests_cover_body_and_bodyless_contracts() {
        async fn capture(
            request: HttpRequest,
            body: web::Bytes,
            requests: web::Data<Arc<Mutex<Vec<LoggedRequest>>>>,
        ) -> HttpResponse {
            let header = |name: &str| {
                request
                    .headers()
                    .get(name)
                    .and_then(|value| value.to_str().ok())
                    .expect("signed request header should be present")
                    .to_string()
            };
            requests
                .lock()
                .expect("signed request log lock should not be poisoned")
                .push(LoggedRequest {
                    method: request.method().to_string(),
                    path_and_query: request
                        .uri()
                        .path_and_query()
                        .expect("signed request should have path and query")
                        .to_string(),
                    access_key: header(INTERNAL_AUTH_ACCESS_KEY_HEADER),
                    timestamp: header(INTERNAL_AUTH_TIMESTAMP_HEADER)
                        .parse()
                        .expect("signed timestamp should parse"),
                    nonce: header(INTERNAL_AUTH_NONCE_HEADER),
                    signature: header(INTERNAL_AUTH_SIGNATURE_HEADER),
                    content_length: request
                        .headers()
                        .get(reqwest::header::CONTENT_LENGTH.as_str())
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.parse().expect("content length should parse")),
                    body: body.to_vec(),
                });
            HttpResponse::NoContent().finish()
        }

        let requests = Arc::new(Mutex::new(Vec::<LoggedRequest>::new()));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("signed request test listener should bind");
        let address = listener
            .local_addr()
            .expect("signed request test listener should expose address");
        let requests_for_server = requests.clone();
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(requests_for_server.clone()))
                .default_service(web::to(capture))
        })
        .listen(listener)
        .expect("signed request test server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        let now = chrono::Utc::now();
        let binding = master_binding::Model {
            id: 1,
            name: "binding".to_string(),
            master_url: format!("http://127.0.0.1:{}", address.port()),
            access_key: "access-key".to_string(),
            secret_key: "secret-key".to_string(),
            is_enabled: true,
            resolved_transport: ResolvedRemoteTransport::ReverseTunnel,
            desired_revision: 1,
            applied_revision: 0,
            storage_namespace: "namespace".to_string(),
            created_at: now,
            updated_at: now,
        };
        let client = reqwest::Client::new();
        let get_path = "/control/state?applied_revision=0";
        let post_path = "/control/report";
        let post_body = br#"{"status":"ready"}"#.to_vec();

        let get_response = send_signed_master_request(
            &client,
            &binding,
            Method::GET,
            &format!("{}{get_path}", binding.master_url),
            None,
        )
        .await
        .expect("bodyless signed request should send");
        assert_eq!(get_response.status(), reqwest::StatusCode::NO_CONTENT);

        let post_response = send_signed_master_request(
            &client,
            &binding,
            Method::POST,
            &format!("{}{post_path}", binding.master_url),
            Some(post_body.clone()),
        )
        .await
        .expect("signed request with body should send");
        assert_eq!(post_response.status(), reqwest::StatusCode::NO_CONTENT);

        {
            let requests = requests
                .lock()
                .expect("signed request log lock should not be poisoned");
            assert_eq!(requests.len(), 2);
            for request in requests.iter() {
                assert_eq!(request.access_key, binding.access_key);
                assert!(!request.nonce.is_empty());
                let body_sha256 = if request.body.is_empty() && request.content_length.is_none() {
                    empty_body_sha256().to_string()
                } else {
                    body_sha256_hex(&request.body)
                };
                assert_eq!(
                    request.signature,
                    super::sign_internal_request(
                        &binding.secret_key,
                        &request.method,
                        &request.path_and_query,
                        request.timestamp,
                        &request.nonce,
                        request.content_length,
                        &body_sha256,
                    )
                );
            }
            assert_eq!(requests[0].content_length, None);
            assert!(requests[0].body.is_empty());
            assert_eq!(
                requests[1].content_length,
                Some(u64::try_from(post_body.len()).expect("test body length should fit u64"))
            );
            assert_eq!(requests[1].body, post_body);
        }

        handle.stop(true).await;
        let _ = task.await;
    }
}
