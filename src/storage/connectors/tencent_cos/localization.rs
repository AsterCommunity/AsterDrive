use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "cos_endpoint_hint",
        "For Tencent COS, use the bucket domain such as https://<bucket-appid>.cos.<region>.myqcloud.com; enter the bucket separately, for example <bucket-appid>.",
        "腾讯云 COS 请填写 bucket 域名，例如 https://<bucket-appid>.cos.<region>.myqcloud.com；bucket 请单独填写，例如 <bucket-appid>。",
    ),
    aster_drive_storage::storage_connector_message!(
        "driver_type_tencent_cos",
        "Tencent COS",
        "腾讯云 COS",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_action",
        "Auto configure COS CORS",
        "自动配置 COS CORS",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_desc",
        "Write AsterDrive's CORS rule to the Tencent COS bucket. AllowedOrigin values come from the system public_site_url setting, while other rules are preserved.",
        "向腾讯云 COS bucket 写入 AsterDrive 的 CORS 规则。AllowedOrigin 来自系统 public_site_url 配置，其他规则会保留。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_transition_from_s3",
        "Use the Tencent COS connector",
        "切换到腾讯云 COS connector",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_connector_transition_from_s3_desc",
        "Keep the current COS bucket and object prefix while replacing generic S3 semantics with Tencent COS capabilities. Existing objects are verified before a saved policy is changed.",
        "保留当前 COS bucket 和对象前缀，将通用 S3 语义切换为腾讯云 COS 能力。保存态策略变更前会验证现有对象。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_bucket_required",
        "Bucket is required for object storage policies.",
        "对象存储策略必须填写 bucket。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_tencent_cos_connection_desc",
        "Set the Tencent COS bucket domain, bucket, and credentials.",
        "填写腾讯云 COS bucket 域名、bucket 和访问凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_tencent_cos_helper",
        "Connection tests and upload strategy are available after the basic connection is filled in. COS CI preview links are signed by the backend per object.",
        "基础连接填好后，可以在下一步测试连接并选择上传策略；COS 数据万象预览会由后端按对象签名生成。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_tencent_cos_storage_desc",
        "Store files in Tencent Cloud COS and enable storage-native document preview through COS CI.",
        "文件存入腾讯云 COS，并可使用 COS 数据万象进行原生文档预览。",
    ),
    aster_drive_storage::storage_connector_message!(
        "tencent_cos_secret_id",
        "Tencent COS SecretId",
        "腾讯云 COS SecretId",
    ),
    aster_drive_storage::storage_connector_message!(
        "tencent_cos_secret_key",
        "Tencent COS SecretKey",
        "腾讯云 COS SecretKey",
    ),
];
