//! 内部对象存储协议路由：`internal_storage`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::middleware::internal_storage_cors::PresignedInternalStorageCors;
use crate::api::response::ApiResponse;
use crate::errors::{AsterError, Result, validation_error_with_code};
use crate::runtime::FollowerAppState;
use crate::services::{audit_service, master_binding_service, remote_storage_target_service};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::object_key;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_SIGNATURE_HEADER, PRESIGNED_AUTH_ACCESS_KEY_QUERY, RemoteBindingSyncRequest,
    RemoteCreateStorageTargetRequest, RemoteStorageCapabilities, RemoteStorageCapacityResponse,
    RemoteStorageComposeRequest, RemoteStorageComposeResponse, RemoteStorageListResponse,
    RemoteStorageObjectMetadata, RemoteUpdateStorageTargetRequest,
};
use crate::storage::{BlobMetadata, StorageDriver};
use crate::utils::numbers;
use actix_web::http::{StatusCode, header::HeaderMap};
use actix_web::{HttpRequest, HttpResponse, dev::HttpServiceFactory, web};
use futures::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

pub fn routes() -> impl HttpServiceFactory {
    web::scope("/internal/storage")
        .wrap(PresignedInternalStorageCors)
        .route("/capabilities", web::get().to(get_capabilities))
        .route("/capacity", web::get().to(get_capacity))
        .route("/binding", web::put().to(sync_binding))
        .route("/targets", web::get().to(list_storage_targets))
        .route("/targets", web::post().to(create_storage_target))
        .route(
            "/targets/{target_key}",
            web::patch().to(update_storage_target),
        )
        .route(
            "/targets/{target_key}",
            web::delete().to(delete_storage_target),
        )
        // TODO(remote-storage-target): deprecated since 0.4.0. Keep the
        // ingress-profile routes as internal protocol compatibility aliases
        // until primary/follower version negotiation can advertise the
        // target-named routes.
        .route("/ingress-profiles", web::get().to(list_ingress_profiles))
        .route("/ingress-profiles", web::post().to(create_ingress_profile))
        .route(
            "/ingress-profiles/{target_key}",
            web::patch().to(update_ingress_profile),
        )
        .route(
            "/ingress-profiles/{target_key}",
            web::delete().to(delete_ingress_profile),
        )
        .route("/compose", web::post().to(compose_objects))
        .route("/objects", web::get().to(list_objects))
        .route(
            "/objects/{tail:.*}/metadata",
            web::get().to(get_object_metadata),
        )
        .route("/objects/{tail:.*}", web::put().to(put_object))
        .route("/objects/{tail:.*}", web::get().to(get_object))
        .route("/objects/{tail:.*}", web::head().to(head_object))
        .route("/objects/{tail:.*}", web::delete().to(delete_object))
}

fn validate_ingress_object_size(size: i64, max_file_size: i64, subject: &str) -> Result<()> {
    if max_file_size > 0 && size > max_file_size {
        return Err(AsterError::file_too_large(format!(
            "{subject} size {} exceeds limit {}",
            size, max_file_size
        )));
    }
    Ok(())
}

fn internal_storage_validation_error(
    api_code: ApiErrorCode,
    message: impl Into<String>,
) -> AsterError {
    validation_error_with_code(api_code, message)
}

fn content_length_header(headers: &HeaderMap) -> Result<i64> {
    let value = headers
        .get(actix_web::http::header::CONTENT_LENGTH)
        .ok_or_else(|| {
            internal_storage_validation_error(
                ApiErrorCode::InternalStorageContentLengthRequired,
                "content-length header is required",
            )
        })?;
    let value = value.to_str().map_err(|_| {
        internal_storage_validation_error(
            ApiErrorCode::InternalStorageContentLengthInvalid,
            "content-length header must be valid ASCII",
        )
    })?;
    value.parse::<i64>().map_err(|_| {
        internal_storage_validation_error(
            ApiErrorCode::InternalStorageContentLengthInvalid,
            "content-length header must be a valid integer",
        )
    })
}

#[derive(Debug, Deserialize, Default)]
struct ObjectQuery {
    offset: Option<u64>,
    length: Option<u64>,
    prefix: Option<String>,
    #[serde(rename = "response-cache-control")]
    response_cache_control: Option<String>,
    #[serde(rename = "response-content-disposition")]
    response_content_disposition: Option<String>,
    #[serde(rename = "response-content-type")]
    response_content_type: Option<String>,
}

async fn metadata_or_not_found(
    driver: &dyn StorageDriver,
    storage_path: &str,
) -> Result<BlobMetadata> {
    match driver.metadata(storage_path).await {
        Ok(metadata) => Ok(metadata),
        Err(error) => {
            if !driver.exists(storage_path).await.unwrap_or(true) {
                Err(AsterError::record_not_found(format!(
                    "storage object '{storage_path}' not found"
                )))
            } else {
                Err(error)
            }
        }
    }
}

#[derive(Debug)]
struct PartialContentRange {
    start: u64,
    end: u64,
    length: u64,
}

fn partial_content_range(
    total_size: u64,
    offset: Option<u64>,
    length: Option<u64>,
) -> Result<Option<PartialContentRange>> {
    if offset.is_none() && length.is_none() {
        return Ok(None);
    }
    if length == Some(0) {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeLengthInvalid,
            "range length must be greater than zero",
        ));
    }
    if total_size == 0 {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeEmptyObject,
            "range cannot be requested for empty object",
        ));
    }

    let start = offset.unwrap_or(0);
    if start >= total_size {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeOffsetOutOfBounds,
            "range offset exceeds object size",
        ));
    }

    let end = match length {
        Some(length) => start
            .saturating_add(length)
            .saturating_sub(1)
            .min(total_size - 1),
        None => total_size - 1,
    };
    let length = end.saturating_sub(start).saturating_add(1);
    Ok(Some(PartialContentRange { start, end, length }))
}

fn requested_partial_content_range(
    req: &HttpRequest,
    total_size: u64,
    query: &ObjectQuery,
) -> Result<Option<PartialContentRange>> {
    if let Some(range_header) = req.headers().get(actix_web::http::header::RANGE) {
        let (offset, length) = parse_range_header(range_header, total_size)?;
        return partial_content_range(total_size, offset, length);
    }

    partial_content_range(total_size, query.offset, query.length)
}

fn parse_range_header(
    value: &actix_web::http::header::HeaderValue,
    total_size: u64,
) -> Result<(Option<u64>, Option<u64>)> {
    let raw = value
        .to_str()
        .map_err(|_| {
            internal_storage_validation_error(
                ApiErrorCode::InternalStorageRangeHeaderInvalid,
                "range header must be valid ASCII",
            )
        })?
        .trim();
    let range = raw.strip_prefix("bytes=").ok_or_else(|| {
        internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeHeaderInvalid,
            "range header must use bytes unit",
        )
    })?;
    if range.contains(',') {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeMultipleUnsupported,
            "multiple range requests are not supported",
        ));
    }

    let (start_raw, end_raw) = range.split_once('-').ok_or_else(|| {
        internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeHeaderInvalid,
            "range header is malformed",
        )
    })?;
    if start_raw.is_empty() && end_raw.is_empty() {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeHeaderInvalid,
            "range header is malformed",
        ));
    }

    if start_raw.is_empty() {
        let suffix_length = parse_range_bound(end_raw, "range suffix length")?;
        if suffix_length == 0 {
            return Err(internal_storage_validation_error(
                ApiErrorCode::InternalStorageRangeLengthInvalid,
                "range suffix length must be greater than zero",
            ));
        }
        return Ok((
            Some(total_size.saturating_sub(suffix_length)),
            Some(suffix_length),
        ));
    }

    let start = parse_range_bound(start_raw, "range start")?;
    if end_raw.is_empty() {
        return Ok((Some(start), None));
    }

    let end = parse_range_bound(end_raw, "range end")?;
    if end < start {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeBoundsInvalid,
            "range end must be greater than or equal to range start",
        ));
    }
    let length = end
        .checked_sub(start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            internal_storage_validation_error(
                ApiErrorCode::InternalStorageRangeLengthInvalid,
                "range length exceeds u64 range",
            )
        })?;
    Ok((Some(start), Some(length)))
}

fn parse_range_bound(value: &str, name: &str) -> Result<u64> {
    value.parse::<u64>().map_err(|_| {
        internal_storage_validation_error(
            ApiErrorCode::InternalStorageRangeBoundsInvalid,
            format!("{name} must be a non-negative integer"),
        )
    })
}

fn follower_audit_context(req: &HttpRequest) -> audit_service::AuditContext {
    audit_service::AuditRequestInfo::from_request(req).to_context(0)
}

fn object_audit_details<'a>(
    binding_id: i64,
    object_key: &'a str,
    storage_path: &'a str,
    size: Option<i64>,
    bytes_written: Option<u64>,
    partial: Option<bool>,
    parts: Option<&'a [String]>,
) -> Option<serde_json::Value> {
    audit_service::details(audit_service::FollowerObjectAuditDetails {
        binding_id,
        object_key,
        storage_path,
        size,
        bytes_written,
        partial,
        parts,
    })
}

async fn get_capabilities(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_internal_binding_request(state.get_ref(), &req).await?;
    tracing::debug!(
        binding_id = binding.id,
        "follower internal storage capabilities requested"
    );
    let capabilities = RemoteStorageCapabilities::current()
        .with_remote_storage_target_driver_types(
            remote_storage_target_service::registered_remote_storage_target_driver_types(),
        );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capabilities)))
}

async fn get_capacity(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let capacity = ctx.ingress_driver.capacity_info().await?;
    tracing::debug!(
        binding_id = ctx.binding.id,
        "follower internal storage capacity requested"
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageCapacityResponse { capacity })))
}

async fn sync_binding(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteBindingSyncRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let synced = master_binding_service::sync_from_primary(
        state.get_ref(),
        &binding.access_key,
        master_binding_service::SyncMasterBindingInput {
            name: body.name.clone(),
            is_enabled: body.is_enabled,
        },
    )
    .await?;
    tracing::info!(
        binding_id = synced.id,
        is_enabled = synced.is_enabled,
        "follower binding synchronized"
    );
    audit_service::log_with_details(
        state.get_ref(),
        &follower_audit_context(&req),
        audit_service::AuditAction::FollowerBindingSync,
        audit_service::AuditEntityType::RemoteNode,
        Some(binding.id),
        Some(&body.name),
        || {
            audit_service::details(audit_service::FollowerBindingAuditDetails {
                binding_id: binding.id,
                name: &body.name,
                is_enabled: body.is_enabled,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn list_objects(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    query: web::Query<ObjectQuery>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let list_driver = ctx.ingress_driver.as_list().ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support list",
        )
    })?;

    let prefix = query
        .prefix
        .as_deref()
        .map(|value| master_binding_service::provider_storage_prefix(&ctx.binding, value))
        .transpose()?;
    let items = list_driver
        .list_paths(prefix.as_deref())
        .await?
        .into_iter()
        .filter_map(|path| {
            object_key::strip_key_prefix(&ctx.binding.storage_namespace, &path).map(str::to_string)
        })
        .collect::<Vec<_>>();
    tracing::debug!(
        binding_id = ctx.binding.id,
        prefix = ?query.prefix,
        item_count = items.len(),
        "follower objects listed"
    );

    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageListResponse { items })))
}

async fn put_object(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
    mut payload: web::Payload,
) -> Result<HttpResponse> {
    const RELAY_UPLOAD_BUFFER_SIZE: usize = 64 * 1024;

    let ctx = if req.headers().contains_key(INTERNAL_AUTH_SIGNATURE_HEADER) {
        master_binding_service::authorize_internal_request(state.get_ref(), &req).await?
    } else {
        master_binding_service::authorize_presigned_put_request(state.get_ref(), &req).await?
    };
    let object_key = path.into_inner();
    let storage_path = master_binding_service::provider_storage_path(&ctx.binding, &object_key)?;
    let content_length = content_length_header(req.headers())?;
    if content_length < 0 {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageContentLengthInvalid,
            "content-length must be non-negative",
        ));
    }
    validate_ingress_object_size(content_length, ctx.ingress_max_file_size, "object")?;
    tracing::debug!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        content_length,
        "follower object write accepted"
    );

    let stream_driver = ctx.ingress_driver.as_stream_upload().ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support stream upload",
        )
    })?;
    let (writer, reader) = tokio::io::duplex(RELAY_UPLOAD_BUFFER_SIZE);
    let upload_storage_path = storage_path.clone();
    let (upload_result, relay_result) = tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(async move {
                let mut writer = writer;
                let mut hasher = Sha256::new();
                while let Some(chunk) = payload.next().await {
                    let chunk = chunk.map_err(|e| {
                        AsterError::validation_error(format!("read upload payload: {e}"))
                    })?;
                    hasher.update(&chunk);
                    writer.write_all(&chunk).await.map_err(|e| {
                        AsterError::storage_driver_error(format!("relay upload payload: {e}"))
                    })?;
                }
                writer.shutdown().await.map_err(|e| {
                    AsterError::storage_driver_error(format!("shutdown relay upload payload: {e}"))
                })?;
                Ok::<String, AsterError>(format!("\"{}\"", hex::encode(hasher.finalize())))
            });

            let upload_result = stream_driver
                .put_reader(&upload_storage_path, Box::new(reader), content_length)
                .await;
            let relay_result = relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("relay upload task failed: {error}"))
            })?;
            Ok::<(Result<String>, Result<String>), AsterError>((upload_result, relay_result))
        })
        .await?;

    upload_result?;
    let etag = relay_result?;
    tracing::info!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        content_length,
        "follower object written"
    );
    if audit_service::should_record(
        state.get_ref(),
        audit_service::AuditAction::FollowerObjectWrite,
    ) {
        audit_service::log_with_details(
            state.get_ref(),
            &follower_audit_context(&req),
            audit_service::AuditAction::FollowerObjectWrite,
            audit_service::AuditEntityType::File,
            None,
            Some(&object_key),
            || {
                object_audit_details(
                    ctx.binding.id,
                    &object_key,
                    &storage_path,
                    Some(content_length),
                    None,
                    None,
                    None,
                )
            },
        )
        .await;
    }
    Ok(HttpResponse::Ok()
        .insert_header((actix_web::http::header::ETAG, etag))
        .json(ApiResponse::<()>::ok_empty()))
}

async fn compose_objects(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteStorageComposeRequest>,
) -> Result<HttpResponse> {
    const COMPOSE_BUFFER_SIZE: usize = 64 * 1024;

    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    if body.part_keys.is_empty() {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageComposePartsRequired,
            "compose request requires part_keys",
        ));
    }
    if body.expected_size < 0 {
        return Err(internal_storage_validation_error(
            ApiErrorCode::InternalStorageComposeExpectedSizeInvalid,
            "compose expected_size must be non-negative",
        ));
    }
    validate_ingress_object_size(
        body.expected_size,
        ctx.ingress_max_file_size,
        "composed object",
    )?;

    let driver = ctx.ingress_driver.clone();
    let stream_driver = driver.as_stream_upload().ok_or_else(|| {
        storage_driver_error(
            StorageErrorKind::Unsupported,
            "ingress target does not support stream upload",
        )
    })?;
    let target_key = body.target_key.clone();
    let target_storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &target_key)?;
    tracing::debug!(
        binding_id = ctx.binding.id,
        target_key = %target_key,
        part_count = body.part_keys.len(),
        expected_size = body.expected_size,
        "follower object compose accepted"
    );
    let part_storage_paths: Vec<String> = body
        .part_keys
        .iter()
        .map(|key| master_binding_service::provider_storage_path(&ctx.binding, key))
        .collect::<Result<_>>()?;
    let cleanup_part_storage_paths = part_storage_paths.clone();
    let expected_size = body.expected_size;
    let expected_size_u64 = numbers::i64_to_u64(expected_size, "compose expected_size")?;

    let read_driver = driver.clone();
    let upload_target_storage_path = target_storage_path.clone();
    let (writer, reader) = tokio::io::duplex(COMPOSE_BUFFER_SIZE);
    let (upload_result, relay_result) = tokio::task::LocalSet::new()
        .run_until(async move {
            let relay_task = tokio::task::spawn_local(async move {
                let mut writer = writer;
                let mut bytes_written = 0u64;
                for source_path in part_storage_paths {
                    let mut stream = read_driver.get_stream(&source_path).await?;
                    let copied = tokio::io::copy(&mut stream, &mut writer)
                        .await
                        .map_err(|e| {
                            AsterError::storage_driver_error(format!(
                                "relay composed object stream: {e}"
                            ))
                        })?;
                    bytes_written = bytes_written.checked_add(copied).ok_or_else(|| {
                        AsterError::storage_driver_error("compose bytes written overflow")
                    })?;
                }
                writer.shutdown().await.map_err(|e| {
                    AsterError::storage_driver_error(format!("shutdown compose stream: {e}"))
                })?;
                Ok::<u64, AsterError>(bytes_written)
            });

            let upload_result = stream_driver
                .put_reader(&upload_target_storage_path, Box::new(reader), expected_size)
                .await;
            let relay_result = relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("compose relay task failed: {error}"))
            })?;
            Ok::<(Result<String>, Result<u64>), AsterError>((upload_result, relay_result))
        })
        .await?;

    let cleanup_target = async {
        if let Err(error) = driver.delete(&target_storage_path).await {
            tracing::warn!(
                target_storage_path = %target_storage_path,
                "failed to cleanup composed target object: {error}"
            );
        }
    };

    if let Err(error) = upload_result {
        cleanup_target.await;
        return Err(error);
    }

    let bytes_written = match relay_result {
        Ok(bytes_written) => bytes_written,
        Err(error) => {
            cleanup_target.await;
            return Err(error);
        }
    };

    if bytes_written != expected_size_u64 {
        cleanup_target.await;
        return Err(AsterError::storage_driver_error(format!(
            "compose size mismatch: expected {expected_size_u64} bytes, got {bytes_written}"
        )));
    }

    for storage_path in cleanup_part_storage_paths {
        if let Err(error) = driver.delete(&storage_path).await {
            tracing::warn!(storage_path = %storage_path, "failed to cleanup composed part: {error}");
        }
    }
    tracing::info!(
        binding_id = ctx.binding.id,
        target_key = %target_key,
        bytes_written,
        "follower object composed"
    );

    if audit_service::should_record(
        state.get_ref(),
        audit_service::AuditAction::FollowerObjectCompose,
    ) {
        audit_service::log_with_details(
            state.get_ref(),
            &follower_audit_context(&req),
            audit_service::AuditAction::FollowerObjectCompose,
            audit_service::AuditEntityType::File,
            None,
            Some(&target_key),
            || {
                object_audit_details(
                    ctx.binding.id,
                    &target_key,
                    &target_storage_path,
                    Some(body.expected_size),
                    Some(bytes_written),
                    None,
                    Some(&body.part_keys),
                )
            },
        )
        .await;
    }

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageComposeResponse {
            bytes_written,
        })),
    )
}

async fn get_object(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
    query: web::Query<ObjectQuery>,
) -> Result<HttpResponse> {
    let ctx = if req.headers().contains_key(INTERNAL_AUTH_SIGNATURE_HEADER) {
        master_binding_service::authorize_internal_request(state.get_ref(), &req).await?
    } else if req
        .query_string()
        .split('&')
        .filter(|segment| !segment.is_empty())
        .any(|segment| segment.split('=').next() == Some(PRESIGNED_AUTH_ACCESS_KEY_QUERY))
    {
        master_binding_service::authorize_presigned_get_request(state.get_ref(), &req).await?
    } else {
        master_binding_service::authorize_internal_request(state.get_ref(), &req).await?
    };
    let object_key = path.into_inner();
    let storage_path = master_binding_service::provider_storage_path(&ctx.binding, &object_key)?;
    let metadata = metadata_or_not_found(ctx.ingress_driver.as_ref(), &storage_path).await?;
    let partial_range = requested_partial_content_range(&req, metadata.size, &query)?;
    let stream = match partial_range.as_ref() {
        Some(range) => {
            ctx.ingress_driver
                .get_range(&storage_path, range.start, Some(range.length))
                .await?
        }
        None => ctx.ingress_driver.get_stream(&storage_path).await?,
    };
    let body = ReaderStream::with_capacity(stream, 64 * 1024);

    let mut response = if partial_range.is_some() {
        HttpResponse::build(StatusCode::PARTIAL_CONTENT)
    } else {
        HttpResponse::Ok()
    };
    response.insert_header((
        actix_web::http::header::CONTENT_TYPE,
        query
            .response_content_type
            .clone()
            .or(metadata.content_type)
            .unwrap_or_else(|| "application/octet-stream".to_string()),
    ));
    response.insert_header((
        actix_web::http::header::CONTENT_LENGTH,
        partial_range
            .as_ref()
            .map(|range| range.length)
            .unwrap_or(metadata.size)
            .to_string(),
    ));
    response.insert_header((actix_web::http::header::ACCEPT_RANGES, "bytes"));
    if let Some(range) = partial_range.as_ref() {
        response.insert_header((
            actix_web::http::header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", range.start, range.end, metadata.size),
        ));
    }
    response.insert_header((actix_web::http::header::CONTENT_ENCODING, "identity"));
    if let Some(content_disposition) = query.response_content_disposition.as_deref() {
        response.insert_header((
            actix_web::http::header::CONTENT_DISPOSITION,
            content_disposition,
        ));
    }
    if let Some(cache_control) = query.response_cache_control.as_deref() {
        response.insert_header((actix_web::http::header::CACHE_CONTROL, cache_control));
    }
    tracing::debug!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        size = metadata.size,
        partial = partial_range.is_some(),
        "follower object read prepared"
    );
    if audit_service::should_record(
        state.get_ref(),
        audit_service::AuditAction::FollowerObjectRead,
    ) {
        audit_service::log_with_details(
            state.get_ref(),
            &follower_audit_context(&req),
            audit_service::AuditAction::FollowerObjectRead,
            audit_service::AuditEntityType::File,
            None,
            Some(&object_key),
            || {
                object_audit_details(
                    ctx.binding.id,
                    &object_key,
                    &storage_path,
                    i64::try_from(metadata.size).ok(),
                    None,
                    Some(partial_range.is_some()),
                    None,
                )
            },
        )
        .await;
    }
    Ok(response.streaming(body))
}

async fn head_object(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let object_key = path.into_inner();
    let storage_path = master_binding_service::provider_storage_path(&ctx.binding, &object_key)?;
    let metadata = metadata_or_not_found(ctx.ingress_driver.as_ref(), &storage_path).await?;
    tracing::debug!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        size = metadata.size,
        "follower object head requested"
    );

    let mut response = HttpResponse::Ok();
    response.no_chunking(metadata.size);
    if let Some(content_type) = metadata.content_type {
        response.insert_header((actix_web::http::header::CONTENT_TYPE, content_type));
    }
    response.insert_header((actix_web::http::header::ACCEPT_RANGES, "bytes"));
    Ok(response.finish())
}

async fn get_object_metadata(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let object_key = path.into_inner();
    let storage_path = master_binding_service::provider_storage_path(&ctx.binding, &object_key)?;
    let metadata = metadata_or_not_found(ctx.ingress_driver.as_ref(), &storage_path).await?;
    tracing::debug!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        size = metadata.size,
        "follower object metadata requested"
    );

    Ok(
        HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageObjectMetadata {
            size: metadata.size,
            content_type: metadata.content_type,
        })),
    )
}

async fn delete_object(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let object_key = path.into_inner();
    let storage_path = master_binding_service::provider_storage_path(&ctx.binding, &object_key)?;
    ctx.ingress_driver.delete(&storage_path).await?;
    tracing::info!(
        binding_id = ctx.binding.id,
        object_key = %object_key,
        "follower object deleted"
    );
    if audit_service::should_record(
        state.get_ref(),
        audit_service::AuditAction::FollowerObjectDelete,
    ) {
        audit_service::log_with_details(
            state.get_ref(),
            &follower_audit_context(&req),
            audit_service::AuditAction::FollowerObjectDelete,
            audit_service::AuditEntityType::File,
            None,
            Some(&object_key),
            || {
                object_audit_details(
                    ctx.binding.id,
                    &object_key,
                    &storage_path,
                    None,
                    None,
                    None,
                    None,
                )
            },
        )
        .await;
    }
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn list_ingress_profiles(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    list_storage_targets(state, req).await
}

async fn list_storage_targets(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let targets = remote_storage_target_service::list(state.get_ref(), &binding).await?;
    tracing::debug!(
        binding_id = binding.id,
        target_count = targets.len(),
        "follower remote storage targets listed"
    );
    Ok(HttpResponse::Ok().json(ApiResponse::ok(targets)))
}

async fn create_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteCreateStorageTargetRequest>,
) -> Result<HttpResponse> {
    create_storage_target(state, req, body).await
}

async fn create_storage_target(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteCreateStorageTargetRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let target =
        remote_storage_target_service::create(state.get_ref(), &binding, body.into_inner()).await?;
    tracing::info!(
        binding_id = binding.id,
        target_key = %target.target_key,
        driver_type = target.driver_type.as_str(),
        is_default = target.is_default,
        "follower remote storage target created"
    );
    audit_service::log_with_details(
        state.get_ref(),
        &follower_audit_context(&req),
        audit_service::AuditAction::FollowerIngressProfileCreate,
        audit_service::AuditEntityType::RemoteIngressProfile,
        None,
        Some(&target.target_key),
        || {
            audit_service::details(audit_service::FollowerIngressProfileAuditDetails {
                binding_id: binding.id,
                target_key: &target.target_key,
                driver_type: target.driver_type.as_str(),
                is_default: target.is_default,
            })
        },
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(target)))
}

async fn update_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RemoteUpdateStorageTargetRequest>,
) -> Result<HttpResponse> {
    update_storage_target(state, req, path, body).await
}

async fn update_storage_target(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RemoteUpdateStorageTargetRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let target_key = path.into_inner();
    let target = remote_storage_target_service::update(
        state.get_ref(),
        &binding,
        &target_key,
        body.into_inner(),
    )
    .await?;
    tracing::info!(
        binding_id = binding.id,
        target_key = %target.target_key,
        driver_type = target.driver_type.as_str(),
        is_default = target.is_default,
        "follower remote storage target updated"
    );
    audit_service::log_with_details(
        state.get_ref(),
        &follower_audit_context(&req),
        audit_service::AuditAction::FollowerIngressProfileUpdate,
        audit_service::AuditEntityType::RemoteIngressProfile,
        None,
        Some(&target.target_key),
        || {
            audit_service::details(audit_service::FollowerIngressProfileAuditDetails {
                binding_id: binding.id,
                target_key: &target.target_key,
                driver_type: target.driver_type.as_str(),
                is_default: target.is_default,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(target)))
}

async fn delete_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    delete_storage_target(state, req, path).await
}

async fn delete_storage_target(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let target_key = path.into_inner();
    let deleted_target =
        remote_storage_target_service::delete(state.get_ref(), &binding, &target_key).await?;
    tracing::info!(
        binding_id = binding.id,
        target_key = %target_key,
        "follower remote storage target deleted"
    );
    audit_service::log_with_details(
        state.get_ref(),
        &follower_audit_context(&req),
        audit_service::AuditAction::FollowerIngressProfileDelete,
        audit_service::AuditEntityType::RemoteIngressProfile,
        None,
        Some(&target_key),
        || {
            audit_service::details(audit_service::FollowerIngressProfileAuditDetails {
                binding_id: binding.id,
                target_key: &deleted_target.target_key,
                driver_type: deleted_target.driver_type.as_str(),
                is_default: deleted_target.is_default,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::header::{CONTENT_LENGTH, HeaderMap, HeaderValue};

    #[test]
    fn partial_content_range_rejects_invalid_shape_with_stable_codes() {
        let zero_length = partial_content_range(10, Some(0), Some(0))
            .expect_err("zero length ranges should fail");
        assert_eq!(
            zero_length.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeLengthInvalid)
        );

        let empty_object = partial_content_range(0, Some(0), Some(1))
            .expect_err("empty object ranges should fail");
        assert_eq!(
            empty_object.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeEmptyObject)
        );

        let out_of_bounds = partial_content_range(10, Some(10), Some(1))
            .expect_err("range offset past object size should fail");
        assert_eq!(
            out_of_bounds.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeOffsetOutOfBounds)
        );
    }

    #[test]
    fn parse_range_header_rejects_invalid_shape_with_stable_codes() {
        let header = actix_web::http::header::HeaderValue::from_static("items=0-1");
        let invalid_unit =
            parse_range_header(&header, 10).expect_err("unsupported range unit should fail");
        assert_eq!(
            invalid_unit.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeHeaderInvalid)
        );

        let header = actix_web::http::header::HeaderValue::from_static("bytes=0-1,3-4");
        let multiple = parse_range_header(&header, 10).expect_err("multiple ranges should fail");
        assert_eq!(
            multiple.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeMultipleUnsupported)
        );

        let header = actix_web::http::header::HeaderValue::from_static("bytes=5-4");
        let invalid_bounds =
            parse_range_header(&header, 10).expect_err("inverted range should fail");
        assert_eq!(
            invalid_bounds.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageRangeBoundsInvalid)
        );
    }

    #[test]
    fn content_length_header_distinguishes_missing_and_invalid_values() {
        let missing = content_length_header(&HeaderMap::new())
            .expect_err("missing content-length should fail");
        assert_eq!(
            missing.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageContentLengthRequired)
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_LENGTH,
            HeaderValue::from_bytes(b"\xff").expect("non-ASCII header bytes should construct"),
        );
        let non_ascii =
            content_length_header(&headers).expect_err("non-ASCII content-length should fail");
        assert_eq!(
            non_ascii.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageContentLengthInvalid)
        );

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("not-a-number"));
        let non_integer =
            content_length_header(&headers).expect_err("non-integer content-length should fail");
        assert_eq!(
            non_integer.api_error_code_override(),
            Some(ApiErrorCode::InternalStorageContentLengthInvalid)
        );

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("42"));
        assert_eq!(
            content_length_header(&headers).expect("valid content-length should parse"),
            42
        );
    }
}
