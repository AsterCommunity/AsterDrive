use crate::config::RuntimeConfig;
use crate::entities::system_config;
use crate::types::{MediaProcessorKind, SystemConfigSource, SystemConfigValueType};
use chrono::Utc;

use super::{
    BUILTIN_IMAGES_SUPPORTED_EXTENSIONS, DEFAULT_FFMPEG_COMMAND, DEFAULT_FFMPEG_EXTENSIONS,
    DEFAULT_FFPROBE_COMMAND, DEFAULT_FFPROBE_EXTENSIONS, DEFAULT_LOFTY_EXTENSIONS,
    DEFAULT_VIPS_COMMAND, DEFAULT_VIPS_EXTENSIONS, MEDIA_PROCESSING_REGISTRY_JSON_KEY,
    MEDIA_PROCESSING_REGISTRY_VERSION, MatchedMediaProcessor, MediaProcessingMatchKind,
    MediaProcessingProcessorConfig, MediaProcessingProcessorRuntimeConfig,
    MediaProcessingRegistryConfig, MediaProcessingUse, PublicThumbnailSupport,
    command_is_available, default_media_processing_registry,
    default_media_processing_registry_json, default_uses_for_kind,
    ffmpeg_command_from_registry_value, ffprobe_command_from_registry_value, file_extension,
    media_processing_registry, normalize_ffmpeg_command, normalize_ffprobe_command,
    normalize_media_processing_registry_config_value, normalize_vips_command,
    parse_media_processor_kind, processor_candidates_for_file_name, processor_config_for_kind,
    public_thumbnail_support, vips_command_from_registry_value,
};

fn config_model(key: &str, value: &str) -> system_config::Model {
    system_config::Model {
        id: 0,
        key: key.to_string(),
        value: value.to_string(),
        value_type: SystemConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: SystemConfigSource::System,
        namespace: String::new(),
        category: "test".to_string(),
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
    for extension in BUILTIN_IMAGES_SUPPORTED_EXTENSIONS {
        assert!(super::builtin_images_supports_extension(extension));
    }
    assert!(!super::builtin_images_supports_extension("heic"));
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
                extensions: DEFAULT_LOFTY_EXTENSIONS
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

    assert_eq!(
        public_thumbnail_support(&runtime_config),
        PublicThumbnailSupport {
            version: 1,
            extensions: vec![
                "avif".to_string(),
                "flac".to_string(),
                "heic".to_string(),
                "mp3".to_string(),
                "mp4".to_string(),
                "webm".to_string(),
            ],
        }
    );
}

#[test]
fn public_thumbnail_support_keeps_builtin_extensions_when_images_are_enabled() {
    let support = public_thumbnail_support(&RuntimeConfig::new());
    let expected = BUILTIN_IMAGES_SUPPORTED_EXTENSIONS
        .iter()
        .chain(DEFAULT_LOFTY_EXTENSIONS.iter())
        .map(|extension| (*extension).to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    assert_eq!(support.version, 1);
    assert_eq!(support.extensions, expected);
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
