use http::Method;

use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, INTERNAL_CONTENT_SHA256_HEADER, sign_internal_request,
};
use aster_drive_model::entities::managed_follower;

pub(super) fn request_headers(
    remote_node: &managed_follower::Model,
    method: &Method,
    path_and_query: &str,
    content_length: Option<u64>,
    body_sha256: &str,
) -> impl Iterator<Item = (String, String)> {
    signed_headers(
        remote_node,
        method,
        path_and_query,
        content_length,
        body_sha256,
    )
    .into_iter()
    .chain(std::iter::once((
        INTERNAL_CONTENT_SHA256_HEADER.to_string(),
        body_sha256.to_string(),
    )))
    .chain(content_length.map(|content_length| {
        (
            http::header::CONTENT_LENGTH.to_string(),
            content_length.to_string(),
        )
    }))
}

fn signed_headers(
    remote_node: &managed_follower::Model,
    method: &Method,
    path_and_query: &str,
    content_length: Option<u64>,
    body_sha256: &str,
) -> Vec<(String, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let nonce = aster_forge_utils::id::new_uuid();
    let signature = sign_internal_request(
        &remote_node.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
        body_sha256,
    );
    vec![
        (
            INTERNAL_AUTH_ACCESS_KEY_HEADER.to_string(),
            remote_node.access_key.clone(),
        ),
        (
            INTERNAL_AUTH_TIMESTAMP_HEADER.to_string(),
            timestamp.to_string(),
        ),
        (INTERNAL_AUTH_NONCE_HEADER.to_string(), nonce),
        (INTERNAL_AUTH_SIGNATURE_HEADER.to_string(), signature),
    ]
}
