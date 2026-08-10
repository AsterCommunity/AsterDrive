use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "driver_type_qiniu",
        "Qiniu Kodo",
        "七牛云 Kodo"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_storage_desc",
        "Store files in Qiniu Cloud Kodo using native UploadToken and Multipart v2 APIs.",
        "使用七牛云 Kodo 原生 UploadToken 和 Multipart v2 API 存储文件。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_helper",
        "Choose the bucket region and a browser-facing download domain. Upload and management endpoints are selected from the region.",
        "选择 bucket 所在区域和浏览器下载域名。上传与管理 endpoint 由区域映射生成。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_qiniu_connection_desc",
        "Set the Qiniu bucket, region, download domain, optional prefix, and AccessKey credentials.",
        "填写七牛 bucket、区域、下载域名、可选前缀和 AccessKey 凭据。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_qiniu_desc",
        "Qiniu native object storage configuration.",
        "七牛原生对象存储配置。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_bucket_required",
        "Qiniu bucket is required.",
        "七牛 bucket 不能为空。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_domain",
        "Download domain",
        "下载域名"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_domain_desc",
        "Browser-facing HTTP(S) domain for object downloads. It is not used for upload or management requests.",
        "用于对象下载的浏览器可见 HTTP(S) 域名，不用于上传或管理请求。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_domain_protocol_error",
        "Download domain must use HTTP or HTTPS.",
        "下载域名必须使用 HTTP 或 HTTPS。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z0",
        "East China (z0)",
        "华东（z0）"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z0_desc",
        "Qiniu East China region.",
        "七牛华东区域。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z1",
        "North China (z1)",
        "华北（z1）"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z1_desc",
        "Qiniu North China region.",
        "七牛华北区域。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z2",
        "South China (z2)",
        "华南（z2）"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_region_z2_desc",
        "Qiniu South China region.",
        "七牛华南区域。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_access_key",
        "Qiniu AccessKey",
        "七牛 AccessKey"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_secret_key",
        "Qiniu SecretKey",
        "七牛 SecretKey"
    ),
];
