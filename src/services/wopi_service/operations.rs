//! WOPI 文件操作入口。
//!
//! 这些函数把 WOPI 协议动作翻译回项目内部的 file/profile service。
//! 重点不是重新实现一套文件系统，而是复用已有的文件主链路，同时补上
//! WOPI 专用的 lock、rename、PUT_RELATIVE 语义。

use crate::api::subcode::ApiSubcode;
use crate::db::repository::file_repo;
use crate::errors::{AsterError, MapAsterErr, Result, precondition_failed_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditRequestInfo},
    file_service, profile_service,
};
use crate::types::NullablePatch;
use bytes::BytesMut;
use futures::StreamExt;

use super::locks::{
    active_wopi_lock_value, ensure_wopi_lock_matches, ensure_wopi_putfile_lock_matches,
    load_active_lock,
};
use super::session::{resolve_access_token, scope_from_payload};
use super::targets::{
    PutRelativeTargetMode, StoreRelativeTargetParams, build_put_relative_response,
    encode_wopi_filename, find_file_by_name_in_scope, normalize_requested_rename_target,
    parse_put_relative_request, parse_wopi_size_header, resolve_available_rename_target,
    response_name_for_rename, store_relative_target_from_stream, suggest_available_relative_target,
};
use super::types::{
    MAX_WOPI_USER_INFO_LEN, WOPI_FILE_NAME_MAX_LEN, WopiCheckFileInfo, WopiPutFileRequest,
    WopiPutFileResult, WopiPutRelativeRequest, WopiPutRelativeResult, WopiRenameFileResponse,
    WopiRenameFileResult, WopiRequestSource,
};

/// WOPI GetFile 的服务层结果，包含文件流下载数据和协议专用的 item version header。
/// 路由层负责把这些字段组装成 HttpResponse，同时附加 X-WOPI-ItemVersion 响应头。
pub struct WopiGetFileResult {
    pub outcome: file_service::DownloadOutcome,
    pub item_version: String,
}

pub async fn check_file_info(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiCheckFileInfo> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let blob = file_repo::find_blob_by_id(&state.db, resolved.file.blob_id).await?;
    let user_info =
        profile_service::get_wopi_user_info(state, resolved.payload.actor_user_id).await?;

    Ok(WopiCheckFileInfo {
        base_file_name: resolved.file.name.clone(),
        file_name_max_length: Some(WOPI_FILE_NAME_MAX_LEN),
        owner_id: match resolved.file.team_id {
            Some(team_id) => format!("team:{team_id}"),
            None => resolved
                .file
                .owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("file has no personal owner"))?
                .to_string(),
        },
        size: resolved.file.size,
        user_id: resolved.payload.actor_user_id.to_string(),
        user_can_not_write_relative: false,
        user_can_rename: true,
        user_info,
        user_can_write: true,
        read_only: false,
        supports_get_lock: true,
        supports_locks: true,
        supports_extended_lock_length: Some(true),
        supports_rename: true,
        supports_user_info: Some(true),
        supports_update: true,
        version: blob.hash,
    })
}

pub async fn get_file_contents(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    if_none_match: Option<&str>,
    max_expected_size: Option<&str>,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiGetFileResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let blob = file_repo::find_blob_by_id(&state.db, resolved.file.blob_id).await?;
    let max_expected_size = parse_wopi_max_expected_size(max_expected_size)?;
    if let Some(max_expected_size) = max_expected_size
        && resolved.file.size > max_expected_size
    {
        return Err(precondition_failed_with_subcode(
            ApiSubcode::WopiMaxExpectedSizeExceeded,
            "file is larger than X-WOPI-MaxExpectedSize",
        ));
    }

    let item_version = blob.hash.clone();
    let outcome = file_service::build_stream_outcome_with_disposition(
        state,
        &resolved.file,
        &blob,
        file_service::DownloadDisposition::Inline,
        if_none_match,
    )
    .await?;
    let audit_ctx = request_info.to_context(resolved.payload.actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::FileDownload,
        crate::services::audit_service::AuditEntityType::File,
        Some(resolved.file.id),
        Some(&resolved.file.name),
        None,
    )
    .await;
    Ok(WopiGetFileResult {
        outcome,
        item_version,
    })
}

pub async fn put_file_contents(
    state: &PrimaryAppState,
    req: WopiPutFileRequest<'_>,
) -> Result<WopiPutFileResult> {
    let WopiPutFileRequest {
        file_id,
        access_token,
        payload,
        content_length,
        requested_lock,
        audit_info,
        request_source,
    } = req;
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    // PutFile 有一个容易漏掉的协议细节：对现有文件，客户端必须先持有 lock。
    // 只有"未锁定且大小为 0 的新建文件"允许直接首写，这对应 editnew 的落盘流程。
    if let Some(conflict) =
        ensure_wopi_putfile_lock_matches(state, &resolved.file, requested_lock).await?
    {
        return Ok(WopiPutFileResult::Conflict(conflict));
    }

    let (updated, item_version) = file_service::update_content_stream_in_scope(
        state,
        scope_from_payload(&resolved.payload),
        resolved.file.id,
        payload,
        content_length,
        None,
    )
    .await?;
    let audit_ctx = audit_info.to_context(resolved.payload.actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::FileEdit,
        crate::services::audit_service::AuditEntityType::File,
        Some(updated.id),
        Some(&updated.name),
        None,
    )
    .await;

    Ok(WopiPutFileResult::Success {
        item_version: item_version_if_present(updated.id, item_version),
    })
}

pub async fn put_relative_file(
    state: &PrimaryAppState,
    req: WopiPutRelativeRequest<'_>,
) -> Result<WopiPutRelativeResult> {
    let WopiPutRelativeRequest {
        file_id,
        access_token,
        payload,
        suggested_target,
        relative_target,
        overwrite_relative_target,
        size_header,
        content_length,
        audit_info,
        request_source,
    } = req;
    let request_public_origin = request_source.public_origin.clone();
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let declared_size = parse_wopi_size_header(size_header)?.or(content_length);
    let request = parse_put_relative_request(
        &resolved.file.name,
        suggested_target,
        relative_target,
        overwrite_relative_target,
    )?;
    let scope = scope_from_payload(&resolved.payload);

    let (target_file, audit_action) = match request.target_mode {
        PutRelativeTargetMode::Suggested(target_name) => {
            // SuggestedTarget 永远表示"新建一个可用名称"，不会覆盖现有文件。
            let target = store_relative_target_from_stream(
                state,
                StoreRelativeTargetParams::new(
                    scope,
                    resolved.file.folder_id,
                    &target_name,
                    payload,
                )
                .declared_size(declared_size),
            )
            .await?;
            (target, audit_service::AuditAction::FileUpload)
        }
        PutRelativeTargetMode::Relative {
            target_name,
            overwrite,
        } => {
            // RelativeTarget 先找目标名是否已存在，再根据 overwrite / 锁状态决定
            // 冲突、覆盖还是新建。
            let existing =
                find_file_by_name_in_scope(&state.db, scope, resolved.file.folder_id, &target_name)
                    .await?;

            match existing {
                None => {
                    let target = store_relative_target_from_stream(
                        state,
                        StoreRelativeTargetParams::new(
                            scope,
                            resolved.file.folder_id,
                            &target_name,
                            payload,
                        )
                        .declared_size(declared_size)
                        .exact_name(),
                    )
                    .await?;
                    (target, audit_service::AuditAction::FileUpload)
                }
                Some(existing) => {
                    if existing.id == resolved.file.id {
                        return Err(AsterError::validation_error(
                            "PUT_RELATIVE target must differ from source file",
                        ));
                    }

                    if !overwrite {
                        let valid_target = encode_wopi_filename(
                            &suggest_available_relative_target(
                                state,
                                scope,
                                resolved.file.folder_id,
                                &target_name,
                            )
                            .await?,
                        );
                        return Ok(WopiPutRelativeResult::Conflict(
                            super::types::WopiPutRelativeConflict {
                                current_lock: Some(String::new()),
                                reason: "target file already exists".to_string(),
                                valid_target: Some(valid_target),
                            },
                        ));
                    }

                    if let Some(active_lock) = load_active_lock(state, existing.id).await? {
                        return Ok(WopiPutRelativeResult::Conflict(
                            super::types::WopiPutRelativeConflict {
                                current_lock: Some(
                                    active_wopi_lock_value(&active_lock).unwrap_or_default(),
                                ),
                                reason: "target file is locked".to_string(),
                                valid_target: None,
                            },
                        ));
                    }

                    let target = store_relative_target_from_stream(
                        state,
                        StoreRelativeTargetParams::new(
                            scope,
                            resolved.file.folder_id,
                            &target_name,
                            payload,
                        )
                        .declared_size(declared_size)
                        .overwrite(existing.id)
                        .exact_name(),
                    )
                    .await?;
                    (target, audit_service::AuditAction::FileEdit)
                }
            }
        }
    };

    let audit_ctx = audit_info.to_context(resolved.payload.actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_action,
        crate::services::audit_service::AuditEntityType::File,
        Some(target_file.id),
        Some(&target_file.name),
        None,
    )
    .await;
    let response = build_put_relative_response(
        state,
        &resolved.payload,
        &target_file.name,
        target_file.id,
        request_public_origin,
    )
    .await?;
    Ok(WopiPutRelativeResult::Success(response))
}

pub async fn rename_file(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    requested_name: Option<&str>,
    requested_lock: Option<&str>,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiRenameFileResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    if let Some(conflict) =
        ensure_wopi_lock_matches(state, resolved.file.id, requested_lock).await?
    {
        return Ok(WopiRenameFileResult::Conflict(conflict));
    }

    let requested_name =
        match normalize_requested_rename_target(&resolved.file.name, requested_name) {
            Ok(name) => name,
            Err(reason) => return Ok(WopiRenameFileResult::InvalidName { reason }),
        };
    let scope = scope_from_payload(&resolved.payload);
    let mut final_name = resolve_available_rename_target(
        state,
        scope,
        resolved.file.folder_id,
        resolved.file.id,
        &requested_name,
    )
    .await?;

    let updated = match file_service::update_in_scope(
        state,
        scope,
        resolved.file.id,
        Some(final_name.clone()),
        NullablePatch::Absent,
    )
    .await
    {
        Ok(updated) => updated,
        Err(err) if file_repo::is_duplicate_name_error(&err, &final_name) => {
            // 即使前面已经算过一个可用名字，这里仍然要接受并发重命名造成的唯一键冲突，
            // 然后再退一步建议新的可用名。
            final_name = suggest_available_relative_target(
                state,
                scope,
                resolved.file.folder_id,
                &final_name,
            )
            .await?;
            file_service::update_in_scope(
                state,
                scope,
                resolved.file.id,
                Some(final_name),
                NullablePatch::Absent,
            )
            .await?
        }
        Err(err) => return Err(err),
    };

    let audit_ctx = request_info.to_context(resolved.payload.actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::FileRename,
        crate::services::audit_service::AuditEntityType::File,
        Some(updated.id),
        Some(&updated.name),
        None,
    )
    .await;
    Ok(WopiRenameFileResult::Success(WopiRenameFileResponse {
        name: response_name_for_rename(&resolved.file.name, &updated.name).to_string(),
    }))
}

pub async fn put_user_info(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    payload: &mut actix_web::web::Payload,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<()> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let body = collect_limited_payload(payload, MAX_WOPI_USER_INFO_LEN).await?;
    let user_info = normalize_wopi_user_info(&body)?;
    profile_service::update_wopi_user_info(state, resolved.payload.actor_user_id, user_info)
        .await?;
    let audit_ctx = request_info.to_context(resolved.payload.actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        audit_service::AuditAction::UserUpdateWopiInfo,
        crate::services::audit_service::AuditEntityType::User,
        Some(resolved.payload.actor_user_id),
        None,
        None,
    )
    .await;
    Ok(())
}

fn item_version_if_present(_file_id: i64, item_version: String) -> String {
    item_version
}

pub(crate) fn parse_wopi_max_expected_size(value: Option<&str>) -> Result<Option<i64>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let parsed = value.parse::<u32>().map_err(|_| {
        AsterError::validation_error(
            "X-WOPI-MaxExpectedSize header must be a non-negative 32-bit integer",
        )
    })?;
    Ok(Some(i64::from(parsed)))
}

async fn collect_limited_payload(
    payload: &mut actix_web::web::Payload,
    max_len: usize,
) -> Result<bytes::Bytes> {
    let mut body = BytesMut::with_capacity(max_len.min(4096));
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_aster_err_with(|| {
            AsterError::validation_error("failed to read PUT_USER_INFO request body")
        })?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| AsterError::validation_error("PUT_USER_INFO body is too large"))?;
        if next_len > max_len {
            return Err(AsterError::validation_error(format!(
                "PUT_USER_INFO body must be {MAX_WOPI_USER_INFO_LEN} bytes or fewer"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body.freeze())
}

fn normalize_wopi_user_info(body: &[u8]) -> Result<String> {
    let user_info = std::str::from_utf8(body).map_aster_err_with(|| {
        AsterError::validation_error("PUT_USER_INFO body must be valid UTF-8")
    })?;
    if !user_info.is_ascii() {
        return Err(AsterError::validation_error(
            "PUT_USER_INFO body must contain ASCII characters only",
        ));
    }
    if user_info.len() > MAX_WOPI_USER_INFO_LEN {
        return Err(AsterError::validation_error(format!(
            "PUT_USER_INFO body must be {MAX_WOPI_USER_INFO_LEN} bytes or fewer"
        )));
    }
    Ok(user_info.to_string())
}
