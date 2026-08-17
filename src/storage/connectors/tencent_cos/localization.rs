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
        "policy_cos_promote_from_s3_desc",
        "This generic S3 policy already points at a Tencent COS endpoint. Switch the draft or promote the saved policy in place to enable COS-owned capabilities without copying objects.",
        "这个通用 S3 策略已经指向腾讯云 COS endpoint。可以切换草稿，或将已保存策略就地提升为 COS connector，无需复制对象即可启用 COS 专属能力。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_promote_from_s3_confirm",
        "Only the connector configuration and encrypted credential envelope will change. The bucket, base path, blob ownership, and stored object paths remain unchanged.",
        "只会更新 connector 配置和加密凭据封装；bucket、基础路径、blob 归属和已存对象路径保持不变。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_request_id",
        "Provider request ID",
        "服务商请求 ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_rule_id",
        "Applied rule",
        "已应用规则",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_allowed_origins",
        "Allowed origins",
        "允许来源",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_preserved_rule_count",
        "Preserved rules",
        "保留规则数",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_replaced_existing_rule",
        "Replaced existing AsterDrive rule",
        "已替换现有 AsterDrive 规则",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_cos_cors_output_response_vary",
        "Response varies by origin",
        "响应按来源区分",
    ),
    aster_drive_storage::storage_connector_message!("policy_cos_cors_output_yes", "Yes", "是",),
    aster_drive_storage::storage_connector_message!("policy_cos_cors_output_no", "No", "否",),
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
        "storage_native_thumbnail_enabled_desc",
        "When enabled, only images matching the extensions below are sent to Tencent COS CI image processing for thumbnail generation. Disabling turns off only COS-native thumbnails: AsterDrive's global thumbnail processor chain remains available, and the saved extension list stays dormant until re-enabled. Tencent COS CI image-processing requests may incur cloud-provider charges.",
        "开启后，只有下方后缀匹配的图片才会交给腾讯云 COS 数据万象生成缩略图。关闭只停用 COS 原生缩略图路径，AsterDrive 的全局缩略图处理链仍可继续处理；已保存的后缀列表会作为休眠配置保留，重新开启后恢复使用。COS 数据万象图片处理请求可能产生腾讯云费用。",
    ),
    aster_drive_storage::storage_connector_message!(
        "storage_native_media_metadata_enabled_desc",
        "When enabled, only audio or video matching the extensions below is sent to Tencent COS CI GetMediainfo to parse duration, codecs, bit rate, and stream information. Disabling turns off only COS-native parsing: AsterDrive's global media-information processor chain remains available, and the saved extension list stays dormant until re-enabled. Tencent COS CI media-information requests may incur request charges.",
        "开启后，只有下方后缀匹配的音视频才会交给腾讯云 COS 数据万象 GetMediainfo，解析时长、编码格式、码率和流信息。关闭只停用 COS 原生解析路径，AsterDrive 的全局媒体信息处理链仍可继续处理；已保存的后缀列表会作为休眠配置保留，重新开启后恢复使用。COS 数据万象媒体信息请求可能按请求计费并产生腾讯云费用。",
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
