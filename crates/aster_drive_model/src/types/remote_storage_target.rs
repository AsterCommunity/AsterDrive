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
    #[sea_orm(string_value = "local")]
    Local,
    #[sea_orm(string_value = "s3")]
    S3,
}

impl RemoteStorageTargetDriverKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "local" => Some(Self::Local),
            "s3" => Some(Self::S3),
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

        for unsupported in ["sftp", "remote", "onedrive", "tencent_cos", "azure_blob"] {
            assert!(
                unsupported
                    .parse::<RemoteStorageTargetDriverKind>()
                    .is_err()
            );
        }
    }
}
