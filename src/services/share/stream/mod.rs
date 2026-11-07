//! 服务模块：`share::stream`。

use actix_web::http::header::HeaderValue;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::config::{operations, site_url};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::{direct_link, file as file_ops};
use aster_drive_model::entities::{file, share};
use aster_forge_utils::numbers::u64_to_i64;

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
    version: u8,
    subject: ShareStreamSubject,
    exp: i64,
    nonce: String,
}

pub(crate) async fn create_session_for_shared_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    request_origin: crate::services::files::preview_link::RequestOrigin<'_>,
) -> Result<ShareStreamSessionInfo> {
    let (share, file) = super::load_preview_shared_file(state, share_token).await?;
    let payload = build_payload(
        ShareStreamSubject::ShareFile {
            share_token: share.token.clone(),
        },
        share_stream_session_ttl_secs_i64(state)?,
    );
    let info = build_session_for_shared_file(state, &share, &file, &payload, Some(request_origin))?;
    super::initialize_share_download(state, &share).await?;
    Ok(info)
}

pub(crate) async fn create_session_for_shared_folder_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    file_id: i64,
    request_origin: crate::services::files::preview_link::RequestOrigin<'_>,
) -> Result<ShareStreamSessionInfo> {
    let (share, file) = super::load_preview_shared_folder_file(state, share_token, file_id).await?;
    let payload = build_payload(
        ShareStreamSubject::ShareFolderFile {
            share_token: share.token.clone(),
            file_id: file.id,
        },
        share_stream_session_ttl_secs_i64(state)?,
    );
    let info = build_session_for_shared_file(state, &share, &file, &payload, Some(request_origin))?;
    super::initialize_share_download(state, &share).await?;
    Ok(info)
}

pub(crate) async fn stream_file(
    state: &PrimaryAppState,
    share_token: &str,
    session_token: &str,
    requested_name: &str,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    let (_share, file) = resolve_session_target(state, share_token, session_token).await?;
    direct_link::validate_public_file_name(&file, requested_name)?;
    let authorized = file;
    let (file, blob, revision) =
        file_ops::load_current_download_snapshot(state, authorized.id).await?;
    if file.deleted_at.is_some()
        || file.owner_user_id != authorized.owner_user_id
        || file.team_id != authorized.team_id
        || file.folder_id != authorized.folder_id
    {
        return Err(AsterError::share_not_found(
            "shared stream target changed location before download",
        ));
    }
    direct_link::validate_public_file_name(&file, requested_name)?;
    let range = file_ops::resolve_range_for_download_snapshot(&file, range_header)?;
    file_ops::build_stream_outcome_with_disposition_and_range(
        state,
        &file,
        &blob,
        file_ops::DownloadDisposition::Inline,
        None,
        range,
        &revision.etag,
    )
    .await
}

fn share_stream_session_ttl_secs_i64(state: &impl SharedRuntimeState) -> Result<i64> {
    Ok(u64_to_i64(
        operations::share_stream_session_ttl_secs(state.runtime_config()),
        operations::SHARE_STREAM_SESSION_TTL_SECS_KEY,
    )?)
}

fn build_payload(subject: ShareStreamSubject, ttl_secs: i64) -> ShareStreamSessionPayload {
    ShareStreamSessionPayload {
        version: 2,
        subject,
        exp: (Utc::now() + Duration::seconds(ttl_secs)).timestamp(),
        nonce: aster_forge_utils::id::new_short_token(),
    }
}

fn build_session_for_shared_file(
    state: &impl SharedRuntimeState,
    share: &share::Model,
    file: &file::Model,
    payload: &ShareStreamSessionPayload,
    request_origin: Option<crate::services::files::preview_link::RequestOrigin<'_>>,
) -> Result<ShareStreamSessionInfo> {
    let token = encode_shared_session(
        share,
        file,
        payload,
        &state.config().auth.direct_link_secret,
    )?;
    Ok(ShareStreamSessionInfo {
        path: stream_path(
            state.runtime_config(),
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
    request_origin: Option<crate::services::files::preview_link::RequestOrigin<'_>>,
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
    let signature = sign_shared_payload(share, file, &payload_segment, secret)?;
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
    state: &impl SharedRuntimeState,
    share_token: &str,
    session_token: &str,
) -> Result<(share::Model, file::Model)> {
    let (payload_segment, signature) = split_token(session_token)?;
    let payload = decode_payload(payload_segment)?;
    let expires_at = decode_expiry(payload.exp)?;
    if expires_at < Utc::now() {
        return Err(AsterError::share_expired("share stream session expired"));
    }

    ensure_payload_share_token(&payload, share_token)?;
    let (share, file) = load_target(state, &payload).await?;
    if !verify_shared_payload(
        &share,
        &file,
        payload_segment,
        signature,
        &state.config().auth.direct_link_secret,
    ) {
        return Err(AsterError::share_not_found(
            "share stream session token signature mismatch",
        ));
    }

    Ok((share, file))
}

async fn load_target(
    state: &impl SharedRuntimeState,
    payload: &ShareStreamSessionPayload,
) -> Result<(share::Model, file::Model)> {
    match &payload.subject {
        ShareStreamSubject::ShareFile { share_token } => {
            super::load_shared_file_ignoring_download_limit(state, share_token).await
        }
        ShareStreamSubject::ShareFolderFile {
            share_token,
            file_id,
        } => {
            super::load_shared_folder_file_ignoring_download_limit(state, share_token, *file_id)
                .await
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
    let payload = serde_json::from_slice::<ShareStreamSessionPayload>(&bytes)
        .map_aster_err_with(|| AsterError::share_not_found("invalid share stream session token"))?;
    match payload.version {
        2 => Ok(payload),
        _ => Err(AsterError::share_not_found(
            "restart pre-upgrade stream session",
        )),
    }
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
) -> Result<String> {
    use hmac::Mac;
    let digest = shared_payload_mac(share, file, payload_segment, secret)?
        .finalize()
        .into_bytes();
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest))
}

fn shared_payload_mac(
    share: &share::Model,
    file: &file::Model,
    payload_segment: &str,
    secret: &str,
) -> Result<hmac::Hmac<sha2::Sha256>> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac =
        <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).map_err(|error| {
            AsterError::internal_error(format!("failed to initialize HMAC: {error}"))
        })?;
    mac.update(b"share_stream_session:v2:");
    mac.update(share.token.as_bytes());
    mac.update(b":");
    mac.update(share.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(file.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(payload_segment.as_bytes());
    Ok(mac)
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
        .is_ok_and(|mac| mac.verify_slice(&decoded).is_ok())
}

#[cfg(test)]
mod tests {
    use super::{ShareStreamSubject, build_payload, decode_payload, encode_payload, stream_path};
    use crate::config::RuntimeConfig;
    use chrono::Utc;

    #[test]
    fn uncharged_legacy_or_unknown_version_sessions_cannot_be_redeemed() {
        use base64::Engine;
        let mut value = serde_json::to_value(build_payload(
            ShareStreamSubject::ShareFile {
                share_token: "share".into(),
            },
            3600,
        ))
        .unwrap();
        value.as_object_mut().unwrap().remove("version");
        let encode = |value: &serde_json::Value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(value).unwrap())
        };
        assert!(decode_payload(&encode(&value)).is_err());
        value["version"] = serde_json::json!(1);
        assert!(decode_payload(&encode(&value)).is_err());
    }

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
