use actix_multipart::Multipart;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::file;
use crate::errors::{AsterError, MapAsterErr, Result, file_upload_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace_storage_service::{
    StorePreuploadedNondedupParams, check_quota, cleanup_preuploaded_blob_upload,
    prepare_non_dedup_blob_upload, store_preuploaded_nondedup,
};
use crate::storage::BlobMetadata;
use crate::utils::numbers::u64_to_i64;

use super::common::{
    DirectUploadParams, upload_direct_relay_shutdown_failed, upload_direct_relay_write_failed,
    upload_empty_file_error, upload_field_read_failed, upload_size_mismatch_error,
};

pub(super) async fn upload_streaming_direct(
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
    const RELAY_DIRECT_BUFFER_SIZE: usize = 64 * 1024;

    if policy.max_file_size > 0 && declared_size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            declared_size, policy.max_file_size
        )));
    }

    check_quota(state.writer_db(), scope, declared_size).await?;
    let driver = state.driver_registry().get_driver(policy)?;
    let prepared_upload = prepare_non_dedup_blob_upload(policy, declared_size)?;
    let storage_path = prepared_upload.storage_path().to_string();

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

            let (writer, reader) = tokio::io::duplex(RELAY_DIRECT_BUFFER_SIZE);
            let upload_driver = driver.clone();
            let upload_storage_path = storage_path.clone();
            let stream_driver = upload_driver.as_stream_upload().ok_or_else(|| {
                crate::errors::AsterError::storage_driver_error("stream upload not supported")
            })?;
            let (upload_result, relay_result) = tokio::task::LocalSet::new()
                .run_until(async move {
                    let relay_task = tokio::task::spawn_local(async move {
                        let mut writer = writer;
                        while let Some(chunk) = field.next().await {
                            let chunk = chunk.map_aster_err(upload_field_read_failed)?;
                            writer.write_all(&chunk).await.map_aster_err_ctx(
                                "relay direct write",
                                upload_direct_relay_write_failed,
                            )?;
                        }
                        writer.shutdown().await.map_aster_err_ctx(
                            "relay direct shutdown",
                            upload_direct_relay_shutdown_failed,
                        )?;
                        Ok::<(), AsterError>(())
                    });

                    let upload_result = stream_driver
                        .put_reader(&upload_storage_path, Box::new(reader), declared_size)
                        .await;
                    let relay_result = relay_task.await.map_err(|err| {
                        file_upload_error_with_code(
                            ApiErrorCode::UploadDirectRelayTaskFailed,
                            format!("relay direct task failed: {err}"),
                        )
                    })?;

                    Ok::<(Result<String>, Result<()>), AsterError>((upload_result, relay_result))
                })
                .await?;

            if let Err(err) = upload_result {
                cleanup_preuploaded_blob_upload(
                    driver.as_ref(),
                    &prepared_upload,
                    "direct stream upload error",
                )
                .await;
                return Err(err);
            }

            if let Err(err) = relay_result {
                cleanup_preuploaded_blob_upload(
                    driver.as_ref(),
                    &prepared_upload,
                    "direct stream relay error",
                )
                .await;
                return Err(err);
            }

            let metadata = match driver.metadata(&storage_path).await {
                Ok(metadata) => metadata,
                Err(err) => {
                    cleanup_preuploaded_blob_upload(
                        driver.as_ref(),
                        &prepared_upload,
                        "direct stream metadata error",
                    )
                    .await;
                    return Err(err);
                }
            };
            let actual_size =
                match validate_streaming_direct_uploaded_size(metadata, declared_size, policy) {
                    Ok(actual_size) => actual_size,
                    Err(err) => {
                        cleanup_preuploaded_blob_upload(
                            driver.as_ref(),
                            &prepared_upload,
                            "direct stream size validation failure",
                        )
                        .await;
                        return Err(err);
                    }
                };
            if let Err(err) = check_quota(state.writer_db(), scope, actual_size).await {
                cleanup_preuploaded_blob_upload(
                    driver.as_ref(),
                    &prepared_upload,
                    "direct stream quota validation failure",
                )
                .await;
                return Err(err);
            }

            return store_preuploaded_nondedup(
                state,
                StorePreuploadedNondedupParams {
                    scope,
                    folder_id,
                    filename: &filename,
                    size: actual_size,
                    existing_file_id: None,
                    skip_lock_check: false,
                    policy,
                    preuploaded_blob: prepared_upload,
                    actor_username,
                },
            )
            .await;
        }
    }

    Err(upload_empty_file_error())
}

fn validate_streaming_direct_uploaded_size(
    metadata: BlobMetadata,
    declared_size: i64,
    policy: &crate::entities::storage_policy::Model,
) -> Result<i64> {
    let actual_size = u64_to_i64(metadata.size, "streaming direct uploaded size")?;
    if actual_size != declared_size {
        return Err(upload_size_mismatch_error(declared_size, actual_size));
    }
    if policy.max_file_size > 0 && actual_size > policy.max_file_size {
        return Err(AsterError::file_too_large(format!(
            "file size {} exceeds limit {}",
            actual_size, policy.max_file_size
        )));
    }
    Ok(actual_size)
}

#[cfg(test)]
mod tests {
    use super::validate_streaming_direct_uploaded_size;
    use crate::storage::BlobMetadata;

    fn policy_with_max_file_size(max_file_size: i64) -> crate::entities::storage_policy::Model {
        let now = chrono::Utc::now();
        crate::entities::storage_policy::Model {
            id: 1,
            name: "test".to_string(),
            driver_type: crate::types::DriverType::S3,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: String::new(),
            remote_node_id: None,
            remote_storage_target_key: None,
            max_file_size,
            allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
            options: crate::types::StoredStoragePolicyOptions::empty(),
            is_default: true,
            chunk_size: 5_242_880,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_rejects_declared_size_mismatch() {
        let policy = policy_with_max_file_size(0);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            1,
            &policy,
        )
        .expect_err("actual uploaded size must match declared size");

        assert!(error.message().contains("size mismatch"));
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_accepts_exact_policy_boundary() {
        let policy = policy_with_max_file_size(10);
        let actual_size = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            10,
            &policy,
        )
        .expect("actual size equal to max_file_size should be accepted");

        assert_eq!(actual_size, 10);
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_accepts_unlimited_policy() {
        let policy = policy_with_max_file_size(0);
        let actual_size = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 1024,
                content_type: None,
            },
            1024,
            &policy,
        )
        .expect("max_file_size 0 should allow any matching declared size");

        assert_eq!(actual_size, 1024);
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_checks_policy_against_actual_size() {
        let policy = policy_with_max_file_size(8);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: 10,
                content_type: None,
            },
            10,
            &policy,
        )
        .expect_err("actual uploaded size must respect policy max_file_size");

        assert!(error.message().contains("exceeds limit 8"));
    }

    #[test]
    fn validate_streaming_direct_uploaded_size_rejects_metadata_size_outside_i64() {
        let policy = policy_with_max_file_size(0);
        let error = validate_streaming_direct_uploaded_size(
            BlobMetadata {
                size: i64::MAX as u64 + 1,
                content_type: None,
            },
            i64::MAX,
            &policy,
        )
        .expect_err("metadata size outside i64 must be rejected");

        assert!(error.message().contains("streaming direct uploaded size"));
    }
}
