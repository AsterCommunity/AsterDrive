use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_action",
        "Configure CORS",
        "配置 CORS",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_desc",
        "Add the current public site origins to this OBS bucket's CORS rules. Existing rules are preserved; an equivalent rule is not written again. This does not configure CDN response headers.",
        "将当前公开站点来源加入此 OBS bucket 的 CORS 规则。保留已有规则，等价规则不会重复写入；不会配置 CDN 响应头。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_output_bucket",
        "Bucket (scope)",
        "Bucket（配置范围）",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_output_origins",
        "Allowed origins",
        "允许的来源",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_output_preserved_rules",
        "Preserved rules",
        "保留规则数",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_output_request_id",
        "OBS request ID",
        "OBS 请求 ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_cors_output_changed",
        "Changed",
        "是否变更",
    ),
    aster_drive_storage::storage_connector_message!("policy_obs_cors_output_yes", "Yes", "是",),
    aster_drive_storage::storage_connector_message!("policy_obs_cors_output_no", "No", "否",),
    aster_drive_storage::storage_connector_message!(
        "driver_type_huawei_obs",
        "Huawei Cloud OBS",
        "华为云 OBS",
    ),
    aster_drive_storage::storage_connector_message!(
        "huawei_obs_endpoint_hint",
        "For virtual-hosted addressing, use the regional OBS endpoint such as https://obs.cn-north-4.myhuaweicloud.com. For a bound custom domain, enter that domain and enable CNAME mode.",
        "虚拟托管模式请填写区域 OBS endpoint，例如 https://obs.cn-north-4.myhuaweicloud.com；使用已绑定的自定义域名时，请填写该域名并开启 CNAME 模式。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_access_key_id",
        "Huawei Cloud Access Key ID",
        "华为云 Access Key ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_use_cname",
        "Use a custom domain (CNAME)",
        "使用自定义域名（CNAME）",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_use_cname_desc",
        "Use Endpoint as an OBS-bound custom domain without adding the bucket to the request host or path. Applies to reads, writes, multipart, and presigned URLs. Configure the domain in OBS first; this does not enable CDN authentication.",
        "将 Endpoint 作为已绑定 OBS 的自定义域名，不向请求主机名或路径追加 bucket。适用于读写、分片和预签名 URL。请先在 OBS 配置域名；此开关不会启用 CDN 鉴权。",
    ),
    aster_drive_storage::storage_connector_message!("obs_region", "OBS region", "OBS 区域",),
    aster_drive_storage::storage_connector_message!(
        "obs_region_desc",
        "Required for regional OBS endpoints and checked against the endpoint host, for example cn-north-4. It may be empty for a custom domain.",
        "区域 OBS endpoint 必填，并会与 endpoint 主机名核对，例如 cn-north-4；自定义域名模式可以留空。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_secret_access_key",
        "Huawei Cloud Secret Access Key",
        "华为云 Secret Access Key",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_bucket_required",
        "Bucket is required for object storage policies.",
        "对象存储策略必须填写 bucket。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_huawei_obs_helper",
        "Connection tests validate native OBS requests and local CDN configuration. Uploads and backend operations use OBS; optional CDN delivery applies only to presigned downloads. Test CDN authentication and private-origin access separately.",
        "连接测试验证 OBS 原生请求及本地 CDN 配置。上传和后端操作使用 OBS，可选 CDN 交付仅用于 presigned 下载；CDN 鉴权和私有回源需另行验收。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_promote_from_s3_desc",
        "This generic S3 policy already uses an official HTTPS Huawei OBS endpoint and an explicit signing region. Promote the saved policy in place without copying objects.",
        "这个通用 S3 策略已经使用华为云官方 HTTPS OBS endpoint 和明确签名区域。可以就地提升已保存策略，无需复制对象。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_obs_promote_from_s3_confirm",
        "The bucket, base path, endpoint, and signing region remain bound to the same object namespace. Huawei OBS addressing and signing become connector-owned.",
        "bucket、基础路径、endpoint 和签名区域继续绑定同一对象 namespace；华为 OBS 的寻址和签名改由 connector 管理。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_huawei_obs_storage_desc",
        "Store files in Huawei Cloud OBS with native OBS authentication and optional custom-domain addressing.",
        "使用华为 OBS 原生认证和可选自定义域名访问，将文件存入华为云 OBS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "policy_wizard_step_huawei_obs_connection_desc",
        "Set the OBS endpoint, bucket, region or custom-domain behavior, and access credentials.",
        "填写 OBS endpoint、bucket、区域或自定义域名行为及访问凭证。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_download_delivery",
        "Download delivery mode",
        "下载交付方式",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_download_delivery_desc",
        "Choose OBS origin for native OBS downloads, or Huawei CDN Type A with SHA-256 for private CDN delivery. This only applies to presigned downloads; backend I/O and uploads still use OBS.",
        "选择 OBS 源站直连（原生 OBS 签名），或华为云 CDN Type A / SHA-256 私有交付。仅对 presigned 下载生效，后端 I/O 和上传仍使用 OBS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_download_origin",
        "OBS origin (native OBS signature)",
        "OBS 源站直连（原生 OBS 签名）",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_download_cdn",
        "Huawei CDN accelerated download (Type A + SHA-256)",
        "华为云 CDN 加速下载（Type A + SHA-256）",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_https_required_error",
        "Huawei CDN domain must use HTTPS (https://).",
        "华为云 CDN 域名必须使用 HTTPS（https://）。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_download_base_url_desc",
        "Required HTTPS domain for Huawei CDN Type A / SHA-256 downloads, without a path prefix. Blob downloads and safe previews can use CDN; strict response-header requests use relay. Backend operations continue to use OBS.",
        "华为云 CDN Type A / SHA-256 下载模式必填的 HTTPS 域名，不包含路径前缀。Blob 下载和安全预览可以使用 CDN，严格要求响应头的请求使用中继；后端操作继续使用 OBS。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_validity_seconds",
        "CDN authentication validity (seconds)",
        "CDN 鉴权有效时长（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_validity_seconds_desc",
        "Must match the URL authentication validity in the Huawei CDN console. Default 300 seconds, range 1–31536000; links are capped by this value and the caller's lifetime. This does not control CDN cache retention.",
        "必须与华为云 CDN 控制台的鉴权 URL 有效时长一致，默认 300 秒，范围 1–31536000；链接有效期不超过该值及调用方请求时长，不控制 CDN 缓存保留时间。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_param",
        "CDN authentication parameter",
        "CDN 鉴权参数名",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_param_desc",
        "Must match the Huawei CDN console, default auth_key. Use 1–100 ASCII letters, digits or underscores, starting with a letter.",
        "必须与华为云 CDN 控制台一致，默认 auth_key。长度 1–100 位，以英文字母开头，仅允许英文字母、数字和下划线。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_key",
        "CDN authentication key",
        "CDN 鉴权密钥",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_cdn_auth_key_desc",
        "Required for Huawei CDN delivery. Use the Type A primary or backup key from the Huawei CDN console: 6–32 ASCII letters or digits. Encrypted at rest, independently of OBS AK/SK. Enable SHA-256 and authentication for all files in the console.",
        "选择华为云 CDN 交付时必填。使用华为云 CDN 控制台 Type A 的主 KEY 或备 KEY，6–32 位英文字母或数字；加密保存，与 OBS AK/SK 相互独立。控制台须选择 SHA-256 并对所有文件启用鉴权。",
    ),
];
