use crate::db::repository::managed_follower_repo;
use crate::entities::managed_follower;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;

use hmac::Mac;

use crate::storage::remote_protocol::{
    INTERNAL_AUTH_ACCESS_KEY_HEADER, INTERNAL_AUTH_NONCE_HEADER, INTERNAL_AUTH_NONCE_TTL_SECS,
    INTERNAL_AUTH_SIGNATURE_HEADER, INTERNAL_AUTH_SKEW_SECS, INTERNAL_AUTH_TIMESTAMP_HEADER,
    internal_request_mac, sign_internal_request,
};

pub async fn authorize_tunnel_request<S: SharedRuntimeState>(
    state: &S,
    method: &actix_web::http::Method,
    path_and_query: &str,
    headers: &actix_web::http::header::HeaderMap,
    content_length: Option<u64>,
) -> Result<managed_follower::Model> {
    let access_key = header_value(headers, INTERNAL_AUTH_ACCESS_KEY_HEADER)?;
    let timestamp = header_value(headers, INTERNAL_AUTH_TIMESTAMP_HEADER)?
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid reverse tunnel auth timestamp"))?;
    let nonce = header_value(headers, INTERNAL_AUTH_NONCE_HEADER)?;
    let signature = header_value(headers, INTERNAL_AUTH_SIGNATURE_HEADER)?;

    let now = chrono::Utc::now().timestamp();
    if (now - timestamp).abs() > INTERNAL_AUTH_SKEW_SECS {
        return Err(AsterError::auth_token_invalid(
            "reverse tunnel auth timestamp is outside allowed skew",
        ));
    }

    let remote_node = managed_follower_repo::find_by_access_key(state.writer_db(), &access_key)
        .await?
        .ok_or_else(|| AsterError::auth_invalid_credentials("unknown remote tunnel access_key"))?;
    if !remote_node.is_enabled {
        return Err(AsterError::validation_error("remote node is disabled"));
    }

    let expected = sign_internal_request(
        &remote_node.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
    );
    let expected = hex::decode(&expected).map_err(|error| {
        AsterError::internal_error(format!("decode reverse tunnel expected signature: {error}"))
    })?;
    let signature = hex::decode(&signature).map_err(|_| {
        AsterError::auth_invalid_credentials("reverse tunnel auth signature mismatch")
    })?;
    let valid_len = signature.len() == expected.len();
    let valid_signature = internal_request_mac(
        &remote_node.secret_key,
        method.as_str(),
        path_and_query,
        timestamp,
        &nonce,
        content_length,
    )
    .verify_slice(&signature)
    .is_ok();
    if !(valid_len & valid_signature) {
        return Err(AsterError::auth_invalid_credentials(
            "reverse tunnel auth signature mismatch",
        ));
    }

    let nonce_cache_key = format!("remote_tunnel_nonce:{access_key}:{nonce}");
    if !state
        .cache()
        .set_bytes_if_absent(
            &nonce_cache_key,
            Vec::new(),
            Some(INTERNAL_AUTH_NONCE_TTL_SECS),
        )
        .await
    {
        return Err(AsterError::auth_token_invalid(
            "reverse tunnel auth nonce has already been used",
        ));
    }

    Ok(remote_node)
}

fn header_value(headers: &actix_web::http::header::HeaderMap, name: &str) -> Result<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AsterError::auth_token_invalid(format!("missing header {name}")))
}
