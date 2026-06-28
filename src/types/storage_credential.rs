use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Provider backing an OAuth-managed storage policy credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum StorageCredentialProvider {
    #[sea_orm(string_value = "microsoft_graph")]
    MicrosoftGraph,
    #[sea_orm(string_value = "google_drive")]
    GoogleDrive,
}

impl StorageCredentialProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MicrosoftGraph => "microsoft_graph",
            Self::GoogleDrive => "google_drive",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "microsoft_graph" => Some(Self::MicrosoftGraph),
            "google_drive" => Some(Self::GoogleDrive),
            _ => None,
        }
    }
}

impl std::str::FromStr for StorageCredentialProvider {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(())
    }
}

impl AsRef<str> for StorageCredentialProvider {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Authentication material shape for a storage policy credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum StorageCredentialKind {
    #[sea_orm(string_value = "oauth_delegated")]
    OauthDelegated,
    #[sea_orm(string_value = "oauth_app_only")]
    OauthAppOnly,
    #[sea_orm(string_value = "service_account")]
    ServiceAccount,
}

impl StorageCredentialKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OauthDelegated => "oauth_delegated",
            Self::OauthAppOnly => "oauth_app_only",
            Self::ServiceAccount => "service_account",
        }
    }
}

/// Current usability state of a stored storage policy credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum StorageCredentialStatus {
    #[sea_orm(string_value = "authorized")]
    Authorized,
    #[sea_orm(string_value = "reauth_required")]
    ReauthRequired,
    #[sea_orm(string_value = "permission_denied")]
    PermissionDenied,
    #[sea_orm(string_value = "revoked")]
    Revoked,
    #[sea_orm(string_value = "invalid")]
    Invalid,
}

impl StorageCredentialStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Authorized => "authorized",
            Self::ReauthRequired => "reauth_required",
            Self::PermissionDenied => "permission_denied",
            Self::Revoked => "revoked",
            Self::Invalid => "invalid",
        }
    }
}

/// Lifecycle state for a temporary storage authorization flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum StorageAuthorizationFlowStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "consumed")]
    Consumed,
    #[sea_orm(string_value = "expired")]
    Expired,
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
}

impl StorageAuthorizationFlowStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Microsoft Graph cloud deployment for OneDrive / SharePoint storage backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum MicrosoftGraphCloud {
    #[default]
    Global,
    China,
}

impl MicrosoftGraphCloud {
    pub const fn graph_base_url(self) -> &'static str {
        match self {
            Self::Global => "https://graph.microsoft.com",
            Self::China => "https://microsoftgraph.chinacloudapi.cn",
        }
    }

    pub const fn login_base_url(self) -> &'static str {
        match self {
            Self::Global => "https://login.microsoftonline.com",
            // Microsoft cloud docs historically reference both China login hosts.
            // Keep the active endpoint centralized so the Graph driver never
            // scatters national cloud URLs through request code.
            Self::China => "https://login.chinacloudapi.cn",
        }
    }

    pub fn authorization_endpoint(self, tenant: &str) -> String {
        format!(
            "{}/{}/oauth2/v2.0/authorize",
            self.login_base_url(),
            normalize_tenant_segment(tenant)
        )
    }

    pub fn token_endpoint(self, tenant: &str) -> String {
        format!(
            "{}/{}/oauth2/v2.0/token",
            self.login_base_url(),
            normalize_tenant_segment(tenant)
        )
    }
}

fn normalize_tenant_segment(tenant: &str) -> &str {
    let tenant = tenant.trim();
    if tenant.is_empty() { "common" } else { tenant }
}

#[cfg(test)]
mod tests {
    use super::MicrosoftGraphCloud;

    #[test]
    fn microsoft_graph_cloud_resolves_global_endpoints() {
        let cloud = MicrosoftGraphCloud::Global;

        assert_eq!(cloud.graph_base_url(), "https://graph.microsoft.com");
        assert_eq!(
            cloud.authorization_endpoint("organizations"),
            "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize"
        );
        assert_eq!(
            cloud.token_endpoint(""),
            "https://login.microsoftonline.com/common/oauth2/v2.0/token"
        );
    }

    #[test]
    fn microsoft_graph_cloud_resolves_china_endpoints() {
        let cloud = MicrosoftGraphCloud::China;

        assert_eq!(
            cloud.graph_base_url(),
            "https://microsoftgraph.chinacloudapi.cn"
        );
        assert_eq!(
            cloud.authorization_endpoint("common"),
            "https://login.chinacloudapi.cn/common/oauth2/v2.0/authorize"
        );
        assert_eq!(
            cloud.token_endpoint("tenant-id"),
            "https://login.chinacloudapi.cn/tenant-id/oauth2/v2.0/token"
        );
    }
}
