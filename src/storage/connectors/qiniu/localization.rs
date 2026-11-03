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
        "policy_qiniu_promote_from_s3_desc",
        "This generic S3 policy already uses an official HTTPS Qiniu Kodo endpoint and explicit SigV4 region. Switch the draft or promote the saved policy in place without copying objects.",
        "这个通用 S3 策略已经使用七牛云 Kodo 官方 HTTPS endpoint 和显式 SigV4 区域。可以切换草稿，或将已保存策略就地提升，无需复制对象。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_qiniu_promote_from_s3_confirm",
        "The Qiniu S3 space name, base path, endpoint, and signing region remain bound to the same object namespace. Addressing becomes connector-owned.",
        "七牛 S3 空间名、基础路径、endpoint 和签名区域继续绑定同一对象 namespace；寻址方式改由 connector 管理。",
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
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_base_url",
        "Download delivery URL",
        "下载交付地址",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_base_url_protocol_error",
        "Qiniu download delivery URL must use HTTPS.",
        "七牛下载交付地址必须使用 HTTPS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_base_url_desc",
        "Optional HTTPS source or CDN domain bound to this Kodo space, with an optional path prefix. AsterDrive uses it only for browser downloads; reads, uploads, multipart operations, cleanup, and connection tests keep using the Kodo S3 endpoint. Configure DNS/CNAME and an HTTPS certificate first. Qiniu source domains do not include the S3 space name in the path.",
        "绑定到该 Kodo 空间的可选 HTTPS 源站或 CDN 域名，可带路径前缀。AsterDrive 仅将其用于浏览器下载；读写、分片上传、清理和连接测试仍使用 Kodo S3 endpoint。请先配置 DNS/CNAME 与 HTTPS 证书。七牛源站域名的路径中不包含 S3 空间名。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_mode",
        "Download authentication",
        "下载鉴权方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_private",
        "Kodo private download token",
        "Kodo 私有下载凭证",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_private_desc",
        "Append Qiniu's native e/token parameters, signed with this policy's SecretKey. Use for a private Kodo source domain or a CDN domain that preserves the download token.",
        "追加七牛原生 e/token 参数，并使用该策略的 SecretKey 签名。适用于私有 Kodo 源站域名，或会保留下载凭证的 CDN 域名。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_timestamp",
        "Qiniu CDN timestamp anti-leech",
        "七牛 CDN 时间戳防盗链",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_timestamp_desc",
        "Append the CDN sign/t parameters calculated with a separately configured timestamp key. Use only for a Qiniu CDN acceleration domain whose timestamp anti-leech key matches the saved credential.",
        "使用单独配置的时间戳密钥生成 CDN sign/t 参数。仅适用于已配置匹配时间戳防盗链密钥的七牛 CDN 加速域名。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_public",
        "Public objects (explicit)",
        "公开对象（显式）",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_public_desc",
        "Append no download credentials or response overrides. Configure this only when the bound domain exposes public objects; AsterDrive does not silently make private objects public.",
        "不追加下载凭证或响应头覆盖参数。仅当绑定域名公开对象时配置；AsterDrive 不会把私有对象静默公开。",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_cdn_timestamp_key",
        "CDN timestamp anti-leech key",
        "CDN 时间戳防盗链密钥",
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_cdn_timestamp_key_desc",
        "Required only when download authentication is Qiniu CDN timestamp anti-leech. Stored with the policy's encrypted static credentials and never returned in plaintext.",
        "仅当下载鉴权为七牛 CDN 时间戳防盗链时需要。随策略的加密静态凭据保存，不会明文返回。",
    ),
];
