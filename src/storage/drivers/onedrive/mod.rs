//! Microsoft Graph OneDrive / SharePoint storage driver building blocks.

mod client;
mod error;
mod paths;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt};

use aster_drive_storage::traits::driver::{BlobMetadata, StorageDriver};
use aster_drive_storage::traits::extensions::{
    PresignedStorageDriver, ProviderResumableUploadCapabilities, ProviderResumableUploadDriver,
    ProviderResumableUploadFragmentOutcome, ProviderResumableUploadSession,
    ProviderResumableUploadStatus, StorageCapacityInfo, StreamUploadAttempt, StreamUploadCleanup,
    StreamUploadDriver,
};
use aster_drive_storage::{
    MapStorageErr, Result, StorageError, StorageErrorKind, storage_driver_error,
};
use aster_forge_utils::numbers;

pub use client::{
    MicrosoftGraphAccessTokenProvider, MicrosoftGraphClient, MicrosoftGraphClientConfig,
    MicrosoftGraphDrive, MicrosoftGraphDriveItem, MicrosoftGraphDriveItemParentReference,
};
pub use paths::{
    graph_drive_item_content_path, graph_drive_item_path, normalize_graph_relative_path,
};

#[derive(Clone)]
pub struct OneDriveDriver {
    client: MicrosoftGraphClient,
    drive_id: String,
    root_item_id: String,
    base_path: String,
    policy_chunk_size: i64,
}

// Microsoft Graph documents the simple PUT content limit as 250 MB, not 250 MiB.
const GRAPH_SIMPLE_UPLOAD_MAX_BYTES: usize = 250_000_000;
const GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES: usize = 1024 * 1024;
// Upload session fragments must align to 320 KiB; Microsoft recommends 5-10 MiB chunks.
const GRAPH_UPLOAD_FRAGMENT_ALIGNMENT: usize = 320 * 1024;
const GRAPH_UPLOAD_FRAGMENT_SIZE: usize = 10 * 1024 * 1024;
const GRAPH_UPLOAD_FRAGMENT_MAX_BYTES: usize = 50 * 1024 * 1024;

fn can_use_graph_simple_upload(size: u64) -> bool {
    size <= GRAPH_SIMPLE_UPLOAD_MAX_BYTES as u64
}

fn can_use_graph_in_memory_upload(size: u64, policy_chunk_size: i64) -> bool {
    if !can_use_graph_simple_upload(size) {
        return false;
    }
    let memory_limit = match u64::try_from(policy_chunk_size) {
        Ok(value) if value > 0 => value.min(GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES as u64),
        _ => GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES as u64,
    };
    size <= memory_limit
}

fn graph_upload_fragment_size(policy_chunk_size: i64) -> usize {
    let requested = match usize::try_from(policy_chunk_size) {
        Ok(value) if value > 0 => value,
        _ => GRAPH_UPLOAD_FRAGMENT_SIZE,
    };
    let capped = requested.clamp(
        GRAPH_UPLOAD_FRAGMENT_ALIGNMENT,
        GRAPH_UPLOAD_FRAGMENT_MAX_BYTES,
    );
    capped - (capped % GRAPH_UPLOAD_FRAGMENT_ALIGNMENT)
}

fn graph_simple_upload_too_large_error() -> StorageError {
    storage_driver_error(
        StorageErrorKind::Unsupported,
        "OneDrive simple upload is limited to 250 MB; use upload session support for larger objects",
    )
}

pub fn microsoft_graph_upload_capabilities() -> ProviderResumableUploadCapabilities {
    ProviderResumableUploadCapabilities {
        provider: "microsoft_graph",
        session_label: "Microsoft Graph upload session",
        min_fragment_size: GRAPH_UPLOAD_FRAGMENT_ALIGNMENT,
        default_fragment_size: GRAPH_UPLOAD_FRAGMENT_SIZE,
        max_fragment_size: GRAPH_UPLOAD_FRAGMENT_MAX_BYTES,
        fragment_alignment: GRAPH_UPLOAD_FRAGMENT_ALIGNMENT,
        max_simple_upload_size: Some(GRAPH_SIMPLE_UPLOAD_MAX_BYTES as u64),
        frontend_direct_upload: true,
        implicit_completion: true,
        abort_supported: true,
        status_query_supported: true,
    }
}

impl OneDriveDriver {
    pub fn new(
        client: MicrosoftGraphClient,
        drive_id: impl Into<String>,
        root_item_id: impl Into<String>,
        base_path: impl Into<String>,
        policy_chunk_size: i64,
    ) -> Self {
        Self {
            client,
            drive_id: drive_id.into(),
            root_item_id: root_item_id.into(),
            base_path: base_path.into(),
            policy_chunk_size,
        }
    }

    fn graph_path(&self, path: &str) -> Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        paths::graph_drive_item_path(&self.drive_id, &self.root_item_id, &relative)
    }

    fn graph_content_path(&self, path: &str) -> Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        paths::graph_drive_item_content_path(&self.drive_id, &self.root_item_id, &relative)
    }

    fn graph_upload_session_path(&self, path: &str) -> Result<String> {
        let relative = paths::join_base_path(&self.base_path, path)?;
        let item_path =
            paths::graph_drive_item_path(&self.drive_id, &self.root_item_id, &relative)?;
        if relative.is_empty() {
            Ok(format!("{item_path}/createUploadSession"))
        } else {
            Ok(format!("{item_path}:/createUploadSession"))
        }
    }

    fn graph_children_path(&self, parent_path: &str) -> Result<String> {
        let relative = paths::join_base_path(&self.base_path, parent_path)?;
        let item_path =
            paths::graph_drive_item_path(&self.drive_id, &self.root_item_id, &relative)?;
        if relative.is_empty() {
            Ok(format!("{item_path}/children"))
        } else {
            Ok(format!("{item_path}:/children"))
        }
    }

    async fn ensure_named_object_parent(&self, path: &str) -> Result<Option<String>> {
        let Some(parent_path) = paths::named_object_parent_path(path) else {
            return Ok(None);
        };
        let upload_id = parent_path.strip_prefix("files/").ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "invalid OneDrive named object path",
            )
        })?;

        match self
            .client
            .create_folder(&self.graph_children_path("")?, "files")
            .await?
        {
            client::MicrosoftGraphCreateFolderOutcome::Created { is_folder } => {
                ensure_created_graph_item_is_folder(is_folder, "OneDrive files container")?;
            }
            client::MicrosoftGraphCreateFolderOutcome::AlreadyExists => {
                let item = self
                    .client
                    .get_drive_item(&self.graph_path("files")?)
                    .await?;
                ensure_graph_item_is_folder(&item, "OneDrive files container")?;
            }
        }

        match self
            .client
            .create_folder(&self.graph_children_path("files")?, upload_id)
            .await?
        {
            client::MicrosoftGraphCreateFolderOutcome::Created { is_folder } => {
                ensure_created_graph_item_is_folder(is_folder, "OneDrive upload namespace")?;
            }
            client::MicrosoftGraphCreateFolderOutcome::AlreadyExists => {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "OneDrive upload namespace already exists",
                ));
            }
        }

        Ok(Some(parent_path))
    }

    async fn cleanup_named_object_parent(&self, parent_path: Option<&str>) -> bool {
        let Some(parent_path) = parent_path else {
            return true;
        };
        let graph_path = match self.graph_path(parent_path) {
            Ok(graph_path) => graph_path,
            Err(error) => {
                tracing::warn!(
                    parent_path,
                    "failed to resolve OneDrive upload namespace for cleanup: {error}"
                );
                return false;
            }
        };
        if let Err(error) = self.client.delete(&graph_path).await {
            tracing::warn!(
                parent_path,
                "failed to cleanup OneDrive upload namespace: {error}"
            );
            return false;
        }
        true
    }

    async fn put_owned(&self, path: &str, data: Bytes) -> Result<String> {
        if data.len() > GRAPH_SIMPLE_UPLOAD_MAX_BYTES {
            return Err(graph_simple_upload_too_large_error());
        }
        let parent_path = self.ensure_named_object_parent(path).await?;
        if let Err(error) = self
            .client
            .put_small_content(&self.graph_content_path(path)?, data)
            .await
        {
            self.cleanup_named_object_parent(parent_path.as_deref())
                .await;
            return Err(error);
        }
        Ok(path.to_string())
    }

    pub async fn validate_root(&self) -> Result<MicrosoftGraphDriveItem> {
        self.client
            .get_drive_item_by_id(&self.drive_id, &self.root_item_id)
            .await
    }

    async fn put_reader_via_upload_session(
        &self,
        path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
        attempt: Option<&StreamUploadAttempt>,
    ) -> Result<String> {
        let total_size = numbers::i64_to_u64(size, "OneDrive put_reader declared size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
        if total_size == 0 {
            return self.put_owned(path, Bytes::new()).await;
        }
        if can_use_graph_in_memory_upload(total_size, self.policy_chunk_size) {
            let capacity = numbers::u64_to_usize(total_size, "OneDrive bounded simple upload size")
                .map_storage_err(StorageErrorKind::Misconfigured)?;
            let mut data = vec![0_u8; capacity];
            reader.read_exact(&mut data).await.map_storage_err_ctx(
                StorageErrorKind::Precondition,
                "read OneDrive bounded simple upload stream",
            )?;
            reject_extra_upload_bytes(reader).await?;
            return self.put_owned(path, Bytes::from(data)).await;
        }
        let parent_path = self.ensure_named_object_parent(path).await?;
        let mut upload_url = None;
        let result = async {
            let upload_session_path = self.graph_upload_session_path(path)?;
            let upload_session = self
                .client
                .create_upload_session(&upload_session_path)
                .await?;
            upload_url = Some(upload_session.upload_url.clone());
            if let Some(attempt) = attempt {
                attempt.set_provider_session(upload_session.upload_url.clone());
            }
            let fragment_size = graph_upload_fragment_size(self.policy_chunk_size);
            let mut uploaded = 0_u64;
            while uploaded < total_size {
                let remaining = total_size - uploaded;
                let read_len = numbers::u64_to_usize(
                    remaining.min(
                        numbers::usize_to_u64(fragment_size, "OneDrive upload fragment size")
                            .map_storage_err(StorageErrorKind::Misconfigured)?,
                    ),
                    "OneDrive upload next fragment size",
                )
                .map_storage_err(StorageErrorKind::Misconfigured)?;
                let mut chunk = vec![0_u8; read_len];
                reader.read_exact(&mut chunk).await.map_storage_err_ctx(
                    StorageErrorKind::Precondition,
                    "read OneDrive upload session fragment",
                )?;
                if remaining
                    > numbers::usize_to_u64(read_len, "OneDrive upload fragment length")
                        .map_storage_err(StorageErrorKind::Misconfigured)?
                    && read_len % GRAPH_UPLOAD_FRAGMENT_ALIGNMENT != 0
                {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "OneDrive upload session fragment size must be a multiple of 320 KiB",
                    ));
                }
                self.client
                    .upload_session_fragment(
                        &upload_session.upload_url,
                        uploaded,
                        total_size,
                        chunk,
                    )
                    .await?;
                uploaded += numbers::usize_to_u64(read_len, "OneDrive uploaded fragment size")
                    .map_storage_err(StorageErrorKind::Misconfigured)?;
            }
            reject_extra_upload_bytes(reader).await?;
            Ok(path.to_string())
        }
        .await;
        if result.is_err() {
            if let Some(upload_url) = upload_url.take() {
                match self.client.abort_upload_session(&upload_url).await {
                    Ok(()) => {
                        if let Some(attempt) = attempt {
                            let _ = attempt.take_provider_session();
                        }
                    }
                    Err(error) => {
                        if let Some(attempt) = attempt {
                            attempt.set_provider_session(upload_url);
                        }
                        tracing::warn!("failed to abort OneDrive upload session: {error}");
                    }
                }
            }
            self.cleanup_named_object_parent(parent_path.as_deref())
                .await;
        } else if let Some(attempt) = attempt {
            let _ = attempt.take_provider_session();
        }
        result
    }
}

fn ensure_graph_item_is_folder(item: &MicrosoftGraphDriveItem, context: &str) -> Result<()> {
    ensure_created_graph_item_is_folder(item.folder.is_some(), context)
}

fn ensure_created_graph_item_is_folder(is_folder: bool, context: &str) -> Result<()> {
    if !is_folder {
        return Err(storage_driver_error(
            StorageErrorKind::Precondition,
            format!("{context} is not a folder"),
        ));
    }
    Ok(())
}

#[async_trait]
impl StorageDriver for OneDriveDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        if data.len() > GRAPH_SIMPLE_UPLOAD_MAX_BYTES {
            return Err(graph_simple_upload_too_large_error());
        }
        self.put_owned(path, Bytes::copy_from_slice(data)).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.client.get_bytes(&self.graph_content_path(path)?).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.graph_content_path(path)?, None, None)
            .await
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.client
            .get_stream(&self.graph_content_path(path)?, Some(offset), length)
            .await
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            presigned: Some(self),
            stream_upload: Some(self),
            provider_resumable: Some(self),
            ..Default::default()
        }
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        let delete_path = paths::named_object_parent_path(path).unwrap_or_else(|| path.to_string());
        self.client.delete(&self.graph_path(&delete_path)?).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.client.exists(&self.graph_path(path)?).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.client.metadata(&self.graph_path(path)?).await
    }

    async fn capacity_info(&self) -> aster_drive_storage::Result<StorageCapacityInfo> {
        self.client.capacity_info(&self.drive_id).await
    }
}

#[async_trait]
impl PresignedStorageDriver for OneDriveDriver {
    async fn presigned_url(
        &self,
        path: &str,
        _expires: std::time::Duration,
        options: aster_drive_storage::traits::driver::PresignedDownloadOptions,
    ) -> aster_drive_storage::Result<Option<String>> {
        if options.require_download_name_match {
            let Some(stored_filename) = paths::provider_resumable_filename(path) else {
                // Legacy objects do not carry a provider filename in their
                // storage path, so strict filename mode cannot prove a match.
                return Ok(None);
            };
            if options
                .download_name
                .as_deref()
                .is_some_and(|name| name != stored_filename)
            {
                return Ok(None);
            }
        }
        // Graph owns the response headers of its preauthenticated URL. The
        // generic response-content-* options are intentionally not appended.
        self.client
            .get_download_url(&self.graph_content_path(path)?)
            .await
            .map(Some)
    }

    async fn presigned_put_request(
        &self,
        _path: &str,
        _expires: std::time::Duration,
    ) -> aster_drive_storage::Result<Option<aster_drive_storage::PresignedUploadRequest>> {
        Ok(None)
    }
}

#[async_trait]
impl StreamUploadDriver for OneDriveDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        let attempt = StreamUploadAttempt::new(storage_path, size)?;
        self.stage_attempt(&attempt, reader).await?;
        self.commit_attempt(&attempt).await
    }

    async fn stage_attempt(
        &self,
        attempt: &StreamUploadAttempt,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
    ) -> aster_drive_storage::Result<()> {
        self.put_reader_via_upload_session(
            &attempt.storage_path,
            reader,
            attempt.expected_size,
            Some(attempt),
        )
        .await
        .map(|_| ())
    }

    async fn abort_attempt(
        &self,
        attempt: &StreamUploadAttempt,
    ) -> aster_drive_storage::Result<StreamUploadCleanup> {
        let parent_path = paths::named_object_parent_path(&attempt.storage_path);
        let mut session_cleanup_deferred = false;
        if let Some(upload_url) = attempt.take_provider_session()
            && let Err(error) = self.client.abort_upload_session(&upload_url).await
        {
            attempt.set_provider_session(upload_url);
            session_cleanup_deferred = true;
            tracing::warn!("failed to abort OneDrive attempt session: {error}");
        }
        let namespace_cleaned = self
            .cleanup_named_object_parent(parent_path.as_deref())
            .await;
        if session_cleanup_deferred || !namespace_cleaned {
            Ok(StreamUploadCleanup::Deferred)
        } else if parent_path.is_some() {
            Ok(StreamUploadCleanup::Cleaned)
        } else {
            Ok(StreamUploadCleanup::NotRequired)
        }
    }

    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "open OneDrive upload file")?;
        let metadata = file
            .metadata()
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "stat OneDrive upload file")?;
        let size = numbers::u64_to_i64(metadata.len(), "OneDrive upload file size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
        self.put_reader(storage_path, Box::new(file), size).await
    }
}

#[async_trait]
impl ProviderResumableUploadDriver for OneDriveDriver {
    fn provider_resumable_upload_capabilities(&self) -> ProviderResumableUploadCapabilities {
        microsoft_graph_upload_capabilities()
    }

    async fn create_upload_session(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<ProviderResumableUploadSession> {
        let parent_path = self.ensure_named_object_parent(path).await?;
        let session = match self
            .client
            .create_upload_session(&self.graph_upload_session_path(path)?)
            .await
        {
            Ok(session) => session,
            Err(error) => {
                self.cleanup_named_object_parent(parent_path.as_deref())
                    .await;
                return Err(error);
            }
        };
        Ok(ProviderResumableUploadSession {
            upload_url: session.upload_url,
            expires_at: session.expires_at,
            next_expected_ranges: session.next_expected_ranges,
        })
    }

    async fn query_upload_session(
        &self,
        upload_url: &str,
    ) -> aster_drive_storage::Result<ProviderResumableUploadStatus> {
        let session = self.client.query_upload_session(upload_url).await?;
        Ok(ProviderResumableUploadStatus {
            expires_at: session.expires_at,
            next_expected_ranges: session.next_expected_ranges,
        })
    }

    async fn abort_upload_session(&self, upload_url: &str) -> aster_drive_storage::Result<()> {
        self.client.abort_upload_session(upload_url).await
    }

    async fn upload_session_fragment_reader(
        &self,
        upload_url: &str,
        start: u64,
        total_size: u64,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        fragment_size: i64,
    ) -> aster_drive_storage::Result<ProviderResumableUploadFragmentOutcome> {
        let size = numbers::bytes_to_usize(fragment_size, "OneDrive upload fragment size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
        if size > GRAPH_UPLOAD_FRAGMENT_MAX_BYTES {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session fragment exceeds the provider request-size limit",
            ));
        }
        let fragment_size_u64 = numbers::usize_to_u64(size, "OneDrive upload fragment size")
            .map_storage_err(StorageErrorKind::Misconfigured)?;
        let is_final = start
            .checked_add(fragment_size_u64)
            .is_some_and(|end| end == total_size);
        if !is_final && !size.is_multiple_of(GRAPH_UPLOAD_FRAGMENT_ALIGNMENT) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session non-final fragment size must be a multiple of 320 KiB",
            ));
        }
        let outcome = self
            .client
            .upload_session_fragment_reader(upload_url, start, total_size, reader, fragment_size)
            .await?;
        Ok(ProviderResumableUploadFragmentOutcome {
            completed: outcome.completed,
            next_expected_ranges: outcome.next_expected_ranges,
        })
    }
}

async fn reject_extra_upload_bytes(
    mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
) -> Result<()> {
    let mut extra = [0_u8; 1];
    let read = reader.read(&mut extra).await.map_storage_err_ctx(
        StorageErrorKind::Precondition,
        "check OneDrive upload stream length",
    )?;
    if read != 0 {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "OneDrive upload stream exceeded declared size",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    const NAMED_PATH: &str = "files/550e8400-e29b-41d4-a716-446655440000/video.mp4";

    #[derive(Clone, Copy, Default)]
    struct GraphLifecycleConfig {
        files_exists: bool,
        uuid_conflict: bool,
        upload_session_failure: bool,
        content_failure: bool,
        abort_session_failures: usize,
        upload_session_delay: Duration,
        fragment_delay: Duration,
    }

    #[derive(Default)]
    struct GraphLifecycleState {
        methods: Vec<String>,
        paths: Vec<String>,
        bodies: Vec<serde_json::Value>,
    }

    struct GraphLifecycleServer {
        base_url: String,
        state: Arc<Mutex<GraphLifecycleState>>,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl GraphLifecycleServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    async fn spawn_graph_lifecycle_server(config: GraphLifecycleConfig) -> GraphLifecycleServer {
        async fn graph(
            request: HttpRequest,
            body: web::Bytes,
            config: web::Data<GraphLifecycleConfig>,
            state: web::Data<Arc<Mutex<GraphLifecycleState>>>,
        ) -> HttpResponse {
            let method = request.method().to_string();
            let path = request.uri().path().to_string();
            let json_body = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
            {
                let mut state = state.lock().expect("Graph lifecycle state lock");
                state.methods.push(method.clone());
                state.paths.push(path.clone());
                state.bodies.push(json_body.clone());
            }

            if method == "POST" && path.ends_with("/createUploadSession") {
                if config.upload_session_failure {
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": { "code": "serverError", "message": "session failed" }
                    }));
                }
                tokio::time::sleep(config.upload_session_delay).await;
                let upload_url =
                    format!("http://{}/upload/session", request.connection_info().host());
                return HttpResponse::Ok().json(serde_json::json!({
                    "uploadUrl": upload_url,
                    "nextExpectedRanges": ["0-"]
                }));
            }

            if method == "POST" && path.ends_with("/children") {
                let name = json_body
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let conflict = (name == "files" && config.files_exists)
                    || (name != "files" && config.uuid_conflict);
                if conflict {
                    return HttpResponse::Conflict().json(serde_json::json!({
                        "error": { "code": "nameAlreadyExists", "message": "exists" }
                    }));
                }
                return HttpResponse::Created().json(serde_json::json!({
                    "id": format!("folder-{name}"),
                    "name": name,
                    "folder": {}
                }));
            }

            if method == "GET" && path.ends_with(":/content") {
                return HttpResponse::Found()
                    .insert_header((
                        actix_web::http::header::LOCATION,
                        "https://download.example/file",
                    ))
                    .finish();
            }

            if method == "GET" && path.ends_with(":/files") {
                return HttpResponse::Ok().json(serde_json::json!({
                    "id": "files-folder",
                    "name": "files",
                    "folder": {}
                }));
            }

            if method == "PUT" && path.ends_with(":/content") {
                if config.content_failure {
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": { "code": "serverError", "message": "content failed" }
                    }));
                }
                return HttpResponse::Created().finish();
            }

            if method == "PUT" && path == "/upload/session" {
                tokio::time::sleep(config.fragment_delay).await;
                return HttpResponse::Created().finish();
            }

            if method == "DELETE" && path == "/upload/session" {
                let abort_attempt = state
                    .lock()
                    .expect("Graph lifecycle state lock")
                    .paths
                    .iter()
                    .filter(|path| path.as_str() == "/upload/session")
                    .count();
                if abort_attempt <= config.abort_session_failures {
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "error": { "code": "serverError", "message": "abort failed" }
                    }));
                }
                return HttpResponse::NotFound().finish();
            }

            if method == "DELETE" {
                return HttpResponse::NotFound().finish();
            }

            HttpResponse::NotFound().json(serde_json::json!({
                "error": { "code": "itemNotFound", "message": "missing" }
            }))
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let base_url = format!(
            "http://{}",
            listener
                .local_addr()
                .expect("listener address should exist")
        );
        let state = Arc::new(Mutex::new(GraphLifecycleState::default()));
        let state_data = web::Data::new(state.clone());
        let config_data = web::Data::new(config);
        let server = HttpServer::new(move || {
            App::new()
                .app_data(state_data.clone())
                .app_data(config_data.clone())
                .app_data(web::PayloadConfig::new(
                    GRAPH_UPLOAD_FRAGMENT_MAX_BYTES + 1024,
                ))
                .default_service(web::to(graph))
        })
        .listen(listener)
        .expect("server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        GraphLifecycleServer {
            base_url,
            state,
            handle,
            task,
        }
    }

    async fn wait_for_graph_request(
        server: &GraphLifecycleServer,
        predicate: impl Fn(&str) -> bool,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if server
                    .state
                    .lock()
                    .expect("Graph lifecycle state lock")
                    .paths
                    .iter()
                    .any(|path| predicate(path))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected Graph request should arrive");
    }

    fn lifecycle_driver(server: &GraphLifecycleServer) -> OneDriveDriver {
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        OneDriveDriver::new(client, "drive-id", "root-id", "", 5 * 1024 * 1024)
    }

    #[test]
    fn graph_simple_upload_limit_uses_decimal_mb() {
        assert_eq!(GRAPH_SIMPLE_UPLOAD_MAX_BYTES, 250_000_000);
        assert!(can_use_graph_simple_upload(250_000_000));
        assert!(!can_use_graph_simple_upload(250_000_001));
        assert!(!can_use_graph_simple_upload(250 * 1024 * 1024));
    }

    #[test]
    fn graph_simple_upload_too_large_error_uses_decimal_units() {
        let error = graph_simple_upload_too_large_error();

        assert_eq!(error.kind(), StorageErrorKind::Unsupported);
        assert!(error.message().contains("250 MB"));
        assert!(!error.message().contains("MiB"));
    }

    #[test]
    fn graph_in_memory_upload_is_bounded_to_one_mib() {
        assert!(can_use_graph_in_memory_upload(1, 0));
        assert!(can_use_graph_in_memory_upload(
            GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES as u64,
            0
        ));
        assert!(!can_use_graph_in_memory_upload(
            GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES as u64 + 1,
            0
        ));
    }

    #[tokio::test]
    async fn simple_reader_uploads_accept_zero_and_one_byte() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);

        driver
            .put_reader(NAMED_PATH, Box::new(tokio::io::empty()), 0)
            .await
            .expect("empty simple upload should succeed");
        driver
            .put_reader(NAMED_PATH, Box::new(std::io::Cursor::new(vec![7_u8])), 1)
            .await
            .expect("one-byte simple upload should succeed");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(
                state
                    .methods
                    .iter()
                    .filter(|method| *method == "PUT")
                    .count(),
                2
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn stream_reader_boundary_selects_simple_or_provider_session_path() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        let driver = OneDriveDriver::new(
            client,
            "drive-id",
            "root-id",
            "",
            GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES as i64,
        );
        let boundary = GRAPH_SIMPLE_UPLOAD_IN_MEMORY_MAX_BYTES;

        driver
            .put_reader(
                NAMED_PATH,
                Box::new(tokio::io::repeat(7).take(boundary as u64)),
                boundary as i64,
            )
            .await
            .expect("exact one MiB should use simple upload");
        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert!(state.methods.iter().any(|method| method == "PUT"));
            assert!(!state.paths.iter().any(|path| path == "/upload/session"));
        }

        server.state.lock().unwrap().methods.clear();
        server.state.lock().unwrap().paths.clear();
        driver
            .put_reader(
                NAMED_PATH,
                Box::new(tokio::io::repeat(7).take((boundary + 1) as u64)),
                (boundary + 1) as i64,
            )
            .await
            .expect("one byte over the bound should use provider session");
        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert!(
                state
                    .paths
                    .iter()
                    .any(|path| path.ends_with("createUploadSession"))
            );
            assert!(state.paths.iter().any(|path| path == "/upload/session"));
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn failed_session_abort_keeps_attempt_retryable_until_provider_confirms_cleanup() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            abort_session_failures: 2,
            ..Default::default()
        })
        .await;
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 1);
        let attempt = StreamUploadAttempt::new(NAMED_PATH, 2).unwrap();

        driver
            .stage_attempt(&attempt, Box::new(tokio::io::empty()))
            .await
            .expect_err("truncated session upload should fail after creating a provider session");
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::Deferred,
            "the first explicit retry should preserve the provider session"
        );
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::Cleaned,
            "the next retry should observe provider cleanup"
        );
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::Cleaned,
            "repeated abort should confirm the exclusive namespace remains absent"
        );

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(
                state
                    .paths
                    .iter()
                    .filter(|path| path.as_str() == "/upload/session")
                    .count(),
                3
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn cancelled_stage_before_session_response_cleans_exclusive_namespace() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            upload_session_delay: Duration::from_secs(2),
            ..Default::default()
        })
        .await;
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 1);
        let attempt = StreamUploadAttempt::new(NAMED_PATH, 2).unwrap();
        let stage_driver = driver.clone();
        let stage_attempt = attempt.clone();
        let stage = tokio::spawn(async move {
            stage_driver
                .stage_attempt(
                    &stage_attempt,
                    Box::new(std::io::Cursor::new(vec![1_u8, 2])),
                )
                .await
        });
        wait_for_graph_request(&server, |path| path.ends_with("/createUploadSession")).await;

        stage.abort();
        assert!(stage.await.unwrap_err().is_cancelled());
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::Cleaned
        );

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert!(!state.paths.iter().any(|path| path == "/upload/session"));
            assert!(
                state
                    .paths
                    .iter()
                    .any(|path| path.ends_with("/files/550e8400-e29b-41d4-a716-446655440000"))
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn cancelled_stage_after_session_creation_aborts_session_and_cleans_namespace() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            fragment_delay: Duration::from_secs(2),
            ..Default::default()
        })
        .await;
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 1);
        let attempt = StreamUploadAttempt::new(NAMED_PATH, 2).unwrap();
        let stage_driver = driver.clone();
        let stage_attempt = attempt.clone();
        let stage = tokio::spawn(async move {
            stage_driver
                .stage_attempt(
                    &stage_attempt,
                    Box::new(std::io::Cursor::new(vec![1_u8, 2])),
                )
                .await
        });
        wait_for_graph_request(&server, |path| path == "/upload/session").await;

        stage.abort();
        assert!(stage.await.unwrap_err().is_cancelled());
        assert_eq!(
            driver.abort_attempt(&attempt).await.unwrap(),
            StreamUploadCleanup::Cleaned
        );

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(
                state
                    .paths
                    .iter()
                    .filter(|path| path.as_str() == "/upload/session")
                    .count(),
                2,
                "the fragment request must be followed by session abort"
            );
            assert!(
                state
                    .paths
                    .iter()
                    .any(|path| path.ends_with("/files/550e8400-e29b-41d4-a716-446655440000"))
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn simple_reader_upload_rejects_short_and_long_sources_before_provider_mutation() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);

        let short = driver
            .put_reader(NAMED_PATH, Box::new(std::io::Cursor::new(vec![1_u8])), 2)
            .await
            .expect_err("short reader must not satisfy the declared size");
        assert_eq!(short.kind(), StorageErrorKind::Precondition);

        let long = driver
            .put_reader(
                NAMED_PATH,
                Box::new(std::io::Cursor::new(vec![1_u8, 2_u8])),
                1,
            )
            .await
            .expect_err("extra reader bytes must be rejected");
        assert_eq!(long.kind(), StorageErrorKind::Misconfigured);

        assert!(
            server
                .state
                .lock()
                .expect("Graph lifecycle state lock")
                .methods
                .is_empty()
        );
        server.stop().await;
    }

    #[test]
    fn graph_upload_fragment_size_uses_policy_chunk_size_with_alignment() {
        assert_eq!(graph_upload_fragment_size(0), GRAPH_UPLOAD_FRAGMENT_SIZE);
        assert_eq!(graph_upload_fragment_size(-1), GRAPH_UPLOAD_FRAGMENT_SIZE);
        assert_eq!(
            graph_upload_fragment_size((5 * 1024 * 1024 + 123) as i64),
            5 * 1024 * 1024
        );
        assert_eq!(
            graph_upload_fragment_size(1),
            GRAPH_UPLOAD_FRAGMENT_ALIGNMENT
        );
        assert_eq!(
            graph_upload_fragment_size(250_000_000),
            GRAPH_UPLOAD_FRAGMENT_MAX_BYTES
        );
    }

    #[test]
    fn onedrive_exposes_provider_native_resumable_upload_capabilities() {
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            "https://graph.microsoft.com/v1.0",
            "token",
        ))
        .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 5 * 1024 * 1024);

        let provider_resumable = driver
            .extensions()
            .provider_resumable
            .expect("OneDrive should expose provider-native resumable upload");
        let capabilities = provider_resumable.provider_resumable_upload_capabilities();

        assert_eq!(capabilities.provider, "microsoft_graph");
        assert_eq!(capabilities.session_label, "Microsoft Graph upload session");
        assert_eq!(
            capabilities.min_fragment_size,
            GRAPH_UPLOAD_FRAGMENT_ALIGNMENT
        );
        assert_eq!(capabilities.fragment_alignment, 320 * 1024);
        assert_eq!(capabilities.default_fragment_size, 10 * 1024 * 1024);
        assert_eq!(capabilities.max_fragment_size, 50 * 1024 * 1024);
        assert_eq!(
            capabilities.max_simple_upload_size,
            Some(GRAPH_SIMPLE_UPLOAD_MAX_BYTES as u64)
        );
        assert!(capabilities.frontend_direct_upload);
        assert!(capabilities.implicit_completion);
        assert!(capabilities.abort_supported);
        assert!(capabilities.status_query_supported);
        assert!(driver.extensions().presigned.is_some());
    }

    #[tokio::test]
    async fn provider_relay_fragments_enforce_alignment_and_request_limit() {
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            "https://graph.microsoft.com",
            "token",
        ))
        .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 5 * 1024 * 1024);
        let provider = driver
            .extensions()
            .provider_resumable
            .expect("OneDrive should expose provider-native resumable upload");

        let misaligned = provider
            .upload_session_fragment_reader(
                "https://upload.example/session",
                0,
                2,
                Box::new(std::io::Cursor::new(Vec::<u8>::new())),
                1,
            )
            .await
            .expect_err("non-final fragments must be aligned");
        assert_eq!(misaligned.kind(), StorageErrorKind::Misconfigured);

        let oversized = provider
            .upload_session_fragment_reader(
                "https://upload.example/session",
                0,
                (GRAPH_UPLOAD_FRAGMENT_MAX_BYTES + 1) as u64,
                Box::new(std::io::Cursor::new(Vec::<u8>::new())),
                (GRAPH_UPLOAD_FRAGMENT_MAX_BYTES + 1) as i64,
            )
            .await
            .expect_err("provider request-size limit must be enforced");
        assert_eq!(oversized.kind(), StorageErrorKind::Misconfigured);
    }

    #[tokio::test]
    async fn strict_mode_relays_legacy_objects_but_provider_native_mode_uses_graph() {
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            "https://graph.microsoft.com",
            "token",
        ))
        .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 5 * 1024 * 1024);
        let options = aster_drive_storage::traits::driver::PresignedDownloadOptions {
            download_name: Some("video.mp4".to_string()),
            require_download_name_match: true,
            ..Default::default()
        };

        assert_eq!(
            driver
                .presigned_url(
                    "files/550e8400-e29b-41d4-a716-446655440000",
                    std::time::Duration::from_secs(60),
                    options.clone(),
                )
                .await
                .expect("legacy path should be classified"),
            None
        );

        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);
        let native_options = aster_drive_storage::traits::driver::PresignedDownloadOptions {
            download_name: Some("video.mp4".to_string()),
            ..Default::default()
        };
        assert_eq!(
            driver
                .presigned_url(
                    "files/550e8400-e29b-41d4-a716-446655440000",
                    std::time::Duration::from_secs(60),
                    native_options,
                )
                .await
                .expect("provider-native legacy path should classify")
                .expect("provider-native filename mode should keep direct download"),
            "https://download.example/file"
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn renamed_object_declines_direct_download_when_filename_match_is_required() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);
        let options = aster_drive_storage::traits::driver::PresignedDownloadOptions {
            download_name: Some("video.mp4".to_string()),
            require_download_name_match: true,
            ..Default::default()
        };

        assert_eq!(
            driver
                .presigned_url(
                    "files/550e8400-e29b-41d4-a716-446655440000/old.mp4",
                    std::time::Duration::from_secs(60),
                    options,
                )
                .await
                .expect("strict filename mode should classify renamed path"),
            None
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn named_object_with_matching_filename_uses_graph_direct_download() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);

        let url = driver
            .presigned_url(
                NAMED_PATH,
                std::time::Duration::from_secs(60),
                aster_drive_storage::traits::driver::PresignedDownloadOptions {
                    download_name: Some("video.mp4".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("matching named object should resolve direct URL")
            .expect("matching named object should return Graph download URL");

        assert_eq!(url, "https://download.example/file");
        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["GET"]);
            assert_eq!(
                state.paths,
                [
                    "/v1.0/drives/drive-id/items/root-id:/files/550e8400-e29b-41d4-a716-446655440000/video.mp4:/content"
                ]
            );
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn named_frontend_upload_creates_shared_and_exclusive_folders_first() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);

        driver
            .create_upload_session(NAMED_PATH)
            .await
            .expect("named upload session should be created");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "POST", "POST"]);
            assert_eq!(
                state.paths,
                [
                    "/v1.0/drives/drive-id/items/root-id/children",
                    "/v1.0/drives/drive-id/items/root-id:/files:/children",
                    "/v1.0/drives/drive-id/items/root-id:/files/550e8400-e29b-41d4-a716-446655440000/video.mp4:/createUploadSession",
                ]
            );
            assert_eq!(
                state.bodies[0],
                serde_json::json!({
                    "name": "files",
                    "folder": {},
                    "@microsoft.graph.conflictBehavior": "fail"
                })
            );
            assert_eq!(
                state.bodies[1],
                serde_json::json!({
                    "name": "550e8400-e29b-41d4-a716-446655440000",
                    "folder": {},
                    "@microsoft.graph.conflictBehavior": "fail"
                })
            );
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn existing_shared_folder_is_verified_before_creating_upload_namespace() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            files_exists: true,
            ..Default::default()
        })
        .await;
        let driver = lifecycle_driver(&server);

        driver
            .create_upload_session(NAMED_PATH)
            .await
            .expect("existing shared files folder should be reused");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "GET", "POST", "POST"]);
            assert_eq!(state.paths[1], "/v1.0/drives/drive-id/items/root-id:/files");
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn existing_uuid_namespace_is_rejected_without_overwrite_or_cleanup() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            uuid_conflict: true,
            ..Default::default()
        })
        .await;
        let driver = lifecycle_driver(&server);

        let error = driver
            .create_upload_session(NAMED_PATH)
            .await
            .expect_err("existing upload namespace must be treated as a collision");

        assert_eq!(error.kind(), StorageErrorKind::Precondition);
        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "POST"]);
            assert!(!state.methods.iter().any(|method| method == "DELETE"));
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn failed_session_creation_deletes_the_exclusive_namespace() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            upload_session_failure: true,
            ..Default::default()
        })
        .await;
        let driver = lifecycle_driver(&server);

        driver
            .create_upload_session(NAMED_PATH)
            .await
            .expect_err("failed provider session should cleanup its namespace");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "POST", "POST", "DELETE"]);
            assert_eq!(
                state.paths[3],
                "/v1.0/drives/drive-id/items/root-id:/files/550e8400-e29b-41d4-a716-446655440000"
            );
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn failed_large_stream_session_deletes_the_exclusive_namespace() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            upload_session_failure: true,
            ..Default::default()
        })
        .await;
        let client =
            MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(&server.base_url, "token"))
                .expect("Graph client should build");
        let driver = OneDriveDriver::new(client, "drive-id", "root-id", "", 1);

        driver
            .put_reader(NAMED_PATH, Box::new(tokio::io::empty()), 2)
            .await
            .expect_err("failed large stream session should cleanup its namespace");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "POST", "POST", "DELETE"]);
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn failed_small_upload_cleans_namespace_and_named_delete_targets_parent() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig {
            content_failure: true,
            ..Default::default()
        })
        .await;
        let driver = lifecycle_driver(&server);

        driver
            .put(NAMED_PATH, b"payload")
            .await
            .expect_err("failed content upload should cleanup its namespace");
        driver
            .delete(NAMED_PATH)
            .await
            .expect("deleting an already absent named object should be idempotent");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST", "POST", "PUT", "DELETE", "DELETE"]);
            assert_eq!(state.paths[3], state.paths[4]);
            assert!(state.paths[3].ends_with("/files/550e8400-e29b-41d4-a716-446655440000"));
            drop(state);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn legacy_frontend_upload_path_keeps_the_existing_flat_layout() {
        let server = spawn_graph_lifecycle_server(GraphLifecycleConfig::default()).await;
        let driver = lifecycle_driver(&server);

        driver
            .create_upload_session("files/550e8400-e29b-41d4-a716-446655440000")
            .await
            .expect("legacy flat upload path should remain supported");

        {
            let state = server.state.lock().expect("Graph lifecycle state lock");
            assert_eq!(state.methods, ["POST"]);
            assert!(
                state.paths[0]
                    .ends_with("/files/550e8400-e29b-41d4-a716-446655440000:/createUploadSession")
            );
            drop(state);
        }
        server.stop().await;
    }
}
