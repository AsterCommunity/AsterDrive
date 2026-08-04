use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!("driver_type_s3", "S3", "S3"),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_s3_storage_desc",
        "Store files in an S3-compatible object bucket such as Amazon S3, MinIO, or RustFS.",
        "文件存入兼容 S3 的对象存储，例如 Amazon S3、MinIO 或 RustFS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_access_key_id",
        "S3 Access Key ID",
        "S3 Access Key ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_connect_timeout_secs",
        "Connect timeout (seconds)",
        "连接超时（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_endpoint_hint",
        "Enter the S3-compatible API endpoint and keep the bucket in the bucket field. Providers differ on path-style support, so test the connection before saving.",
        "填写兼容 S3 的 API endpoint，bucket 请单独填写。不同厂商对 path-style 的要求不同，保存前请先测试连接。",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_operation_timeout_secs",
        "Operation timeout (seconds)",
        "操作超时（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_path_style",
        "Path-style addressing",
        "Path-style 访问",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_path_style_desc",
        "When enabled, requests use /bucket/key URLs for MinIO, RustFS, and similar compatible services. Services that support virtual-hosted-style addressing can usually disable this and use bucket.endpoint/key URLs.",
        "开启后生成 /bucket/key 形式的请求，适合 MinIO、RustFS 等兼容服务；支持虚拟托管风格的服务通常可以关闭，改用 bucket.endpoint/key。",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_read_timeout_secs",
        "Read timeout (seconds)",
        "读取超时（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_region",
        "S3 signing region",
        "S3 签名区域",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_region_desc",
        "The SigV4 signing region, such as us-east-1. Leave blank to use auto. For custom endpoints that require a fixed region, use the value provided by the service.",
        "用于 SigV4 请求签名，例如 us-east-1。留空时使用 auto；自定义 endpoint 若要求固定区域，请填写服务商提供的 region。",
    ),
    aster_drive_storage::storage_connector_message!(
        "s3_secret_access_key",
        "S3 Secret Access Key",
        "S3 Secret Access Key",
    ),
];
