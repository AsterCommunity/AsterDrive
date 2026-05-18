//! 服务模块：`share_stream_service`。

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration as StdDuration;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::cache::CacheExt;
use crate::config::{operations, site_url};
use crate::db::repository::{file_repo, share_repo};
use crate::entities::{file, share};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    direct_link_service, file_service, file_service::ResolvedDownloadRange, share_service,
};
use crate::utils::numbers::u64_to_i64;

const SHARE_STREAM_COUNTED_CACHE_PREFIX: &str = "share_stream_session_counted:";
static FALLBACK_COUNTED_SESSIONS: LazyLock<Cache<String, CountMarkerState>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10_000)
        .time_to_live(StdDuration::from_secs(
            operations::MAX_SHARE_STREAM_SESSION_TTL_SECS,
        ))
        .build()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CountMarkerState {
    Pending,
    Counted,
}

enum CountReservation {
    Reserved,
    AlreadyCounted,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ShareStreamSessionInfo {
    pub path: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ShareStreamSubject {
    ShareFile { share_token: String },
    ShareFolderFile { share_token: String, file_id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ShareStreamSessionPayload {
    subject: ShareStreamSubject,
    exp: i64,
    nonce: String,
}

enum ResolvedShareStreamTarget {
    Fresh {
        share: share::Model,
        file: file::Model,
    },
    Marked {
        share: share::Model,
        file: file::Model,
    },
}

pub(crate) async fn create_session_for_shared_file_for_origin(
    state: &PrimaryAppState,
    share_token: &str,
    request_origin: crate::services::preview_link_service::RequestOrigin<'_>,
) -> Result<ShareStreamSessionInfo> {
    let (share, file) = share_service::load_preview_shared_file(state, share_token).await?;
    let payload = build_payload(
        ShareStreamSubject::ShareFile {
            share_token: share.token.clone(),
        },
        share_stream_session_ttl_secs_i64(state)?,
    );
    build_session_for_shared_file(state, &share, &file, &payload, Some(request_origin))
}

pub(crate) async fn create_session_for_shared_folder_file_for_origin(
    state: &PrimaryAppState,
    share_token: &str,
    file_id: i64,
    request_origin: crate::services::preview_link_service::RequestOrigin<'_>,
) -> Result<ShareStreamSessionInfo> {
    let (share, file) =
        share_service::load_preview_shared_folder_file(state, share_token, file_id).await?;
    let payload = build_payload(
        ShareStreamSubject::ShareFolderFile {
            share_token: share.token.clone(),
            file_id: file.id,
        },
        share_stream_session_ttl_secs_i64(state)?,
    );
    build_session_for_shared_file(state, &share, &file, &payload, Some(request_origin))
}

pub(crate) async fn resolve_file_for_stream(
    state: &PrimaryAppState,
    share_token: &str,
    session_token: &str,
    requested_name: &str,
) -> Result<file::Model> {
    let (_, target) = resolve_session_target(state, share_token, session_token).await?;
    let file = match target {
        ResolvedShareStreamTarget::Fresh { file, .. }
        | ResolvedShareStreamTarget::Marked { file, .. } => file,
    };
    direct_link_service::validate_public_file_name(&file, requested_name)?;
    Ok(file)
}

pub(crate) async fn stream_file(
    state: &PrimaryAppState,
    share_token: &str,
    session_token: &str,
    requested_name: &str,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_service::DownloadOutcome> {
    let (payload, target) = resolve_session_target(state, share_token, session_token).await?;
    let (share, file) = match target {
        ResolvedShareStreamTarget::Fresh { share, file }
        | ResolvedShareStreamTarget::Marked { share, file } => (share, file),
    };
    direct_link_service::validate_public_file_name(&file, requested_name)?;

    let blob = file_repo::find_blob_by_id(&state.db, file.blob_id).await?;
    let count_reservation = ensure_counted_once(state, session_token, &payload).await?;

    if matches!(count_reservation, CountReservation::Reserved) {
        match share_repo::increment_download_count(&state.db, share.id).await {
            Ok(true) => {
                mark_counted(state, session_token, &payload).await?;
                share_service::invalidate_all_share_token_record_cache(state).await;
            }
            Ok(false) => {
                release_counted_marker(state, session_token).await;
                return Err(AsterError::share_download_limit("download limit reached"));
            }
            Err(error) => {
                release_counted_marker(state, session_token).await;
                tracing::warn!(
                    share_id = share.id,
                    "failed to increment share stream download count: {error}"
                );
                return Err(error);
            }
        }
    }

    match file_service::build_stream_outcome_with_disposition_and_range(
        state,
        &file,
        &blob,
        file_service::DownloadDisposition::Inline,
        None,
        range,
    )
    .await
    {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            if matches!(count_reservation, CountReservation::Reserved) {
                release_counted_marker(state, session_token).await;
                match share_repo::decrement_download_count(&state.db, share.id).await {
                    Ok(true) | Ok(false) => {}
                    Err(rollback_error) => {
                        tracing::warn!(
                            share_id = share.id,
                            "failed to roll back share stream count after response build failure: {rollback_error}"
                        );
                    }
                }
            }
            Err(error)
        }
    }
}

fn share_stream_session_ttl_secs_i64(state: &PrimaryAppState) -> Result<i64> {
    u64_to_i64(
        operations::share_stream_session_ttl_secs(&state.runtime_config),
        operations::SHARE_STREAM_SESSION_TTL_SECS_KEY,
    )
}

fn build_payload(subject: ShareStreamSubject, ttl_secs: i64) -> ShareStreamSessionPayload {
    ShareStreamSessionPayload {
        subject,
        exp: (Utc::now() + Duration::seconds(ttl_secs)).timestamp(),
        nonce: crate::utils::id::new_short_token(),
    }
}

fn build_session_for_shared_file(
    state: &PrimaryAppState,
    share: &share::Model,
    file: &file::Model,
    payload: &ShareStreamSessionPayload,
    request_origin: Option<crate::services::preview_link_service::RequestOrigin<'_>>,
) -> Result<ShareStreamSessionInfo> {
    let token = encode_shared_session(share, file, payload, &state.config.auth.jwt_secret)?;
    Ok(ShareStreamSessionInfo {
        path: stream_path(
            &state.runtime_config,
            &share.token,
            &token,
            &file.name,
            request_origin,
        ),
        expires_at: decode_expiry(payload.exp)?,
    })
}

fn stream_path(
    runtime_config: &crate::config::RuntimeConfig,
    share_token: &str,
    session_token: &str,
    file_name: &str,
    request_origin: Option<crate::services::preview_link_service::RequestOrigin<'_>>,
) -> String {
    let path = format!(
        "/api/v1/s/{share_token}/stream/{session_token}/{}",
        urlencoding::encode(file_name)
    );
    request_origin
        .and_then(|origin| {
            site_url::public_app_url_for_request(runtime_config, &path, origin.scheme, origin.host)
        })
        .unwrap_or_else(|| site_url::public_app_url_or_path(runtime_config, &path))
}

fn encode_shared_session(
    share: &share::Model,
    file: &file::Model,
    payload: &ShareStreamSessionPayload,
    secret: &str,
) -> Result<String> {
    let payload_segment = encode_payload(payload)?;
    let signature = sign_shared_payload(share, file, &payload_segment, secret);
    Ok(format!("{payload_segment}.{signature}"))
}

fn encode_payload(payload: &ShareStreamSessionPayload) -> Result<String> {
    let bytes = serde_json::to_vec(payload).map_aster_err_ctx(
        "failed to encode share stream session",
        AsterError::internal_error,
    )?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

async fn resolve_session_target(
    state: &PrimaryAppState,
    share_token: &str,
    session_token: &str,
) -> Result<(ShareStreamSessionPayload, ResolvedShareStreamTarget)> {
    let (payload_segment, signature) = split_token(session_token)?;
    let payload = decode_payload(payload_segment)?;
    let expires_at = decode_expiry(payload.exp)?;
    if expires_at < Utc::now() {
        release_counted_marker(state, session_token).await;
        return Err(AsterError::share_expired("share stream session expired"));
    }

    ensure_payload_share_token(&payload, share_token)?;
    let marker_state = count_marker_state(state, session_token).await;
    let (share, file) = load_target(state, &payload, marker_state.is_some()).await?;
    if !verify_shared_payload(
        &share,
        &file,
        payload_segment,
        signature,
        &state.config.auth.jwt_secret,
    ) {
        return Err(AsterError::share_not_found(
            "share stream session token signature mismatch",
        ));
    }

    let target = if marker_state.is_some() {
        ResolvedShareStreamTarget::Marked { share, file }
    } else {
        ResolvedShareStreamTarget::Fresh { share, file }
    };
    Ok((payload, target))
}

async fn load_target(
    state: &PrimaryAppState,
    payload: &ShareStreamSessionPayload,
    counted: bool,
) -> Result<(share::Model, file::Model)> {
    match &payload.subject {
        ShareStreamSubject::ShareFile { share_token } => {
            if counted {
                share_service::load_shared_file_ignoring_download_limit(state, share_token).await
            } else {
                share_service::load_preview_shared_file(state, share_token).await
            }
        }
        ShareStreamSubject::ShareFolderFile {
            share_token,
            file_id,
        } => {
            if counted {
                share_service::load_shared_folder_file_ignoring_download_limit(
                    state,
                    share_token,
                    *file_id,
                )
                .await
            } else {
                share_service::load_preview_shared_folder_file(state, share_token, *file_id).await
            }
        }
    }
}

fn split_token(token: &str) -> Result<(&str, &str)> {
    let (payload_segment, signature) = token
        .split_once('.')
        .ok_or_else(|| AsterError::share_not_found("invalid share stream session token"))?;
    if payload_segment.is_empty() || signature.is_empty() {
        return Err(AsterError::share_not_found(
            "invalid share stream session token",
        ));
    }
    Ok((payload_segment, signature))
}

fn decode_payload(payload_segment: &str) -> Result<ShareStreamSessionPayload> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_segment)
        .map_aster_err_with(|| AsterError::share_not_found("invalid share stream session token"))?;
    serde_json::from_slice::<ShareStreamSessionPayload>(&bytes)
        .map_aster_err_with(|| AsterError::share_not_found("invalid share stream session token"))
}

fn decode_expiry(exp: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(exp, 0)
        .ok_or_else(|| AsterError::share_not_found("invalid share stream session expiry"))
}

fn ensure_payload_share_token(
    payload: &ShareStreamSessionPayload,
    requested_share_token: &str,
) -> Result<()> {
    let payload_share_token = match &payload.subject {
        ShareStreamSubject::ShareFile { share_token }
        | ShareStreamSubject::ShareFolderFile { share_token, .. } => share_token,
    };
    if payload_share_token != requested_share_token {
        return Err(AsterError::share_not_found(
            "share stream session token target mismatch",
        ));
    }
    Ok(())
}

fn sign_shared_payload(
    share: &share::Model,
    file: &file::Model,
    payload_segment: &str,
    secret: &str,
) -> String {
    use hmac::Mac;
    let digest = shared_payload_mac(share, file, payload_segment, secret)
        .finalize()
        .into_bytes();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn shared_payload_mac(
    share: &share::Model,
    file: &file::Model,
    payload_segment: &str,
    secret: &str,
) -> hmac::Hmac<sha2::Sha256> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"share_stream_session:");
    mac.update(share.token.as_bytes());
    mac.update(b":");
    mac.update(share.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(file.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(payload_segment.as_bytes());
    mac
}

fn verify_shared_payload(
    share: &share::Model,
    file: &file::Model,
    payload_segment: &str,
    signature: &str,
    secret: &str,
) -> bool {
    use hmac::Mac;
    let Ok(decoded) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    shared_payload_mac(share, file, payload_segment, secret)
        .verify_slice(&decoded)
        .is_ok()
}

async fn ensure_counted_once(
    state: &PrimaryAppState,
    session_token: &str,
    payload: &ShareStreamSessionPayload,
) -> Result<CountReservation> {
    let key = counted_cache_key(session_token);
    let ttl_secs = ttl_seconds(payload)?;
    for _ in 0..20 {
        match count_marker_state_by_key(state, &key).await {
            Some(CountMarkerState::Counted) => return Ok(CountReservation::AlreadyCounted),
            Some(CountMarkerState::Pending) => {
                tokio::time::sleep(StdDuration::from_millis(25)).await;
                continue;
            }
            None => {}
        }

        let pending = encode_marker_state(CountMarkerState::Pending)?;
        if state
            .cache
            .set_bytes_if_absent(&key, pending, Some(ttl_secs))
            .await
        {
            FALLBACK_COUNTED_SESSIONS
                .insert(key.clone(), CountMarkerState::Pending)
                .await;
            return Ok(CountReservation::Reserved);
        }

        tokio::time::sleep(StdDuration::from_millis(25)).await;
    }

    Err(AsterError::validation_error(
        "share stream session count is still being initialized",
    ))
}

async fn mark_counted(
    state: &PrimaryAppState,
    session_token: &str,
    payload: &ShareStreamSessionPayload,
) -> Result<()> {
    let key = counted_cache_key(session_token);
    let ttl_secs = ttl_seconds(payload)?;
    let counted = CountMarkerState::Counted;
    FALLBACK_COUNTED_SESSIONS.insert(key.clone(), counted).await;
    state
        .cache
        .set_bytes(&key, encode_marker_state(counted)?, Some(ttl_secs))
        .await;
    Ok(())
}

async fn count_marker_state(
    state: &PrimaryAppState,
    session_token: &str,
) -> Option<CountMarkerState> {
    count_marker_state_by_key(state, &counted_cache_key(session_token)).await
}

async fn count_marker_state_by_key(state: &PrimaryAppState, key: &str) -> Option<CountMarkerState> {
    let primary = state.cache.get::<CountMarkerState>(key).await;
    let fallback = FALLBACK_COUNTED_SESSIONS.get(key).await;

    match (primary, fallback) {
        (Some(CountMarkerState::Counted), _) | (_, Some(CountMarkerState::Counted)) => {
            Some(CountMarkerState::Counted)
        }
        (Some(CountMarkerState::Pending), _) | (_, Some(CountMarkerState::Pending)) => {
            Some(CountMarkerState::Pending)
        }
        (None, None) => None,
    }
}

async fn release_counted_marker(state: &PrimaryAppState, session_token: &str) {
    let key = counted_cache_key(session_token);
    state.cache.delete(&key).await;
    FALLBACK_COUNTED_SESSIONS.remove(&key).await;
}

fn ttl_seconds(payload: &ShareStreamSessionPayload) -> Result<u64> {
    let remaining = payload.exp.saturating_sub(Utc::now().timestamp());
    if remaining <= 0 {
        return Err(AsterError::share_expired("share stream session expired"));
    }
    u64::try_from(remaining).map_aster_err_ctx(
        "share stream session ttl conversion failed",
        AsterError::internal_error,
    )
}

fn counted_cache_key(session_token: &str) -> String {
    format!("{SHARE_STREAM_COUNTED_CACHE_PREFIX}{session_token}")
}

fn encode_marker_state(state: CountMarkerState) -> Result<Vec<u8>> {
    serde_json::to_vec(&state).map_aster_err_ctx(
        "failed to encode share stream count marker",
        AsterError::internal_error,
    )
}

#[cfg(test)]
mod tests {
    use super::{ShareStreamSubject, build_payload, decode_payload, encode_payload, stream_path};
    use crate::config::RuntimeConfig;
    use chrono::Utc;

    #[test]
    fn payload_roundtrips() {
        let payload = build_payload(
            ShareStreamSubject::ShareFolderFile {
                share_token: "share_token".to_string(),
                file_id: 42,
            },
            3600,
        );
        let encoded = encode_payload(&payload).unwrap();
        let decoded = decode_payload(&encoded).unwrap();

        match decoded.subject {
            ShareStreamSubject::ShareFolderFile {
                share_token,
                file_id,
            } => {
                assert_eq!(share_token, "share_token");
                assert_eq!(file_id, 42);
            }
            ShareStreamSubject::ShareFile { .. } => panic!("unexpected subject"),
        }
    }

    #[test]
    fn payload_uses_supplied_ttl() {
        let before = Utc::now().timestamp();
        let payload = build_payload(
            ShareStreamSubject::ShareFile {
                share_token: "share_token".to_string(),
            },
            7200,
        );
        let after = Utc::now().timestamp();

        assert!(payload.exp >= before + 7200);
        assert!(payload.exp <= after + 7200);
    }

    #[test]
    fn stream_path_uses_api_share_route() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            stream_path(&runtime_config, "share", "session", "movie final.mp4", None),
            "/api/v1/s/share/stream/session/movie%20final.mp4"
        );
    }
}
