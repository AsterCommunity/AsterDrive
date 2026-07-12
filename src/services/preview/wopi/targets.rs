//! WOPI PUT_RELATIVE / rename 目标名处理。
//!
//! 这些 helper 的职责是把 WOPI 头部里的目标名规范化，再映射回项目内部
//! “文件名必须合法、不能越界、必要时要生成唯一副本名”的语义。

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use sea_orm::ConnectionTrait;

use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::file as file_ops,
    workspace::storage::{self, WorkspaceStorageScope},
};
use crate::utils::numbers::u64_to_i64;

use super::session::{
    WopiAccessTokenPayload, create_access_token_for_file, select_public_origin_from_preselected,
};
use super::types::{WOPI_FILE_NAME_MAX_LEN, WopiPutRelativeResponse};

#[derive(Debug, Clone)]
pub(crate) enum PutRelativeTargetMode {
    Suggested(String),
    Relative {
        target_name: String,
        overwrite: bool,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedPutRelativeRequest {
    pub(crate) target_mode: PutRelativeTargetMode,
}

pub(crate) struct StoreRelativeTargetParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub existing_file_id: Option<i64>,
    pub payload: &'a mut actix_web::web::Payload,
    pub declared_size: Option<i64>,
    pub exact_name: bool,
}

impl<'a> StoreRelativeTargetParams<'a> {
    pub(crate) fn new(
        scope: WorkspaceStorageScope,
        folder_id: Option<i64>,
        filename: &'a str,
        payload: &'a mut actix_web::web::Payload,
    ) -> Self {
        Self {
            scope,
            folder_id,
            filename,
            existing_file_id: None,
            payload,
            declared_size: None,
            exact_name: false,
        }
    }

    pub(crate) fn declared_size(mut self, declared_size: Option<i64>) -> Self {
        self.declared_size = declared_size;
        self
    }

    pub(crate) fn overwrite(mut self, existing_file_id: i64) -> Self {
        self.existing_file_id = Some(existing_file_id);
        self
    }

    pub(crate) fn exact_name(mut self) -> Self {
        self.exact_name = true;
        self
    }
}

pub(crate) fn parse_put_relative_request(
    source_file_name: &str,
    suggested_target: Option<&str>,
    relative_target: Option<&str>,
    overwrite_relative_target: Option<&str>,
) -> Result<ParsedPutRelativeRequest> {
    // WOPI 规范要求 SuggestedTarget 和 RelativeTarget 二选一。
    // 这里先把 header 级别的协议约束收口，再交给后续逻辑处理实际文件操作。
    match (suggested_target, relative_target) {
        (Some(_), Some(_)) => Err(AsterError::validation_error(
            "PUT_RELATIVE requires exactly one of X-WOPI-SuggestedTarget or X-WOPI-RelativeTarget",
        )),
        (None, None) => Err(AsterError::validation_error(
            "PUT_RELATIVE requires X-WOPI-SuggestedTarget or X-WOPI-RelativeTarget",
        )),
        (Some(suggested_target), None) => {
            let decoded = decode_wopi_filename(suggested_target)?;
            let target_name = normalize_suggested_target_name(source_file_name, &decoded);
            Ok(ParsedPutRelativeRequest {
                target_mode: PutRelativeTargetMode::Suggested(target_name),
            })
        }
        (None, Some(relative_target)) => {
            let decoded = decode_wopi_filename(relative_target)?;
            let overwrite = parse_overwrite_relative_target(overwrite_relative_target)?;
            Ok(ParsedPutRelativeRequest {
                target_mode: PutRelativeTargetMode::Relative {
                    target_name: normalize_relative_target_name(&decoded)?,
                    overwrite,
                },
            })
        }
    }
}

pub(crate) fn parse_wopi_size_header(value: Option<&str>) -> Result<Option<i64>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = value.parse::<u64>().map_aster_err_with(|| {
        AsterError::validation_error("X-WOPI-Size header must be a non-negative integer")
    })?;
    Ok(Some(u64_to_i64(parsed, "wopi size header")?))
}

fn parse_overwrite_relative_target(raw: Option<&str>) -> Result<bool> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(false);
    };

    if raw.eq_ignore_ascii_case("true") {
        Ok(true)
    } else if raw.eq_ignore_ascii_case("false") {
        Ok(false)
    } else {
        Err(AsterError::validation_error(
            "X-WOPI-OverwriteRelativeTarget must be true or false",
        ))
    }
}

pub(crate) fn normalize_relative_target_name(value: &str) -> Result<String> {
    Ok(aster_forge_validation::filename::normalize_validate_name(
        value,
    )?)
}

pub(crate) fn normalize_requested_rename_target(
    source_file_name: &str,
    requested_name: Option<&str>,
) -> std::result::Result<String, String> {
    let Some(requested_name) = requested_name else {
        return Err("X-WOPI-RequestedName header is required".to_string());
    };

    // rename 的 header 可能只传 stem，不带原扩展名。
    // 这里会按 WOPI 约定把扩展名拼回去，并尽量把非法名字降级成可接受的候选值。
    let decoded = decode_wopi_filename(requested_name).map_err(|err| err.message().to_string())?;
    match build_requested_rename_filename(source_file_name, &decoded) {
        Ok(name) => Ok(name),
        Err(_) => sanitize_requested_rename_name(source_file_name, &decoded)
            .ok_or_else(|| "invalid requested file name".to_string()),
    }
}

fn build_requested_rename_filename(source_file_name: &str, requested_name: &str) -> Result<String> {
    let requested_name = requested_name.trim();
    if requested_name.is_empty() {
        return Err(AsterError::validation_error(
            "requested file name cannot be empty",
        ));
    }

    let full_name = rename_target_name(source_file_name, requested_name);
    Ok(aster_forge_validation::filename::normalize_validate_name(
        &full_name,
    )?)
}

fn sanitize_requested_rename_name(source_file_name: &str, requested_name: &str) -> Option<String> {
    let mut sanitized: String = requested_name
        .chars()
        .filter(|ch| !is_forbidden_file_name_char(*ch))
        .collect();
    sanitized = sanitized.trim().trim_end_matches('.').to_string();
    truncate_utf8_to_len(&mut sanitized, max_requested_rename_len(source_file_name));
    sanitized = sanitized.trim().trim_end_matches('.').to_string();

    (!sanitized.is_empty())
        .then(|| build_requested_rename_filename(source_file_name, &sanitized).ok())
        .flatten()
}

fn rename_target_name(source_file_name: &str, requested_name: &str) -> String {
    match file_extension(source_file_name) {
        Some(ext) => format!("{requested_name}.{ext}"),
        None => requested_name.to_string(),
    }
}

fn max_requested_rename_len(source_file_name: &str) -> usize {
    usize::try_from(WOPI_FILE_NAME_MAX_LEN).unwrap_or(255)
        - file_extension(source_file_name).map_or(0, |ext| ext.len() + 1)
}

pub(crate) fn response_name_for_rename<'a>(
    source_file_name: &str,
    renamed_file_name: &'a str,
) -> &'a str {
    if file_extension(source_file_name).is_some() {
        source_file_stem(renamed_file_name)
    } else {
        renamed_file_name
    }
}

fn truncate_utf8_to_len(value: &mut String, max_len: usize) {
    if value.len() <= max_len {
        return;
    }

    let mut truncate_at = 0;
    for (index, ch) in value.char_indices() {
        let next = index + ch.len_utf8();
        if next > max_len {
            break;
        }
        truncate_at = next;
    }

    value.truncate(truncate_at);
}

fn normalize_suggested_target_name(source_file_name: &str, value: &str) -> String {
    let candidate = if value.starts_with('.') {
        format!("{}{}", source_file_stem(source_file_name), value)
    } else {
        value.to_string()
    };

    sanitize_suggested_target_name(&candidate, source_file_name)
}

fn sanitize_suggested_target_name(candidate: &str, fallback: &str) -> String {
    let mut sanitized: String = candidate
        .chars()
        .filter(|ch| !is_forbidden_file_name_char(*ch))
        .collect();
    sanitized = sanitized.trim().trim_end_matches('.').to_string();

    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        return fallback.to_string();
    }

    if sanitized.len() > 255 {
        truncate_utf8_to_len(&mut sanitized, 255);
        sanitized = sanitized.trim().trim_end_matches('.').to_string();
    }

    if let Ok(normalized) = aster_forge_validation::filename::normalize_validate_name(&sanitized) {
        normalized
    } else {
        fallback.to_string()
    }
}

fn source_file_stem(value: &str) -> &str {
    match value.rfind('.') {
        Some(dot) if dot > 0 => &value[..dot],
        _ => value,
    }
}

fn is_forbidden_file_name_char(ch: char) -> bool {
    matches!(
        ch,
        '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
    ) || ch.is_ascii_control()
}

pub(crate) async fn find_file_by_name_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<file::Model>> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_name_in_folder(db, user_id, folder_id, name).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_name_in_team_folder(db, team_id, folder_id, name).await
        }
    }
}

pub(crate) async fn suggest_available_relative_target(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<String> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::resolve_unique_filename(state.writer_db(), user_id, folder_id, name).await
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::resolve_unique_team_filename(state.writer_db(), team_id, folder_id, name)
                .await
        }
    }
}

pub(crate) async fn resolve_available_rename_target(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    current_file_id: i64,
    requested_name: &str,
) -> Result<String> {
    let existing =
        find_file_by_name_in_scope(state.writer_db(), scope, folder_id, requested_name).await?;
    if match existing.as_ref() {
        None => true,
        Some(file) => file.id == current_file_id,
    } {
        return Ok(requested_name.to_string());
    }

    suggest_available_relative_target(state, scope, folder_id, requested_name).await
}

pub(crate) async fn store_relative_target_from_stream(
    state: &PrimaryAppState,
    params: StoreRelativeTargetParams<'_>,
) -> Result<file::Model> {
    let StoreRelativeTargetParams {
        scope,
        folder_id,
        filename,
        existing_file_id,
        payload,
        declared_size,
        exact_name,
    } = params;
    let resolved_policy_hint = match declared_size {
        Some(size) => Some(storage::resolve_policy_for_size(state, scope, folder_id, size).await?),
        None => None,
    };
    let streamed = file_ops::stream_request_body_to_temp_upload(
        state,
        payload,
        resolved_policy_hint,
        declared_size,
    )
    .await?;
    let file_ops::StreamedTempUpload {
        temp_path,
        size,
        resolved_policy,
        precomputed_hash,
    } = streamed;

    let result = if exact_name {
        storage::store_from_temp_exact_name_with_hints(
            state,
            storage::StoreFromTempParams {
                scope,
                folder_id,
                filename,
                temp_path: &temp_path,
                size,
                existing_file_id,
                skip_lock_check: existing_file_id.is_some(),
            },
            storage::StoreFromTempHints {
                resolved_policy,
                precomputed_hash: precomputed_hash.as_deref(),
                actor_username: None,
                ..Default::default()
            },
        )
        .await
    } else {
        storage::store_from_temp_with_hints(
            state,
            storage::StoreFromTempParams {
                scope,
                folder_id,
                filename,
                temp_path: &temp_path,
                size,
                existing_file_id,
                skip_lock_check: existing_file_id.is_some(),
            },
            storage::StoreFromTempHints {
                resolved_policy,
                precomputed_hash: precomputed_hash.as_deref(),
                actor_username: None,
                ..Default::default()
            },
        )
        .await
    };
    crate::utils::cleanup_temp_file(&temp_path).await;
    result
}

pub(crate) async fn build_put_relative_response(
    state: &impl SharedRuntimeState,
    payload: &WopiAccessTokenPayload,
    target_name: &str,
    target_file_id: i64,
    request_public_origin: Option<String>,
) -> Result<WopiPutRelativeResponse> {
    let access_token = create_access_token_for_file(state, payload, target_file_id).await?;
    let public_origin =
        select_public_origin_from_preselected(state, request_public_origin.as_deref())?;
    let url = format!(
        "{}?access_token={}",
        crate::config::site_url::join_origin_and_path(
            &public_origin,
            &format!("/api/v1/wopi/files/{target_file_id}"),
        ),
        urlencoding::encode(&access_token)
    );

    Ok(WopiPutRelativeResponse {
        name: target_name.to_string(),
        url,
    })
}

pub(crate) fn decode_wopi_filename(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AsterError::validation_error(
            "WOPI target header must not be empty",
        ));
    }

    let mut decoded = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '+' {
            decoded.push(ch);
            continue;
        }

        if matches!(chars.peek(), Some('-')) {
            chars.next();
            decoded.push('+');
            continue;
        }

        let mut shifted = String::new();
        while let Some(&next) = chars.peek() {
            if next == '-' {
                chars.next();
                break;
            }
            shifted.push(next);
            chars.next();
        }

        if shifted.is_empty() {
            return Err(AsterError::validation_error(
                "invalid UTF-7 sequence in WOPI target header",
            ));
        }

        let mut padded = shifted.clone();
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        let bytes = STANDARD.decode(padded.as_bytes()).map_aster_err_with(|| {
            AsterError::validation_error("invalid UTF-7 base64 payload in WOPI target header")
        })?;
        if bytes.len() % 2 != 0 {
            return Err(AsterError::validation_error(
                "invalid UTF-7 payload length in WOPI target header",
            ));
        }

        let utf16 = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]));
        for ch in char::decode_utf16(utf16) {
            decoded.push(ch.map_aster_err_with(|| {
                AsterError::validation_error("invalid UTF-16 sequence in WOPI target header")
            })?);
        }
    }

    Ok(decoded)
}

pub(crate) fn encode_wopi_filename(value: &str) -> String {
    let mut encoded = String::new();
    let mut shifted = String::new();

    let flush_shifted = |encoded: &mut String, shifted: &mut String| {
        if shifted.is_empty() {
            return;
        }

        let mut utf16 = Vec::with_capacity(shifted.len() * 2);
        for unit in shifted.encode_utf16() {
            utf16.extend_from_slice(&unit.to_be_bytes());
        }
        encoded.push('+');
        encoded.push_str(&STANDARD_NO_PAD.encode(utf16));
        encoded.push('-');
        shifted.clear();
    };

    for ch in value.chars() {
        if ch == '+' {
            flush_shifted(&mut encoded, &mut shifted);
            encoded.push_str("+-");
        } else if is_direct_utf7_char(ch) {
            flush_shifted(&mut encoded, &mut shifted);
            encoded.push(ch);
        } else {
            shifted.push(ch);
        }
    }

    flush_shifted(&mut encoded, &mut shifted);
    encoded
}

fn is_direct_utf7_char(ch: char) -> bool {
    ch.is_ascii() && !ch.is_ascii_control()
}

pub(crate) fn file_extension(file_name: &str) -> Option<String> {
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.trim().to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}
