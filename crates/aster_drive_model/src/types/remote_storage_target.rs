//! Transitional built-in driver kinds for follower-side remote storage targets.
//!
//! Remote storage targets are still limited to Local and S3 while their admin
//! form and wire contract are descriptor-driven only at the field level. This
//! domain type prevents that temporary limitation from leaking back into the
//! plugin-safe storage-policy connector model. Issue #461 tracks replacing it
//! with connector identity and connector-owned payloads.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize, Deserialize,
)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "lowercase")]
pub enum RemoteStorageTargetDriverKind {
    // TODO(remote-storage-target-0.7.0): remove this enum after all runtime
    // paths use ConnectorId directly.
    #[sea_orm(string_value = "local")]
    Local,
    #[sea_orm(string_value = "s3")]
    S3,
    #[sea_orm(string_value = "sftp")]
    Sftp,
    #[sea_orm(string_value = "tencent_cos")]
    #[serde(rename = "tencent_cos")]
    TencentCos,
    #[sea_orm(string_value = "alibaba_oss")]
    #[serde(rename = "alibaba_oss")]
    AlibabaOss,
    #[sea_orm(string_value = "qiniu")]
    Qiniu,
    #[sea_orm(string_value = "azure_blob")]
    AzureBlob,
    #[sea_orm(string_value = "huawei_obs")]
    HuaweiObs,
}

impl RemoteStorageTargetDriverKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Sftp => "sftp",
            Self::TencentCos => "tencent_cos",
            Self::AlibabaOss => "alibaba_oss",
            Self::Qiniu => "qiniu",
            Self::AzureBlob => "azure_blob",
            Self::HuaweiObs => "huawei_obs",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "s3" => Some(Self::S3),
            "sftp" => Some(Self::Sftp),
            "tencent_cos" => Some(Self::TencentCos),
            "alibaba_oss" => Some(Self::AlibabaOss),
            "qiniu" => Some(Self::Qiniu),
            "azure_blob" => Some(Self::AzureBlob),
            "huawei_obs" => Some(Self::HuaweiObs),
            _ => None,
        }
    }
}

impl std::str::FromStr for RemoteStorageTargetDriverKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(())
    }
}

impl AsRef<str> for RemoteStorageTargetDriverKind {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_current_remote_target_driver_kinds() {
        for (raw, expected) in [
            ("local", RemoteStorageTargetDriverKind::Local),
            ("s3", RemoteStorageTargetDriverKind::S3),
        ] {
            assert_eq!(raw.parse(), Ok(expected));
            assert_eq!(expected.as_str(), raw);
        }

        for unsupported in ["remote", "onedrive", "unknown_provider"] {
            assert!(
                unsupported
                    .parse::<RemoteStorageTargetDriverKind>()
                    .is_err()
            );
        }
    }
}
