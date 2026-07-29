use http::Method;

use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_SIGNATURE_HEADER,
    INTERNAL_AUTH_TIMESTAMP_HEADER, sign_internal_request,
};
use aster_drive_model::entities::managed_follower;

pub(super) fn request_headers(
    remote_node: &managed_follower::Model,
    method: &Method,
    path_and_query: &str,
    content_length: Option<u64>,
) -> impl Iterator<Item = (String, String)> {
    signed_headers(remote_node, method, path_and_query, content_length)
        .into_iter()
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
