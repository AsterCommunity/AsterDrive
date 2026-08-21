use aster_drive_storage::StorageConnectorLocalizationMessage;

pub(super) const MESSAGES: &[StorageConnectorLocalizationMessage<'static>] = &[
    aster_drive_storage::storage_connector_message!(
        "driver_type_huawei_obs",
        "Huawei Cloud OBS",
        "华为云 OBS",
    ),
    aster_drive_storage::storage_connector_message!(
        "huawei_obs_endpoint_hint",
        "For virtual-hosted addressing, use the regional OBS endpoint such as https://obs.cn-north-4.myhuaweicloud.com. For a bound custom domain, enter that domain and select custom-domain addressing.",
        "虚拟托管模式请填写区域 OBS endpoint，例如 https://obs.cn-north-4.myhuaweicloud.com；使用已绑定的自定义域名时，请填写该域名并选择自定义域名访问模式。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_access_key_id",
        "Huawei Cloud Access Key ID",
        "华为云 Access Key ID",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_addressing_mode",
        "OBS addressing mode",
        "OBS 访问模式",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_addressing_mode_custom_domain",
        "Custom domain",
        "自定义域名",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_addressing_mode_custom_domain_desc",
        "Send requests directly to an OBS-bound custom hostname. The custom hostname is used in the canonical signed resource, and the bucket is not added to the request hostname or path.",
        "请求直接发送到已绑定 OBS 的自定义域名；签名规范资源使用该自定义主机名，bucket 不会再拼入请求主机名或路径。",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_addressing_mode_virtual_hosted",
        "Virtual-hosted OBS endpoint",
        "OBS 虚拟托管 endpoint",
    ),
    aster_drive_storage::storage_connector_message!(
        "obs_addressing_mode_virtual_hosted_desc",
        "Use bucket.obs.<region>.myhuaweicloud.com requests generated from the regional OBS endpoint.",
        "根据区域 OBS endpoint 生成 bucket.obs.<region>.myhuaweicloud.com 形式的请求。",
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
];
