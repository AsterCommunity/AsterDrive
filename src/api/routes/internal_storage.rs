//! 内部对象存储协议路由：`internal_storage`。

use crate::api::middleware::internal_storage_cors::PresignedInternalStorageCors;
use crate::api::response::ApiResponse;
use crate::errors::{AsterError, Result};
use crate::runtime::FollowerAppState;
use crate::services::{managed_ingress_profile_service, master_binding_service};
use crate::storage::driver::{BlobMetadata, StorageDriver};
use crate::storage::object_key;
use crate::storage::remote_protocol::{
    INTERNAL_AUTH_SIGNATURE_HEADER, PRESIGNED_AUTH_ACCESS_KEY_QUERY, RemoteBindingSyncRequest,
    RemoteCreateIngressProfileRequest, RemoteStorageCapabilities, RemoteStorageCapacityResponse,
    RemoteStorageComposeRequest, RemoteStorageComposeResponse, RemoteStorageListResponse,
    RemoteStorageObjectMetadata, RemoteUpdateIngressProfileRequest,
};
use actix_web::http::StatusCode;
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
        .route("/ingress-profiles", web::get().to(list_ingress_profiles))
        .route("/ingress-profiles", web::post().to(create_ingress_profile))
        .route(
            "/ingress-profiles/{profile_key}",
            web::patch().to(update_ingress_profile),
        )
        .route(
            "/ingress-profiles/{profile_key}",
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
        return Err(AsterError::validation_error(
            "range length must be greater than zero",
        ));
    }
    if total_size == 0 {
        return Err(AsterError::validation_error(
            "range cannot be requested for empty object",
        ));
    }

    let start = offset.unwrap_or(0);
    if start >= total_size {
        return Err(AsterError::validation_error(
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
        .map_err(|_| AsterError::validation_error("range header must be valid ASCII"))?
        .trim();
    let range = raw
        .strip_prefix("bytes=")
        .ok_or_else(|| AsterError::validation_error("range header must use bytes unit"))?;
    if range.contains(',') {
        return Err(AsterError::validation_error(
            "multiple range requests are not supported",
        ));
    }

    let (start_raw, end_raw) = range
        .split_once('-')
        .ok_or_else(|| AsterError::validation_error("range header is malformed"))?;
    if start_raw.is_empty() && end_raw.is_empty() {
        return Err(AsterError::validation_error("range header is malformed"));
    }

    if start_raw.is_empty() {
        let suffix_length = parse_range_bound(end_raw, "range suffix length")?;
        if suffix_length == 0 {
            return Err(AsterError::validation_error(
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
        return Err(AsterError::validation_error(
            "range end must be greater than or equal to range start",
        ));
    }
    let length = end
        .checked_sub(start)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| AsterError::validation_error("range length exceeds u64 range"))?;
    Ok((Some(start), Some(length)))
}

fn parse_range_bound(value: &str, name: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .map_err(|_| AsterError::validation_error(format!("{name} must be a non-negative integer")))
}

async fn get_capabilities(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    master_binding_service::authorize_internal_binding_request(state.get_ref(), &req).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageCapabilities::current())))
}

async fn get_capacity(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let capacity = ctx.ingress_driver.capacity_info().await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(RemoteStorageCapacityResponse { capacity })))
}

async fn sync_binding(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteBindingSyncRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    master_binding_service::sync_from_primary(
        state.get_ref(),
        &binding.access_key,
        master_binding_service::SyncMasterBindingInput {
            name: body.name.clone(),
            is_enabled: body.is_enabled,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn list_objects(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    query: web::Query<ObjectQuery>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let list_driver = ctx
        .ingress_driver
        .as_list()
        .ok_or_else(|| AsterError::storage_driver_error("ingress target does not support list"))?;

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
        .collect();

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
    let storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &path.into_inner())?;
    let content_length = req
        .headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| AsterError::validation_error("content-length header is required"))?;
    if content_length < 0 {
        return Err(AsterError::validation_error(
            "content-length must be non-negative",
        ));
    }
    validate_ingress_object_size(content_length, ctx.ingress_max_file_size, "object")?;

    let stream_driver = ctx.ingress_driver.as_stream_upload().ok_or_else(|| {
        AsterError::storage_driver_error("ingress target does not support stream upload")
    })?;
    let (writer, reader) = tokio::io::duplex(RELAY_UPLOAD_BUFFER_SIZE);
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
                .put_reader(&storage_path, Box::new(reader), content_length)
                .await;
            let relay_result = relay_task.await.map_err(|error| {
                AsterError::storage_driver_error(format!("relay upload task failed: {error}"))
            })?;
            Ok::<(Result<String>, Result<String>), AsterError>((upload_result, relay_result))
        })
        .await?;

    upload_result?;
    let etag = relay_result?;
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
        return Err(AsterError::validation_error(
            "compose request requires part_keys",
        ));
    }
    if body.expected_size < 0 {
        return Err(AsterError::validation_error(
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
        AsterError::storage_driver_error("ingress target does not support stream upload")
    })?;
    let target_storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &body.target_key)?;
    let part_storage_paths: Vec<String> = body
        .part_keys
        .iter()
        .map(|key| master_binding_service::provider_storage_path(&ctx.binding, key))
        .collect::<Result<_>>()?;
    let cleanup_part_storage_paths = part_storage_paths.clone();
    let expected_size = body.expected_size;
    let expected_size_u64 = u64::try_from(expected_size)
        .map_err(|_| AsterError::validation_error("compose expected_size exceeds u64 range"))?;

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
    let storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &path.into_inner())?;
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
    if let Some(range) = partial_range {
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
    Ok(response.streaming(body))
}

async fn head_object(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let ctx = master_binding_service::authorize_internal_request(state.get_ref(), &req).await?;
    let storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &path.into_inner())?;
    let metadata = metadata_or_not_found(ctx.ingress_driver.as_ref(), &storage_path).await?;

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
    let storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &path.into_inner())?;
    let metadata = metadata_or_not_found(ctx.ingress_driver.as_ref(), &storage_path).await?;

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
    let storage_path =
        master_binding_service::provider_storage_path(&ctx.binding, &path.into_inner())?;
    ctx.ingress_driver.delete(&storage_path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

async fn list_ingress_profiles(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let profiles = managed_ingress_profile_service::list(state.get_ref(), &binding).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profiles)))
}

async fn create_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    body: web::Json<RemoteCreateIngressProfileRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let profile =
        managed_ingress_profile_service::create(state.get_ref(), &binding, body.into_inner())
            .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(profile)))
}

async fn update_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Json<RemoteUpdateIngressProfileRequest>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    let profile = managed_ingress_profile_service::update(
        state.get_ref(),
        &binding,
        &path.into_inner(),
        body.into_inner(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

async fn delete_ingress_profile(
    state: web::Data<FollowerAppState>,
    req: HttpRequest,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let binding =
        master_binding_service::authorize_binding_sync_request(state.get_ref(), &req).await?;
    managed_ingress_profile_service::delete(state.get_ref(), &binding, &path.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
