use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "aliyun_oss_access_key_id",
        "Alibaba Cloud AccessKey ID",
        "阿里云 AccessKey ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "aliyun_oss_access_key_secret",
        "Alibaba Cloud AccessKey Secret",
        "阿里云 AccessKey Secret",
    ),
    aster_drive_storage::storage_connector_message!(
        "driver_type_alibaba_oss",
        "Alibaba Cloud OSS",
        "阿里云 OSS",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_public_endpoint",
        "Public endpoint",
        "公网 endpoint",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_public_endpoint_desc",
        "Used for browser-facing presigned URLs and for backend requests when no server-side endpoint is configured. Use an aliyuncs.com OSS endpoint, or enable CNAME mode for a custom domain.",
        "用于浏览器可见的 presigned URL；未配置服务端 endpoint 时也用于后端请求。普通模式填写 aliyuncs.com OSS endpoint，自定义域名则启用 CNAME 模式。",
    ),
    aster_drive_storage::storage_connector_message!("oss_region", "OSS region", "OSS 地域",),
    aster_drive_storage::storage_connector_message!(
        "oss_region_desc",
        "Region used by OSS V4 signing, such as cn-hangzhou. It must match the bucket endpoint.",
        "用于 OSS V4 签名的地域，例如 cn-hangzhou，必须与 bucket endpoint 匹配。",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_server_side_endpoint",
        "Server-side endpoint",
        "服务端 endpoint",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_server_side_endpoint_desc",
        "Optional OSS endpoint used only by AsterDrive backend I/O, for example an internal aliyuncs.com endpoint. Presigned URLs continue to use the public endpoint.",
        "仅供 AsterDrive 后端 I/O 使用的可选 OSS endpoint，例如 aliyuncs.com 内网 endpoint；presigned URL 始终使用公网 endpoint。",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_use_cname",
        "Use CNAME custom domain",
        "使用 CNAME 自定义域名",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_use_cname_desc",
        "Treat the public endpoint as a bucket-bound custom domain. The bucket remains part of the OSS V4 canonical URI but is omitted from the transmitted URL path.",
        "将公网 endpoint 视为绑定到 bucket 的自定义域名。bucket 仍参与 OSS V4 canonical URI，但不会出现在实际 URL path 中。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_alibaba_oss_helper",
        "Connection tests validate OSS V4 request signing. Presigned upload and download URLs always use the public endpoint.",
        "连接测试会验证 OSS V4 请求签名；presigned 上传和下载 URL 始终使用公网 endpoint。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_oss_promote_from_s3_desc",
        "This generic S3 policy already uses a public Alibaba Cloud OSS endpoint and explicit signing region. Switch the draft or promote the saved policy in place to use native OSS V4 signing without copying objects.",
        "这个通用 S3 策略已经使用阿里云 OSS 公网 endpoint 和显式签名地域。可以切换草稿，或将已保存策略就地提升为原生 OSS V4 签名，无需复制对象。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_oss_promote_from_s3_confirm",
        "The bucket and base path remain unchanged. The public endpoint and signing region are preserved; optional server-side endpoint and CNAME mode start with their connector defaults.",
        "bucket 和基础路径保持不变；公网 endpoint 与签名地域会保留，可选服务端 endpoint 和 CNAME 模式使用 connector 默认值。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_alibaba_oss_storage_desc",
        "Store files in Alibaba Cloud Object Storage Service with native OSS V4 signing.",
        "使用原生 OSS V4 签名将文件存入阿里云对象存储 OSS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_alibaba_oss_connection_desc",
        "Set the OSS public endpoint, optional server-side endpoint, region, bucket, and credentials.",
        "填写 OSS 公网 endpoint、可选服务端 endpoint、地域、bucket 和访问凭证。",
    ),
];
