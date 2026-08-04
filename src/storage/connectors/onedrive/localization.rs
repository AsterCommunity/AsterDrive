use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("account_mode", "Account mode", "账户模式",),
    aster_drive_storage::storage_connector_message!(
        "client_id",
        "Application (client) ID",
        "应用程序（客户端）ID",
    ),
    aster_drive_storage::storage_connector_message!("client_secret", "Client secret", "客户端密钥"),
    aster_drive_storage::storage_connector_message!(
        "cloud",
        "Microsoft Graph cloud",
        "Microsoft Graph 云",
    ),
    aster_drive_storage::storage_connector_message!("drive_id", "Drive ID", "Drive ID"),
    aster_drive_storage::storage_connector_message!("driver_type_onedrive", "OneDrive", "OneDrive",),
    aster_drive_storage::storage_connector_message!(
        "group_id",
        "Microsoft 365 group ID",
        "Microsoft 365 组 ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_account_mode_group_drive",
        "Microsoft 365 group drive",
        "Microsoft 365 组 drive",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_account_mode_personal",
        "Personal OneDrive",
        "个人 OneDrive",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_account_mode_sharepoint_site",
        "SharePoint site drive",
        "SharePoint 站点 drive",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_account_mode_work_or_school",
        "Work or school OneDrive",
        "工作或学校 OneDrive",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_cloud_china",
        "China (21Vianet)",
        "中国版（世纪互联）",
    ),
    aster_drive_storage::storage_connector_message!("onedrive_cloud_global", "Global", "国际版",),
    aster_drive_storage::storage_connector_message!(
        "onedrive_authorization_started",
        "Microsoft authorization opened",
        "已打开 Microsoft 授权",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_loading",
        "Loading",
        "加载中",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_authorized",
        "Authorized",
        "已授权",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_invalid",
        "Invalid",
        "无效",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_missing",
        "Not authorized",
        "未授权",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_permission_denied",
        "Permission denied",
        "权限被拒绝",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_reauth_required",
        "Reauthorization required",
        "需要重新授权",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_status_revoked",
        "Revoked",
        "已撤销",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_credential_title",
        "Microsoft Graph credential",
        "Microsoft Graph 凭据",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_redirect_uri",
        "Redirect URI",
        "重定向 URL",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_save_before_authorize",
        "Save OneDrive policy changes before starting authorization.",
        "开始授权前请先保存 OneDrive 策略更改。",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_save_before_validate",
        "Save OneDrive policy changes before validating the credential.",
        "验证凭据前请先保存 OneDrive 策略更改。",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_validation_success",
        "OneDrive credential validated",
        "OneDrive 凭据验证成功",
    ),
    aster_drive_storage::storage_connector_message!(
        "onedrive_validation_success_root",
        "Root: {{name}}",
        "Root：{{name}}",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_created_authorize_next",
        "OneDrive policy created. Authorize Microsoft Graph next.",
        "OneDrive 策略已创建。下一步授权 Microsoft Graph。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization",
        "Start authorization",
        "开始授权",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_start_authorization_desc",
        "Open the connector authorization flow for this saved storage policy.",
        "为这个已保存的存储策略启动 connector 授权流程。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential",
        "Validate credential",
        "验证凭据",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_validate_credential_desc",
        "Ask the connector to validate the credential saved for this storage policy.",
        "让 connector 验证这个存储策略已保存的凭据。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_onedrive_desc",
        "OneDrive policies use saved Microsoft Graph OAuth credentials. Save target changes before starting authorization.",
        "OneDrive 策略使用已保存的 Microsoft Graph OAuth 凭据。开始授权前请先保存目标配置。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_onedrive_helper",
        "Enter the Microsoft application client ID while creating the policy. After saving, the dialog switches to editing so you can authorize Microsoft Graph directly. Choose Global for microsoft.com tenants and China for 21Vianet tenants.",
        "创建时填写 Microsoft 应用 Client ID；保存后会直接进入编辑态授权 Microsoft Graph。microsoft.com 租户选国际版，世纪互联租户选中国版。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_onedrive_storage_desc",
        "Store files in OneDrive, SharePoint document libraries, or Microsoft 365 group drives through Microsoft Graph.",
        "通过 Microsoft Graph 把文件存入 OneDrive、SharePoint 文档库或 Microsoft 365 组 drive。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_onedrive_desc",
        "Choose the Microsoft Graph cloud, then authorize with Microsoft sign-in after saving.",
        "选择 Microsoft Graph 云端点，保存后使用 Microsoft 登录授权。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_onedrive_title",
        "Configure Microsoft Graph",
        "配置 Microsoft Graph",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_filename_mode",
        "Download filename",
        "下载文件名",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_filename_mode_provider_native",
        "Prefer the OneDrive filename",
        "优先使用 OneDrive 文件名",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_filename_mode_provider_native_desc",
        "Prefer direct OneDrive downloads. After a rename, the older filename stored in OneDrive may still be used.",
        "优先直接从 OneDrive 下载，文件重命名后可能仍使用 OneDrive 中保存的旧文件名。",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_filename_mode_strict_current",
        "Always use the AsterDrive filename",
        "严格使用 AsterDrive 文件名",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_filename_mode_strict_current_desc",
        "Always use the filename shown by AsterDrive. AsterDrive uses relay streaming when the names differ.",
        "始终使用 AsterDrive 中显示的文件名；名称不一致时使用代理流式下载。",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy",
        "OneDrive Download Strategy",
        "OneDrive 下载方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_frontend_direct",
        "Direct from Microsoft Graph",
        "Microsoft Graph 直接下载",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_frontend_direct_desc",
        "After AsterDrive checks access, the browser is redirected to a short-lived preauthenticated Microsoft Graph download URL, reducing AsterDrive download bandwidth. New OneDrive policies use this by default.",
        "AsterDrive 完成权限检查后，浏览器直接从 Microsoft Graph 提供的短期预认证地址下载，减少 AsterDrive 的下载带宽。新建 OneDrive 策略默认使用此方式。",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_server_relay",
        "Server Relay Stream",
        "服务端流式中继",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_download_strategy_server_relay_desc",
        "AsterDrive reads the file from Microsoft Graph and returns it to the browser. It is the most compatible path; existing policies keep their saved choice.",
        "AsterDrive 从 Microsoft Graph 拉取文件并返回给浏览器。兼容性最好；已有策略会继续保留原来的选择。",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy",
        "OneDrive Upload Strategy",
        "OneDrive 上传方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_frontend_direct",
        "Direct to Microsoft Graph",
        "Microsoft Graph 直传",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_frontend_direct_desc",
        "The browser uploads sequential ranges through a temporary Graph upload URL, so AsterDrive does not carry the file bytes. Microsoft must allow CORS preflight for PUT and Content-Range; use server relay when it does not.",
        "浏览器通过临时 Graph upload URL 顺序上传分片，AsterDrive 不承载文件流量。需要 Microsoft 端点允许 PUT 和 Content-Range 的跨域预检；不满足时请使用服务端中继。",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_server_relay",
        "Server Relay Stream",
        "服务端流式中继",
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_server_relay_desc",
        "The browser uploads to AsterDrive, which writes the file to Microsoft Graph. This is the default and most compatible path.",
        "浏览器把文件上传到 AsterDrive，再由服务端写入 Microsoft Graph。兼容性最好，也是默认方式。",
    ),
    aster_drive_storage::storage_connector_message!("root_item_id", "Root item ID", "根项目 ID",),
    aster_drive_storage::storage_connector_message!("scopes", "OAuth scopes", "OAuth 权限范围"),
    aster_drive_storage::storage_connector_message!(
        "site_id",
        "SharePoint site ID",
        "SharePoint 站点 ID",
    ),
    aster_drive_storage::storage_connector_message!("tenant", "Microsoft tenant", "Microsoft 租户",),
];
