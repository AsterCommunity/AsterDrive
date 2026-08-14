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
        "Enter the Kodo S3-compatible endpoint, signing region, bucket, and addressing style supplied for this storage space.",
        "填写该存储空间使用的 Kodo S3 兼容 endpoint、签名区域、bucket 和寻址方式。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_qiniu_connection_desc",
        "Set the Qiniu Kodo S3-compatible endpoint, bucket, SigV4 region, optional prefix, and AccessKey credentials.",
        "填写七牛云 Kodo S3 兼容 endpoint、bucket、SigV4 区域、可选前缀和 AccessKey 凭据。",
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
        "The HTTP(S) S3-compatible endpoint supplied for this Kodo bucket. Keep the bucket name in the bucket field.",
        "为该 Kodo bucket 提供的 HTTP(S) S3 兼容 endpoint。bucket 名称请单独填写。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_endpoint_protocol_error",
        "Kodo S3 endpoint must use HTTP or HTTPS.",
        "Kodo S3 endpoint 必须使用 HTTP 或 HTTPS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_bucket_required",
        "Qiniu Kodo bucket is required.",
        "七牛云 Kodo bucket 不能为空。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_region",
        "Kodo SigV4 signing region",
        "Kodo SigV4 签名区域",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_region_desc",
        "The SigV4 signing region required by the configured Kodo S3 endpoint, such as cn-east-1.",
        "当前 Kodo S3 endpoint 要求的 SigV4 签名区域，例如 cn-east-1。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_path_style",
        "Path-style addressing",
        "Path-style 寻址",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_s3_path_style_desc",
        "When enabled, requests use /bucket/key URLs. Disable only when the configured endpoint supports virtual-hosted-style bucket URLs.",
        "启用后请求使用 /bucket/key URL。仅当当前 endpoint 支持 virtual-hosted-style bucket URL 时关闭。",
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
