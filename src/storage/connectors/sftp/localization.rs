use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("driver_type_sftp", "SFTP", "SFTP"),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_sftp_desc",
        "SFTP policies stream through the app server and write to the configured remote root. Blank password fields keep the current credential.",
        "SFTP 策略通过应用服务器流式中继，并写入配置的远程根目录；留空密码字段会保留现有凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_sftp_helper",
        "AsterDrive streams uploads and downloads through the app server, then reads and writes files over SFTP using the configured SSH credentials.",
        "AsterDrive 会通过应用服务器流式中继上传和下载，再使用配置的 SSH 凭据通过 SFTP 读写文件。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_sftp_storage_desc",
        "Store files on an SFTP server through server-side streaming. Browsers never connect to SFTP directly.",
        "通过服务端流式中继把文件存入 SFTP 服务器，浏览器不会直接连接 SFTP。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_sftp_desc",
        "Set the SFTP endpoint, SSH username, password, and remote root path.",
        "填写 SFTP endpoint、SSH 用户名、密码和远程根目录。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_sftp_title",
        "Configure SFTP",
        "配置 SFTP",
    ),
    aster_drive_storage::storage_connector_message!(
        "sftp_endpoint_hint",
        "Enter an SFTP server endpoint such as sftp://example.com:22. Put the remote root directory in base path.",
        "填写 SFTP 服务器 endpoint，例如 sftp://example.com:22；远程根目录请填写在基础路径里。",
    ),
    aster_drive_storage::storage_connector_message!(
        "sftp_endpoint_protocol_required_error",
        "SFTP endpoint must use sftp:// or omit the scheme.",
        "SFTP endpoint 必须使用 sftp://，或省略协议。",
    ),
    aster_drive_storage::storage_connector_message!(
        "sftp_host_key_fingerprint",
        "SSH Host Key Fingerprint",
        "SSH 主机密钥指纹",
    ),
    aster_drive_storage::storage_connector_message!(
        "sftp_host_key_fingerprint_hint",
        "Run the connection test once. If the host key is unknown, confirm the reported SHA256 fingerprint here before saving.",
        "先运行一次连接测试。若主机密钥未知，请确认错误信息里的 SHA256 指纹并填到这里再保存。",
    ),
    aster_drive_storage::storage_connector_message!("sftp_password", "SSH Password", "SSH 密码",),
    aster_drive_storage::storage_connector_message!("sftp_username", "SSH Username", "SSH 用户名",),
];
