use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
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
        "This connector uses native Huawei OBS signatures for normal requests, range reads, multipart operations, and presigned URLs. Generic S3 SigV4 policies remain a separate connector.",
        "此连接器对普通请求、Range 读取、分片操作和预签名 URL 均使用华为 OBS 原生签名；通用 S3 SigV4 策略仍由独立连接器负责。",
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
        "huawei_obs_cdn_download_desc",
        "Optional HTTPS CDN or source domain used only for browser downloads. Reads, uploads, multipart operations, cleanup, and connection tests keep using the OBS endpoint. AsterDrive does not stamp an expiration into the delivered URL; Huawei CDN honors the configured anti-leech validity window.",
        "仅用于浏览器下载的可选 HTTPS CDN 或源站域名。读写、分片上传、清理和连接测试仍使用 OBS endpoint。AsterDrive 不会在交付 URL 中内置过期时间；实际有效期由华为 CDN 配置的防盗链有效期决定。",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_type",
        "Download authentication",
        "下载鉴权方式",
    ),
    aster_drive_storage::storage_connector_message!("cdn_auth_type_a", "Type A", "鉴权方式 A",),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_type_a_desc",
        "Signed query string combining timestamp, random value, user id, and hash.",
        "带时间戳、随机值、用户 ID 与哈希的签名查询串。",
    ),
    aster_drive_storage::storage_connector_message!("cdn_auth_type_b", "Type B", "鉴权方式 B",),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_type_b_desc",
        "Timestamp and hash are inserted into the URL path.",
        "时间戳与哈希嵌入 URL 路径。",
    ),
    aster_drive_storage::storage_connector_message!("cdn_auth_type_c1", "Type C1", "鉴权方式 C1",),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_type_c1_desc",
        "Hash and hexadecimal timestamp are inserted into the URL path.",
        "哈希与十六进制时间戳嵌入 URL 路径。",
    ),
    aster_drive_storage::storage_connector_message!("cdn_auth_type_c2", "Type C2", "鉴权方式 C2",),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_type_c2_desc",
        "Hash and decimal timestamp are appended as query parameters.",
        "哈希与十进制时间戳追加为查询参数。",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_public",
        "Public (explicit)",
        "公开（显式）",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_auth_public_desc",
        "Append no download authentication. Configure only when the bound domain exposes public objects; AsterDrive does not silently make private objects public.",
        "不追加下载鉴权。仅当绑定域名公开对象时配置；AsterDrive 不会静默将私有对象公开。",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_expire_time",
        "Anti-leech validity (seconds)",
        "防盗链有效期（秒）",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_sign_arg",
        "Signature parameter name",
        "签名参数名",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_time_arg",
        "Timestamp parameter name",
        "时间戳参数名",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_signing_algorithm",
        "Signing algorithm",
        "签名算法",
    ),
    aster_drive_storage::storage_connector_message!("cdn_signing_algorithm_md5", "MD5", "MD5",),
    aster_drive_storage::storage_connector_message!(
        "cdn_signing_algorithm_md5_desc",
        "Compute the anti-leech hash with MD5.",
        "使用 MD5 计算防盗链哈希。",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_signing_algorithm_sha256",
        "SHA-256",
        "SHA-256",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_signing_algorithm_sha256_desc",
        "Compute the anti-leech hash with SHA-256.",
        "使用 SHA-256 计算防盗链哈希。",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_primary_key",
        "CDN primary key",
        "CDN 主密钥",
    ),
    aster_drive_storage::storage_connector_message!(
        "cdn_backup_key",
        "CDN backup key",
        "CDN 备用密钥",
    ),
];
