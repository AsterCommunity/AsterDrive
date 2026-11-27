//! 预览应用注册表的 runtime config 键与默认值。
//!
//! 键与种子默认值归 config 所有；预览应用的解析、校验和运行时回退由
//! `services::preview::apps` 承担。默认 JSON 必须与内置应用集保持一致，
//! 由 services 侧的对齐测试锁定。

/// Runtime config key storing the public preview app registry JSON.
pub const PREVIEW_APPS_CONFIG_KEY: &str = "frontend_preview_apps_json";

/// Pretty JSON document seeded into the runtime config when the preview app
/// registry is missing. Must stay byte-identical to the serialization of the
/// built-in app set; pinned by a parity test in `services::preview::apps`.
pub const DEFAULT_PUBLIC_PREVIEW_APPS_JSON: &str = r#"{
  "version": 2,
  "apps": [
    {
      "key": "builtin.image",
      "provider": "builtin",
      "icon": "/static/preview-apps/image.svg",
      "enabled": true,
      "labels": {
        "en": "Image preview",
        "zh": "图片预览"
      }
    },
    {
      "key": "builtin.video",
      "provider": "builtin",
      "icon": "/static/preview-apps/video.svg",
      "enabled": true,
      "labels": {
        "en": "Video preview",
        "zh": "视频预览"
      }
    },
    {
      "key": "builtin.audio",
      "provider": "builtin",
      "icon": "/static/preview-apps/audio.svg",
      "enabled": true,
      "labels": {
        "en": "Audio preview",
        "zh": "音频预览"
      }
    },
    {
      "key": "builtin.pdf",
      "provider": "builtin",
      "icon": "/static/preview-apps/pdf.svg",
      "enabled": true,
      "labels": {
        "en": "PDF preview",
        "zh": "PDF 预览"
      },
      "extensions": [
        "pdf"
      ]
    },
    {
      "key": "builtin.office_microsoft",
      "provider": "url_template",
      "icon": "/static/preview-apps/microsoft-onedrive.svg",
      "enabled": true,
      "labels": {
        "en": "Microsoft Viewer",
        "zh": "Microsoft 预览器"
      },
      "extensions": [
        "doc",
        "docx",
        "xls",
        "xlsx",
        "ppt",
        "pptx"
      ],
      "config": {
        "mode": "iframe",
        "url_template": "https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}",
        "allowed_origins": [
          "https://view.officeapps.live.com"
        ]
      }
    },
    {
      "key": "builtin.office_google",
      "provider": "url_template",
      "icon": "/static/preview-apps/google-drive.svg",
      "enabled": true,
      "labels": {
        "en": "Google Viewer",
        "zh": "Google 预览器"
      },
      "extensions": [
        "doc",
        "docx",
        "xls",
        "xlsx",
        "ppt",
        "pptx",
        "odt",
        "ods",
        "odp"
      ],
      "config": {
        "mode": "iframe",
        "url_template": "https://docs.google.com/gview?embedded=true&url={{file_preview_url}}",
        "allowed_origins": [
          "https://docs.google.com"
        ]
      }
    },
    {
      "key": "builtin.markdown",
      "provider": "builtin",
      "icon": "/static/preview-apps/markdown.svg",
      "enabled": true,
      "labels": {
        "en": "Markdown preview",
        "zh": "Markdown 预览"
      },
      "extensions": [
        "md",
        "markdown"
      ]
    },
    {
      "key": "builtin.table",
      "provider": "builtin",
      "icon": "/static/preview-apps/table.svg",
      "enabled": true,
      "labels": {
        "en": "Table preview",
        "zh": "表格预览"
      },
      "extensions": [
        "csv",
        "tsv"
      ],
      "config": {
        "delimiter": "auto"
      }
    },
    {
      "key": "builtin.formatted",
      "provider": "builtin",
      "icon": "/static/preview-apps/json.svg",
      "enabled": true,
      "labels": {
        "en": "Formatted view",
        "zh": "格式化视图"
      },
      "extensions": [
        "json",
        "xml"
      ]
    },
    {
      "key": "builtin.code",
      "provider": "builtin",
      "icon": "/static/preview-apps/code.svg",
      "enabled": true,
      "labels": {
        "en": "Source view",
        "zh": "源码视图"
      }
    },
    {
      "key": "builtin.try_text",
      "provider": "builtin",
      "icon": "/static/preview-apps/file.svg",
      "enabled": true,
      "labels": {
        "en": "Open as text",
        "zh": "以文本方式打开"
      }
    },
    {
      "key": "builtin.archive",
      "provider": "builtin",
      "icon": "/static/preview-apps/archive.svg",
      "enabled": true,
      "labels": {
        "en": "Archive preview",
        "zh": "压缩包预览"
      },
      "extensions": [
        "zip"
      ]
    }
  ]
}"#;
