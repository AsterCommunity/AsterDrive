use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::tag::{Accessor, Tag, TagExt, TagType};
use sea_orm::{ActiveModelTrait, Set};
use tokio::io::AsyncRead;

use super::audio::parse_audio_metadata_from_reader;
use super::image::parse_image_metadata_with_reader_factory;
use super::source::PreparedRangeMediaMetadataSource;
use super::*;
use crate::storage::StorageDriver;

struct RangeOnlyDriver {
    data: Vec<u8>,
    range_calls: AtomicUsize,
    range_bytes_requested: AtomicUsize,
    stream_calls: AtomicUsize,
}

impl RangeOnlyDriver {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            range_calls: AtomicUsize::new(0),
            range_bytes_requested: AtomicUsize::new(0),
            stream_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl StorageDriver for RangeOnlyDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.stream_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(Cursor::new(self.data.clone())))
    }

    async fn get_range(
        &self,
        _path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.range_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(length) = length {
            let length = crate::utils::numbers::u64_to_usize(length, "range test length")?;
            self.range_bytes_requested
                .fetch_add(length, Ordering::SeqCst);
        }
        let start = crate::utils::numbers::u64_to_usize(offset, "range test start")?;
        let end = length
            .map(|length| {
                offset
                    .checked_add(length)
                    .ok_or_else(|| AsterError::internal_error("range test end offset overflow"))
            })
            .transpose()?
            .map(|end| crate::utils::numbers::u64_to_usize(end, "range test end"))
            .transpose()?
            .unwrap_or(self.data.len())
            .min(self.data.len());
        let bytes = if start >= self.data.len() {
            Vec::new()
        } else {
            self.data[start..end].to_vec()
        };
        Ok(Box::new(Cursor::new(bytes)))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> Result<crate::storage::BlobMetadata> {
        Ok(crate::storage::BlobMetadata {
            size: crate::utils::numbers::usize_to_u64(self.data.len(), "range test data")?,
            content_type: None,
        })
    }
}

fn test_blob(size: usize) -> file_blob::Model {
    file_blob::Model {
        id: 1,
        hash: "hash".to_string(),
        size: i64::try_from(size).expect("test size should fit i64"),
        policy_id: 1,
        storage_path: "blob".to_string(),
        thumbnail_path: None,
        thumbnail_processor: None,
        thumbnail_version: None,
        ref_count: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn tiny_png_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = ::image::codecs::png::PngEncoder::new(&mut bytes);
    ::image::ImageEncoder::write_image(
        encoder,
        &[255, 0, 0],
        1,
        1,
        ::image::ExtendedColorType::Rgb8,
    )
    .expect("tiny PNG should encode");
    bytes
}

fn tiff_like_raw_with_large_tail() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"II");
    bytes.extend_from_slice(&42_u16.to_le_bytes());
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&0x0100_u16.to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&6016_u32.to_le_bytes());
    bytes.extend_from_slice(&0x0101_u16.to_le_bytes());
    bytes.extend_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&4016_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.resize(16 * 1024 * 1024, 0);
    bytes
}

fn wav_with_id3v2_embedded_picture() -> Vec<u8> {
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_artist("Aster Tester".to_string());
    tag.push_picture(
        Picture::unchecked(vec![0; 128])
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build(),
    );

    let mut id3_bytes = Vec::new();
    tag.dump_to(&mut id3_bytes, WriteOptions::default())
        .expect("ID3v2 tag should encode");
    let mut riff_body = Vec::new();
    riff_body.extend_from_slice(b"WAVE");
    push_wav_chunk(
        &mut riff_body,
        b"fmt ",
        &[
            1, 0, // PCM
            1, 0, // channels
            0x40, 0x1f, 0, 0, // 8000 Hz
            0x40, 0x1f, 0, 0, // byte rate
            1, 0, // block align
            8, 0, // bits per sample
        ],
    );
    push_wav_chunk(&mut riff_body, b"data", &[0]);
    push_wav_chunk(&mut riff_body, b"ID3 ", &id3_bytes);

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(
        &u32::try_from(riff_body.len())
            .expect("test WAV should fit u32")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&riff_body);
    bytes
}

fn push_wav_chunk(bytes: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(fourcc);
    bytes.extend_from_slice(
        &u32::try_from(payload.len())
            .expect("test WAV chunk should fit u32")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        bytes.push(0);
    }
}

#[tokio::test]
async fn range_media_metadata_source_uses_get_range_without_streaming_whole_blob() {
    let bytes = tiny_png_bytes();
    let blob = test_blob(bytes.len());
    let driver = Arc::new(RangeOnlyDriver::new(bytes));
    let source = PreparedRangeMediaMetadataSource::new(
        driver.clone(),
        &blob,
        "pixel.png",
        "image/png",
        tokio::runtime::Handle::current(),
    )
    .expect("range source should build");

    let metadata = tokio::task::spawn_blocking(move || {
        parse_image_metadata_with_reader_factory("storage range", || Ok(source.reader()))
    })
    .await
    .expect("range metadata task should not panic")
    .expect("range metadata should parse");

    assert_eq!(metadata.width, 1);
    assert_eq!(metadata.height, 1);
    assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
    assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn range_tiff_fallback_uses_seekable_ranges_without_streaming_whole_blob() {
    let bytes = tiff_like_raw_with_large_tail();
    let blob = test_blob(bytes.len());
    let driver = Arc::new(RangeOnlyDriver::new(bytes));
    let source = PreparedRangeMediaMetadataSource::new(
        driver.clone(),
        &blob,
        "large-tail.nef",
        "image/x-nikon-nef",
        tokio::runtime::Handle::current(),
    )
    .expect("range source should build");

    let metadata = tokio::task::spawn_blocking(move || {
        parse_image_metadata_with_reader_factory("storage range", || Ok(source.reader()))
    })
    .await
    .expect("range metadata task should not panic")
    .expect("range TIFF metadata should parse");

    assert_eq!(metadata.width, 6016);
    assert_eq!(metadata.height, 4016);
    assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
    assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
    assert!(driver.range_bytes_requested.load(Ordering::SeqCst) < 2 * 1024 * 1024);
}

#[tokio::test]
async fn image_extract_uses_range_source_for_efficient_remote_driver() {
    let bytes = tiny_png_bytes();
    let driver = Arc::new(RangeOnlyDriver::new(bytes.clone()));
    let state = test_state_with_driver(driver.clone()).await;
    let blob = file_blob::Model {
        policy_id: 1,
        ..test_blob(bytes.len())
    };

    let extracted = extract_for_blob(
        &state,
        &blob,
        "pixel.png",
        "image/png",
        MediaMetadataKind::Image,
    )
    .await
    .expect("image metadata should extract");

    assert_eq!(extracted.status, MediaMetadataStatus::Ready);
    match extracted.metadata {
        Some(MediaMetadataPayload::Image(metadata)) => {
            assert_eq!(metadata.width, 1);
            assert_eq!(metadata.height, 1);
        }
        other => panic!("expected image metadata, got {other:?}"),
    }
    assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
    assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn audio_metadata_does_not_read_embedded_cover_art() {
    let metadata = parse_audio_metadata_from_reader(
        Cursor::new(wav_with_id3v2_embedded_picture()),
        Some(lofty::file::FileType::Wav),
    )
    .expect("ID3v2 tag should parse");

    assert_eq!(metadata.artist.as_deref(), Some("Aster Tester"));
    assert!(!metadata.has_embedded_picture);
    assert!(metadata.embedded_picture_mime_type.is_none());
}

async fn test_state_with_driver(driver: Arc<dyn StorageDriver>) -> PrimaryAppState {
    let db = crate::db::connect_with_metrics(
        &crate::config::DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .expect("test database should connect");
    migration::Migrator::up(&db, None)
        .await
        .expect("test migrations should run");

    let now = Utc::now();
    let policy = crate::entities::storage_policy::ActiveModel {
        id: Set(1),
        name: Set("Range metadata policy".to_string()),
        driver_type: Set(crate::types::DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(String::new()),
        remote_node_id: Set(None),
        max_file_size: Set(0),
        allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
        options: Set(crate::types::StoredStoragePolicyOptions::empty()),
        is_default: Set(true),
        chunk_size: Set(5_242_880),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(&db)
    .await
    .expect("test policy should insert");

    let policy_snapshot = Arc::new(crate::storage::PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("policy snapshot should reload");
    let driver_registry = Arc::new(crate::storage::DriverRegistry::noop());
    driver_registry.insert_for_test(policy.id, driver);
    let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
    let cache = crate::cache::create_cache(&crate::config::CacheConfig {
        enabled: false,
        ..Default::default()
    })
    .await;
    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share_service::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    PrimaryAppState {
        db_handles: crate::db::DbHandles::single(db),
        driver_registry,
        runtime_config,
        policy_snapshot,
        config: Arc::new(crate::config::Config::default()),
        cache,
        metrics: crate::metrics_core::NoopMetrics::arc(),
        mail_sender: crate::services::mail_service::memory_sender(),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
    }
}
