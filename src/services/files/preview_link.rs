//! 服务模块：`preview_link`。

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
use crate::services::{
    files::{
        direct_link,
        file::{self as file_ops, ResolvedDownloadRange},
    },
    share::{
        load_shared_file_ignoring_download_limit, load_shared_folder_file_ignoring_download_limit,
    },
    workspace::storage::{self, WorkspaceStorageScope},
};

const PREVIEW_LINK_TTL_SECS: i64 = 5 * 60;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PreviewLinkInfo {
    pub path: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
    pub etag: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct RequestOrigin<'a> {
    pub scheme: &'a str,
    pub host: &'a str,
}

enum ResolvedPreviewTarget {
    File { file: file::Model },
    Shared { file: file::Model },
}

pub(crate) async fn create_token_for_file_in_scope_for_origin(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let file = storage::verify_file_access(state, scope, file_id).await?;
    let payload = build_payload(PreviewSubject::File { file_id: file.id });
    build_link_for_file(state, &file, &payload, Some(request_origin)).await
}

pub async fn create_token_for_shared_file(
    state: &impl SharedRuntimeState,
    share_token: &str,
) -> Result<PreviewLinkInfo> {
    let (share, file) = load_shared_file_ignoring_download_limit(state, share_token).await?;
    let payload = build_payload(PreviewSubject::ShareFile {
        share_token: share.token.clone(),
    });
    build_link_for_shared_file(state, &share, &file, &payload, None).await
}

pub async fn create_token_for_shared_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let (share, file) = load_shared_file_ignoring_download_limit(state, share_token).await?;
    let payload = build_payload(PreviewSubject::ShareFile {
        share_token: share.token.clone(),
    });
    build_link_for_shared_file(state, &share, &file, &payload, Some(request_origin)).await
}

pub async fn create_token_for_shared_folder_file(
    state: &impl SharedRuntimeState,
    share_token: &str,
    file_id: i64,
) -> Result<PreviewLinkInfo> {
    let (share, file) =
        load_shared_folder_file_ignoring_download_limit(state, share_token, file_id).await?;
    let payload = build_payload(PreviewSubject::ShareFolderFile {
        share_token: share.token.clone(),
        file_id: file.id,
    });
    build_link_for_shared_file(state, &share, &file, &payload, None).await
}

pub async fn create_token_for_shared_folder_file_for_origin(
    state: &impl SharedRuntimeState,
    share_token: &str,
    file_id: i64,
    request_origin: RequestOrigin<'_>,
) -> Result<PreviewLinkInfo> {
    let (share, file) =
        load_shared_folder_file_ignoring_download_limit(state, share_token, file_id).await?;
    let payload = build_payload(PreviewSubject::ShareFolderFile {
        share_token: share.token.clone(),
        file_id: file.id,
    });
    build_link_for_shared_file(state, &share, &file, &payload, Some(request_origin)).await
}

pub(crate) async fn download_file(
    state: &PrimaryAppState,
    token: &str,
    requested_name: &str,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_ops::DownloadOutcome> {
    let resolved = resolve_token(state, token).await?;
    let file = match &resolved {
        ResolvedPreviewTarget::File { file, .. } => file,
        ResolvedPreviewTarget::Shared { file, .. } => file,
    };

    direct_link::validate_public_file_name(file, requested_name)?;

    let blob = file_repo::find_blob_by_id(state.reader_db(), file.blob_id).await?;
    if let Some(if_none_match) = if_none_match
        && file_ops::if_none_match_matches(if_none_match, &blob.hash)
    {
        return file_ops::build_download_outcome_with_disposition_and_range(
            state,
            file,
            &blob,
            file_ops::DownloadDisposition::Inline,
            Some(if_none_match),
            None,
        )
        .await;
    }

    file_ops::build_download_outcome_with_disposition_and_range(
        state,
        file,
        &blob,
        file_ops::DownloadDisposition::Inline,
        None,
        range,
    )
    .await
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
    direct_link::validate_public_file_name(file, requested_name)?;
    Ok(file.clone())
}

fn build_payload(subject: PreviewSubject) -> PreviewTokenPayload {
    PreviewTokenPayload {
        subject,
        exp: (Utc::now() + Duration::seconds(PREVIEW_LINK_TTL_SECS)).timestamp(),
        nonce: Some(crate::utils::id::new_short_token()),
    }
}

async fn build_link_for_file(
    state: &impl SharedRuntimeState,
    file: &file::Model,
    payload: &PreviewTokenPayload,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<PreviewLinkInfo> {
    let token = encode_file_token(file, payload, &state.config().auth.direct_link_secret)?;
    let etag = canonical_file_etag(state, file).await?;
    Ok(PreviewLinkInfo {
        path: preview_path(state.runtime_config(), &token, &file.name, request_origin),
        expires_at: decode_expiry(payload.exp)?,
        etag,
    })
}

async fn build_link_for_shared_file(
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
    let etag = canonical_file_etag(state, file).await?;
    Ok(PreviewLinkInfo {
        path: preview_path(state.runtime_config(), &token, &file.name, request_origin),
        expires_at: decode_expiry(payload.exp)?,
        etag,
    })
}

async fn canonical_file_etag(
    state: &impl SharedRuntimeState,
    file: &file::Model,
) -> Result<String> {
    let blob = file_repo::find_blob_by_id(state.reader_db(), file.blob_id).await?;
    Ok(format!("\"{}\"", blob.hash))
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
            let file = direct_link::load_public_file(state, *file_id).await?;
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
            Ok(ResolvedPreviewTarget::File { file })
        }
        PreviewSubject::ShareFile { share_token } => {
            let (share, file) =
                load_shared_file_ignoring_download_limit(state, share_token).await?;
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
            Ok(ResolvedPreviewTarget::Shared { file })
        }
        PreviewSubject::ShareFolderFile {
            share_token,
            file_id,
        } => {
            let (share, file) =
                load_shared_folder_file_ignoring_download_limit(state, share_token, *file_id)
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
            Ok(ResolvedPreviewTarget::Shared { file })
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

#[cfg(test)]
mod tests {
    use super::{
        PreviewSubject, RequestOrigin, build_payload, decode_payload, encode_payload, preview_path,
        split_token,
    };
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use aster_forge_db::system_config;
    use base64::Engine;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: aster_forge_config::ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: aster_forge_config::ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
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
    fn decode_payload_accepts_legacy_payload_with_usage_limit_fields() {
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
