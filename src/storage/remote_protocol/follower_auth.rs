use reqwest::Method;

use crate::errors::Result;
use aster_drive_model::entities::master_binding;
use aster_drive_storage::StorageErrorKind;

use super::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, sign_internal_request,
};

pub async fn send_signed_master_request(
    client: &reqwest::Client,
    binding: &master_binding::Model,
    method: Method,
    url: &str,
    path_and_query: &str,
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
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = aster_forge_utils::id::new_uuid();
    let signature = sign_internal_request(
        &binding.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
    );

    let mut builder = client
        .request(method, url)
        .header(INTERNAL_AUTH_ACCESS_KEY_HEADER, &binding.access_key)
        .header(INTERNAL_AUTH_TIMESTAMP_HEADER, timestamp.to_string())
        .header(INTERNAL_AUTH_NONCE_HEADER, nonce)
        .header(INTERNAL_AUTH_SIGNATURE_HEADER, signature)
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
