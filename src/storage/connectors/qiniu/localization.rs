use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "driver_type_qiniu",
        "Qiniu Kodo",
        "七牛云 Kodo"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_storage_desc",
        "Store files through native Kodo upload, management and private download APIs.",
        "使用 Kodo 原生上传、管理和私有下载 API 存储文件。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_qiniu_helper",
        "Use the native bucket name and a bound HTTPS origin domain. Upload and management endpoints are discovered separately.",
        "填写原生空间名与已绑定的 HTTPS 源站域名；上传和管理服务入口分别自动发现。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_qiniu_connection_desc",
        "Configure native bucket, origin domain, optional CDN delivery, prefix and AccessKey credentials.",
        "配置原生空间名、源站域名、可选 CDN 交付、对象前缀和 AccessKey 凭据。"
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_edit_context_qiniu_desc",
        "Native Kodo storage; existing object keys and prefixes stay unchanged.",
        "Kodo 原生存储；保留已有对象 key 和前缀。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_native_bucket",
        "Native Kodo bucket name",
        "Kodo 原生空间名"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_native_bucket_desc",
        "Use the ordinary bucket name, not the S3 space alias. Legacy policies are verified and converted online at startup.",
        "填写普通空间名称，不是 S3 空间别名。旧策略在启动升级时联网验证并自动转换。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_bucket_required",
        "Native Kodo bucket name is required.",
        "Kodo 原生空间名不能为空。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_origin",
        "Kodo origin download domain",
        "Kodo 源站下载域名"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_origin_desc",
        "Bound HTTPS Kodo origin domain used for internal GET/Range reads. Download domains do not provide upload services.",
        "已绑定 HTTPS 的 Kodo 源站域名，供内部 GET/Range 读取。下载域名不承担上传服务。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_delivery",
        "Private download delivery",
        "私有下载交付"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_delivery_origin",
        "Kodo origin",
        "Kodo 源站"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_delivery_cdn",
        "Qiniu CDN with private-bucket authentication",
        "七牛 CDN（私有空间鉴权）"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_cdn_domain",
        "Qiniu CDN domain",
        "七牛 CDN 域名"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_cdn_domain_desc",
        "Enable private-bucket origin authentication in Qiniu CDN. Native e/token links sign the attachment filename; CDN controls MIME and caching. Test authentication on cache HIT and MISS.",
        "在七牛 CDN 开启私有空间回源鉴权，使用原生 e/token 链接并签署附件文件名；MIME 和缓存头由 CDN 决定。分别验收缓存 HIT/MISS 鉴权。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_access_key",
        "Qiniu AccessKey",
        "七牛云 AccessKey"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_secret_key",
        "Qiniu SecretKey",
        "七牛云 SecretKey"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy",
        "Native resumable upload",
        "原生可续传上传"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_server_relay",
        "Stream through server",
        "服务端流式代理"
    ),
    aster_drive_storage::storage_connector_message!(
        "provider_resumable_upload_strategy_frontend_direct",
        "Browser direct resumable",
        "浏览器原生可续传直传"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_https_required",
        "Use an HTTPS origin without a path or query.",
        "请填写不含路径和查询参数的 HTTPS 源站地址。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_upload_relay_desc",
        "Stream file bytes through the server with bounded buffers and native resumable parts.",
        "通过服务器有界缓冲流式转发，使用原生可续传分片。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_upload_direct_desc",
        "Upload concurrent native parts from the browser using short-lived object-scoped credentials; configure CORS.",
        "浏览器使用对象级短期凭证并发上传原生分片；需要配置 CORS。"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_validity_seconds",
        "Download signature validity (seconds)",
        "下载签名有效期（秒）"
    ),
    aster_drive_storage::storage_connector_message!(
        "qiniu_download_auth_validity_seconds_desc",
        "1–3600 seconds, default 180. Applies to server-side origin reads and private origin/CDN URLs. The effective expiry is the minimum of the requested lifetime, this setting and the target limit; default Kodo source domains are capped at 300 seconds. Existing transfers are not cut off by this signing lifetime.",
        "1–3600 秒，默认 180 秒。用于服务端源站读取和源站/CDN 私有链接。实际有效期取调用方要求、本设置和入口限制的最小值；默认 Kodo 源站最多 300 秒。此签名有效期不会截断已建立的传输。"
    ),
];
