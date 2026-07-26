use futures::StreamExt;

use crate::errors::{AsterError, MapAsterErr, Result};

pub(crate) async fn read_reqwest_body_limited(
    response: reqwest::Response,
    context: &str,
    max_bytes: usize,
    error: impl Copy + Fn(String) -> AsterError,
) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|content_length| {
        usize::try_from(content_length).map_or(true, |length| length > max_bytes)
    }) {
        return Err(error(format!("{context} exceeds {max_bytes} bytes limit")));
    }
    let mut body = Vec::with_capacity(max_bytes.min(4096));
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_aster_err_ctx(context, error)?;
        extend_body_limited(&mut body, &chunk, context, max_bytes, error)?;
    }
    Ok(body)
}

fn extend_body_limited(
    body: &mut Vec<u8>,
    chunk: &[u8],
    context: &str,
    max_bytes: usize,
    error: impl Copy + Fn(String) -> AsterError,
) -> Result<()> {
    let next_len = body
        .len()
        .checked_add(chunk.len())
        .ok_or_else(|| error(format!("{context} size overflow")))?;
    if next_len > max_bytes {
        return Err(error(format!("{context} exceeds {max_bytes} bytes limit")));
    }
    body.extend_from_slice(chunk);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::errors::AsterError;

    use super::{extend_body_limited, read_reqwest_body_limited};

    async fn response_with_body(body: &'static [u8], chunked: bool) -> reqwest::Response {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should expose address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = socket
                    .read(&mut buffer)
                    .await
                    .expect("test server should read request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            if chunked {
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                    )
                    .await
                    .expect("test server should write chunked headers");
                socket
                    .write_all(format!("{:x}\r\n", body.len()).as_bytes())
                    .await
                    .expect("test server should write chunk size");
                socket
                    .write_all(body)
                    .await
                    .expect("test server should write chunk");
                socket
                    .write_all(b"\r\n0\r\n\r\n")
                    .await
                    .expect("test server should finish chunked body");
            } else {
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await
                    .expect("test server should write headers");
                socket
                    .write_all(body)
                    .await
                    .expect("test server should write body");
            }
        });
        let response = reqwest::get(format!("http://{addr}/"))
            .await
            .expect("test request should succeed");
        server.await.expect("test server should finish");
        response
    }

    #[test]
    fn limited_body_accumulation_accepts_exact_limit_and_rejects_one_byte_over() {
        let mut body = Vec::new();
        extend_body_limited(
            &mut body,
            b"123",
            "test response body",
            4,
            AsterError::validation_error,
        )
        .expect("body below limit should be accepted");
        extend_body_limited(
            &mut body,
            b"4",
            "test response body",
            4,
            AsterError::validation_error,
        )
        .expect("body at exact limit should be accepted");
        let error = extend_body_limited(
            &mut body,
            b"5",
            "test response body",
            4,
            AsterError::validation_error,
        )
        .expect_err("body over limit should be rejected");

        assert_eq!(body, b"1234");
        assert!(error.message().contains("exceeds 4 bytes limit"));
    }

    #[tokio::test]
    async fn reqwest_body_reader_enforces_exact_network_boundary() {
        let exact = read_reqwest_body_limited(
            response_with_body(b"1234", false).await,
            "test network body",
            4,
            AsterError::validation_error,
        )
        .await
        .expect("body at exact network limit should be accepted");
        assert_eq!(exact, b"1234");

        let error = read_reqwest_body_limited(
            response_with_body(b"12345", true).await,
            "test network body",
            4,
            AsterError::validation_error,
        )
        .await
        .expect_err("body over network limit should be rejected");
        assert!(error.message().contains("exceeds 4 bytes limit"));
    }
}
