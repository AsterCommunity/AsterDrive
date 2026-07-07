//! 服务模块：`direct_link`。

use base64::Engine;
use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::db::repository::{file_repo, team_repo};
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::file::{self as file_ops, ResolvedDownloadRange},
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::utils::numbers::{u64_to_usize, usize_to_u64};

const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const DIRECT_LINK_SIG_LEN: usize = 6;
const DIRECT_LINK_V2_PREFIX: &str = "v2";

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct DirectLinkTokenInfo {
    pub token: String,
}

pub(crate) async fn create_token_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<DirectLinkTokenInfo> {
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    let token = build_token(&file, &state.config().auth.direct_link_secret)?;
    Ok(DirectLinkTokenInfo { token })
}

pub(crate) async fn load_public_file(
    state: &impl SharedRuntimeState,
    file_id: i64,
) -> Result<file::Model> {
    let file = file_repo::find_by_id(state.reader_db(), file_id).await?;
    validate_file_scope(state, &file).await?;
    Ok(file)
}

pub(crate) async fn resolve_file_for_download(
    state: &impl SharedRuntimeState,
    token: &str,
    requested_name: &str,
) -> Result<file::Model> {
    let parsed = parse_token(token)?;
    let file_id = parsed.file_id();
    let file = load_public_file(state, file_id).await?;

    if !verify_token_signature(&file, &parsed, &state.config().auth.direct_link_secret)? {
        return Err(AsterError::share_not_found(
            "direct link token signature mismatch",
        ));
    }

    validate_public_file_name(&file, requested_name)?;
    Ok(file)
}

pub(crate) async fn download_file(
    state: &PrimaryAppState,
    token: &str,
    requested_name: &str,
    force_download: bool,
    if_none_match: Option<&str>,
    range: Option<ResolvedDownloadRange>,
) -> Result<file_ops::DownloadOutcome> {
    let file = resolve_file_for_download(state, token, requested_name).await?;
    let blob = file_repo::find_blob_by_id(state.reader_db(), file.blob_id).await?;
    let disposition = if force_download {
        file_ops::DownloadDisposition::Attachment
    } else {
        file_ops::DownloadDisposition::Inline
    };

    file_ops::build_download_outcome_with_disposition_and_range(
        state,
        &file,
        &blob,
        disposition,
        if_none_match,
        range,
    )
    .await
}

fn build_token(file: &file::Model, secret: &str) -> Result<String> {
    let file_id = u64::try_from(file.id)
        .map_aster_err_with(|| AsterError::validation_error("file id must be non-negative"))?;
    let file_part = encode_base62(file_id);
    let signature = sign_v2_file(file, secret)?;
    Ok(format!("{DIRECT_LINK_V2_PREFIX}.{file_part}.{signature}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParsedDirectLinkToken<'a> {
    V2 { file_id: i64, signature: &'a str },
    // Legacy tokens are historical public direct links. They use the old compact
    // `<base62 file id><6-char signature>` shape, so they have no explicit
    // version marker. Keep verification support here to avoid breaking copied
    // links, but never mint new tokens in this format.
    Legacy { file_id: i64, signature: &'a str },
}

impl ParsedDirectLinkToken<'_> {
    fn file_id(&self) -> i64 {
        match self {
            Self::V2 { file_id, .. } | Self::Legacy { file_id, .. } => *file_id,
        }
    }
}

fn parse_token(token: &str) -> Result<ParsedDirectLinkToken<'_>> {
    // New direct links are explicit and unambiguous:
    // `v2.<base62 file id>.<base64url HMAC-SHA256 signature>`.
    // The prefix lets us change token construction again later without guessing
    // where the file id ends.
    if let Some(rest) = token.strip_prefix("v2.") {
        let (file_part, signature) = rest
            .split_once('.')
            .ok_or_else(|| AsterError::share_not_found("invalid direct link token"))?;
        if file_part.is_empty() || signature.is_empty() {
            return Err(AsterError::share_not_found("invalid direct link token"));
        }
        let file_id = decode_base62_file_id(file_part)?;
        return Ok(ParsedDirectLinkToken::V2 { file_id, signature });
    }

    // No prefix means an old direct link. The old format appended a fixed-width
    // 6-character signature to the base62 file id, so parsing is only possible
    // by cutting the suffix. This branch can be removed if we decide to revoke
    // all pre-v2 public direct links.
    if token.len() <= DIRECT_LINK_SIG_LEN {
        return Err(AsterError::share_not_found("invalid direct link token"));
    }

    let (file_part, signature) = token.split_at(token.len() - DIRECT_LINK_SIG_LEN);
    let file_id = decode_base62_file_id(file_part)?;

    Ok(ParsedDirectLinkToken::Legacy { file_id, signature })
}

async fn validate_file_scope(state: &impl SharedRuntimeState, file: &file::Model) -> Result<()> {
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{} is in trash",
            file.id
        )));
    }

    if let Some(team_id) = file.team_id {
        match team_repo::find_active_by_id(state.reader_db(), team_id).await {
            Ok(_) => {}
            Err(AsterError::RecordNotFound(_)) => {
                return Err(AsterError::share_not_found("direct link team is inactive"));
            }
            Err(error) => return Err(error),
        }
    } else {
        workspace_storage_service::ensure_personal_file_scope(file)?;
    }

    Ok(())
}

fn file_scope_signature(file: &file::Model) -> Result<String> {
    // Bind tokens to the namespace that owns the file, not just the file id.
    // That prevents a token from remaining valid if inconsistent data points
    // the same id at a different personal/team scope.
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

fn direct_link_mac(file: &file::Model, secret: &str) -> Result<hmac::Hmac<sha2::Sha256>> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac =
        <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).map_err(|error| {
            AsterError::internal_error(format!("failed to initialize HMAC: {error}"))
        })?;
    // v2 direct links use HMAC-SHA256 instead of the old truncated SHA256
    // construction. The message includes a purpose string, scope, and file id
    // so the same JWT secret cannot accidentally sign another token family.
    mac.update(b"direct_link:v2:");
    mac.update(file_scope_signature(file)?.as_bytes());
    mac.update(b":");
    mac.update(file.id.to_string().as_bytes());
    Ok(mac)
}

fn sign_v2_file(file: &file::Model, secret: &str) -> Result<String> {
    use hmac::Mac;
    let digest = direct_link_mac(file, secret)?.finalize().into_bytes();
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest))
}

fn verify_token_signature(
    file: &file::Model,
    parsed: &ParsedDirectLinkToken<'_>,
    secret: &str,
) -> Result<bool> {
    use hmac::Mac;
    match parsed {
        ParsedDirectLinkToken::V2 { signature, .. } => {
            let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(signature)
                .map_aster_err_with(|| AsterError::share_not_found("invalid direct link token"))?;
            Ok(direct_link_mac(file, secret)?
                .verify_slice(&decoded)
                .is_ok())
        }
        ParsedDirectLinkToken::Legacy { signature, .. } => {
            // Compatibility only: legacy_signature_for_file reproduces the old
            // 6-character signature algorithm, but verification still depends
            // on auth.direct_link_secret, so rotating that secret invalidates
            // legacy links. New tokens never use this path.
            let expected = legacy_signature_for_file(file, secret)?;
            Ok(*signature == expected)
        }
    }
}

fn legacy_signature_for_file(file: &file::Model, secret: &str) -> Result<String> {
    // Historical algorithm for pre-v2 direct links:
    // SHA256("direct_link:{secret}:{scope}:{file_id}") -> first 32 bits ->
    // fixed-width base62. This is much weaker than v2 HMAC and remains here only
    // as a migration bridge for already-shared links.
    let scope_part = if let Some(team_id) = file.team_id {
        format!("team:{team_id}")
    } else {
        format!(
            "user:{}",
            file.owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("file has no personal owner"))?
        )
    };

    let mut hasher = Sha256::new();
    hasher.update(format!("direct_link:{secret}:{scope_part}:{}", file.id).as_bytes());
    let digest = hasher.finalize();
    let signature_value = u64::from(u32::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3],
    ]));

    encode_base62_fixed(signature_value, DIRECT_LINK_SIG_LEN)
}

fn decode_base62_file_id(file_part: &str) -> Result<i64> {
    let file_id = decode_base62(file_part)
        .ok_or_else(|| AsterError::share_not_found("invalid direct link token"))?;
    i64::try_from(file_id)
        .map_aster_err_with(|| AsterError::share_not_found("invalid direct link token"))
}

fn encode_base62(mut value: u64) -> String {
    if value == 0 {
        return "a".to_string();
    }

    let mut encoded = Vec::new();
    while value > 0 {
        let digit_index = u64_to_usize(value % 62, "base62 digit index").unwrap_or(0);
        encoded.push(char::from(BASE62[digit_index]));
        value /= 62;
    }
    encoded.iter().rev().collect()
}

fn encode_base62_fixed(mut value: u64, width: usize) -> Result<String> {
    let mut encoded = vec![char::from(BASE62[0]); width];
    for index in (0..width).rev() {
        let digit_index = u64_to_usize(value % 62, "base62 digit index")?;
        encoded[index] = char::from(BASE62[digit_index]);
        value /= 62;
    }

    if value > 0 {
        return Err(AsterError::internal_error(
            "direct link signature overflowed fixed width",
        ));
    }

    Ok(encoded.into_iter().collect())
}

fn decode_base62(value: &str) -> Option<u64> {
    if value.is_empty() {
        return None;
    }

    let mut decoded = 0u64;
    for byte in value.bytes() {
        let digit = usize_to_u64(
            BASE62.iter().position(|candidate| *candidate == byte)?,
            "base62 digit index",
        )
        .ok()?;
        decoded = decoded.checked_mul(62)?.checked_add(digit)?;
    }
    Some(decoded)
}

pub(crate) fn validate_public_file_name(file: &file::Model, requested_name: &str) -> Result<()> {
    if requested_name == file.name {
        return Ok(());
    }

    if let Ok(decoded_name) = urlencoding::decode(requested_name)
        && decoded_name.as_ref() == file.name.as_str()
    {
        return Ok(());
    }

    Err(AsterError::share_not_found(format!(
        "direct link path mismatch for file #{}",
        file.id
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file() -> file::Model {
        crate::entities::file::Model {
            id: 1,
            name: "test.txt".to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 100,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "text/plain".to_string(),
            extension: "txt".to_string(),
            compound_extension: None,
            file_category: crate::types::FileCategory::Document,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        }
    }

    #[test]
    fn encode_base62_zero_returns_a() {
        assert_eq!(encode_base62(0), "a");
    }

    #[test]
    fn encode_base62_roundtrip() {
        let original: u64 = 12345678901234567890;
        let encoded = encode_base62(original);
        let decoded = decode_base62(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn decode_base62_empty_returns_none() {
        assert_eq!(decode_base62(""), None);
    }

    #[test]
    fn decode_base62_invalid_char_returns_none() {
        assert_eq!(decode_base62("!@#$"), None);
    }

    #[test]
    fn encode_base62_fixed_width_exact() {
        // value that fits exactly in 6 chars
        let value: u64 = 62 * 62 * 62; // small enough
        let encoded = encode_base62_fixed(value, 6).unwrap();
        assert_eq!(encoded.len(), 6);
    }

    #[test]
    fn encode_base62_fixed_overflow_fails() {
        // u64::MAX doesn't fit in 6 chars
        let result = encode_base62_fixed(u64::MAX, 6);
        assert!(result.is_err());
    }

    #[test]
    fn parse_token_valid_legacy() {
        // "a" encoded 0 + 6 char signature = "aaaaaa"
        let token = "baaaaaa"; // file_part + signature
        let parsed = parse_token(token).unwrap();
        assert_eq!(parsed.file_id(), 1); // "b" is 1 in base62
        assert_eq!(
            parsed,
            ParsedDirectLinkToken::Legacy {
                file_id: 1,
                signature: "aaaaaa"
            }
        );
    }

    #[test]
    fn build_token_uses_v2_hmac_signature() {
        let file = test_file();
        let token = build_token(&file, "secret").unwrap();
        let parsed = parse_token(&token).unwrap();

        assert!(token.starts_with("v2.b."));
        assert_eq!(parsed.file_id(), file.id);
        assert!(verify_token_signature(&file, &parsed, "secret").unwrap());
    }

    #[test]
    fn verify_v2_token_rejects_tampered_signature() {
        let file = test_file();
        let token = build_token(&file, "secret").unwrap();
        let tampered = format!("{token}a");
        let parsed = parse_token(&tampered).unwrap();

        assert!(!verify_token_signature(&file, &parsed, "secret").unwrap());
    }

    #[test]
    fn verify_v2_token_rejects_jwt_secret_when_direct_link_secret_differs() {
        let file = test_file();
        let token = build_token(&file, "dedicated-direct-link-secret").unwrap();
        let parsed = parse_token(&token).unwrap();

        assert!(
            !verify_token_signature(&file, &parsed, "jwt-secret").unwrap(),
            "direct link tokens must not validate with auth.jwt_secret"
        );
    }

    #[test]
    fn verify_v2_token_rejects_wrong_scope() {
        let file = test_file();
        let mut other_file = test_file();
        other_file.owner_user_id = Some(2);
        let token = build_token(&file, "secret").unwrap();
        let parsed = parse_token(&token).unwrap();

        assert!(!verify_token_signature(&other_file, &parsed, "secret").unwrap());
    }

    #[test]
    fn verify_legacy_token_still_supported() {
        let file = test_file();
        let signature = legacy_signature_for_file(&file, "secret").unwrap();
        let parsed = ParsedDirectLinkToken::Legacy {
            file_id: file.id,
            signature: &signature,
        };

        assert!(verify_token_signature(&file, &parsed, "secret").unwrap());
    }

    #[test]
    fn verify_legacy_token_rejects_jwt_secret_when_direct_link_secret_differs() {
        let file = test_file();
        let signature = legacy_signature_for_file(&file, "dedicated-direct-link-secret").unwrap();
        let parsed = ParsedDirectLinkToken::Legacy {
            file_id: file.id,
            signature: &signature,
        };

        assert!(
            !verify_token_signature(&file, &parsed, "jwt-secret").unwrap(),
            "legacy direct links must also be scoped to direct_link_secret"
        );
    }

    #[test]
    fn parse_token_too_short_fails() {
        let result = parse_token("short");
        assert!(result.is_err());
    }

    #[test]
    fn validate_public_file_name_exact_match() {
        let file = test_file();
        assert!(validate_public_file_name(&file, "test.txt").is_ok());
    }

    #[test]
    fn validate_public_file_name_url_encoded_match() {
        let file = file::Model {
            name: "hello world.txt".to_string(),
            ..test_file()
        };
        // URL encoded space as %20
        assert!(validate_public_file_name(&file, "hello%20world.txt").is_ok());
    }

    #[test]
    fn validate_public_file_name_mismatch_fails() {
        let file = test_file();
        assert!(validate_public_file_name(&file, "wrong.txt").is_err());
    }
}
