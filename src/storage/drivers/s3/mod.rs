//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#s3
//! 存储驱动实现：`s3`。

mod error;
mod list;
mod multipart;
pub(crate) mod presigned;
mod storage_driver;
mod stream_upload;
#[cfg(test)]
mod tests;

use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Region, timeout::TimeoutConfig};
use std::time::Duration;

use super::s3_config::normalize_s3_endpoint_and_bucket;
use aster_drive_storage::Result;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_drive_storage::object_key;

/// 基于 AWS SDK client 的共享 S3 驱动，供 S3 形态的各厂商 driver 复用。
pub struct S3Driver {
    client: Client,
    bucket: String,
    base_path: String,
}

/// [`S3Driver`] 的连接配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3DriverConfig {
    /// S3 endpoint URL；留空时使用 `region` 对应的 AWS 默认 endpoint。
    pub endpoint: String,
    /// 所有对象操作寻址的 bucket 名称。
    pub bucket: String,
    /// 拼接在每条 driver 相对路径之前的对象 key 前缀。
    pub base_path: String,
    /// SigV4 region 字符串，如 `us-east-1` 或厂商特定取值。
    pub region: String,
    /// 默认寻址风格；`true` 为 path-style URL。设置时可被
    /// [`S3DriverOptions::force_path_style`] 覆盖。
    pub path_style: bool,
    /// SDK 请求的 TCP 连接超时。
    pub connect_timeout: Duration,
    /// 等待 SDK 响应字节的空闲读取超时。
    pub read_timeout: Duration,
    /// 限制单次 SDK API 调用整体耗时。
    pub operation_timeout: Duration,
}

/// [`S3Driver`] 的静态 secret-key 凭据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3StaticCredentials {
    /// secret-key 访问密钥 ID。
    pub access_key: String,
    /// secret-key secret 访问密钥。
    pub secret_key: String,
}

/// [`S3Driver`] 的 SDK 寻址风格覆盖项。
#[derive(Debug, Clone, Copy, Default)]
pub struct S3DriverOptions {
    /// 显式寻址选择；`None` 时回落到 [`S3DriverConfig::path_style`]。
    pub force_path_style: Option<bool>,
}

impl S3DriverOptions {
    /// 强制 path-style 寻址（`endpoint/bucket/key`）。
    pub const fn path_style() -> Self {
        Self {
            force_path_style: Some(true),
        }
    }

    /// 强制 virtual-hosted 寻址（`bucket.endpoint/key`）。
    pub const fn virtual_hosted_style() -> Self {
        Self {
            force_path_style: Some(false),
        }
    }
}

impl S3Driver {
    /// 校验 endpoint/bucket 形态与凭据非空。
    pub fn validate_config(
        config: &S3DriverConfig,
        credentials: &S3StaticCredentials,
    ) -> Result<()> {
        normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(Self::rewrap_s3_config_error)?;
        if credentials.access_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "access_key cannot be empty",
            ));
        }
        if credentials.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "secret_key cannot be empty",
            ));
        }
        Ok(())
    }

    /// 构建 SDK client；`configure` 允许厂商包装在配置定稿前注入自定义签名行为
    /// （OSS V4、OBS、COS）。
    pub fn new<F>(
        driver_config: S3DriverConfig,
        credentials: S3StaticCredentials,
        driver_options: S3DriverOptions,
        configure: F,
    ) -> Result<Self>
    where
        F: FnOnce(aws_sdk_s3::config::Builder) -> aws_sdk_s3::config::Builder,
    {
        Self::validate_config(&driver_config, &credentials)?;
        let normalized =
            normalize_s3_endpoint_and_bucket(&driver_config.endpoint, &driver_config.bucket)
                .map_err(Self::rewrap_s3_config_error)?;

        let credentials = Credentials::new(
            credentials.access_key,
            credentials.secret_key,
            None,
            None,
            "aster-s3-driver",
        );

        let timeout_config = TimeoutConfig::builder()
            .connect_timeout(driver_config.connect_timeout)
            .read_timeout(driver_config.read_timeout)
            .operation_timeout(driver_config.operation_timeout)
            .build();
        let force_path_style = driver_options
            .force_path_style
            // Provider wrappers such as Tencent COS may override addressing
            // style explicitly; plain S3 policies read the persisted option.
            .unwrap_or(driver_config.path_style);

        let mut config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(driver_config.region))
            .credentials_provider(credentials)
            .timeout_config(timeout_config)
            .force_path_style(force_path_style);

        if !normalized.endpoint.is_empty() {
            config_builder = config_builder.endpoint_url(&normalized.endpoint);
        }

        let config = configure(config_builder).build();
        let client = Client::from_conf(config);

        Ok(Self {
            client,
            bucket: normalized.bucket,
            base_path: driver_config.base_path,
        })
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn relative_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, key)
    }

    fn normalize_multipart_etag(etag: &str) -> String {
        let etag = etag.trim();
        if etag.starts_with('"') && etag.ends_with('"') && etag.len() >= 2 {
            etag.to_string()
        } else {
            format!("\"{etag}\"")
        }
    }
}
