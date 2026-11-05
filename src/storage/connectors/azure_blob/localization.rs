use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "azure_download_delivery_desc",
        "Choose the download endpoint for presigned downloads. Uploads and backend operations continue to use the Blob endpoint.",
        "选择预签名下载的交付入口；上传和后端操作仍使用 Blob Endpoint。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_origin_desc",
        "Download directly from the configured Blob endpoint using SAS.",
        "通过 SAS 从配置的 Blob Endpoint 直接下载。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_front_door_desc",
        "Download through a separate HTTPS Front Door domain using Blob SAS.",
        "通过独立的 HTTPS Front Door 域名下载，沿用 Blob SAS 鉴权。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_delivery",
        "Download delivery mode",
        "下载交付方式"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_origin",
        "Azure Blob origin",
        "Azure Blob 源站"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_front_door",
        "Azure Front Door",
        "Azure Front Door"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_download_base_url_desc",
        "Enter the HTTPS download domain without the container or object path.",
        "填写 HTTPS 下载域名，不包含容器或对象路径。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cdn_cache_max_age_seconds",
        "CDN cache time (seconds)",
        "CDN 缓存时间（秒）"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cdn_cache_max_age_seconds_desc",
        "0 disables caching. Cache time is independent of SAS expiry.",
        "0 表示不缓存；缓存时间独立于 SAS 有效期。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_action",
        "Auto configure Azure Blob CORS",
        "自动配置 Azure Blob CORS"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_description",
        "Write AsterDrive's CORS rule to the Blob Service of the entire storage account currently used by this policy, affecting all its containers. Allowed origins come from the configured public site URLs. Other CORS rules and service settings are preserved.",
        "向当前使用的整个存储账户的 Blob Service 写入 AsterDrive 的 CORS 规则，影响该账户所有容器。允许来源取自系统配置的公开站点地址，保留其他 CORS 规则和服务配置。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_origins",
        "Site origins",
        "站点 Origin"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_preserved",
        "Preserved rules",
        "保留规则数"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_request_id",
        "Azure request ID",
        "Azure 请求 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_changed",
        "Configuration changed",
        "配置已变更"
    ),
    aster_drive_storage::storage_connector_message!("azure_cors_yes", "Yes", "是"),
    aster_drive_storage::storage_connector_message!("azure_cors_no", "No", "否"),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_account_key",
        "Storage Account Key",
        "存储账户密钥",
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_account_name",
        "Storage Account Name",
        "存储账户名",
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_endpoint_hint",
        "Enter the Azure Blob service endpoint or an HTTPS custom domain. Enter the container name separately in the bucket field.",
        "填写 Azure Blob 服务地址或 HTTPS 自定义域名；容器名称单独填写在 bucket 字段。",
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_endpoint_protocol_required_error",
        "Azure Blob endpoint must include http:// or https://.",
        "Azure Blob endpoint 必须包含 http:// 或 https://。",
    ),
    aster_drive_storage::storage_connector_message!(
        "driver_type_azure_blob",
        "Azure Blob",
        "Azure Blob",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_azure_blob_desc",
        "Azure Blob policies use the storage account key to issue short-lived SAS URLs. Test the connection before saving; blank secret fields keep the current credentials.",
        "Azure Blob 策略使用存储账户密钥签发短期 SAS URL；保存前建议测试连接，留空密钥字段会保留现有凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_azure_blob_helper",
        "Connection tests and upload strategy are available after the basic connection is filled in. Large files map to Azure Block Blob uploads.",
        "基础连接填好后，可以测试连接并选择上传策略；大文件会映射到 Azure Block Blob 分块上传。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_azure_blob_storage_desc",
        "Store files in an Azure Blob container with native SAS URLs and Block Blob multipart uploads.",
        "文件存入 Azure Blob 容器，使用原生 SAS URL 和 Block Blob 分块上传。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_container_required",
        "Container is required for Azure Blob storage policies.",
        "Azure Blob 存储策略必须填写容器。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_azure_blob_connection_desc",
        "Set the Azure Blob endpoint, container name, storage account name, and key.",
        "填写 Azure Blob endpoint、容器名称、存储账户名和密钥。",
    ),
];
