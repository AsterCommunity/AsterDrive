use aster_drive_storage::StorageConnectorLocalizationMessage;
pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "driver_type_google_drive",
        "Google Drive",
        "Google Drive"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_google_drive_storage_desc",
        "Administrator-managed Google Drive blob storage",
        "管理员托管的 Google Drive blob 存储"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_google_drive_helper",
        "Google Drive is used only as the server-side blob backend.",
        "Google Drive 仅作为服务端 blob 存储后端。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_google_drive_title",
        "Google Drive",
        "Google Drive"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_google_drive_desc",
        "Configure the managed Google Drive storage pool.",
        "配置托管的 Google Drive 存储池。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_google_drive_desc",
        "Server-side Google Drive storage policy.",
        "服务端 Google Drive 存储策略。"
    ),
    aster_drive_storage::storage_connector_message!(
        "root_folder_id",
        "Root folder ID",
        "根目录 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "client_id",
        "OAuth client ID",
        "OAuth 客户端 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "client_secret",
        "OAuth client secret",
        "OAuth 客户端密钥"
    ),
    aster_drive_storage::storage_connector_message!("scopes", "OAuth scopes", "OAuth scope"),
    aster_drive_storage::storage_connector_message!(
        "shared_drive_id",
        "Shared drive ID",
        "共享盘 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "app_data_folder",
        "Use appDataFolder",
        "使用 appDataFolder"
    ),
    aster_drive_storage::storage_connector_message!(
        "chunk_size",
        "Upload chunk size",
        "上传分片大小"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy",
        "Upload strategy",
        "上传策略"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy",
        "Download strategy",
        "下载策略"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_title",
        "Google Drive authorization",
        "Google Drive 授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_loading",
        "Loading Google Drive authorization",
        "正在加载 Google Drive 授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_authorized",
        "Authorized",
        "已授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_missing",
        "Not authorized",
        "未授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_reauth_required",
        "Reauthorization required",
        "需要重新授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_permission_denied",
        "Permission denied",
        "权限不足"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_invalid",
        "Invalid",
        "无效"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_reauthorize_action",
        "Reauthorize",
        "重新授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization",
        "Start authorization",
        "Start authorization"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization_desc",
        "Open the connector authorization flow for this saved storage policy.",
        "Open the connector authorization flow for this saved storage policy."
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential",
        "Validate credential",
        "Validate credential"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential_desc",
        "Ask the connector to validate the credential saved for this storage policy.",
        "Ask the connector to validate the credential saved for this storage policy."
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_validated_at",
        "Validated {{time}}",
        "Validated {{time}}"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_server_relay",
        "Server Relay Stream",
        "Server Relay Stream"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_server_relay_desc",
        "The browser uploads to AsterDrive, which writes the file to Google Drive.",
        "The browser uploads to AsterDrive, which writes the file to Google Drive."
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_server_relay",
        "Server Relay Stream",
        "Server Relay Stream"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_server_relay_desc",
        "AsterDrive reads the file from Google Drive and returns it to the browser.",
        "AsterDrive reads the file from Google Drive and returns it to the browser."
    ),
];
