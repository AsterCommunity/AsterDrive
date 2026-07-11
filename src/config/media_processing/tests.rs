use crate::config::RuntimeConfig;
use crate::config::definitions::CONFIG_CATEGORY_FILE_PROCESSING_MEDIA;
use crate::config::operations::{
    DEFAULT_MEDIA_METADATA_MAX_SOURCE_BYTES, MEDIA_METADATA_ENABLED_KEY,
    MEDIA_METADATA_MAX_SOURCE_BYTES_KEY,
};
use crate::types::{ConfigSource, ConfigValueType, MediaProcessorKind};
use aster_forge_db::system_config;
use chrono::Utc;

use super::{
    BUILTIN_AUDIO_METADATA_EXTENSIONS, BUILTIN_IMAGE_METADATA_EXTENSIONS,
    BUILTIN_IMAGES_SUPPORTED_EXTENSIONS, DEFAULT_FFMPEG_COMMAND, DEFAULT_FFMPEG_EXTENSIONS,
    DEFAULT_FFPROBE_COMMAND, DEFAULT_FFPROBE_EXTENSIONS, DEFAULT_VIPS_COMMAND,
    DEFAULT_VIPS_EXTENSIONS, MEDIA_PROCESSING_REGISTRY_JSON_KEY, MEDIA_PROCESSING_REGISTRY_VERSION,
    MatchedMediaProcessor, MediaProcessingMatchKind, MediaProcessingProcessorConfig,
    MediaProcessingProcessorRuntimeConfig, MediaProcessingRegistryConfig, MediaProcessingUse,
    PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES, PUBLIC_MEDIA_DATA_SUPPORT_VERSION,
    PublicExtensionSupport, PublicMediaDataKindSupport, PublicMediaDataSupport,
    PublicMediaDataSupportMatch, PublicThumbnailSupport, builtin_audio_metadata_supports_extension,
    builtin_image_metadata_supports_extension, command_is_available,
    default_media_processing_registry, default_media_processing_registry_json,
    default_uses_for_kind, ffmpeg_command_from_registry_value, ffprobe_command_from_registry_value,
    file_extension, media_processing_registry, normalize_ffmpeg_command, normalize_ffprobe_command,
    normalize_media_processing_registry_config_value, normalize_vips_command,
    parse_media_processor_kind, processor_candidates_for_file_name, processor_candidates_for_use,
    processor_config_for_kind, public_media_data_support, public_thumbnail_support,
    vips_command_from_registry_value,
};

fn config_model(key: &str, value: &str) -> system_config::Model {
    system_config::Model {
        id: 0,
        key: key.to_string(),
        value: value.to_string(),
        value_type: ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: ConfigSource::System,
        visibility: crate::types::ConfigVisibility::Private,
        namespace: String::new(),
        category: CONFIG_CATEGORY_FILE_PROCESSING_MEDIA.to_string(),
        description: "test".to_string(),
        updated_at: Utc::now(),
        updated_by: None,
    }
}

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
}

fn default_media_metadata_max_source_bytes() -> i64 {
    i64::try_from(DEFAULT_MEDIA_METADATA_MAX_SOURCE_BYTES)
        .expect("default media metadata max source bytes should fit i64")
}

#[test]
fn parse_media_processor_kind_understands_known_values() {
    assert_eq!(
        parse_media_processor_kind(" images "),
        Some(MediaProcessorKind::Images)
    );
    assert_eq!(
        parse_media_processor_kind("lofty"),
        Some(MediaProcessorKind::Lofty)
    );
    assert_eq!(
        parse_media_processor_kind("vips_cli"),
        Some(MediaProcessorKind::VipsCli)
    );
    assert_eq!(
        parse_media_processor_kind("ffmpeg_cli"),
        Some(MediaProcessorKind::FfmpegCli)
    );
    assert_eq!(
        parse_media_processor_kind("ffprobe_cli"),
        Some(MediaProcessorKind::FfprobeCli)
    );
    assert_eq!(
        parse_media_processor_kind("storage_native"),
        Some(MediaProcessorKind::StorageNative)
    );
    assert_eq!(parse_media_processor_kind("nope"), None);
}

#[test]
fn normalize_vips_command_trims_and_defaults() {
    assert_eq!(
        normalize_vips_command("  /usr/bin/vips  ").unwrap(),
        "/usr/bin/vips"
    );
    assert_eq!(normalize_vips_command(" ").unwrap(), DEFAULT_VIPS_COMMAND);
}

#[test]
fn normalize_ffmpeg_command_trims_and_defaults() {
    assert_eq!(
        normalize_ffmpeg_command("  /usr/bin/ffmpeg  ").unwrap(),
        "/usr/bin/ffmpeg"
    );
    assert_eq!(
        normalize_ffmpeg_command(" ").unwrap(),
        DEFAULT_FFMPEG_COMMAND
    );
}

#[test]
fn normalize_ffprobe_command_trims_and_defaults() {
    assert_eq!(
        normalize_ffprobe_command("  /usr/bin/ffprobe  ").unwrap(),
        "/usr/bin/ffprobe"
    );
    assert_eq!(
        normalize_ffprobe_command(" ").unwrap(),
        DEFAULT_FFPROBE_COMMAND
    );
}

#[test]
fn builtin_images_supports_known_extensions() {
    let expected_extensions = [
        "apng", "bmp", "exr", "ff", "gif", "hdr", "ico", "jfif", "jpeg", "jpg", "pam", "pbm",
        "pgm", "png", "pnm", "ppm", "qoi", "tga", "tif", "tiff", "webp",
    ];
    assert_eq!(
        BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.as_slice(),
        expected_extensions
    );

    for extension in BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.iter() {
        assert!(super::builtin_images_supports_extension(extension));
    }
    for format in image::ImageFormat::all().filter(|format| format.reading_enabled()) {
        for extension in format.extensions_str() {
            assert!(
                BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.contains(extension),
                "image extension '{extension}' should be exposed"
            );
        }
    }
    assert!(BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.contains(&"apng"));
    assert!(BUILTIN_IMAGES_SUPPORTED_EXTENSIONS.contains(&"jfif"));
    assert!(!super::builtin_images_supports_extension("avif"));
    assert!(!image::ImageFormat::Avif.reading_enabled());
    assert!(!super::builtin_images_supports_extension("dds"));
    assert!(!super::builtin_images_supports_extension("heic"));
    assert!(!super::builtin_images_supports_extension("nef"));
    assert!(!super::builtin_images_supports_extension("pfm"));
}

#[test]
fn builtin_image_metadata_supports_nom_exif_image_extensions() {
    for extension in BUILTIN_IMAGE_METADATA_EXTENSIONS {
        assert!(builtin_image_metadata_supports_extension(extension));
    }
    assert!(builtin_image_metadata_supports_extension("heic"));
    assert!(builtin_image_metadata_supports_extension("cr3"));
    assert!(builtin_image_metadata_supports_extension("raf"));
    assert!(builtin_image_metadata_supports_extension("nef"));
    assert!(!super::builtin_images_supports_extension("nef"));
    assert!(!builtin_image_metadata_supports_extension("webp"));
}

#[test]
fn builtin_audio_metadata_supports_lofty_extensions() {
    for extension in BUILTIN_AUDIO_METADATA_EXTENSIONS.iter() {
        assert!(builtin_audio_metadata_supports_extension(extension));
        assert!(
            lofty::file::FileType::from_ext(extension).is_some(),
            "lofty should recognize extension '{extension}'"
        );
    }
    for extension in lofty::file::EXTENSIONS {
        assert!(
            BUILTIN_AUDIO_METADATA_EXTENSIONS.contains(extension),
            "lofty extension '{extension}' should be exposed"
        );
    }
    assert!(builtin_audio_metadata_supports_extension("mp2"));
    assert!(builtin_audio_metadata_supports_extension("aifc"));
    assert!(builtin_audio_metadata_supports_extension("spx"));
    assert!(builtin_audio_metadata_supports_extension("mpc"));
    assert!(builtin_audio_metadata_supports_extension("wave"));
    assert!(!builtin_audio_metadata_supports_extension("mka"));
    assert!(!builtin_audio_metadata_supports_extension("oga"));
}

#[test]
fn vips_command_from_registry_value_prefers_draft_command() {
    let command = vips_command_from_registry_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": false,
                        "config": {
                            "command": "  /usr/local/bin/vips  "
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    )
    .unwrap();

    assert_eq!(command, "/usr/local/bin/vips");
}

#[test]
fn ffmpeg_command_from_registry_value_prefers_draft_command() {
    let command = ffmpeg_command_from_registry_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "ffmpeg_cli",
                        "enabled": false,
                        "config": {
                            "command": "  /usr/local/bin/ffmpeg  "
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    )
    .unwrap();

    assert_eq!(command, "/usr/local/bin/ffmpeg");
}

#[test]
fn ffprobe_command_from_registry_value_prefers_draft_command() {
    let command = ffprobe_command_from_registry_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "ffprobe_cli",
                        "enabled": false,
                        "uses": ["metadata:video"],
                        "config": {
                            "command": "  /usr/local/bin/ffprobe  "
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    )
    .unwrap();

    assert_eq!(command, "/usr/local/bin/ffprobe");
}

#[test]
fn command_is_available_rejects_blank_command() {
    assert!(!command_is_available(""));
    assert!(!command_is_available("   "));
}

#[cfg(unix)]
#[test]
fn command_is_available_rejects_non_executable_files() {
    use std::os::unix::fs::PermissionsExt;

    let dir = std::env::temp_dir().join(format!(
        "aster-media-command-test-{}",
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let command = dir.join("plain-file");
    std::fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();

    let mut permissions = std::fs::metadata(&command).unwrap().permissions();
    permissions.set_mode(0o644);
    std::fs::set_permissions(&command, permissions).unwrap();

    assert!(!command_is_available(command.to_str().unwrap()));

    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn command_is_available_accepts_extensionless_windows_paths_matching_pathext() {
    let dir = std::env::temp_dir().join(format!(
        "aster-media-command-test-{}",
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let extensionless = dir.join("fake-tool");
    let executable = dir.join("fake-tool.exe");
    std::fs::write(&executable, b"").unwrap();

    assert!(command_is_available(extensionless.to_str().unwrap()));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn default_registry_includes_known_processors_in_fixed_order() {
    let config = default_media_processing_registry();
    assert_eq!(config.version, MEDIA_PROCESSING_REGISTRY_VERSION);
    assert_eq!(
        config.processors,
        vec![
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::VipsCli,
                enabled: false,
                uses: vec![MediaProcessingUse::ThumbnailImage],
                extensions: DEFAULT_VIPS_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_string())
                    .collect(),
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_VIPS_COMMAND.to_string()),
                },
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::FfmpegCli,
                enabled: false,
                uses: vec![MediaProcessingUse::ThumbnailVideo],
                extensions: DEFAULT_FFMPEG_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_string())
                    .collect(),
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_FFMPEG_COMMAND.to_string()),
                },
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::FfprobeCli,
                enabled: false,
                uses: vec![MediaProcessingUse::MetadataVideo],
                extensions: DEFAULT_FFPROBE_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_string())
                    .collect(),
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_FFPROBE_COMMAND.to_string()),
                },
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::Lofty,
                enabled: true,
                uses: vec![
                    MediaProcessingUse::ThumbnailAudio,
                    MediaProcessingUse::MetadataAudio,
                ],
                extensions: BUILTIN_AUDIO_METADATA_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_string())
                    .collect(),
                config: MediaProcessingProcessorRuntimeConfig::default(),
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::Images,
                enabled: true,
                uses: vec![
                    MediaProcessingUse::ThumbnailImage,
                    MediaProcessingUse::MetadataImage,
                ],
                extensions: vec![],
                config: MediaProcessingProcessorRuntimeConfig::default(),
            },
        ]
    );

    let json = default_media_processing_registry_json();
    assert!(json.contains("\"vips_cli\""));
    assert!(json.contains("\"ffmpeg_cli\""));
    assert!(json.contains("\"ffprobe_cli\""));
    assert!(json.contains("\"lofty\""));
    assert!(json.contains("\"thumbnail:audio\""));
    assert!(json.contains("\"images\""));
    assert!(json.contains("\"metadata:video\""));
    assert!(json.contains("\"heic\""));
    assert!(json.contains("\"avif\""));
    assert!(json.contains("\"mp4\""));
    assert!(json.contains("\"webm\""));
}

#[test]
fn public_thumbnail_support_exposes_enabled_processor_capabilities() {
    let runtime_config = RuntimeConfig::new();
    let command = available_test_command();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        &serde_json::json!({
            "version": 2,
            "processors": [
                {
                    "kind": "vips_cli",
                    "enabled": true,
                    "uses": ["thumbnail:image"],
                    "extensions": ["HEIC", ".avif"],
                    "config": {
                        "command": command,
                    },
                },
                {
                    "kind": "ffmpeg_cli",
                    "enabled": true,
                    "uses": ["thumbnail:video"],
                    "extensions": ["MP4", ".webm"],
                    "config": {
                        "command": available_test_command(),
                    },
                },
                {
                    "kind": "lofty",
                    "enabled": true,
                    "uses": ["thumbnail:audio", "metadata:audio"],
                    "extensions": ["MP3", ".flac"],
                },
                {
                    "kind": "images",
                    "enabled": false,
                    "uses": ["thumbnail:image", "metadata:image"],
                },
            ],
        })
        .to_string(),
    ));

    let expected_image = ["avif", "heic"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let expected_audio = ["flac", "mp3"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let expected_video = ["mp4", "webm"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        public_thumbnail_support(&runtime_config),
        PublicThumbnailSupport {
            version: 1,
            image_preview: PublicExtensionSupport {
                enabled: true,
                extensions: expected_image.clone(),
            },
            image_thumbnail: PublicExtensionSupport {
                enabled: true,
                extensions: expected_image,
            },
            audio_thumbnail: PublicExtensionSupport {
                enabled: true,
                extensions: expected_audio,
            },
            video_thumbnail: PublicExtensionSupport {
                enabled: true,
                extensions: expected_video,
            },
        }
    );
}

#[test]
fn public_thumbnail_support_keeps_builtin_extensions_when_images_are_enabled() {
    let support = public_thumbnail_support(&RuntimeConfig::new());

    assert_eq!(support.version, 1);
    assert_eq!(support.image_preview, support.image_thumbnail);
    assert!(support.image_preview.enabled);
    assert!(
        support
            .image_preview
            .extensions
            .iter()
            .any(|extension| extension == "png")
    );
    assert!(
        support
            .audio_thumbnail
            .extensions
            .iter()
            .any(|extension| extension == "mp3")
    );
    assert!(!support.video_thumbnail.enabled);
    assert!(support.video_thumbnail.extensions.is_empty());
    assert!(
        !support
            .image_preview
            .extensions
            .iter()
            .any(|extension| extension == "mp3")
    );
}

#[test]
fn public_media_data_support_exposes_default_metadata_capabilities() {
    let support = public_media_data_support(&RuntimeConfig::new());

    assert!(support.enabled);
    assert_eq!(support.version, PUBLIC_MEDIA_DATA_SUPPORT_VERSION);
    assert_eq!(
        support.max_source_bytes,
        default_media_metadata_max_source_bytes()
    );
    assert!(support.kinds.image.enabled);
    assert_eq!(
        support.kinds.image.match_kind,
        PublicMediaDataSupportMatch::Extensions
    );
    assert!(
        support
            .kinds
            .image
            .extensions
            .iter()
            .any(|value| value == "jpg")
    );
    assert!(support.kinds.audio.enabled);
    assert!(
        support
            .kinds
            .audio
            .extensions
            .iter()
            .any(|value| value == "mp3")
    );
    assert!(!support.kinds.video.enabled);
}

#[test]
fn public_media_data_support_clamps_source_limit_to_js_safe_integer() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        MEDIA_METADATA_MAX_SOURCE_BYTES_KEY,
        "9007199254740992",
    ));

    let support = public_media_data_support(&runtime_config);

    assert_eq!(
        support.max_source_bytes,
        PUBLIC_MEDIA_DATA_MAX_SAFE_SOURCE_BYTES
    );
}

#[test]
fn public_media_data_support_respects_global_disable_and_source_limit() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(MEDIA_METADATA_ENABLED_KEY, "false"));
    runtime_config.apply(config_model(MEDIA_METADATA_MAX_SOURCE_BYTES_KEY, "12345"));

    let support = public_media_data_support(&runtime_config);

    assert_eq!(
        support,
        PublicMediaDataSupport {
            version: PUBLIC_MEDIA_DATA_SUPPORT_VERSION,
            enabled: false,
            max_source_bytes: 12345,
            kinds: super::types::PublicMediaDataKindsSupport {
                image: PublicMediaDataKindSupport {
                    enabled: false,
                    match_kind: PublicMediaDataSupportMatch::Extensions,
                    extensions: Vec::new(),
                },
                audio: PublicMediaDataKindSupport {
                    enabled: false,
                    match_kind: PublicMediaDataSupportMatch::Extensions,
                    extensions: Vec::new(),
                },
                video: PublicMediaDataKindSupport {
                    enabled: false,
                    match_kind: PublicMediaDataSupportMatch::Extensions,
                    extensions: Vec::new(),
                },
            },
        }
    );
}

#[test]
fn public_media_data_support_exposes_enabled_ffprobe_extensions() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        &serde_json::json!({
            "version": 2,
            "processors": [
                {
                    "kind": "ffprobe_cli",
                    "enabled": true,
                    "uses": ["metadata:video"],
                    "extensions": ["MP4", ".mov"],
                    "config": {
                        "command": available_test_command(),
                    },
                },
                {
                    "kind": "images",
                    "enabled": true,
                    "uses": ["metadata:image"],
                },
                {
                    "kind": "lofty",
                    "enabled": true,
                    "uses": ["metadata:audio"],
                },
            ],
        })
        .to_string(),
    ));

    let support = public_media_data_support(&runtime_config);

    assert!(support.kinds.video.enabled);
    assert_eq!(
        support.kinds.video.match_kind,
        PublicMediaDataSupportMatch::Extensions
    );
    assert_eq!(support.kinds.video.extensions, vec!["mov", "mp4"]);
}

#[test]
fn public_media_data_support_preserves_ffprobe_any_match() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        &serde_json::json!({
            "version": 2,
            "processors": [
                {
                    "kind": "ffprobe_cli",
                    "enabled": true,
                    "uses": ["metadata:video"],
                    "config": {
                        "command": available_test_command(),
                    },
                },
            ],
        })
        .to_string(),
    ));

    let support = public_media_data_support(&runtime_config);

    assert!(support.kinds.video.enabled);
    assert_eq!(
        support.kinds.video.match_kind,
        PublicMediaDataSupportMatch::Any
    );
    assert!(support.kinds.video.extensions.is_empty());
}

#[test]
fn normalize_media_processing_registry_merges_missing_processors_with_defaults() {
    let normalized = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": false,
                        "uses": ["thumbnail:image"],
                        "extensions": ["HEIC", ".heif", "heic"],
                        "config": {
                            "command": "  custom-vips  "
                        }
                    }
                ]
            }"#,
    )
    .unwrap();

    let parsed: MediaProcessingRegistryConfig = serde_json::from_str(&normalized).unwrap();
    assert_eq!(parsed.processors.len(), 5);
    assert_eq!(
        parsed.processors[0],
        MediaProcessingProcessorConfig {
            kind: MediaProcessorKind::VipsCli,
            enabled: false,
            uses: vec![MediaProcessingUse::ThumbnailImage],
            extensions: vec!["heic".to_string(), "heif".to_string()],
            config: MediaProcessingProcessorRuntimeConfig {
                command: Some("custom-vips".to_string()),
            },
        }
    );
    assert_eq!(
        parsed.processors[1],
        MediaProcessingProcessorConfig {
            kind: MediaProcessorKind::FfmpegCli,
            enabled: false,
            uses: vec![MediaProcessingUse::ThumbnailVideo],
            extensions: DEFAULT_FFMPEG_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
            config: MediaProcessingProcessorRuntimeConfig {
                command: Some(DEFAULT_FFMPEG_COMMAND.to_string()),
            },
        }
    );
    assert_eq!(parsed.processors[2].kind, MediaProcessorKind::FfprobeCli);
    assert_eq!(
        parsed.processors[2].uses,
        default_uses_for_kind(MediaProcessorKind::FfprobeCli)
    );
    assert_eq!(parsed.processors[3].kind, MediaProcessorKind::Lofty);
    assert_eq!(
        parsed.processors[3].uses,
        default_uses_for_kind(MediaProcessorKind::Lofty)
    );
    assert!(parsed.processors[3].enabled);
    assert_eq!(parsed.processors[4].kind, MediaProcessorKind::Images);
    assert_eq!(
        parsed.processors[4].uses,
        default_uses_for_kind(MediaProcessorKind::Images)
    );
    assert!(parsed.processors[4].enabled);
}

#[test]
fn normalize_media_processing_registry_backfills_new_default_uses() {
    let normalized = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "lofty",
                        "enabled": true,
                        "uses": ["metadata:audio"],
                        "extensions": ["mp3"]
                    },
                    {
                        "kind": "images",
                        "enabled": true,
                        "uses": ["thumbnail:image", "metadata:image"]
                    }
                ]
            }"#,
    )
    .unwrap();

    let parsed: MediaProcessingRegistryConfig = serde_json::from_str(&normalized).unwrap();
    let lofty = processor_config_for_kind(&parsed, MediaProcessorKind::Lofty)
        .expect("lofty processor should exist");
    assert_eq!(
        lofty.uses,
        vec![
            MediaProcessingUse::MetadataAudio,
            MediaProcessingUse::ThumbnailAudio,
        ]
    );

    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        MEDIA_PROCESSING_REGISTRY_JSON_KEY,
        &normalized,
    ));
    assert!(
        public_thumbnail_support(&runtime_config)
            .audio_thumbnail
            .extensions
            .contains(&"mp3".to_string())
    );
}

#[test]
fn normalize_media_processing_registry_rejects_storage_native_processor() {
    let error = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "storage_native",
                        "enabled": true,
                        "extensions": ["png"]
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("does not support 'storage_native'")
    );
}

#[test]
fn normalize_media_processing_registry_requires_one_enabled_processor() {
    let error = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": false
                    },
                    {
                        "kind": "ffmpeg_cli",
                        "enabled": false
                    },
                    {
                        "kind": "ffprobe_cli",
                        "enabled": false
                    },
                    {
                        "kind": "lofty",
                        "enabled": false
                    },
                    {
                        "kind": "images",
                        "enabled": false
                    }
                ]
            }"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("enable at least one processor"));
}

#[test]
fn normalize_media_processing_registry_rejects_unavailable_enabled_vips_command() {
    let error = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true,
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
    )
    .unwrap_err();

    assert!(error.to_string().contains("not available"));
}

#[test]
fn normalize_media_processing_registry_rejects_unavailable_enabled_ffmpeg_command() {
    let error = normalize_media_processing_registry_config_value(
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "ffmpeg_cli",
                        "enabled": true,
                        "config": {
                            "command": "definitely-missing-ffmpeg-cli"
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("not available"));
}

#[test]
fn processor_candidates_for_file_name_use_fixed_processor_priority() {
    let config = MediaProcessingRegistryConfig {
        version: MEDIA_PROCESSING_REGISTRY_VERSION,
        processors: vec![
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::VipsCli,
                enabled: true,
                uses: vec![MediaProcessingUse::ThumbnailImage],
                extensions: vec!["heic".to_string()],
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_VIPS_COMMAND.to_string()),
                },
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::FfmpegCli,
                enabled: true,
                uses: vec![MediaProcessingUse::ThumbnailVideo],
                extensions: vec!["mp4".to_string()],
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_FFMPEG_COMMAND.to_string()),
                },
            },
            MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::Images,
                enabled: true,
                uses: vec![
                    MediaProcessingUse::ThumbnailImage,
                    MediaProcessingUse::MetadataImage,
                ],
                extensions: vec![],
                config: MediaProcessingProcessorRuntimeConfig::default(),
            },
        ],
    };

    assert_eq!(
        processor_candidates_for_file_name(&config, "photo.heic"),
        vec![MatchedMediaProcessor {
            processor: MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::VipsCli,
                enabled: true,
                uses: vec![MediaProcessingUse::ThumbnailImage],
                extensions: vec!["heic".to_string()],
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_VIPS_COMMAND.to_string()),
                },
            },
            match_kind: MediaProcessingMatchKind::Extension,
        }]
    );
    assert_eq!(
        processor_candidates_for_file_name(&config, "photo.png"),
        vec![MatchedMediaProcessor {
            processor: MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::Images,
                enabled: true,
                uses: vec![
                    MediaProcessingUse::ThumbnailImage,
                    MediaProcessingUse::MetadataImage,
                ],
                extensions: vec![],
                config: MediaProcessingProcessorRuntimeConfig::default(),
            },
            match_kind: MediaProcessingMatchKind::Extension,
        },]
    );
    assert_eq!(
        processor_candidates_for_file_name(&config, "clip.mp4"),
        vec![MatchedMediaProcessor {
            processor: MediaProcessingProcessorConfig {
                kind: MediaProcessorKind::FfmpegCli,
                enabled: true,
                uses: vec![MediaProcessingUse::ThumbnailVideo],
                extensions: vec!["mp4".to_string()],
                config: MediaProcessingProcessorRuntimeConfig {
                    command: Some(DEFAULT_FFMPEG_COMMAND.to_string()),
                },
            },
            match_kind: MediaProcessingMatchKind::Extension,
        },]
    );
}

#[test]
fn nef_uses_builtin_image_metadata_and_vips_thumbnail_bindings() {
    let mut config = default_media_processing_registry();
    processor_config_for_kind_mut(&mut config, MediaProcessorKind::VipsCli)
        .expect("vips processor should exist")
        .enabled = true;

    let metadata_candidates =
        processor_candidates_for_use(&config, MediaProcessingUse::MetadataImage, "photo.NEF");
    assert_eq!(metadata_candidates.len(), 1);
    assert_eq!(
        metadata_candidates[0].processor.kind,
        MediaProcessorKind::Images
    );
    assert_eq!(
        metadata_candidates[0].match_kind,
        MediaProcessingMatchKind::Extension
    );

    let thumbnail_candidates = processor_candidates_for_file_name(&config, "photo.NEF");
    assert_eq!(thumbnail_candidates.len(), 1);
    assert_eq!(
        thumbnail_candidates[0].processor.kind,
        MediaProcessorKind::VipsCli
    );
    assert_eq!(
        thumbnail_candidates[0].match_kind,
        MediaProcessingMatchKind::Extension
    );

    processor_config_for_kind_mut(&mut config, MediaProcessorKind::VipsCli)
        .expect("vips processor should exist")
        .enabled = false;
    assert!(processor_candidates_for_file_name(&config, "photo.NEF").is_empty());
}

#[test]
fn audio_metadata_uses_builtin_lofty_extensions_over_stored_match_list() {
    let config = MediaProcessingRegistryConfig {
        version: MEDIA_PROCESSING_REGISTRY_VERSION,
        processors: vec![MediaProcessingProcessorConfig {
            kind: MediaProcessorKind::Lofty,
            enabled: true,
            uses: vec![MediaProcessingUse::MetadataAudio],
            extensions: vec!["mp3".to_string()],
            config: MediaProcessingProcessorRuntimeConfig::default(),
        }],
    };

    let candidates =
        processor_candidates_for_use(&config, MediaProcessingUse::MetadataAudio, "track.spx");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].processor.kind, MediaProcessorKind::Lofty);
    assert_eq!(
        candidates[0].match_kind,
        MediaProcessingMatchKind::Extension
    );

    let disabled = MediaProcessingRegistryConfig {
        processors: vec![MediaProcessingProcessorConfig {
            enabled: false,
            ..config.processors[0].clone()
        }],
        ..config
    };
    assert!(
        processor_candidates_for_use(&disabled, MediaProcessingUse::MetadataAudio, "track.spx")
            .is_empty()
    );
}

fn processor_config_for_kind_mut(
    config: &mut MediaProcessingRegistryConfig,
    kind: MediaProcessorKind,
) -> Option<&mut MediaProcessingProcessorConfig> {
    config
        .processors
        .iter_mut()
        .find(|processor| processor.kind == kind)
}

#[test]
fn file_extension_normalizes_suffixes() {
    assert_eq!(file_extension("photo.HEIC"), Some("heic".to_string()));
    assert_eq!(file_extension("archive"), None);
}

#[test]
fn runtime_readers_fall_back_to_defaults() {
    let runtime_config = RuntimeConfig::new();
    assert_eq!(
        media_processing_registry(&runtime_config),
        default_media_processing_registry()
    );
}

#[test]
fn runtime_readers_use_applied_values() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        "media_processing_registry_json",
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true
                    }
                ]
            }"#,
    ));

    assert_eq!(
        media_processing_registry(&runtime_config).processors[0].kind,
        MediaProcessorKind::VipsCli
    );
}

#[test]
fn runtime_readers_keep_vips_processor_even_when_command_is_unavailable() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        "media_processing_registry_json",
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true,
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

    let config = media_processing_registry(&runtime_config);
    let processor = processor_config_for_kind(&config, MediaProcessorKind::VipsCli)
        .expect("vips_cli processor should exist");
    assert!(processor.enabled);
    assert_eq!(
        processor.config.command.as_deref(),
        Some("definitely-missing-vips-cli")
    );
}

#[test]
fn runtime_readers_keep_ffmpeg_processor_even_when_command_is_unavailable() {
    let runtime_config = RuntimeConfig::new();
    runtime_config.apply(config_model(
        "media_processing_registry_json",
        r#"{
                "version": 2,
                "processors": [
                    {
                        "kind": "ffmpeg_cli",
                        "enabled": true,
                        "config": {
                            "command": "definitely-missing-ffmpeg-cli"
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            }"#,
    ));

    let config = media_processing_registry(&runtime_config);
    let processor = processor_config_for_kind(&config, MediaProcessorKind::FfmpegCli)
        .expect("ffmpeg_cli processor should exist");
    assert!(processor.enabled);
    assert_eq!(
        processor.config.command.as_deref(),
        Some("definitely-missing-ffmpeg-cli")
    );
}

#[test]
fn normalize_existing_media_processing_registry_adds_metadata_raw_extensions_without_enabling_processors()
 {
    let normalized = super::normalize_existing_media_processing_registry_config_value(
        r#"{
                "version": 1,
                "processors": [
                    {
                        "kind": "images",
                        "enabled": false
                    }
                ]
            }"#,
    )
    .unwrap();

    let parsed: MediaProcessingRegistryConfig = serde_json::from_str(&normalized).unwrap();
    let images = processor_config_for_kind(&parsed, MediaProcessorKind::Images)
        .expect("images processor should exist");
    assert!(!images.enabled);
    assert_eq!(
        images.uses,
        vec![
            MediaProcessingUse::ThumbnailImage,
            MediaProcessingUse::MetadataImage,
        ]
    );
}
