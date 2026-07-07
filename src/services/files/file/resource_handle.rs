use std::time::Duration;

use crate::api::dto::files::{
    FileResourceConditionalHeaders, FileResourceCredentials, FileResourceDeliveryInfo,
    FileResourceDeliveryMode, FileResourceHandle, FileResourceHandleRequest, FileResourceIdentity,
    FileResourcePurpose, FileResourceRedirectPolicy, FileResourceRepresentation,
    FileResourceRequestInfo,
};
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{media::processing, workspace::storage::WorkspaceStorageScope};
use crate::storage::PresignedDownloadOptions;

use super::{DownloadDisposition, get_info_in_scope, requires_inline_sandbox};

const PRESIGNED_PREVIEW_TTL_SECS: u64 = 5 * 60;

pub(crate) struct FileResourcePathSet {
    pub download: String,
    pub image_preview: String,
    pub thumbnail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedRepresentation {
    Original,
    ImagePreview,
    Thumbnail,
}

pub(crate) async fn resolve_file_resource_handle(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    paths: FileResourcePathSet,
    request: &FileResourceHandleRequest,
) -> Result<FileResourceHandle> {
    let file = get_info_in_scope(state, scope, file_id).await?;
    let blob = file_repo::find_blob_by_id(state.reader_db(), file.blob_id).await?;
    resolve_file_resource_handle_for_file(
        state,
        &file,
        &blob,
        paths,
        request,
        Some(match scope {
            WorkspaceStorageScope::Personal { .. } => "personal",
            WorkspaceStorageScope::Team { .. } => "team",
        }),
    )
    .await
}

pub(crate) async fn resolve_file_resource_handle_for_file(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
    paths: FileResourcePathSet,
    request: &FileResourceHandleRequest,
    scope: Option<&str>,
) -> Result<FileResourceHandle> {
    let representation = resolve_representation(file, request);

    match representation {
        ResolvedRepresentation::Original => {
            original_handle(
                state,
                paths.download,
                file,
                blob,
                request.delivery_mode,
                scope,
            )
            .await
        }
        ResolvedRepresentation::ImagePreview => image_preview_handle(
            state,
            paths.image_preview,
            file,
            blob,
            request.delivery_mode,
            scope,
        ),
        ResolvedRepresentation::Thumbnail => thumbnail_handle(
            state,
            paths.thumbnail,
            file,
            blob,
            request.delivery_mode,
            scope,
        ),
    }
}

fn resolve_representation(
    file: &file::Model,
    request: &FileResourceHandleRequest,
) -> ResolvedRepresentation {
    match request.representation {
        FileResourceRepresentation::Original => ResolvedRepresentation::Original,
        FileResourceRepresentation::ImagePreview => ResolvedRepresentation::ImagePreview,
        FileResourceRepresentation::Thumbnail => ResolvedRepresentation::Thumbnail,
        FileResourceRepresentation::Auto => {
            if request.purpose == FileResourcePurpose::Preview
                && request.delivery_mode == FileResourceDeliveryMode::BlobUrl
                && is_image_mime(&file.mime_type)
                && !can_browser_render_image(file)
            {
                return ResolvedRepresentation::ImagePreview;
            }
            ResolvedRepresentation::Original
        }
    }
}

async fn original_handle(
    state: &PrimaryAppState,
    download_path: String,
    file: &file::Model,
    blob: &file_blob::Model,
    delivery_mode: FileResourceDeliveryMode,
    scope: Option<&str>,
) -> Result<FileResourceHandle> {
    if let Some(presigned_url) = presigned_original_url(state, file, blob).await? {
        return Ok(FileResourceHandle {
            identity: FileResourceIdentity {
                cache_key: download_path,
                etag: Some(format!("\"{}\"", blob.hash)),
                scope: scope.map(str::to_string),
            },
            request: FileResourceRequestInfo {
                url: presigned_url,
                credentials: FileResourceCredentials::Omit,
                conditional_headers: FileResourceConditionalHeaders::Forbidden,
                redirect_policy: FileResourceRedirectPolicy::MayCrossOrigin,
            },
            delivery: FileResourceDeliveryInfo {
                mode: delivery_mode,
                mime_type: Some(file.mime_type.clone()),
            },
        });
    }

    Ok(FileResourceHandle {
        identity: FileResourceIdentity {
            cache_key: download_path.clone(),
            etag: Some(format!("\"{}\"", blob.hash)),
            scope: scope.map(str::to_string),
        },
        request: FileResourceRequestInfo {
            url: with_download_query(&download_path, "inline"),
            credentials: FileResourceCredentials::Include,
            conditional_headers: FileResourceConditionalHeaders::Allowed,
            redirect_policy: FileResourceRedirectPolicy::SameOriginOnly,
        },
        delivery: FileResourceDeliveryInfo {
            mode: delivery_mode,
            mime_type: Some(file.mime_type.clone()),
        },
    })
}

async fn presigned_original_url(
    state: &PrimaryAppState,
    file: &file::Model,
    blob: &file_blob::Model,
) -> Result<Option<String>> {
    if requires_inline_sandbox(&file.mime_type) {
        return Ok(None);
    }

    let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
    if !crate::storage::connectors::presigned_download_enabled(&policy)? {
        return Ok(None);
    }

    let driver = state.driver_registry().get_driver(&policy)?;
    let Some(presigned) = driver.as_presigned() else {
        return Ok(None);
    };

    presigned
        .presigned_url(
            &blob.storage_path,
            Duration::from_secs(PRESIGNED_PREVIEW_TTL_SECS),
            PresignedDownloadOptions {
                response_cache_control: Some("private, max-age=0, must-revalidate".to_string()),
                response_content_disposition: Some(
                    DownloadDisposition::Inline.header_value(&file.name),
                ),
                response_content_type: Some(file.mime_type.clone()),
            },
        )
        .await
}

fn image_preview_handle(
    state: &PrimaryAppState,
    image_preview_path: String,
    file: &file::Model,
    blob: &file_blob::Model,
    delivery_mode: FileResourceDeliveryMode,
    scope: Option<&str>,
) -> Result<FileResourceHandle> {
    let processor = processing::resolve_thumbnail_processor_for_blob(
        state,
        blob,
        &file.name,
        &file.mime_type,
    )
    .map_err(processing::map_thumbnail_request_error)?;
    let processor_name = processor.image_preview_processor();
    let version = processor.image_preview_version(state.runtime_config());
    Ok(derived_image_handle(
        image_preview_path,
        processing::image_preview_etag_value_for(
            &blob.hash,
            processor_name,
            &version,
        ),
        delivery_mode,
        scope,
    ))
}

fn thumbnail_handle(
    state: &PrimaryAppState,
    thumbnail_path: String,
    file: &file::Model,
    blob: &file_blob::Model,
    delivery_mode: FileResourceDeliveryMode,
    scope: Option<&str>,
) -> Result<FileResourceHandle> {
    let processor = processing::resolve_thumbnail_processor_for_blob(
        state,
        blob,
        &file.name,
        &file.mime_type,
    )
    .map_err(processing::map_thumbnail_request_error)?;
    let processor_name = processor.thumbnail_processor();
    let version = processor.thumbnail_version(state.runtime_config());
    Ok(derived_image_handle(
        thumbnail_path,
        processing::thumbnail_etag_value_for(
            &blob.hash,
            Some(processor_name),
            Some(&version),
        ),
        delivery_mode,
        scope,
    ))
}

fn derived_image_handle(
    path: String,
    etag_value: String,
    delivery_mode: FileResourceDeliveryMode,
    scope: Option<&str>,
) -> FileResourceHandle {
    FileResourceHandle {
        identity: FileResourceIdentity {
            cache_key: path.clone(),
            etag: Some(format!("\"{etag_value}\"")),
            scope: scope.map(str::to_string),
        },
        request: FileResourceRequestInfo {
            url: path,
            credentials: FileResourceCredentials::Include,
            conditional_headers: FileResourceConditionalHeaders::Allowed,
            redirect_policy: FileResourceRedirectPolicy::SameOriginOnly,
        },
        delivery: FileResourceDeliveryInfo {
            mode: delivery_mode,
            mime_type: Some("image/webp".to_string()),
        },
    }
}

fn with_download_query(path: &str, disposition: &str) -> String {
    let hash_index = path.find('#');
    let (base, hash) = match hash_index {
        Some(index) => (&path[..index], &path[index..]),
        None => (path, ""),
    };
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}disposition={disposition}{hash}")
}

fn is_image_mime(mime_type: &str) -> bool {
    mime_type.trim().to_ascii_lowercase().starts_with("image/")
}

fn file_extension(file_name: &str) -> &str {
    let Some((_, extension)) = file_name.rsplit_once('.') else {
        return "";
    };
    extension
}

fn can_browser_render_image(file: &file::Model) -> bool {
    let extension = file_extension(&file.name).to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "3fr"
            | "arw"
            | "cr2"
            | "cr3"
            | "dng"
            | "erf"
            | "heic"
            | "heif"
            | "j2k"
            | "jpf"
            | "jp2"
            | "jpx"
            | "jxl"
            | "kdc"
            | "mrw"
            | "nef"
            | "nrw"
            | "orf"
            | "pef"
            | "raf"
            | "raw"
            | "rw2"
            | "srw"
            | "tif"
            | "tiff"
            | "x3f"
    ) {
        return false;
    }

    let browser_renderable_extension = matches!(
        extension.as_str(),
        "avif"
            | "bmp"
            | "dib"
            | "gif"
            | "ico"
            | "jpe"
            | "jpeg"
            | "jfif"
            | "jpg"
            | "png"
            | "svg"
            | "webp"
    );
    let normalized_mime_type = file.mime_type.trim().to_ascii_lowercase();
    let browser_renderable_mime_type = matches!(
        normalized_mime_type.as_str(),
        "image/avif"
            | "image/bmp"
            | "image/gif"
            | "image/jpg"
            | "image/jpeg"
            | "image/pjpeg"
            | "image/png"
            | "image/svg+xml"
            | "image/vnd.microsoft.icon"
            | "image/webp"
            | "image/x-icon"
            | "image/x-ms-bmp"
            | "image/x-png"
    );

    if normalized_mime_type.starts_with("image/") && !browser_renderable_mime_type {
        return false;
    }

    browser_renderable_extension || browser_renderable_mime_type
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};
    use tokio::io::AsyncRead;

    use crate::api::dto::files::{
        FileResourceConditionalHeaders, FileResourceCredentials, FileResourceDeliveryMode,
        FileResourceHandleRequest, FileResourcePurpose, FileResourceRedirectPolicy,
        FileResourceRepresentation,
    };
    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::db::repository::file_repo;
    use crate::entities::{file, file_blob, storage_policy, user};
    use crate::runtime::PrimaryAppState;
    use crate::services::{mail::sender, media::processing, policy_service};
    use crate::storage::traits::driver::PresignedDownloadOptions;
    use crate::storage::traits::extensions::PresignedStorageDriver;
    use crate::storage::{BlobMetadata, DriverRegistry, PolicySnapshot, StorageDriver};
    use crate::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole,
        UserStatus,
    };

    use super::{
        FileResourcePathSet, can_browser_render_image, resolve_file_resource_handle_for_file,
        with_download_query,
    };

    #[derive(Clone)]
    struct TestDriver;

    #[async_trait]
    impl StorageDriver for TestDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }
    }

    #[derive(Clone)]
    struct PresignedTestDriver;

    #[async_trait]
    impl StorageDriver for PresignedTestDriver {
        async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
            Ok(path.to_string())
        }

        async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
            Ok(Box::new(tokio::io::empty()))
        }

        async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: 0,
                content_type: None,
            })
        }

        fn as_presigned(&self) -> Option<&dyn PresignedStorageDriver> {
            Some(self)
        }
    }

    #[async_trait]
    impl PresignedStorageDriver for PresignedTestDriver {
        async fn presigned_url(
            &self,
            path: &str,
            expires: Duration,
            options: PresignedDownloadOptions,
        ) -> crate::errors::Result<Option<String>> {
            let mut url = reqwest::Url::parse("https://objects.example.test/download")
                .expect("test presigned URL base should parse");
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("path", path);
                query.append_pair("expires", &expires.as_secs().to_string());
                if let Some(value) = options.response_cache_control {
                    query.append_pair("response-cache-control", &value);
                }
                if let Some(value) = options.response_content_disposition {
                    query.append_pair("response-content-disposition", &value);
                }
                if let Some(value) = options.response_content_type {
                    query.append_pair("response-content-type", &value);
                }
            }
            Ok(Some(url.to_string()))
        }

        async fn presigned_put_url(
            &self,
            path: &str,
            _expires: Duration,
        ) -> crate::errors::Result<Option<String>> {
            Ok(Some(format!(
                "https://objects.example.test/upload?path={path}"
            )))
        }
    }

    async fn build_resource_handle_state<D>(
        driver: D,
        driver_type: DriverType,
        options: StoredStoragePolicyOptions,
        file_name: &str,
        mime_type: &str,
    ) -> (PrimaryAppState, file::Model, file_blob::Model)
    where
        D: StorageDriver + Clone + 'static,
    {
        let temp_root = std::env::temp_dir().join(format!(
            "asterdrive-resource-handle-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_root).expect("resource handle temp root should exist");

        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("resource handle database should connect");
        Migrator::up(&db, None)
            .await
            .expect("resource handle migrations should succeed");

        let now = Utc::now();
        let policy = storage_policy::ActiveModel {
            name: Set("Resource Handle Policy".to_string()),
            driver_type: Set(driver_type),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(temp_root.to_string_lossy().into_owned()),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(options),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("resource handle policy should be inserted");

        let test_user = user::ActiveModel {
            username: Set("resource-handle".to_string()),
            email: Set("resource-handle@example.com".to_string()),
            password_hash: Set("unused".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(0),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("resource handle user should be inserted");

        policy_service::ensure_policy_groups_seeded(&db)
            .await
            .expect("resource handle policy groups should be seeded");

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("resource handle policy snapshot should reload");

        let driver_registry = Arc::new(DriverRegistry::noop());
        driver_registry.insert_for_test(policy.id, Arc::new(driver));

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;

        let mut config = Config::default();
        config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
        config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        let state = PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db.clone()),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(config),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        };

        let blob = file_repo::create_blob(
            &db,
            file_blob::ActiveModel {
                hash: Set("resource-handle-hash".to_string()),
                size: Set(123),
                policy_id: Set(policy.id),
                storage_path: Set("objects/resource.bin".to_string()),
                thumbnail_path: Set(None),
                thumbnail_processor: Set(None),
                thumbnail_version: Set(None),
                ref_count: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("resource handle blob should be inserted");

        let file = file_repo::create(
            &db,
            file::ActiveModel {
                name: Set(file_name.to_string()),
                folder_id: Set(None),
                team_id: Set(None),
                blob_id: Set(blob.id),
                size: Set(blob.size),
                owner_user_id: Set(Some(test_user.id)),
                created_by_user_id: Set(Some(test_user.id)),
                created_by_username: Set(test_user.username),
                mime_type: Set(mime_type.to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("resource handle file should be inserted");

        (state, file, blob)
    }

    fn paths() -> FileResourcePathSet {
        FileResourcePathSet {
            download: "/files/42/download?existing=1#frag".to_string(),
            image_preview: "/files/42/image-preview".to_string(),
            thumbnail: "/files/42/thumbnail".to_string(),
        }
    }

    fn request(
        purpose: FileResourcePurpose,
        delivery_mode: FileResourceDeliveryMode,
        representation: FileResourceRepresentation,
    ) -> FileResourceHandleRequest {
        FileResourceHandleRequest {
            purpose,
            delivery_mode,
            representation,
        }
    }

    fn query_pairs(url: &str) -> std::collections::HashMap<String, String> {
        reqwest::Url::parse(url)
            .expect("URL should parse")
            .query_pairs()
            .into_owned()
            .collect()
    }

    #[test]
    fn with_download_query_preserves_existing_query_and_fragment() {
        assert_eq!(
            with_download_query("/files/1/download", "inline"),
            "/files/1/download?disposition=inline"
        );
        assert_eq!(
            with_download_query("/files/1/download?existing=1", "inline"),
            "/files/1/download?existing=1&disposition=inline"
        );
        assert_eq!(
            with_download_query("/files/1/download#frag", "inline"),
            "/files/1/download?disposition=inline#frag"
        );
        assert_eq!(
            with_download_query("/files/1/download?existing=1#frag", "inline"),
            "/files/1/download?existing=1&disposition=inline#frag"
        );
    }

    #[test]
    fn browser_renderable_image_detection_handles_extensions_and_mime_edges() {
        let mut image = file::Model {
            id: 1,
            name: "photo.JPG".to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 1,
            size: 1,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "application/octet-stream".to_string(),
            extension: "jpg".to_string(),
            compound_extension: None,
            file_category: crate::types::FileCategory::Image,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            is_locked: false,
        };

        assert!(can_browser_render_image(&image));

        image.name = "capture.HEIC".to_string();
        image.mime_type = "image/jpeg".to_string();
        image.extension = "heic".to_string();
        assert!(!can_browser_render_image(&image));

        image.name = "camera.raw".to_string();
        image.mime_type = "image/png".to_string();
        image.extension = "raw".to_string();
        assert!(!can_browser_render_image(&image));

        image.name = "no-extension".to_string();
        image.mime_type = " image/webp ".to_string();
        image.extension = String::new();
        assert!(can_browser_render_image(&image));

        image.mime_type = "image/heic".to_string();
        assert!(!can_browser_render_image(&image));

        image.name = "photo.jpg".to_string();
        image.mime_type = "image/heic".to_string();
        image.extension = "jpg".to_string();
        assert!(!can_browser_render_image(&image));
    }

    #[actix_web::test]
    async fn original_handle_uses_same_origin_when_presigned_download_is_disabled() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "report.txt",
            "text/plain",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Original,
            ),
            Some("personal"),
        )
        .await
        .expect("same-origin original handle should resolve");

        assert_eq!(
            handle.identity.cache_key,
            "/files/42/download?existing=1#frag"
        );
        assert_eq!(
            handle.identity.etag.as_deref(),
            Some("\"resource-handle-hash\"")
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("personal"));
        assert_eq!(
            handle.request.url,
            "/files/42/download?existing=1&disposition=inline#frag"
        );
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
        assert_eq!(handle.delivery.mode, FileResourceDeliveryMode::BlobUrl);
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("text/plain"));
    }

    #[actix_web::test]
    async fn original_handle_uses_presigned_url_without_credentials_or_conditional_headers() {
        let (state, file, blob) = build_resource_handle_state(
            PresignedTestDriver,
            DriverType::S3,
            StoredStoragePolicyOptions::from(
                r#"{"object_storage_download_strategy":"presigned"}"#.to_string(),
            ),
            "space name.png",
            "image/png",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::DirectUrl,
                FileResourceRepresentation::Original,
            ),
            Some("team"),
        )
        .await
        .expect("presigned original handle should resolve");

        assert_eq!(
            handle.identity.cache_key,
            "/files/42/download?existing=1#frag"
        );
        assert_eq!(
            handle.identity.etag.as_deref(),
            Some("\"resource-handle-hash\"")
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("team"));
        assert_eq!(handle.request.credentials, FileResourceCredentials::Omit);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Forbidden
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::MayCrossOrigin
        );
        assert_eq!(handle.delivery.mode, FileResourceDeliveryMode::DirectUrl);
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/png"));

        let parsed =
            reqwest::Url::parse(&handle.request.url).expect("presigned resource URL should parse");
        assert_eq!(parsed.scheme(), "https");
        let query = query_pairs(&handle.request.url);
        assert_eq!(
            query.get("path").map(String::as_str),
            Some("objects/resource.bin")
        );
        assert_eq!(query.get("expires").map(String::as_str), Some("300"));
        assert_eq!(
            query.get("response-cache-control").map(String::as_str),
            Some("private, max-age=0, must-revalidate")
        );
        assert_eq!(
            query
                .get("response-content-disposition")
                .map(String::as_str),
            Some("inline; filename*=UTF-8''space%20name.png")
        );
        assert_eq!(
            query.get("response-content-type").map(String::as_str),
            Some("image/png")
        );
    }

    #[actix_web::test]
    async fn sandboxed_original_handle_does_not_use_presigned_url() {
        let (state, file, blob) = build_resource_handle_state(
            PresignedTestDriver,
            DriverType::S3,
            StoredStoragePolicyOptions::from(
                r#"{"object_storage_download_strategy":"presigned"}"#.to_string(),
            ),
            "preview.html",
            "text/html; charset=utf-8",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Original,
            ),
            None,
        )
        .await
        .expect("sandboxed original handle should resolve");

        assert_eq!(
            handle.request.url,
            "/files/42/download?existing=1&disposition=inline#frag"
        );
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
    }

    #[actix_web::test]
    async fn auto_preview_for_non_browser_renderable_image_uses_image_preview_representation() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "scan.tiff",
            "image/tiff",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Auto,
            ),
            Some("personal"),
        )
        .await
        .expect("auto image preview handle should resolve");

        let expected_etag = processing::image_preview_etag_value_for(
            &blob.hash,
            crate::services::files::thumbnail::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
            crate::services::files::thumbnail::CURRENT_IMAGE_PREVIEW_VERSION,
        );
        assert_eq!(handle.identity.cache_key, "/files/42/image-preview");
        assert_eq!(
            handle.identity.etag.as_deref(),
            Some(format!("\"{expected_etag}\"").as_str())
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("personal"));
        assert_eq!(handle.request.url, "/files/42/image-preview");
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
        assert_eq!(handle.delivery.mode, FileResourceDeliveryMode::BlobUrl);
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/webp"));
    }

    #[actix_web::test]
    async fn auto_preview_keeps_browser_renderable_images_on_original_representation() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "photo.jpg",
            "image/jpeg",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Auto,
            ),
            Some("personal"),
        )
        .await
        .expect("auto original image handle should resolve");

        assert_eq!(
            handle.identity.cache_key,
            "/files/42/download?existing=1#frag"
        );
        assert_eq!(
            handle.request.url,
            "/files/42/download?existing=1&disposition=inline#frag"
        );
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/jpeg"));
    }

    #[actix_web::test]
    async fn auto_preview_only_converts_for_blob_url_preview_requests() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "capture.heic",
            "image/heic",
        )
        .await;

        for (purpose, delivery_mode) in [
            (
                FileResourcePurpose::Download,
                FileResourceDeliveryMode::BlobUrl,
            ),
            (
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::DirectUrl,
            ),
            (
                FileResourcePurpose::ExternalViewer,
                FileResourceDeliveryMode::BlobUrl,
            ),
        ] {
            let handle = resolve_file_resource_handle_for_file(
                &state,
                &file,
                &blob,
                paths(),
                &request(purpose, delivery_mode, FileResourceRepresentation::Auto),
                Some("personal"),
            )
            .await
            .expect("non preview blob auto request should keep original");

            assert_eq!(
                handle.identity.cache_key,
                "/files/42/download?existing=1#frag"
            );
            assert_eq!(
                handle.request.url,
                "/files/42/download?existing=1&disposition=inline#frag"
            );
            assert_eq!(handle.delivery.mode, delivery_mode);
            assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/heic"));
        }
    }

    #[actix_web::test]
    async fn explicit_thumbnail_representation_uses_thumbnail_identity_and_webp_delivery() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "photo.png",
            "image/png",
        )
        .await;

        let handle = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Thumbnail,
            ),
            Some("team"),
        )
        .await
        .expect("thumbnail handle should resolve");

        let expected_etag = processing::thumbnail_etag_value_for(
            &blob.hash,
            Some(crate::services::files::thumbnail::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE),
            Some(crate::services::files::thumbnail::CURRENT_THUMBNAIL_VERSION),
        );
        assert_eq!(handle.identity.cache_key, "/files/42/thumbnail");
        assert_eq!(
            handle.identity.etag.as_deref(),
            Some(format!("\"{expected_etag}\"").as_str())
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("team"));
        assert_eq!(handle.request.url, "/files/42/thumbnail");
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/webp"));
    }

    #[actix_web::test]
    async fn derived_image_representations_return_validation_error_when_no_processor_matches() {
        let (state, file, blob) = build_resource_handle_state(
            TestDriver,
            DriverType::Local,
            StoredStoragePolicyOptions::empty(),
            "notes.txt",
            "text/plain",
        )
        .await;

        let error = resolve_file_resource_handle_for_file(
            &state,
            &file,
            &blob,
            paths(),
            &request(
                FileResourcePurpose::Preview,
                FileResourceDeliveryMode::BlobUrl,
                FileResourceRepresentation::Thumbnail,
            ),
            Some("personal"),
        )
        .await
        .expect_err("text thumbnail handle should fail validation");

        assert!(matches!(
            error,
            crate::errors::AsterError::ValidationError(_)
        ));
        assert!(
            error
                .message()
                .contains("no enabled thumbnail processor matched"),
            "unexpected error: {error}"
        );
    }
}
