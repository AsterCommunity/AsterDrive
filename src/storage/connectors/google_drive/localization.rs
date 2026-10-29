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
        "Files remain managed by AsterDrive; Google Drive stores the blobs.",
        "文件仍由 AsterDrive 管理，Google Drive 只保存 blob。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_google_drive_title",
        "Google Drive",
        "Google Drive"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_google_drive_desc",
        "Configure an administrator-managed Google Drive storage pool.",
        "配置管理员托管的 Google Drive 存储池。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_google_drive_desc",
        "Google Drive is used as an administrator-managed storage backend.",
        "Google Drive 用作管理员托管的存储后端。"
    ),
    aster_drive_storage::storage_connector_message!(
        "root_folder_id",
        "Root folder ID",
        "根目录 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "root_folder_id_desc",
        "Optional folder ID. Leave empty to use the Drive root.",
        "可选目录 ID。留空使用 Drive 根目录。"
    ),
    aster_drive_storage::storage_connector_message!(
        "shared_drive_id",
        "Shared drive ID",
        "共享盘 ID"
    ),
    aster_drive_storage::storage_connector_message!(
        "shared_drive_id_desc",
        "Optional shared drive ID.",
        "可选共享盘 ID。"
    ),
    aster_drive_storage::storage_connector_message!(
        "app_data_folder",
        "Use appDataFolder",
        "使用 appDataFolder"
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
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_title",
        "Google Drive authorization",
        "Google Drive 授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_loading",
        "Loading",
        "加载中"
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
        "google_drive_reauthorize_action",
        "Authorize again",
        "重新授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_save_before_authorize",
        "Save the policy before authorizing Google Drive.",
        "授权 Google Drive 前请先保存策略。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_authorization_started",
        "Google authorization opened",
        "已打开 Google 授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_save_before_validate",
        "Save the policy before validating credentials.",
        "验证凭据前请先保存策略。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_validation_success",
        "Google Drive authorization is valid",
        "Google Drive 授权有效"
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
        "google_drive_credential_status_revoked",
        "Revoked",
        "已撤销"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_credential_status_invalid",
        "Invalid configuration",
        "配置无效"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_reauth_help",
        "Authorize Google Drive again to restore access.",
        "请重新授权 Google Drive 以恢复访问。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_permission_help",
        "Check Drive folder permissions and the granted OAuth scopes.",
        "请检查 Drive 目录权限和已授予的 OAuth 权限。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_invalid_help",
        "Check the OAuth application, root folder and shared drive IDs.",
        "请检查 OAuth 应用、根目录和共享盘 ID。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_authorized_at",
        "Authorized at",
        "授权时间"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_refreshed_at",
        "Last refreshed",
        "最近刷新"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_validated_at",
        "Last validated",
        "最近验证"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_redirect_uri",
        "OAuth callback URL",
        "OAuth 回调地址"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_redirect_uri_help",
        "Register this exact URL as an authorized redirect URI in your Google OAuth web application.",
        "在 Google OAuth Web 应用中将此完整地址注册为已授权的重定向 URI。"
    ),
    aster_drive_storage::storage_connector_message!(
        "google_drive_copy_redirect_uri",
        "Copy callback URL",
        "复制回调地址"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization",
        "Start authorization",
        "开始授权"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization_desc",
        "Open the Google authorization flow for this saved storage policy.",
        "为这个已保存的存储策略启动 Google 授权流程。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential",
        "Validate credential",
        "验证凭据"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential_desc",
        "Ask Google Drive to validate the credential saved for this storage policy.",
        "让 Google Drive 验证这个存储策略已保存的凭据。"
    ),
];
