use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("driver_type_local", "Local", "本机"),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_local_desc",
        "Local policies write directly to the server filesystem. Adjust paths and upload rules below.",
        "本机策略直接写入服务器文件系统；路径和上传规则在下方调整。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_local_helper",
        "Use a relative or absolute path. Leave it empty to fall back to the application's default data directory.",
        "支持相对路径或绝对路径。留空时会回退到应用默认的数据目录。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_local_storage_desc",
        "Store files on the AsterDrive server filesystem. Simple setup and suitable for single-node deployments.",
        "文件直接落在 AsterDrive 所在服务器的文件系统上，配置简单，适合单机部署。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_local_desc",
        "Name the policy and choose the local storage path.",
        "填写策略名称，并设置本机存储路径。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_local_title",
        "Configure Path",
        "配置路径",
    ),
];
