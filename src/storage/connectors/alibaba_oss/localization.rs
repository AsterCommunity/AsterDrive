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
        "Use a custom domain (CNAME)",
        "使用自定义域名（CNAME）",
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
    aster_drive_storage::storage_connector_message!(
        "oss_download_delivery",
        "Download delivery mode",
        "下载交付方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_download_delivery_desc",
        "Choose OSS origin for native OSS V4 downloads, or Alibaba CDN Type A for private CDN delivery. This only applies to presigned downloads; backend I/O and uploads still use OSS.",
        "选择 OSS 源站直连（原生 OSS V4），或阿里云 CDN Type A 私有交付。仅对 presigned 下载生效，后端 I/O 和上传仍使用 OSS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_download_origin",
        "OSS origin (OSS V4)",
        "OSS 源站直连（OSS V4）",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_download_cdn",
        "Alibaba CDN accelerated download (Type A)",
        "阿里云 CDN 加速下载（Type A）",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_download_base_url_desc",
        "Required HTTPS domain for Alibaba CDN Type A downloads. Blob downloads and safe previews can use CDN; strict response-header requests use relay. Backend operations continue to use OSS.",
        "阿里云 CDN Type A 下载模式必填的 HTTPS 域名。Blob 下载和安全预览可以使用 CDN，严格要求响应头的请求使用中继；后端操作继续使用 OSS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_cdn_auth_validity_seconds",
        "CDN authentication validity (seconds)",
        "CDN 鉴权有效时长（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_cdn_auth_validity_seconds_desc",
        "Must match the Authentication URL Validity Period in the Alibaba CDN console. Default 300 seconds; links are capped by this value and the caller's lifetime. This does not control CDN cache retention.",
        "必须与阿里云 CDN 控制台的鉴权 URL 有效时长一致，默认 300 秒；链接有效期不超过该值及调用方请求时长，不控制 CDN 缓存保留时间。",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_cdn_auth_key",
        "CDN authentication key",
        "CDN 鉴权密钥",
    ),
    aster_drive_storage::storage_connector_message!(
        "oss_cdn_auth_key_desc",
        "Required for Alibaba CDN delivery. Use the Type A primary or backup key from the Alibaba CDN console: 6 to 128 ASCII letters or digits. Encrypted at rest, independently of the OSS AccessKey.",
        "选择阿里云 CDN 交付时必填。使用阿里云 CDN 控制台 Type A 的主 KEY 或备 KEY，6–128 位英文字母或数字；加密保存，与 OSS AccessKey 相互独立。",
    ),
];
