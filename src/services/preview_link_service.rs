//! 服务模块：`preview_link_service`。

mod cache;

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::config::site_url;
use crate::db::repository::file_repo;
use crate::entities::{file, share};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::file_service::ResolvedDownloadRange;
use crate::services::{
    direct_link_service, file_service, share_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};

const PREVIEW_LINK_TTL_SECS: i64 = 5 * 60;
const PREVIEW_LINK_MAX_USES: u32 = 5;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PreviewLinkInfo {
    pub path: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
    pub max_uses: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PreviewSubject {
    File { file_id: i64 },
    ShareFile { share_token: String },
    ShareFolderFile { share_token: String, file_id: i64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviewTokenPayload {
    subject: PreviewSubject,
    exp: i64,
    max_uses: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
}

struct ReservedUse {
    token: String,
    slot: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct RequestOrigin<'a> {
    pub scheme: &'a str,
    pub host: &'a str,
}

enum ResolvedPreviewTarget {
    File {
        payload: PreviewTokenPayload,
        file: file::Model,
    },
    Shared {
        payload: PreviewTokenPayload,
        file: file::Model,
    },
}

pub(crate) async fn create_token_for_file_in_scope_for_origin(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    let payload = build_payload(PreviewSubject::File { file_id: file.id });
    build_link_for_file(state, &file, &payload, Some(request_origin))
}

pub async fn create_token_for_shared_file(
    state: &impl SharedRuntimeState,
    share_token: &str,
) -> Result<PreviewLinkInfo> {
    let (share, file) = share_service::load_preview_shared_file(state, share_token).await?;
    let payload = build_payload(PreviewSubject::ShareFile {
        share_token: share.token.clone(),
    });
    build_link_for_shared_file(state, &share, &file, &payload, None)
}

pub async fn create_token_for_shared_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let (share, file) = share_service::load_preview_shared_file(state, share_token).await?;
    let payload = build_payload(PreviewSubject::ShareFile {
        share_token: share.token.clone(),
    });
    build_link_for_shared_file(state, &share, &file, &payload, Some(request_origin))
}

pub async fn create_token_for_shared_folder_file(
    state: &impl SharedRuntimeState,
    share_token: &str,
    file_id: i64,
) -> Result<PreviewLinkInfo> {
    let (share, file) =
        share_service::load_preview_shared_folder_file(state, share_token, file_id).await?;
    let payload = build_payload(PreviewSubject::ShareFolderFile {
        share_token: share.token.clone(),
        file_id: file.id,
    });
    build_link_for_shared_file(state, &share, &file, &payload, None)
}

pub async fn create_token_for_shared_folder_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    file_id: i64,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let (share, file) =
        share_service::load_preview_shared_folder_file(state, share_token, file_id).await?;
    let payload = build_payload(PreviewSubject::ShareFolderFile {
        share_token: share.token.clone(),
        file_id: file.id,
    });
    build_link_for_shared_file(state, &share, &file, &payload, Some(request_origin))
}

pub(crate) async fn download_file(
    state: &PrimaryAppState,
    token: &str,
    requested_name: &str,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_service::DownloadOutcome> {
    let resolved = resolve_token(state, token).await?;
    let (payload, file) = match &resolved {
        ResolvedPreviewTarget::File { payload, file } => (payload, file),
        ResolvedPreviewTarget::Shared { payload, file, .. } => (payload, file),
    };

    direct_link_service::validate_public_file_name(file, requested_name)?;

    let blob = file_repo::find_blob_by_id(state.reader_db(), file.blob_id).await?;
    if let Some(if_none_match) = if_none_match
        && file_service::if_none_match_matches(if_none_match, &blob.hash)
    {
        return file_service::build_stream_outcome_with_disposition_and_range(
            state,
            file,
            &blob,
            file_service::DownloadDisposition::Inline,
            Some(if_none_match),
            None,
        )
        .await;
    }

    let reserved = reserve_usage(state, token, payload).await?;
    match file_service::build_stream_outcome_with_disposition_and_range(
        state,
        file,
        &blob,
        file_service::DownloadDisposition::Inline,
        None,
        range,
    )
    .await
    {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            rollback_usage(state, &reserved).await;
            Err(error)
        }
    }
}

pub(crate) async fn resolve_file_for_download(
    state: &impl SharedRuntimeState,
    token: &str,
    requested_name: &str,
) -> Result<crate::entities::file::Model> {
    let resolved = resolve_token(state, token).await?;
    let file = match &resolved {
        ResolvedPreviewTarget::File { file, .. } => file,
        ResolvedPreviewTarget::Shared { file, .. } => file,
    };
    direct_link_service::validate_public_file_name(file, requested_name)?;
    Ok(file.clone())
}

fn build_payload(subject: PreviewSubject) -> PreviewTokenPayload {
    PreviewTokenPayload {
        subject,
        exp: (Utc::now() + Duration::seconds(PREVIEW_LINK_TTL_SECS)).timestamp(),
        max_uses: PREVIEW_LINK_MAX_USES,
        nonce: Some(crate::utils::id::new_short_token()),
    }
}

fn build_link_for_file(
    state: &impl SharedRuntimeState,
    file: &file::Model,
    payload: &PreviewTokenPayload,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<PreviewLinkInfo> {
    let token = encode_file_token(file, payload, &state.config().auth.direct_link_secret)?;
    Ok(PreviewLinkInfo {
        path: preview_path(state.runtime_config(), &token, &file.name, request_origin),
        expires_at: decode_expiry(payload.exp)?,
        max_uses: payload.max_uses,
    })
}

fn build_link_for_shared_file(
    state: &impl SharedRuntimeState,
    share: &share::Model,
    file: &file::Model,
    payload: &PreviewTokenPayload,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<PreviewLinkInfo> {
    let token = encode_shared_token(
        share,
        file,
        payload,
        &state.config().auth.direct_link_secret,
    )?;
    Ok(PreviewLinkInfo {
        path: preview_path(state.runtime_config(), &token, &file.name, request_origin),
        expires_at: decode_expiry(payload.exp)?,
        max_uses: payload.max_uses,
    })
}

fn preview_path(
    runtime_config: &crate::config::RuntimeConfig,
    token: &str,
    file_name: &str,
    request_origin: Option<RequestOrigin<'_>>,
) -> String {
    let path = format!("/pv/{token}/{}", urlencoding::encode(file_name));
    request_origin
        .and_then(|origin| {
            site_url::public_app_url_for_request(runtime_config, &path, origin.scheme, origin.host)
        })
        .unwrap_or_else(|| site_url::public_app_url_or_path(runtime_config, &path))
}

fn encode_file_token(
    file: &file::Model,
    payload: &PreviewTokenPayload,
    secret: &str,
) -> Result<String> {
    let payload_segment = encode_payload(payload)?;
    let signature = sign_file_payload(file, &payload_segment, secret)?;
    Ok(format!("{payload_segment}.{signature}"))
}

fn encode_shared_token(
    share: &share::Model,
    file: &file::Model,
    payload: &PreviewTokenPayload,
    secret: &str,
) -> Result<String> {
    let payload_segment = encode_payload(payload)?;
    let signature = sign_shared_payload(share, file, &payload_segment, secret)?;
    Ok(format!("{payload_segment}.{signature}"))
}

fn encode_payload(payload: &PreviewTokenPayload) -> Result<String> {
    let bytes = serde_json::to_vec(payload)
        .map_aster_err_ctx("failed to encode preview token", AsterError::internal_error)?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

async fn resolve_token(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<ResolvedPreviewTarget> {
    let (payload_segment, signature) = split_token(token)?;
    let payload = decode_payload(payload_segment)?;
    let expires_at = decode_expiry(payload.exp)?;
    if expires_at < Utc::now() {
        return Err(AsterError::share_expired("preview link expired"));
    }

    match &payload.subject {
        PreviewSubject::File { file_id } => {
            let file = direct_link_service::load_public_file(state, *file_id).await?;
            if !verify_file_payload(
                &file,
                payload_segment,
                signature,
                &state.config().auth.direct_link_secret,
            )? {
                return Err(AsterError::share_not_found(
                    "preview link token signature mismatch",
                ));
            }
            Ok(ResolvedPreviewTarget::File { payload, file })
        }
        PreviewSubject::ShareFile { share_token } => {
            let (share, file) = share_service::load_preview_shared_file(state, share_token).await?;
            if !verify_shared_payload(
                &share,
                &file,
                payload_segment,
                signature,
                &state.config().auth.direct_link_secret,
            )? {
                return Err(AsterError::share_not_found(
                    "preview link token signature mismatch",
                ));
            }
            Ok(ResolvedPreviewTarget::Shared { payload, file })
        }
        PreviewSubject::ShareFolderFile {
            share_token,
            file_id,
        } => {
            let (share, file) =
                share_service::load_preview_shared_folder_file(state, share_token, *file_id)
                    .await?;
            if !verify_shared_payload(
                &share,
                &file,
                payload_segment,
                signature,
                &state.config().auth.direct_link_secret,
            )? {
                return Err(AsterError::share_not_found(
                    "preview link token signature mismatch",
                ));
            }
            Ok(ResolvedPreviewTarget::Shared { payload, file })
        }
    }
}

fn split_token(token: &str) -> Result<(&str, &str)> {
    let (payload_segment, signature) = token
        .split_once('.')
        .ok_or_else(|| AsterError::share_not_found("invalid preview link token"))?;
    if payload_segment.is_empty() || signature.is_empty() {
        return Err(AsterError::share_not_found("invalid preview link token"));
    }
    Ok((payload_segment, signature))
}

fn decode_payload(payload_segment: &str) -> Result<PreviewTokenPayload> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_segment)
        .map_aster_err_with(|| AsterError::share_not_found("invalid preview link token"))?;
    serde_json::from_slice::<PreviewTokenPayload>(&bytes)
        .map_aster_err_with(|| AsterError::share_not_found("invalid preview link token"))
}

fn decode_expiry(exp: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(exp, 0)
        .ok_or_else(|| AsterError::share_not_found("invalid preview link expiry"))
}

fn file_scope_signature(file: &file::Model) -> Result<String> {
    if let Some(team_id) = file.team_id {
        Ok(format!("team:{team_id}"))
    } else {
        Ok(format!(
            "user:{}",
            file.owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("file has no personal owner"))?
        ))
    }
}

fn sign_file_payload(file: &file::Model, payload_segment: &str, secret: &str) -> Result<String> {
    use hmac::Mac;
    let digest = file_payload_mac(file, payload_segment, secret)?
        .finalize()
        .into_bytes();
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest))
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

fn file_payload_mac(
    file: &file::Model,
    payload_segment: &str,
    secret: &str,
) -> Result<hmac::Hmac<sha2::Sha256>> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac =
        <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).map_err(|error| {
            AsterError::internal_error(format!("failed to initialize HMAC: {error}"))
        })?;
    mac.update(b"preview_link:file:");
    mac.update(file_scope_signature(file)?.as_bytes());
    mac.update(b":");
    mac.update(file.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(payload_segment.as_bytes());
    Ok(mac)
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
    mac.update(b"preview_link:share:");
    mac.update(share.token.as_bytes());
    mac.update(b":");
    mac.update(share.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(file.id.to_string().as_bytes());
    mac.update(b":");
    mac.update(payload_segment.as_bytes());
    Ok(mac)
}

fn verify_file_payload(
    file: &file::Model,
    payload_segment: &str,
    signature: &str,
    secret: &str,
) -> Result<bool> {
    use hmac::Mac;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature)
        .map_aster_err_with(|| AsterError::share_not_found("invalid preview link token"))?;
    Ok(file_payload_mac(file, payload_segment, secret)?
        .verify_slice(&decoded)
        .is_ok())
}

fn verify_shared_payload(
    share: &share::Model,
    file: &file::Model,
    payload_segment: &str,
    signature: &str,
    secret: &str,
) -> Result<bool> {
    use hmac::Mac;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(signature)
        .map_aster_err_with(|| AsterError::share_not_found("invalid preview link token"))?;
    Ok(shared_payload_mac(share, file, payload_segment, secret)?
        .verify_slice(&decoded)
        .is_ok())
}

async fn reserve_usage(
    state: &impl SharedRuntimeState,
    token: &str,
    payload: &PreviewTokenPayload,
) -> Result<ReservedUse> {
    let ttl_secs = ttl_seconds(payload)?;
    let marker = Vec::new();
    for slot in 0..payload.max_uses {
        if cache::reserve_usage_slot(state, token, slot, marker.clone(), ttl_secs).await {
            return Ok(ReservedUse {
                token: token.to_string(),
                slot,
            });
        }
    }

    Err(AsterError::share_download_limit(
        "preview link usage limit reached",
    ))
}

async fn rollback_usage(state: &impl SharedRuntimeState, reserved: &ReservedUse) {
    cache::release_usage_slot(state, &reserved.token, reserved.slot).await;
}

fn ttl_seconds(payload: &PreviewTokenPayload) -> Result<u64> {
    let remaining = payload.exp.saturating_sub(Utc::now().timestamp());
    if remaining <= 0 {
        return Err(AsterError::share_expired("preview link expired"));
    }
    u64::try_from(remaining).map_aster_err_ctx(
        "preview link ttl conversion failed",
        AsterError::internal_error,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        PreviewSubject, RequestOrigin, build_payload, decode_payload, encode_payload, preview_path,
        split_token,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use crate::entities::system_config;
    use base64::Engine;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: crate::types::SystemConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: crate::types::SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_SITE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn preview_path_encodes_file_name() {
        let runtime_config = RuntimeConfig::new();
        assert_eq!(
            preview_path(&runtime_config, "abc", "deck final.pptx", None),
            "/pv/abc/deck%20final.pptx"
        );
    }

    #[test]
    fn preview_path_uses_public_site_url_when_configured() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            crate::config::site_url::PUBLIC_SITE_URL_KEY,
            r#"["https://drive.example.com"]"#,
        ));

        assert_eq!(
            preview_path(&runtime_config, "abc", "deck final.pptx", None),
            "https://drive.example.com/pv/abc/deck%20final.pptx"
        );
    }

    #[test]
    fn preview_path_uses_matching_request_origin_from_public_site_url_list() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            crate::config::site_url::PUBLIC_SITE_URL_KEY,
            r#"["https://drive.example.com","https://panel.example.com"]"#,
        ));

        assert_eq!(
            preview_path(
                &runtime_config,
                "abc",
                "deck final.pptx",
                Some(RequestOrigin {
                    scheme: "https",
                    host: "panel.example.com",
                }),
            ),
            "https://panel.example.com/pv/abc/deck%20final.pptx"
        );
        assert_eq!(
            preview_path(
                &runtime_config,
                "abc",
                "deck final.pptx",
                Some(RequestOrigin {
                    scheme: "https",
                    host: "evil.example.com",
                }),
            ),
            "https://drive.example.com/pv/abc/deck%20final.pptx"
        );
    }

    #[test]
    fn split_token_rejects_invalid_value() {
        assert!(split_token("invalid").is_err());
        assert!(split_token(".sig").is_err());
        assert!(split_token("payload.").is_err());
    }

    #[test]
    fn decode_payload_rejects_garbage() {
        assert!(decode_payload("%%%").is_err());
    }

    #[test]
    fn preview_payload_nonce_makes_each_token_unique() {
        let payload_a = build_payload(PreviewSubject::File { file_id: 1 });
        let payload_b = build_payload(PreviewSubject::File { file_id: 1 });

        let encoded_a = encode_payload(&payload_a).expect("preview payload should encode");
        let encoded_b = encode_payload(&payload_b).expect("preview payload should encode");

        assert_ne!(encoded_a, encoded_b);
        assert!(payload_a.nonce.is_some());
        assert!(payload_b.nonce.is_some());
    }

    #[test]
    fn decode_payload_accepts_legacy_payload_without_nonce() {
        let legacy_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
            serde_json::json!({
                "subject": {
                    "kind": "file",
                    "file_id": 1
                },
                "exp": Utc::now().timestamp() + 60,
                "max_uses": 5
            })
            .to_string(),
        );

        let decoded = decode_payload(&legacy_payload).expect("legacy payload should decode");

        assert!(decoded.nonce.is_none());
    }
}
