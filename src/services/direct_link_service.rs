//! 服务模块：`direct_link_service`。

use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::db::repository::{file_repo, team_repo};
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    file_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};
use crate::utils::numbers::{u64_to_usize, usize_to_u64};

const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const DIRECT_LINK_SIG_LEN: usize = 6;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct DirectLinkTokenInfo {
    pub token: String,
}

pub(crate) async fn create_token_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<DirectLinkTokenInfo> {
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    let token = build_token(&file, &state.config.auth.jwt_secret)?;
    Ok(DirectLinkTokenInfo { token })
}

pub(crate) async fn load_public_file(state: &PrimaryAppState, file_id: i64) -> Result<file::Model> {
    let file = file_repo::find_by_id(&state.db, file_id).await?;
    validate_file_scope(state, &file).await?;
    Ok(file)
}

pub(crate) async fn download_file(
    state: &PrimaryAppState,
    token: &str,
    requested_name: &str,
    force_download: bool,
    if_none_match: Option<&str>,
) -> Result<file_service::DownloadOutcome> {
    let (file_id, signature) = parse_token(token)?;
    let file = load_public_file(state, file_id).await?;

    let expected_signature = signature_for_file(&file, &state.config.auth.jwt_secret)?;
    if signature != expected_signature {
        return Err(AsterError::share_not_found(
            "direct link token signature mismatch",
        ));
    }

    validate_public_file_name(&file, requested_name)?;

    let blob = file_repo::find_blob_by_id(&state.db, file.blob_id).await?;
    let disposition = if force_download {
        file_service::DownloadDisposition::Attachment
    } else {
        file_service::DownloadDisposition::Inline
    };

    file_service::build_download_outcome_with_disposition(
        state,
        &file,
        &blob,
        disposition,
        if_none_match,
    )
    .await
}

fn build_token(file: &file::Model, secret: &str) -> Result<String> {
    let file_id = u64::try_from(file.id)
        .map_aster_err_with(|| AsterError::validation_error("file id must be non-negative"))?;
    let file_part = encode_base62(file_id);
    let signature = signature_for_file(file, secret)?;
    Ok(format!("{file_part}{signature}"))
}

fn parse_token(token: &str) -> Result<(i64, &str)> {
    if token.len() <= DIRECT_LINK_SIG_LEN {
        return Err(AsterError::share_not_found("invalid direct link token"));
    }

    let (file_part, signature) = token.split_at(token.len() - DIRECT_LINK_SIG_LEN);
    let file_id = decode_base62(file_part)
        .ok_or_else(|| AsterError::share_not_found("invalid direct link token"))?;
    let file_id = i64::try_from(file_id)
        .map_aster_err_with(|| AsterError::share_not_found("invalid direct link token"))?;

    Ok((file_id, signature))
}

async fn validate_file_scope(state: &PrimaryAppState, file: &file::Model) -> Result<()> {
    if file.deleted_at.is_some() {
        return Err(AsterError::file_not_found(format!(
            "file #{} is in trash",
            file.id
        )));
    }

    if let Some(team_id) = file.team_id {
        match team_repo::find_active_by_id(&state.db, team_id).await {
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

fn signature_for_file(file: &file::Model, secret: &str) -> Result<String> {
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
    fn parse_token_valid() {
        // "a" encoded 0 + 6 char signature = "aaaaaa"
        let token = "baaaaaa"; // file_part + signature
        let (file_id, signature) = parse_token(token).unwrap();
        assert_eq!(file_id, 1); // "b" is 1 in base62
        assert_eq!(signature, "aaaaaa");
    }

    #[test]
    fn parse_token_too_short_fails() {
        let result = parse_token("short");
        assert!(result.is_err());
    }

    #[test]
    fn validate_public_file_name_exact_match() {
        let file = crate::entities::file::Model {
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        };
        assert!(validate_public_file_name(&file, "test.txt").is_ok());
    }

    #[test]
    fn validate_public_file_name_url_encoded_match() {
        let file = crate::entities::file::Model {
            id: 1,
            name: "hello world.txt".to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 100,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "text/plain".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        };
        // URL encoded space as %20
        assert!(validate_public_file_name(&file, "hello%20world.txt").is_ok());
    }

    #[test]
    fn validate_public_file_name_mismatch_fails() {
        let file = crate::entities::file::Model {
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            deleted_at: None,
            is_locked: false,
        };
        assert!(validate_public_file_name(&file, "wrong.txt").is_err());
    }
}
