//! Request-body diagnostics without credentials, signed URLs or provider paths.

use super::*;

pub(super) struct UploadBodyContext {
    pub object_key: String,
    pub binding_id: i64,
    pub content_length: i64,
    pub auth_path: &'static str,
    pub source_ip: Option<String>,
}

impl UploadBodyContext {
    fn failure(&self, received_bytes: u64, code: ApiErrorCode, reason: &'static str) -> AsterError {
        tracing::warn!(
            object_key = %self.object_key,
            binding_id = self.binding_id,
            content_length = self.content_length,
            received_bytes,
            auth_path = self.auth_path,
            source_ip = self.source_ip.as_deref(),
            reason,
            api_code = code.as_str(),
            "follower upload payload read failed"
        );
        validation_error_with_code(
            code,
            format!(
                "Upload body {}: declared {} bytes, received {received_bytes} bytes. Check the connection and retry.",
                match code {
                    ApiErrorCode::UploadRequestBodyIncomplete => "incomplete",
                    ApiErrorCode::UploadRequestSizeMismatch => "size mismatch",
                    _ => "read failed",
                },
                self.content_length,
            ),
        )
    }
}

pub(super) fn diagnosed_upload_body<S>(payload: S, context: UploadBodyContext) -> FollowerUploadBody
where
    S: futures::Stream<Item = std::result::Result<bytes::Bytes, actix_web::error::PayloadError>>
        + 'static,
{
    Box::pin(async_stream::try_stream! {
        futures::pin_mut!(payload);
        let declared_bytes = u64::try_from(context.content_length)
            .map_err(|_| validation_error_with_code(ApiErrorCode::InternalStorageContentLengthInvalid, "negative content length"))?;
        let mut received_bytes = 0u64;
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_err(|error| {
                let incomplete = matches!(error,
                    actix_web::error::PayloadError::Incomplete(_)
                    | actix_web::error::PayloadError::Io(_)
                );
                let code = if incomplete { ApiErrorCode::UploadRequestBodyIncomplete } else { ApiErrorCode::UploadRequestBodyReadFailed };
                context.failure(received_bytes, code,
                    if incomplete { "connection_interrupted" } else { "payload_read_error" })
            })?;
            received_bytes = received_bytes.checked_add(u64::try_from(chunk.len()).map_err(|_| validation_error_with_code(ApiErrorCode::UploadRequestBodySizeOverflow, "upload chunk size overflow"))?)
                .ok_or_else(|| validation_error_with_code(ApiErrorCode::UploadRequestBodySizeOverflow, "upload body size overflow"))?;
            if received_bytes > declared_bytes {
                Err(context.failure(received_bytes, ApiErrorCode::UploadRequestSizeMismatch, "declared_size_exceeded"))?;
            }
            yield chunk;
        }
        if received_bytes < declared_bytes {
            Err(context.failure(received_bytes, ApiErrorCode::UploadRequestBodyIncomplete, "premature_eof"))?;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::error::PayloadError;
    use aster_drive_storage::StorageDriver;
    use bytes::Bytes;
    use std::sync::Arc;
    use tracing::{Instrument, instrument::WithSubscriber};

    fn context(auth_path: &'static str) -> UploadBodyContext {
        UploadBodyContext {
            object_key: "object.bin".to_string(),
            binding_id: 7,
            content_length: 16,
            auth_path,
            source_ip: Some("192.0.2.1".to_string()),
        }
    }

    #[tokio::test]
    async fn interrupted_body_keeps_diagnostics_error_code_and_cleans_staging() {
        for auth_path in ["signed", "presigned"] {
            for (tail, expected_code, received_bytes) in [
                (
                    Err(PayloadError::Incomplete(None)),
                    ApiErrorCode::UploadRequestBodyIncomplete,
                    4,
                ),
                (
                    Ok(Bytes::from_static(b"0123456789abcdef")),
                    ApiErrorCode::UploadRequestSizeMismatch,
                    20,
                ),
            ] {
                let root =
                    std::env::temp_dir().join(format!("upload-body-{}", uuid::Uuid::new_v4()));
                tokio::fs::create_dir_all(&root).await.unwrap();
                let driver = Arc::new(
                    crate::storage::drivers::local::LocalDriver::new(root.to_str().unwrap())
                        .unwrap(),
                );
                driver.put("object.bin", b"original").await.unwrap();
                let logs = crate::runtime::test_logs::TestLogs::default();
                let payload = diagnosed_upload_body(
                    futures::stream::iter([Ok(Bytes::from_static(b"part")), tail]),
                    context(auth_path),
                );
                let result = async {
                    write_follower_object(
                        driver.clone(),
                        &aster_drive_metrics::NoopMetrics::new(),
                        FollowerObjectWriteRequest {
                            binding_id: 7,
                            object_key: "object.bin",
                            storage_path: "object.bin",
                            content_length: 16,
                            expected_sha256: None,
                            temp_root: None,
                        },
                        payload,
                    )
                    .instrument(tracing::info_span!(
                        "request",
                        request_id = "upload-request"
                    ))
                    .await
                }
                .with_subscriber(logs.subscriber())
                .await;
                let error = result.unwrap_err();
                assert_eq!(error.api_error_code(), expected_code);
                let response = actix_web::ResponseError::error_response(&error);
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
                let body = actix_web::body::to_bytes(response.into_body())
                    .await
                    .unwrap();
                let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(body["code"], expected_code.as_str());
                assert_eq!(
                    body["error"]["retryable"],
                    expected_code == ApiErrorCode::UploadRequestBodyIncomplete
                );
                assert!(body["msg"].as_str().unwrap().contains(&format!(
                    "declared 16 bytes, received {received_bytes} bytes"
                )));
                assert_eq!(driver.get("object.bin").await.unwrap(), b"original");
                let names: Vec<_> = std::fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect();
                assert_eq!(names, vec![std::ffi::OsString::from("object.bin")]);
                let text = logs.text();
                for field in [
                    "object_key=object.bin",
                    "binding_id=7",
                    "content_length=16",
                    "source_ip=\"192.0.2.1\"",
                    auth_path,
                    "upload-request",
                ] {
                    assert!(text.contains(field), "missing {field}: {text}");
                }
                assert!(
                    text.contains(&format!("received_bytes={received_bytes}")),
                    "{text}"
                );
                assert!(!text.contains(root.to_str().unwrap()));
                tokio::fs::remove_dir_all(root).await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn oversized_chunk_is_rejected_before_forwarding_and_stops_polling() {
        use std::{cell::Cell, rc::Rc};
        for (declared, chunks, forwarded, received) in [
            (0, vec![b"x".as_slice()], 0, 1),
            (4, vec![b"12345".as_slice()], 0, 5),
            (
                4,
                vec![
                    b"12".as_slice(),
                    b"345".as_slice(),
                    b"must not be polled".as_slice(),
                ],
                2,
                5,
            ),
        ] {
            let polls = Rc::new(Cell::new(0));
            let observed = polls.clone();
            let payload = futures::stream::iter(
                chunks
                    .into_iter()
                    .map(|bytes| Ok(Bytes::copy_from_slice(bytes))),
            )
            .map(move |chunk| {
                observed.set(observed.get() + 1);
                chunk
            });
            let mut ctx = context("presigned");
            ctx.content_length = declared;
            let mut body = diagnosed_upload_body(payload, ctx);
            let mut forwarded_bytes = 0;
            let error = loop {
                match body.next().await.unwrap() {
                    Ok(chunk) => forwarded_bytes += chunk.len(),
                    Err(error) => break error,
                }
            };
            assert_eq!(forwarded_bytes, forwarded);
            assert_eq!(polls.get(), if forwarded == 0 { 1 } else { 2 });
            assert_eq!(
                error.api_error_code(),
                ApiErrorCode::UploadRequestSizeMismatch
            );
            assert!(!error.api_error_info().retryable);
            assert!(error.message().contains(&format!(
                "declared {declared} bytes, received {received} bytes"
            )));
            assert!(body.next().await.is_none());
        }
    }

    #[tokio::test]
    async fn body_counting_handles_empty_negative_and_multichunk_boundaries() {
        let mut invalid = context("signed");
        invalid.content_length = -1;
        let mut body = diagnosed_upload_body(futures::stream::empty(), invalid);
        assert_eq!(
            body.next().await.unwrap().unwrap_err().api_error_code(),
            ApiErrorCode::InternalStorageContentLengthInvalid
        );

        let mut empty = context("presigned");
        empty.content_length = 0;
        assert!(
            diagnosed_upload_body(futures::stream::empty(), empty)
                .next()
                .await
                .is_none()
        );

        for chunks in [
            Vec::new(),
            vec![
                Ok(Bytes::from_static(b"part")),
                Ok(Bytes::from_static(b"ial")),
            ],
            vec![
                Ok(Bytes::from_static(b"part")),
                Ok(Bytes::from_static(b"ial")),
                Err(PayloadError::Incomplete(None)),
            ],
        ] {
            let received: usize = chunks
                .iter()
                .filter_map(|chunk| chunk.as_ref().ok())
                .map(Bytes::len)
                .sum();
            let mut body =
                diagnosed_upload_body(futures::stream::iter(chunks), context("presigned"));
            let error = loop {
                match body
                    .next()
                    .await
                    .expect("truncated stream must produce an error")
                {
                    Ok(_) => {}
                    Err(error) => break error,
                }
            };
            assert_eq!(
                error.api_error_code(),
                ApiErrorCode::UploadRequestBodyIncomplete
            );
            assert!(
                error
                    .message()
                    .contains(&format!("declared 16 bytes, received {received} bytes"))
            );
        }
    }

    #[tokio::test]
    async fn eof_and_other_payload_errors_have_distinct_codes() {
        for (error, expected) in [
            (None, ApiErrorCode::UploadRequestBodyIncomplete),
            (
                Some(PayloadError::Io(std::io::Error::from(
                    std::io::ErrorKind::ConnectionReset,
                ))),
                ApiErrorCode::UploadRequestBodyIncomplete,
            ),
            (
                Some(PayloadError::EncodingCorrupted),
                ApiErrorCode::UploadRequestBodyReadFailed,
            ),
        ] {
            let mut chunks = vec![Ok(Bytes::from_static(b"part"))];
            if let Some(error) = error {
                chunks.push(Err(error));
            }
            let mut body =
                diagnosed_upload_body(futures::stream::iter(chunks), context("presigned"));
            assert_eq!(body.next().await.unwrap().unwrap(), "part");
            assert_eq!(
                body.next().await.unwrap().unwrap_err().api_error_code(),
                expected
            );
        }
        for bytes in [b"".as_slice(), b"complete".as_slice()] {
            let mut ctx = context("signed");
            ctx.content_length = i64::try_from(bytes.len()).unwrap();
            let mut body =
                diagnosed_upload_body(futures::stream::iter([Ok(Bytes::from(bytes))]), ctx);
            assert!(body.next().await.unwrap().is_ok());
            assert!(body.next().await.is_none());
        }
    }
}
