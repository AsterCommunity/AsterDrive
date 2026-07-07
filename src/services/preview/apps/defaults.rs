//! 预览应用服务子模块：`defaults`。

use std::collections::BTreeMap;

use super::{
    BUILTIN_ARCHIVE_PREVIEW_APP_KEY, BUILTIN_TABLE_PREVIEW_APP_KEY,
    DEFAULT_TABLE_PREVIEW_DELIMITER, PREVIEW_APP_ICON_ARCHIVE, PREVIEW_APP_ICON_AUDIO,
    PREVIEW_APP_ICON_CODE, PREVIEW_APP_ICON_FILE, PREVIEW_APP_ICON_GOOGLE_DRIVE,
    PREVIEW_APP_ICON_IMAGE, PREVIEW_APP_ICON_JSON, PREVIEW_APP_ICON_MARKDOWN,
    PREVIEW_APP_ICON_MICROSOFT_ONEDRIVE, PREVIEW_APP_ICON_PDF, PREVIEW_APP_ICON_TABLE,
    PREVIEW_APP_ICON_VIDEO, PREVIEW_APPS_VERSION, PreviewAppProvider, PreviewOpenMode,
    PublicPreviewAppConfig, PublicPreviewAppDefinition, PublicPreviewAppsConfig,
};

pub fn default_public_preview_apps() -> PublicPreviewAppsConfig {
    PublicPreviewAppsConfig {
        version: PREVIEW_APPS_VERSION,
        apps: vec![
            builtin_app(
                "builtin.image",
                PREVIEW_APP_ICON_IMAGE,
                labels(("en", "Image preview"), ("zh", "图片预览")),
                &[],
            ),
            builtin_app(
                "builtin.video",
                PREVIEW_APP_ICON_VIDEO,
                labels(("en", "Video preview"), ("zh", "视频预览")),
                &[],
            ),
            builtin_app(
                "builtin.audio",
                PREVIEW_APP_ICON_AUDIO,
                labels(("en", "Audio preview"), ("zh", "音频预览")),
                &[],
            ),
            builtin_app(
                "builtin.pdf",
                PREVIEW_APP_ICON_PDF,
                labels(("en", "PDF preview"), ("zh", "PDF 预览")),
                &["pdf"],
            ),
            url_template_app(
                "builtin.office_microsoft",
                PREVIEW_APP_ICON_MICROSOFT_ONEDRIVE,
                labels(("en", "Microsoft Viewer"), ("zh", "Microsoft 预览器")),
                &["doc", "docx", "xls", "xlsx", "ppt", "pptx"],
                PublicPreviewAppConfig {
                    mode: Some(PreviewOpenMode::Iframe),
                    url_template: Some(
                        "https://view.officeapps.live.com/op/embed.aspx?src={{file_preview_url}}"
                            .to_string(),
                    ),
                    allowed_origins: vec!["https://view.officeapps.live.com".to_string()],
                    ..Default::default()
                },
            ),
            url_template_app(
                "builtin.office_google",
                PREVIEW_APP_ICON_GOOGLE_DRIVE,
                labels(("en", "Google Viewer"), ("zh", "Google 预览器")),
                &[
                    "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp",
                ],
                PublicPreviewAppConfig {
                    mode: Some(PreviewOpenMode::Iframe),
                    url_template: Some(
                        "https://docs.google.com/gview?embedded=true&url={{file_preview_url}}"
                            .to_string(),
                    ),
                    allowed_origins: vec!["https://docs.google.com".to_string()],
                    ..Default::default()
                },
            ),
            builtin_app(
                "builtin.markdown",
                PREVIEW_APP_ICON_MARKDOWN,
                labels(("en", "Markdown preview"), ("zh", "Markdown 预览")),
                &["md", "markdown"],
            ),
            builtin_app_with_config(
                BUILTIN_TABLE_PREVIEW_APP_KEY,
                PREVIEW_APP_ICON_TABLE,
                labels(("en", "Table preview"), ("zh", "表格预览")),
                &["csv", "tsv"],
                PublicPreviewAppConfig {
                    delimiter: Some(DEFAULT_TABLE_PREVIEW_DELIMITER.to_string()),
                    ..Default::default()
                },
            ),
            builtin_app(
                "builtin.formatted",
                PREVIEW_APP_ICON_JSON,
                labels(("en", "Formatted view"), ("zh", "格式化视图")),
                &["json", "xml"],
            ),
            builtin_app(
                "builtin.code",
                PREVIEW_APP_ICON_CODE,
                labels(("en", "Source view"), ("zh", "源码视图")),
                &[],
            ),
            builtin_app(
                "builtin.try_text",
                PREVIEW_APP_ICON_FILE,
                labels(("en", "Open as text"), ("zh", "以文本方式打开")),
                &[],
            ),
            builtin_app(
                BUILTIN_ARCHIVE_PREVIEW_APP_KEY,
                PREVIEW_APP_ICON_ARCHIVE,
                labels(("en", "Archive preview"), ("zh", "压缩包预览")),
                &["zip"],
            ),
        ],
    }
}

pub fn default_public_preview_apps_json() -> String {
    serde_json::to_string_pretty(&default_public_preview_apps()).unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to serialize default preview apps config");
        serde_json::to_string(&PublicPreviewAppsConfig {
            version: PREVIEW_APPS_VERSION,
            apps: Vec::new(),
        })
        .unwrap_or_else(|fallback_error| {
            tracing::error!(%fallback_error, "failed to serialize fallback preview apps config");
            "{}".to_string()
        })
    })
}

fn builtin_app(
    key: &str,
    icon: &str,
    labels: BTreeMap<String, String>,
    extensions: &[&str],
) -> PublicPreviewAppDefinition {
    builtin_app_with_config(
        key,
        icon,
        labels,
        extensions,
        PublicPreviewAppConfig::default(),
    )
}

fn builtin_app_with_config(
    key: &str,
    icon: &str,
    labels: BTreeMap<String, String>,
    extensions: &[&str],
    config: PublicPreviewAppConfig,
) -> PublicPreviewAppDefinition {
    app_with_config(
        PreviewAppProvider::Builtin,
        key,
        icon,
        labels,
        extensions,
        config,
    )
}

fn url_template_app(
    key: &str,
    icon: &str,
    labels: BTreeMap<String, String>,
    extensions: &[&str],
    config: PublicPreviewAppConfig,
) -> PublicPreviewAppDefinition {
    app_with_config(
        PreviewAppProvider::UrlTemplate,
        key,
        icon,
        labels,
        extensions,
        config,
    )
}

fn app_with_config(
    provider: PreviewAppProvider,
    key: &str,
    icon: &str,
    labels: BTreeMap<String, String>,
    extensions: &[&str],
    config: PublicPreviewAppConfig,
) -> PublicPreviewAppDefinition {
    PublicPreviewAppDefinition {
        key: key.to_string(),
        provider,
        icon: icon.to_string(),
        enabled: true,
        labels,
        extensions: extensions.iter().map(|value| value.to_string()).collect(),
        config,
    }
}

fn labels(primary: (&str, &str), secondary: (&str, &str)) -> BTreeMap<String, String> {
    BTreeMap::from([
        (primary.0.to_string(), primary.1.to_string()),
        (secondary.0.to_string(), secondary.1.to_string()),
    ])
}
