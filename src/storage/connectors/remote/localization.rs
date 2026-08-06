use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("driver_type_remote", "Remote", "远程节点"),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_remote_desc",
        "Remote policies transfer through the bound node. Adjust paths, node binding, and upload rules below.",
        "远程策略由绑定节点负责传输；这里调整路径、节点和上传规则。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_remote_helper",
        "The selected remote node handles network transport. This policy controls the remote path prefix, upload mode, and size limits.",
        "实际网络传输由远程节点负责，这个策略控制远端路径前缀、上传方式和大小限制。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_remote_node_required",
        "Choose a remote node before continuing.",
        "继续前必须选择一个远程节点。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_remote_storage_target_required",
        "Choose a remote storage target before continuing.",
        "继续前必须选择一个远程存储目标。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_remote_storage_desc",
        "Store files on another AsterDrive node over the internal remote storage protocol. Good for tiered or federated deployments.",
        "通过内部远程存储协议把文件写入另一台 AsterDrive 节点，适合分层或多节点部署。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_remote_desc",
        "Choose the remote node that will store objects for this policy.",
        "选择这个策略要写入的远程节点。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_remote_title",
        "Bind Remote Node",
        "绑定远程节点",
    ),
    aster_drive_storage::storage_connector_message!(
        "remote_download_strategy",
        "Remote Download Strategy",
        "远程下载方式",
    ),
    aster_drive_storage::storage_connector_message!("remote_node_id", "Remote node", "远程节点",),
    aster_drive_storage::storage_connector_message!(
        "remote_storage_target_key",
        "Remote storage target",
        "远程存储目标",
    ),
    aster_drive_storage::storage_connector_message!(
        "remote_upload_strategy",
        "Remote Upload Strategy",
        "远程上传方式",
    ),
];
