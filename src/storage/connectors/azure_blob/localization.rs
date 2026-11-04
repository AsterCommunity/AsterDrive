use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "azure_cors_action",
        "Auto configure Azure Blob CORS",
        "自动配置 Azure Blob CORS"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_cors_description",
        "Changes Blob service CORS for the entire storage account currently used by this policy, affecting all its containers. Allowed origins: {{public_site_url}}. Other service properties and existing rules are preserved. CDN and Front Door require separate configuration.",
        "修改当前使用的整个存储账户的 Blob service CORS，影响该账户所有容器。允许来源：{{public_site_url}}。保留其他服务属性和现有规则。CDN 和 Front Door 需单独配置。"
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
        "Enter the Azure Blob service endpoint, for example https://<account>.blob.core.windows.net. Use the bucket field for the container name.",
        "填写 Azure Blob 服务 endpoint，例如 https://<account>.blob.core.windows.net；bucket 字段填写容器名称。",
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
