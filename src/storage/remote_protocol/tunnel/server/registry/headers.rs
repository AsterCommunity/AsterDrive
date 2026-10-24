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
    nonce: &str,
) -> impl Iterator<Item = (String, String)> {
    signed_headers(
        remote_node,
        method,
        path_and_query,
        content_length,
        body_sha256,
        nonce,
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
    nonce: &str,
) -> Vec<(String, String)> {
    let timestamp = chrono::Utc::now().timestamp();
    let signature = sign_internal_request(
        &remote_node.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        nonce,
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
        (INTERNAL_AUTH_NONCE_HEADER.to_string(), nonce.to_string()),
        (INTERNAL_AUTH_SIGNATURE_HEADER.to_string(), signature),
    ]
}

pub(super) fn take_expected_response_nonce(
    extra_headers: &mut Vec<(String, String)>,
) -> Result<String, crate::errors::AsterError> {
    let positions = extra_headers
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| {
            name == crate::storage::remote_protocol::INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if positions.len() > 1 {
        return Err(crate::errors::AsterError::validation_error(
            "duplicate expected response nonce metadata",
        ));
    }
    match positions.first().copied() {
        Some(index) => {
            let nonce = extra_headers.remove(index).1;
            if nonce.is_empty() {
                return Err(crate::errors::AsterError::validation_error(
                    "expected response nonce metadata cannot be empty",
                ));
            }
            Ok(nonce)
        }
        None => Ok(aster_forge_utils::id::new_uuid()),
    }
}

#[cfg(test)]
mod tests {
    use super::take_expected_response_nonce;
    use crate::storage::remote_protocol::INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER;

    #[test]
    fn expected_response_nonce_is_consumed_before_forwarding_headers() {
        let mut headers = vec![
            (
                "content-type".to_string(),
                "application/octet-stream".to_string(),
            ),
            (
                INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER.to_string(),
                "nonce-1".to_string(),
            ),
        ];

        assert_eq!(
            take_expected_response_nonce(&mut headers).unwrap(),
            "nonce-1"
        );
        assert_eq!(
            headers,
            [(
                "content-type".to_string(),
                "application/octet-stream".to_string()
            )]
        );
    }

    #[test]
    fn expected_response_nonce_rejects_empty_and_duplicate_metadata() {
        let mut empty = vec![(
            INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER.to_string(),
            String::new(),
        )];
        assert!(take_expected_response_nonce(&mut empty).is_err());

        let mut duplicate = vec![
            (
                INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER.to_string(),
                "one".to_string(),
            ),
            (
                INTERNAL_EXPECTED_RESPONSE_NONCE_HEADER.to_string(),
                "two".to_string(),
            ),
        ];
        assert!(take_expected_response_nonce(&mut duplicate).is_err());
    }
}
