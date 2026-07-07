use actix_multipart::Multipart;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::workspace::storage::{
    StoreFromTempHints, StoreFromTempParams, create_empty, local_content_dedup_enabled,
    store_from_temp_with_hints,
};
use crate::utils::numbers::usize_to_i64;

use super::common::{
    DirectUploadParams, upload_body_size_overflow_error, upload_empty_file_error,
    upload_field_read_failed, upload_local_staging_dir_create_failed,
    upload_local_staging_file_create_failed, upload_local_staging_flush_failed,
    upload_local_staging_path_resolve_failed, upload_local_staging_write_failed,
    upload_size_mismatch_error,
};

pub(super) async fn upload_local_direct(
    state: &PrimaryAppState,
    payload: &mut Multipart,
    params: DirectUploadParams<'_>,
) -> Result<file::Model> {
    let DirectUploadParams {
        scope,
        folder_id,
        relative_path,
        resolved_filename,
        policy,
        declared_size,
        actor_username,
    } = params;
    let should_dedup = local_content_dedup_enabled(policy);

    while let Some(field) = payload.next().await {
        let mut field = field.map_aster_err(upload_field_read_failed)?;
        let is_file = field
            .content_disposition()
            .and_then(|content| content.get_filename().map(|name| name.to_string()));

        if let Some(name) = is_file {
            let filename = if relative_path.is_some() {
                resolved_filename.to_string()
            } else {
                name
            };
            let filename = crate::utils::normalize_validate_name(&filename)?;

            let staging_token = format!("{}.upload", crate::utils::id::new_uuid());
            let staging_path =
                crate::storage::drivers::local::upload_staging_path(policy, &staging_token)
                    .map_aster_err_ctx(
                        "resolve local staging path",
                        upload_local_staging_path_resolve_failed,
                    )?;
            if let Some(parent) = staging_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_aster_err_ctx(
                    "create local staging dir",
                    upload_local_staging_dir_create_failed,
                )?;
            }

            let staging_file = tokio::fs::File::create(&staging_path)
                .await
                .map_aster_err_ctx(
                    "create local staging file",
                    upload_local_staging_file_create_failed,
                )?;
            let mut staging_file = BufWriter::new(staging_file);
            let mut hasher = should_dedup.then(Sha256::new);
            let mut size: i64 = 0;
            let staging_path = staging_path.to_string_lossy().into_owned();

            let write_result = async {
                while let Some(chunk) = field.next().await {
                    let chunk = chunk.map_aster_err(upload_field_read_failed)?;
                    if let Some(hasher) = hasher.as_mut() {
                        hasher.update(&chunk);
                    }
                    staging_file.write_all(&chunk).await.map_aster_err_ctx(
                        "write local staging file",
                        upload_local_staging_write_failed,
                    )?;
                    size = size
                        .checked_add(usize_to_i64(chunk.len(), "chunk length")?)
                        .ok_or_else(upload_body_size_overflow_error)?;
                }
                staging_file.flush().await.map_aster_err_ctx(
                    "flush local staging file",
                    upload_local_staging_flush_failed,
                )?;
                Ok::<(), AsterError>(())
            }
            .await;

            drop(staging_file);

            if let Err(err) = write_result {
                crate::utils::cleanup_temp_file(&staging_path).await;
                return Err(err);
            }

            if size != declared_size {
                crate::utils::cleanup_temp_file(&staging_path).await;
                return Err(upload_size_mismatch_error(declared_size, size));
            }

            if size == 0 {
                crate::utils::cleanup_temp_file(&staging_path).await;
                return create_empty(state, scope, folder_id, &filename).await;
            }

            let precomputed_hash =
                hasher.map(|hasher| crate::utils::hash::sha256_digest_to_hex(&hasher.finalize()));
            let resolved_policy = Some(policy.clone());
            let result = store_from_temp_with_hints(
                state,
                StoreFromTempParams::new(scope, folder_id, &filename, &staging_path, size),
                StoreFromTempHints {
                    resolved_policy,
                    precomputed_hash: precomputed_hash.as_deref(),
                    actor_username,
                    ..Default::default()
                },
            )
            .await;

            crate::utils::cleanup_temp_file(&staging_path).await;
            return result;
        }
    }

    Err(upload_empty_file_error())
}
