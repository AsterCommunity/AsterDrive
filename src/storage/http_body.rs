use futures::StreamExt;

use crate::errors::{AsterError, Result};

/// Reads a reqwest response without allocating beyond the caller's byte budget.
pub(crate) async fn read_reqwest_response_body_limited<F>(
    response: reqwest::Response,
    context: &str,
    max_body_bytes: usize,
    map_error: F,
) -> Result<Vec<u8>>
where
    F: Fn(String) -> AsterError + Copy,
{
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(max_body_bytes).unwrap_or(u64::MAX))
    {
        return Err(map_error(format!(
            "{context} response body exceeds {max_body_bytes} bytes limit"
        )));
    }

    let mut body = Vec::with_capacity(max_body_bytes.min(4096));
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| map_error(format!("{context}: {error}")))?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| map_error(format!("{context} response body size overflow")))?;
        if next_len > max_body_bytes {
            return Err(map_error(format!(
                "{context} response body exceeds {max_body_bytes} bytes limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use tokio::io::AsyncWriteExt;

    use super::read_reqwest_response_body_limited;
    use crate::errors::AsterError;

    #[tokio::test]
    async fn rejects_declared_body_larger_than_limit() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n12345")
                .await
                .expect("test server should write response");
        });

        let response = reqwest::Client::new()
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect("request should succeed");
        let error = read_reqwest_response_body_limited(
            response,
            "test body",
            4,
            AsterError::validation_error,
        )
        .await
        .expect_err("declared oversized body should be rejected");
        assert!(error.to_string().contains("exceeds 4 bytes"));
        server.await.expect("test server should finish");
    }

    #[tokio::test]
    async fn rejects_chunked_body_after_accumulated_limit() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
                )
                .await
                .expect("test server should write response");
        });

        let response = reqwest::Client::new()
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect("request should succeed");
        let error = read_reqwest_response_body_limited(
            response,
            "test body",
            5,
            AsterError::validation_error,
        )
        .await
        .expect_err("chunked oversized body should be rejected");
        assert!(error.to_string().contains("exceeds 5 bytes"));
        server.await.expect("test server should finish");
    }

    #[tokio::test]
    async fn accepts_body_at_exact_limit() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("test listener should bind");
        let address = listener
            .local_addr()
            .expect("test listener should expose address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener
                .accept()
                .await
                .expect("test server should accept request");
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n1234")
                .await
                .expect("test server should write response");
        });

        let response = reqwest::Client::new()
            .get(format!("http://{address}/"))
            .send()
            .await
            .expect("request should succeed");
        let body = read_reqwest_response_body_limited(
            response,
            "test body",
            4,
            AsterError::validation_error,
        )
        .await
        .expect("body at the exact limit should be accepted");
        assert_eq!(body, b"1234");
        server.await.expect("test server should finish");
    }
}
