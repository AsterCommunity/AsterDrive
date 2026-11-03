use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_protocol_error",
        "Front Door download URL must use https://.",
        "Front Door 下载地址必须使用 https://。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_delivery",
        "Download delivery",
        "下载入口"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_origin",
        "Blob origin (service SAS)",
        "Blob 源站（service SAS）"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_front_door",
        "Front Door Standard/Premium (client SAS, no cache)",
        "Front Door Standard/Premium（客户端 SAS、不缓存）"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_delivery_hint",
        "Applies to presigned downloads only. Front Door must forward the original container/blob path and complete SAS query over HTTPS with caching disabled. Private Link is not user authentication.",
        "仅作用于 presigned 下载。Front Door 必须通过 HTTPS 转发原始容器/对象路径及完整 SAS query，并关闭缓存；Private Link 不代替用户鉴权。"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_base_url",
        "Front Door download origin URL",
        "Front Door 下载域名"
    ),
    aster_drive_storage::storage_connector_message!(
        "azure_blob_download_base_url_hint",
        "HTTPS origin only, e.g. https://files.example.com, without a path or SAS. Configure the Blob origin host header, empty origin path, no path rewrite, no SAS injection and no caching. Connection tests only verify Blob API.",
        "仅填 HTTPS 域名，例如 https://files.example.com，不带路径或 SAS。云端设置 Blob 回源 Host、空回源路径、不改写路径、不注入 SAS、关闭缓存；连接测试只检查 Blob API。"
    ),
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
