//! Reviewable documentation projection for the built-in connector catalog.
//!
//! This test reads the same authenticated descriptor and localization APIs used
//! by the admin frontend, then projects those facts into a committed JSON
//! manifest and generated documentation blocks. Provider tutorials remain
//! curated prose and contribute only their route and short use-case summary.

#[macro_use]
#[path = "common/mod.rs"]
mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use actix_web::test;
use aster_drive_storage::{
    StorageConnectorCapabilities, StorageConnectorCredentialMode, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorLocalizationCatalog,
    StorageConnectorUploadWorkflows,
};
use serde::Serialize;

const UPDATE_ENV: &str = "ASTER_UPDATE_STORAGE_CONNECTOR_DOCS";
const MANIFEST_PATH: &str = "docs/generated/storage-connectors.json";
const GENERATED_SOURCE: &str =
    "authenticated built-in StorageConnector descriptor and localization catalogs";

const INDEX_START: &str = "<!-- storage-connectors:index:start -->";
const INDEX_END: &str = "<!-- storage-connectors:index:end -->";
const MATRIX_START: &str = "<!-- storage-connectors:matrix:start -->";
const MATRIX_END: &str = "<!-- storage-connectors:matrix:end -->";
const POLICY_START: &str = "<!-- storage-connectors:policy-catalog:start -->";
const POLICY_END: &str = "<!-- storage-connectors:policy-catalog:end -->";

const GENERATED_PAGES: &[GeneratedPage] = &[
    GeneratedPage {
        path: "docs/src/content/docs/admin/storage-backends/index.md",
        locale: DocumentationLocale::Zh,
        block: GeneratedBlock::Index,
    },
    GeneratedPage {
        path: "docs/src/content/docs/en/admin/storage-backends/index.md",
        locale: DocumentationLocale::En,
        block: GeneratedBlock::Index,
    },
    GeneratedPage {
        path: "docs/src/content/docs/reference/storage-matrix.md",
        locale: DocumentationLocale::Zh,
        block: GeneratedBlock::Matrix,
    },
    GeneratedPage {
        path: "docs/src/content/docs/en/reference/storage-matrix.md",
        locale: DocumentationLocale::En,
        block: GeneratedBlock::Matrix,
    },
    GeneratedPage {
        path: "docs/src/content/docs/admin/storage-policies.md",
        locale: DocumentationLocale::Zh,
        block: GeneratedBlock::PolicyCatalog,
    },
    GeneratedPage {
        path: "docs/src/content/docs/en/admin/storage-policies.md",
        locale: DocumentationLocale::En,
        block: GeneratedBlock::PolicyCatalog,
    },
];

const PRESENTATIONS: &[ConnectorDocumentationPresentation] = &[
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.local",
        tutorial_slug: "local",
        best_for_en: "Single machine, NAS, small teams, minimal dependencies",
        best_for_zh: "单机、NAS、小团队、最少依赖",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.s3",
        tutorial_slug: "s3",
        best_for_en: "S3-compatible object storage, external buckets, and large files",
        best_for_zh: "S3 兼容对象存储、外部 bucket 和大文件",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.qiniu",
        tutorial_slug: "qiniu-kodo",
        best_for_en: "Qiniu Cloud Kodo S3 spaces with official endpoint diagnostics",
        best_for_zh: "带官方 endpoint 诊断的七牛云 Kodo S3 空间",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.alibaba_oss",
        tutorial_slug: "alibaba-oss",
        best_for_en: "Alibaba Cloud OSS with native V4 signing, split endpoints, or CNAME",
        best_for_zh: "阿里云 OSS 原生 V4 签名、内外网 endpoint 分流或 CNAME",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.huawei_obs",
        tutorial_slug: "huawei-obs",
        best_for_en: "Huawei Cloud OBS with native SignatureObs and regional or custom-domain endpoints",
        best_for_zh: "使用原生 SignatureObs、区域 endpoint 或自定义域名的华为云 OBS",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.azure_blob",
        tutorial_slug: "azure-blob",
        best_for_en: "Azure Storage accounts and Blob containers",
        best_for_zh: "Azure Storage account 和 Blob container",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.tencent_cos",
        tutorial_slug: "tencent-cos",
        best_for_en: "Tencent COS and per-policy COS CI processing",
        best_for_zh: "腾讯云 COS 和按策略启用的 COS 数据万象",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.onedrive",
        tutorial_slug: "onedrive",
        best_for_en: "Microsoft 365, OneDrive, SharePoint, and group drives",
        best_for_zh: "Microsoft 365、OneDrive、SharePoint 和 group drive",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.sftp",
        tutorial_slug: "sftp",
        best_for_en: "SSH/SFTP file servers and server-side streaming",
        best_for_zh: "SSH/SFTP 文件服务器和服务端流式读写",
    },
    ConnectorDocumentationPresentation {
        connector_id: "asterdrive.storage.remote",
        tutorial_slug: "remote-follower",
        best_for_en: "Objects stored by another AsterDrive follower node",
        best_for_zh: "由另一台 AsterDrive follower 节点保存对象",
    },
];

#[derive(Clone, Copy)]
enum DocumentationLocale {
    En,
    Zh,
}

impl DocumentationLocale {
    const fn path_prefix(self) -> &'static str {
        match self {
            Self::En => "/en",
            Self::Zh => "",
        }
    }
}

#[derive(Clone, Copy)]
enum GeneratedBlock {
    Index,
    Matrix,
    PolicyCatalog,
}

struct GeneratedPage {
    path: &'static str,
    locale: DocumentationLocale,
    block: GeneratedBlock,
}

struct ConnectorDocumentationPresentation {
    connector_id: &'static str,
    tutorial_slug: &'static str,
    best_for_en: &'static str,
    best_for_zh: &'static str,
}

#[derive(Serialize)]
struct StorageConnectorDocumentationManifest {
    schema_version: u8,
    generated_from: &'static str,
    connectors: Vec<StorageConnectorDocumentationEntry>,
}

#[derive(Serialize)]
struct StorageConnectorDocumentationEntry {
    connector_id: String,
    display_name: LocalizedText,
    documentation: ConnectorDocumentation,
    deployment_scope: StorageConnectorDeploymentScope,
    supports_initial_setup: bool,
    credential_mode: StorageConnectorCredentialMode,
    capabilities: StorageConnectorCapabilities,
    upload_workflows: StorageConnectorUploadWorkflows,
}

#[derive(Serialize)]
struct LocalizedText {
    en: String,
    zh: String,
}

#[derive(Serialize)]
struct ConnectorDocumentation {
    tutorial_slug: String,
    best_for: LocalizedText,
}

#[actix_web::test]
async fn generated_storage_connector_docs_are_current() {
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let request = test::TestRequest::get()
        .uri("/api/v1/admin/policies/storage-drivers?context=manage")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = test::read_body_json(response).await;
    let descriptors =
        serde_json::from_value::<Vec<StorageConnectorDescriptor>>(body["data"].clone())
            .expect("storage connector descriptor response");
    let en_localizations = fetch_localizations(&app, &admin_token, "en").await;
    let zh_localizations = fetch_localizations(&app, &admin_token, "zh-CN").await;
    let manifest = build_manifest(descriptors, &en_localizations, &zh_localizations);
    validate_tutorials_exist(repository_root, &manifest);

    let manifest_json = format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("serialize storage connector docs manifest")
    );
    assert_or_update(repository_root.join(MANIFEST_PATH), manifest_json);

    for page in GENERATED_PAGES {
        let path = repository_root.join(page.path);
        let current = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let (start, end, generated) = match page.block {
            GeneratedBlock::Index => (INDEX_START, INDEX_END, render_index(&manifest, page.locale)),
            GeneratedBlock::Matrix => (
                MATRIX_START,
                MATRIX_END,
                render_matrix(&manifest, page.locale),
            ),
            GeneratedBlock::PolicyCatalog => (
                POLICY_START,
                POLICY_END,
                render_policy_catalog(&manifest, page.locale),
            ),
        };
        let expected = replace_generated_block(&current, start, end, &generated, &path);
        assert_or_update(path, expected);
    }
}

async fn fetch_localizations<S, B>(
    app: &S,
    admin_token: &str,
    locale: &str,
) -> StorageConnectorLocalizationCatalog
where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse<B>,
            Error = actix_web::Error,
        >,
    B: actix_web::body::MessageBody + 'static,
{
    let request = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/policies/storage-drivers/localizations?context=manage&locale={locale}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(admin_token)))
        .to_request();
    let response = test::call_service(app, request).await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = test::read_body_json(response).await;
    serde_json::from_value(body["data"].clone()).expect("storage connector localization response")
}

fn build_manifest(
    descriptors: Vec<StorageConnectorDescriptor>,
    en_localizations: &StorageConnectorLocalizationCatalog,
    zh_localizations: &StorageConnectorLocalizationCatalog,
) -> StorageConnectorDocumentationManifest {
    let presentations = PRESENTATIONS
        .iter()
        .map(|presentation| (presentation.connector_id, presentation))
        .collect::<BTreeMap<_, _>>();
    let connectors = descriptors
        .into_iter()
        .map(|descriptor| {
            let connector_id = descriptor.connector_id.as_str();
            let presentation = presentations.get(connector_id).unwrap_or_else(|| {
                panic!(
                    "built-in connector '{connector_id}' needs one provider-owned documentation presentation"
                )
            });
            let display_name = LocalizedText {
                en: localized_label(en_localizations, &descriptor),
                zh: localized_label(zh_localizations, &descriptor),
            };
            StorageConnectorDocumentationEntry {
                connector_id: connector_id.to_string(),
                display_name,
                documentation: ConnectorDocumentation {
                    tutorial_slug: presentation.tutorial_slug.to_string(),
                    best_for: LocalizedText {
                        en: presentation.best_for_en.to_string(),
                        zh: presentation.best_for_zh.to_string(),
                    },
                },
                deployment_scope: descriptor.deployment_scope,
                supports_initial_setup: descriptor.supports_initial_setup,
                credential_mode: descriptor.credential_mode,
                capabilities: descriptor.capabilities,
                upload_workflows: descriptor.upload_workflows,
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(
        connectors.len(),
        presentations.len(),
        "documentation presentations contain an unavailable connector"
    );
    StorageConnectorDocumentationManifest {
        schema_version: 1,
        generated_from: GENERATED_SOURCE,
        connectors,
    }
}

fn localized_label(
    catalog: &StorageConnectorLocalizationCatalog,
    descriptor: &StorageConnectorDescriptor,
) -> String {
    catalog
        .resources
        .iter()
        .find(|resource| resource.connector_id == descriptor.connector_id)
        .unwrap_or_else(|| {
            panic!(
                "connector '{}' has no localization resource",
                descriptor.connector_id
            )
        })
        .messages
        .get(&descriptor.ui.label_key)
        .unwrap_or_else(|| {
            panic!(
                "connector '{}' localization is missing UI label '{}'",
                descriptor.connector_id, descriptor.ui.label_key
            )
        })
        .clone()
}

fn validate_tutorials_exist(
    repository_root: &Path,
    manifest: &StorageConnectorDocumentationManifest,
) {
    for connector in &manifest.connectors {
        for locale_path in ["", "en/"] {
            let path = repository_root.join(format!(
                "docs/src/content/docs/{locale_path}admin/storage-backends/{}.md",
                connector.documentation.tutorial_slug
            ));
            assert!(
                path.is_file(),
                "connector '{}' tutorial does not exist: {}",
                connector.connector_id,
                path.display()
            );
        }
    }
}

fn render_index(
    manifest: &StorageConnectorDocumentationManifest,
    locale: DocumentationLocale,
) -> String {
    let mut output = match locale {
        DocumentationLocale::Zh => {
            "| 后端 | Connector ID | 部署范围 | 适合场景 | 教程 |\n| --- | --- | --- | --- | --- |\n"
                .to_string()
        }
        DocumentationLocale::En => {
            "| Backend | Connector ID | Deployment scope | Best for | Tutorial |\n| --- | --- | --- | --- | --- |\n"
                .to_string()
        }
    };
    for connector in &manifest.connectors {
        let name = localized(&connector.display_name, locale);
        let best_for = localized(&connector.documentation.best_for, locale);
        let deployment = deployment_scope(connector.deployment_scope, locale);
        let tutorial = tutorial_link(connector, locale);
        output.push_str(&format!(
            "| {name} | `{}` | {deployment} | {best_for} | [{name}]({tutorial}) |\n",
            connector.connector_id
        ));
    }
    output
}

fn render_policy_catalog(
    manifest: &StorageConnectorDocumentationManifest,
    locale: DocumentationLocale,
) -> String {
    let mut output = match locale {
        DocumentationLocale::Zh => {
            "| Connector ID | 后端 | 凭据模式 | 详细教程 |\n| --- | --- | --- | --- |\n"
                .to_string()
        }
        DocumentationLocale::En => {
            "| Connector ID | Backend | Credential mode | Full tutorial |\n| --- | --- | --- | --- |\n"
                .to_string()
        }
    };
    for connector in &manifest.connectors {
        let name = localized(&connector.display_name, locale);
        let credential = credential_mode(connector.credential_mode, locale);
        output.push_str(&format!(
            "| `{}` | {name} | {credential} | [{name}]({}) |\n",
            connector.connector_id,
            tutorial_link(connector, locale)
        ));
    }
    output
}

fn render_matrix(
    manifest: &StorageConnectorDocumentationManifest,
    locale: DocumentationLocale,
) -> String {
    let mut output = match locale {
        DocumentationLocale::Zh => "| 后端 | 部署范围 | 浏览器直传 | 直连下载 | 容量观测 | 存储原生处理 | 凭据模式 |\n| --- | --- | --- | --- | --- | --- | --- |\n".to_string(),
        DocumentationLocale::En => "| Backend | Deployment scope | Browser direct upload | Direct download | Capacity | Storage-native processing | Credential mode |\n| --- | --- | --- | --- | --- | --- | --- |\n".to_string(),
    };
    for connector in &manifest.connectors {
        let name = localized(&connector.display_name, locale);
        output.push_str(&format!(
            "| [{name}]({}) | {} | {} | {} | {} | {} | {} |\n",
            tutorial_link(connector, locale),
            deployment_scope(connector.deployment_scope, locale),
            direct_upload(connector, locale),
            yes_no(connector.capabilities.presigned_download, locale),
            yes_no(connector.capabilities.capacity, locale),
            native_processing(&connector.capabilities, locale),
            credential_mode(connector.credential_mode, locale),
        ));
    }
    output
}

fn localized(text: &LocalizedText, locale: DocumentationLocale) -> &str {
    match locale {
        DocumentationLocale::En => &text.en,
        DocumentationLocale::Zh => &text.zh,
    }
}

fn tutorial_link(
    connector: &StorageConnectorDocumentationEntry,
    locale: DocumentationLocale,
) -> String {
    format!(
        "{}/admin/storage-backends/{}/",
        locale.path_prefix(),
        connector.documentation.tutorial_slug
    )
}

fn deployment_scope(
    scope: StorageConnectorDeploymentScope,
    locale: DocumentationLocale,
) -> &'static str {
    match (scope, locale) {
        (StorageConnectorDeploymentScope::InstanceLocal, DocumentationLocale::En) => {
            "Instance-local"
        }
        (StorageConnectorDeploymentScope::InstanceLocal, DocumentationLocale::Zh) => "单实例本地",
        (
            StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            DocumentationLocale::En,
        ) => "Shared across Primary instances",
        (
            StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            DocumentationLocale::Zh,
        ) => "Primary 间共享",
    }
}

fn credential_mode(
    mode: StorageConnectorCredentialMode,
    locale: DocumentationLocale,
) -> &'static str {
    match (mode, locale) {
        (StorageConnectorCredentialMode::None, DocumentationLocale::En) => "None",
        (StorageConnectorCredentialMode::None, DocumentationLocale::Zh) => "无 connector 凭据",
        (StorageConnectorCredentialMode::StaticSecret, DocumentationLocale::En) => "Static secret",
        (StorageConnectorCredentialMode::StaticSecret, DocumentationLocale::Zh) => "静态密钥",
        (StorageConnectorCredentialMode::RemoteNode, DocumentationLocale::En) => {
            "Remote-node binding"
        }
        (StorageConnectorCredentialMode::RemoteNode, DocumentationLocale::Zh) => "远程节点绑定",
        (StorageConnectorCredentialMode::OauthDelegated, DocumentationLocale::En) => {
            "Delegated OAuth"
        }
        (StorageConnectorCredentialMode::OauthDelegated, DocumentationLocale::Zh) => "委托 OAuth",
    }
}

fn direct_upload(
    connector: &StorageConnectorDocumentationEntry,
    locale: DocumentationLocale,
) -> &'static str {
    match (
        connector.upload_workflows.presigned_upload,
        connector
            .upload_workflows
            .frontend_direct_provider_resumable_upload,
        locale,
    ) {
        (true, true, DocumentationLocale::En) => "Presigned + provider-direct",
        (true, true, DocumentationLocale::Zh) => "Presigned + provider direct",
        (true, false, _) => "Presigned",
        (false, true, DocumentationLocale::En) => "Provider-direct",
        (false, true, DocumentationLocale::Zh) => "Provider direct",
        (false, false, DocumentationLocale::En) => "No",
        (false, false, DocumentationLocale::Zh) => "不支持",
    }
}

fn yes_no(value: bool, locale: DocumentationLocale) -> &'static str {
    match (value, locale) {
        (true, DocumentationLocale::En) => "Yes",
        (true, DocumentationLocale::Zh) => "支持",
        (false, DocumentationLocale::En) => "No",
        (false, DocumentationLocale::Zh) => "不支持",
    }
}

fn native_processing(
    capabilities: &StorageConnectorCapabilities,
    locale: DocumentationLocale,
) -> &'static str {
    match (
        capabilities.storage_native_thumbnail,
        capabilities.storage_native_media_metadata,
        locale,
    ) {
        (true, true, DocumentationLocale::En) => "Thumbnail + media metadata",
        (true, true, DocumentationLocale::Zh) => "缩略图 + 媒体元数据",
        (true, false, DocumentationLocale::En) => "Thumbnail",
        (true, false, DocumentationLocale::Zh) => "缩略图",
        (false, true, DocumentationLocale::En) => "Media metadata",
        (false, true, DocumentationLocale::Zh) => "媒体元数据",
        (false, false, DocumentationLocale::En) => "No",
        (false, false, DocumentationLocale::Zh) => "不支持",
    }
}

fn replace_generated_block(
    current: &str,
    start_marker: &str,
    end_marker: &str,
    generated: &str,
    path: &Path,
) -> String {
    let start = current
        .find(start_marker)
        .unwrap_or_else(|| panic!("{} is missing marker {start_marker}", path.display()));
    let content_start = start + start_marker.len();
    let end_offset = current[content_start..]
        .find(end_marker)
        .unwrap_or_else(|| panic!("{} is missing marker {end_marker}", path.display()));
    let end = content_start + end_offset;
    assert_eq!(
        current.matches(start_marker).count(),
        1,
        "{} must contain exactly one {start_marker}",
        path.display()
    );
    assert_eq!(
        current.matches(end_marker).count(),
        1,
        "{} must contain exactly one {end_marker}",
        path.display()
    );
    format!(
        "{}\n{}{}",
        &current[..content_start],
        generated,
        &current[end..]
    )
}

fn assert_or_update(path: PathBuf, expected: String) {
    if std::env::var_os(UPDATE_ENV).is_some() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
        }
        fs::write(&path, expected)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
        return;
    }

    let actual = fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        actual,
        expected,
        "{} is stale; run `make storage-docs` and review the generated diff",
        path.display()
    );
}
