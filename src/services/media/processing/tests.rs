use crate::config::definitions::CONFIG_CATEGORY_FILE_PROCESSING_MEDIA;
use crate::config::media_processing::command_is_available;
use crate::config::{RuntimeConfig, media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY};
use crate::types::MediaProcessorKind;
use actix_web::ResponseError;
use aster_forge_config::{ConfigSource, ConfigValueType};
use aster_forge_db::system_config;
use chrono::Utc;
use image::{GenericImageView, ImageFormat, Rgb, RgbImage};
use std::io::Cursor;

use super::avatar::generate_avatar_variants;
use super::resolve::resolve_avatar_processor;
use super::shared::{known_image_preview_cache_paths, known_thumbnail_cache_paths};

fn config_model(key: &str, value: &str) -> system_config::Model {
    system_config::Model {
        id: 0,
        key: key.to_string(),
        value: value.to_string(),
        value_type: ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA.to_string(),
        description: "test".to_string(),
        updated_at: Utc::now(),
        updated_by: None,
    }
}

fn sample_avatar_png(width: u32, height: u32) -> Vec<u8> {
    let image =
        image::DynamicImage::ImageRgb8(RgbImage::from_pixel(width, height, Rgb([255, 0, 0])));
    let mut buf = Cursor::new(Vec::new());
    image.write_to(&mut buf, ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn known_thumbnail_cache_paths_include_normalized_namespaces() {
    let hash = "abc".repeat(21) + "a";
    let paths = known_thumbnail_cache_paths(
        &hash,
        crate::config::operations::DEFAULT_THUMBNAIL_MAX_DIMENSION,
    );
    assert_eq!(
        paths,
        vec![
            format!("_thumb/images/1/ab/ca/{hash}.webp"),
            format!("_thumb/vips-cli/1/ab/ca/{hash}.webp"),
            format!("_thumb/ffmpeg-cli/1/ab/ca/{hash}.webp"),
            format!("_thumb/lofty/1/ab/ca/{hash}.webp"),
            format!("_thumb/storage-native/1/ab/ca/{hash}.webp"),
        ]
    );
}

#[test]
fn known_image_preview_cache_paths_include_normalized_namespaces() {
    let hash = "abc".repeat(21) + "a";
    let paths = known_image_preview_cache_paths(
        &hash,
        crate::config::operations::DEFAULT_IMAGE_PREVIEW_MAX_DIMENSION,
    );
    assert_eq!(
        paths,
        vec![
            format!("_preview/images/1/ab/ca/{hash}.webp"),
            format!("_preview/vips-cli/1/ab/ca/{hash}.webp"),
            format!("_preview/ffmpeg-cli/1/ab/ca/{hash}.webp"),
            format!("_preview/storage-native/1/ab/ca/{hash}.webp"),
        ]
    );
}

#[test]
fn command_is_available_rejects_blank_command() {
    assert!(!command_is_available(""));
    assert!(!command_is_available("   "));
}

#[test]
fn generate_avatar_variants_generates_expected_webp_variants() {
    let processed = generate_avatar_variants(sample_avatar_png(8, 4)).unwrap();
    let small = image::load_from_memory(&processed.small_bytes).unwrap();
    let large = image::load_from_memory(&processed.large_bytes).unwrap();

    assert_eq!(small.dimensions(), (512, 512));
    assert_eq!(large.dimensions(), (1024, 1024));
}

#[test]
fn generate_avatar_variants_rejects_invalid_image_bytes() {
    let error = generate_avatar_variants(b"not-an-image".to_vec()).unwrap_err();
    assert_eq!(error.status_code().as_u16(), 400);
}

#[test]
fn resolve_avatar_processor_uses_images_by_default() {
    let runtime_config = RuntimeConfig::new();
    let processor = resolve_avatar_processor(&runtime_config, "avatar.png").unwrap();
    assert_eq!(processor.kind(), MediaProcessorKind::Images);
}

#[test]
fn resolve_avatar_processor_uses_vips_when_enabled_and_extension_matches() {
    let runtime_config = RuntimeConfig::new();
    let command = available_test_command();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        &serde_json::json!({
            "version": 1,
            "processors": [
                {
                    "kind": "vips_cli",
                    "enabled": true,
                    "extensions": ["heic"],
                    "config": {
                        "command": command,
                    },
                },
                {
                    "kind": "images",
                    "enabled": true,
                },
            ],
        })
        .to_string(),
    ));
    let processor = resolve_avatar_processor(&runtime_config, "avatar.heic").unwrap();
    assert_eq!(processor.kind(), MediaProcessorKind::VipsCli);
}

#[test]
fn resolve_avatar_processor_falls_back_to_images_when_vips_command_is_unavailable() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        r#"{
                "version": 1,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true,
                        "extensions": ["png"],
                        "config": {
                            "command": "definitely-missing-vips-cli"
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    ));
    let processor = resolve_avatar_processor(&runtime_config, "avatar.png").unwrap();
    assert_eq!(processor.kind(), MediaProcessorKind::Images);
}
