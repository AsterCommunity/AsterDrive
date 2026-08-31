//! 工作空间存储服务子模块：`multipart`。

mod common;
mod local_direct;
mod staged;
mod streaming_direct;

use actix_multipart::Multipart;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{Result, validation_error_with_code};
use crate::runtime::PrimaryAppState;
use aster_drive_model::entities::file;

use super::{
    WorkspaceStorageScope, ensure_upload_parent_path, parse_relative_upload_path,
    resolve_blob_policy_for_write, resolve_policy_upload_transport_for_execution,
    resolve_verified_folder_policy_hint, verify_folder_access,
};

use self::common::DirectUploadParams;
use self::local_direct::upload_local_direct;
use self::staged::{StagedUploadParams, upload_staged};
use self::streaming_direct::upload_streaming_direct;

#[derive(Clone, Copy, Default)]
pub(crate) struct WorkspaceUploadHints<'a> {
    pub actor_username: Option<&'a str>,
}

pub(crate) async fn upload_with_hints(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    payload: &mut Multipart,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
    declared_size: Option<i64>,
    hints: WorkspaceUploadHints<'_>,
) -> Result<file::Model> {
    tracing::debug!(
        scope = ?scope,
        folder_id,
        relative_path = relative_path.unwrap_or(""),
        declared_size,
        "starting multipart upload"
    );

    if let Some(declared_size) = declared_size
        && declared_size < 0
    {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadDeclaredSizeInvalid,
            "declared_size cannot be negative",
        ));
    }

    let (effective_folder_id, effective_folder, resolved_filename) = match relative_path {
        Some(path) => {
            let parsed = parse_relative_upload_path(state, scope, folder_id, path).await?;
            let resolved_parent =
                ensure_upload_parent_path(state, scope, &parsed, hints.actor_username).await?;
            (
                resolved_parent.folder_id,
                resolved_parent.folder,
                parsed.filename,
            )
        }
        None => {
            let folder = match folder_id {
                Some(folder_id) => {
                    let folder = verify_folder_access(state, scope, folder_id).await?;
                    Some(resolve_verified_folder_policy_hint(state, scope, folder).await?)
                }
                None => None,
            };
            (folder_id, folder, String::new())
        }
    };

    tracing::debug!(
        scope = ?scope,
        folder_id = effective_folder_id,
        resolved_filename = %resolved_filename,
        has_relative_path = relative_path.is_some(),
        "resolved upload target"
    );

    if let Some(declared_size) = declared_size
        && relative_path.is_some()
    {
        let resolution = resolve_blob_policy_for_write(
            state,
            crate::services::workspace::storage::BlobPolicyRequest {
                scope,
                folder_id: effective_folder_id,
                folder_hint: effective_folder,
                filename: &resolved_filename,
                file_size: declared_size,
                mime_type: "application/octet-stream",
                existing_file_id: None,
            },
        )
        .await?;
        let policy = resolution.policy;
        let transport = resolve_policy_upload_transport_for_execution(
            state.driver_registry().connectors(),
            &policy,
            resolution
                .routing_decision
                .expect("new blob placement decision")
                .execution_preference,
        )?;
        if transport.supports_streaming_direct_upload(&policy, declared_size) {
            tracing::debug!(
                scope = ?scope,
                folder_id = effective_folder_id,
                resolved_filename = %resolved_filename,
                policy_id = policy.id,
                connector_id = %policy.connector_id,
                declared_size,
                "using streaming direct upload fast path"
            );

            let result = upload_streaming_direct(
                state,
                payload,
                DirectUploadParams {
                    scope,
                    folder_id: effective_folder_id,
                    relative_path,
                    resolved_filename: &resolved_filename,
                    policy: &policy,
                    declared_size,
                    actor_username: hints.actor_username,
                },
            )
            .await;
            if let Ok(file) = &result {
                tracing::debug!(
                    scope = ?scope,
                    file_id = file.id,
                    folder_id = file.folder_id,
                    size = file.size,
                    "completed streaming direct upload"
                );
            }
            return result;
        }
        if crate::storage::connectors::resolve_local_filesystem_projection(
            state.driver_registry().connectors(),
            &policy,
        )?
        .is_some()
        {
            tracing::debug!(
                scope = ?scope,
                folder_id = effective_folder_id,
                resolved_filename = %resolved_filename,
                policy_id = policy.id,
                connector_id = %policy.connector_id,
                declared_size,
                "using local direct upload fast path"
            );

            let result = upload_local_direct(
                state,
                payload,
                DirectUploadParams {
                    scope,
                    folder_id: effective_folder_id,
                    relative_path,
                    resolved_filename: &resolved_filename,
                    policy: &policy,
                    declared_size,
                    actor_username: hints.actor_username,
                },
            )
            .await;
            if let Ok(file) = &result {
                tracing::debug!(
                    scope = ?scope,
                    file_id = file.id,
                    folder_id = file.folder_id,
                    size = file.size,
                    "completed local direct upload"
                );
            }
            return result;
        }
    }

    upload_staged(
        state,
        payload,
        StagedUploadParams {
            scope,
            folder_id: effective_folder_id,
            relative_path,
            resolved_filename: &resolved_filename,
            verified_folder: effective_folder,
            declared_size,
            actor_username: hints.actor_username,
        },
    )
    .await
}
