use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "driver_type_qiniu",
        "Qiniu Kodo",
        "七牛云 Kodo"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_storage_desc",
        "Store files in Qiniu Cloud Kodo through its S3-compatible API.",
        "通过七牛云 Kodo 的 S3 兼容 API 存储文件。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_helper",
        "Enter an official Kodo S3 service or S3-space endpoint, matching signing region, and Qiniu S3 space name. AsterDrive selects the addressing style.",
        "填写官方 Kodo S3 服务或 S3 空间 endpoint、匹配的签名区域和七牛 S3 空间名；寻址方式由 AsterDrive 自动选择。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_qiniu_connection_desc",
        "Set an official Qiniu Kodo S3 endpoint, S3 space name, matching SigV4 region, optional prefix, and AccessKey credentials.",
        "填写七牛云 Kodo S3 官方 endpoint、S3 空间名、匹配的 SigV4 区域、可选前缀和 AccessKey 凭据。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_qiniu_desc",
        "Qiniu Kodo S3-compatible object storage configuration.",
        "七牛云 Kodo S3 兼容对象存储配置。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_endpoint",
        "Kodo S3 endpoint",
        "Kodo S3 endpoint",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_endpoint_desc",
        "An official endpoint for the matching region, such as https://s3.cn-east-1.qiniucs.com or https://<S3-space-name>.s3.cn-east-1.qiniucs.com. AsterDrive normalizes both forms and selects the addressing style automatically.",
        "与区域匹配的官方 endpoint，例如 https://s3.cn-east-1.qiniucs.com 或 https://<S3-空间名>.s3.cn-east-1.qiniucs.com。AsterDrive 会规范化两种形式并自动选择寻址方式。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_endpoint_protocol_error",
        "Kodo S3 endpoint must use HTTPS.",
        "Kodo S3 endpoint 必须使用 HTTPS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_bucket",
        "Qiniu S3 space name",
        "七牛 S3 空间名",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_bucket_desc",
        "The globally unique S3 space name shown in the Kodo console or returned by Get Service. It can differ from the ordinary Kodo space name.",
        "在 Kodo 控制台中显示或由 Get Service 返回的全局唯一 S3 空间名；它可能不同于普通 Kodo 空间名称。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_bucket_required",
        "Qiniu S3 space name is required.",
        "七牛 S3 空间名不能为空。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_region",
        "Kodo SigV4 signing region",
        "Kodo SigV4 签名区域",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_region_desc",
        "The Region ID embedded in the official Kodo S3 service endpoint, such as cn-east-1. It must match the endpoint host.",
        "官方 Kodo S3 服务 endpoint 中的 Region ID，例如 cn-east-1；必须与 endpoint 主机名匹配。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_access_key",
        "Qiniu AccessKey",
        "七牛云 AccessKey",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_secret_key",
        "Qiniu SecretKey",
        "七牛云 SecretKey",
    ),
];
