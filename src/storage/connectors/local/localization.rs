use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("driver_type_local", "Local", "本机"),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_local_desc",
        "Filesystem policies write through paths mounted into AsterDrive. Adjust paths and upload rules below.",
        "文件系统策略通过挂载到 AsterDrive 的路径写入；路径和上传规则在下方调整。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_local_helper",
        "Use a relative or absolute path. Leave it empty to fall back to the application's default data directory.",
        "支持相对路径或绝对路径。留空时会回退到应用默认的数据目录。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_local_storage_desc",
        "Store files through a filesystem path. In cluster deployments, every Primary must share the policy path and upload staging directory.",
        "通过文件系统路径保存文件。集群部署必须让所有 Primary 共享策略路径和上传暂存目录。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_local_desc",
        "Name the policy and choose the mounted filesystem path.",
        "填写策略名称，并设置挂载的文件系统路径。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_local_title",
        "Configure Path",
        "配置路径",
    ),
];
