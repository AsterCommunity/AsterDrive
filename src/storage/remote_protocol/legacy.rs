//! Legacy 0.5.0 remote-target request shapes.
//!
//! This module is intentionally outside the V6 wire models. It exists only for
//! startup/test conversion and is scheduled for removal in 0.7.0.

use super::models::RemoteCreateStorageTargetRequest;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CreateRequest {
    Local(LocalRequest),
    S3(S3Request),
    Sftp(ProviderRequest),
    TencentCos(ProviderRequest),
    AlibabaOss(ProviderRequest),
    Qiniu(ProviderRequest),
    AzureBlob(ProviderRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRequest {
    pub name: String,
    pub base_path: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct S3Request {
    pub name: String,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderRequest {
    pub name: String,
    pub endpoint: String,
    #[serde(default)]
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default)]
    pub base_path: String,
    #[serde(default)]
    pub is_default: bool,
}

// Legacy fixture aliases. Kept inside this module so production V6 models do
// not expose the flattened request contract.
pub type RemoteCreateLocalStorageTargetRequest = LocalRequest;
pub type RemoteCreateS3StorageTargetRequest = S3Request;
pub type RemoteCreateProviderStorageTargetRequest = ProviderRequest;

impl fmt::Debug for S3Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyS3Request")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("base_path", &self.base_path)
            .field("is_default", &self.is_default)
            .finish()
    }
}

impl fmt::Debug for ProviderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyProviderRequest")
            .field("name", &self.name)
            .field("endpoint", &self.endpoint)
            .field("bucket", &self.bucket)
            .field("access_key", &"<redacted>")
            .field("secret_key", &"<redacted>")
            .field("base_path", &self.base_path)
            .field("is_default", &self.is_default)
            .finish()
    }
}

pub fn local(input: LocalRequest) -> RemoteCreateStorageTargetRequest {
    convert(
        "local",
        input.name,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        input.base_path,
        input.is_default,
    )
}

pub fn s3(input: S3Request) -> RemoteCreateStorageTargetRequest {
    convert(
        "s3",
        input.name,
        input.endpoint,
        input.bucket,
        input.access_key,
        input.secret_key,
        input.base_path,
        input.is_default,
    )
}

pub fn sftp(input: ProviderRequest) -> RemoteCreateStorageTargetRequest {
    convert(
        "sftp",
        input.name,
        input.endpoint,
        input.bucket,
        input.access_key,
        input.secret_key,
        input.base_path,
        input.is_default,
    )
}

fn convert(
    kind: &str,
    name: String,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
    is_default: bool,
) -> RemoteCreateStorageTargetRequest {
    let connector_id = format!("asterdrive.storage.{kind}");
    let values = if kind == "local" {
        serde_json::json!({"base_path": base_path})
    } else {
        serde_json::json!({"endpoint": endpoint, "bucket": bucket, "base_path": base_path})
    };
    RemoteCreateStorageTargetRequest {
        name,
        connector_config: aster_drive_storage::ConnectorConfigEnvelope::new(
            aster_drive_storage::ConnectorId::declared(connector_id),
            1,
            values,
        ),
        credential: Some(serde_json::json!({"access_key": access_key, "secret_key": secret_key})),
        is_default,
    }
}
