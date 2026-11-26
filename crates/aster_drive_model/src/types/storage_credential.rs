//! OAuth 存储凭据与 Microsoft Graph 云端点类型。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Provider backing an OAuth-managed storage policy credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
#[serde(rename_all = "snake_case")]
pub enum StorageCredentialProvider {
    /// Microsoft Graph (OneDrive / SharePoint).
    #[sea_orm(string_value = "microsoft_graph")]
    MicrosoftGraph,
    /// Google Drive.
    #[sea_orm(string_value = "google_drive")]
    GoogleDrive,
}

impl StorageCredentialProvider {
    /// Returns the snake_case provider name shared by storage and API values.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MicrosoftGraph => "microsoft_graph",
            Self::GoogleDrive => "google_drive",
        }
    }

    /// Parses a provider name produced by [`StorageCredentialProvider::as_str`].
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
    /// Delegated OAuth tokens acting as the authorizing user.
    #[sea_orm(string_value = "oauth_delegated")]
    OauthDelegated,
    /// App-only OAuth tokens acting as the application identity.
    #[sea_orm(string_value = "oauth_app_only")]
    OauthAppOnly,
    /// Google service account key material.
    #[sea_orm(string_value = "service_account")]
    ServiceAccount,
}

impl StorageCredentialKind {
    /// Returns the snake_case credential kind name shared by storage and API values.
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
    /// Credential refreshes and authorizes normally.
    #[sea_orm(string_value = "authorized")]
    Authorized,
    /// Authorization expired or was rejected; the user must re-authorize.
    #[sea_orm(string_value = "reauth_required")]
    ReauthRequired,
    /// The grant no longer covers the scopes the policy needs.
    #[sea_orm(string_value = "permission_denied")]
    PermissionDenied,
    /// Authorization was explicitly revoked.
    #[sea_orm(string_value = "revoked")]
    Revoked,
    /// Credential is misconfigured or otherwise unusable.
    #[sea_orm(string_value = "invalid")]
    Invalid,
}

impl StorageCredentialStatus {
    /// Returns the snake_case status name shared by storage and API values.
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
    /// Flow started and awaits the provider callback.
    #[sea_orm(string_value = "pending")]
    Pending,
    /// Callback completed and the credential was stored.
    #[sea_orm(string_value = "consumed")]
    Consumed,
    /// Flow expired before the callback completed.
    #[sea_orm(string_value = "expired")]
    Expired,
    /// Flow was cancelled before the callback completed.
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
}

impl StorageAuthorizationFlowStatus {
    /// Returns the snake_case status name shared by storage and API values.
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
    /// Microsoft global public cloud.
    #[default]
    Global,
    /// Microsoft cloud operated by 21Vianet in China.
    China,
}

impl MicrosoftGraphCloud {
    /// Returns the Graph API base URL for this cloud.
    pub const fn graph_base_url(self) -> &'static str {
        match self {
            Self::Global => "https://graph.microsoft.com",
            Self::China => "https://microsoftgraph.chinacloudapi.cn",
        }
    }

    /// Returns the OAuth 2.0 login (authority) base URL for this cloud.
    pub const fn login_base_url(self) -> &'static str {
        match self {
            Self::Global => "https://login.microsoftonline.com",
            // Microsoft cloud docs historically reference both China login hosts.
            // Keep the active endpoint centralized so the Graph driver never
            // scatters national cloud URLs through request code.
            Self::China => "https://login.chinacloudapi.cn",
        }
    }

    /// Builds the OAuth 2.0 authorization endpoint URL for a validated tenant.
    pub fn authorization_endpoint(self, tenant: &str) -> Result<String, MicrosoftGraphTenantError> {
        Ok(format!(
            "{}/{}/oauth2/v2.0/authorize",
            self.login_base_url(),
            validate_microsoft_graph_tenant(tenant)?
        ))
    }

    /// Builds the OAuth 2.0 token endpoint URL for a validated tenant.
    pub fn token_endpoint(self, tenant: &str) -> Result<String, MicrosoftGraphTenantError> {
        Ok(format!(
            "{}/{}/oauth2/v2.0/token",
            self.login_base_url(),
            validate_microsoft_graph_tenant(tenant)?
        ))
    }
}

/// Returned when a tenant identifier cannot be embedded in an auth URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicrosoftGraphTenantError;

impl fmt::Display for MicrosoftGraphTenantError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            "Microsoft Graph tenant must be common, consumers, organizations, a tenant GUID, or a verified domain",
        )
    }
}

impl std::error::Error for MicrosoftGraphTenantError {}

/// Validates a Microsoft Graph tenant identifier: trims the input, defaults the
/// empty value to `common`, and accepts the well-known aliases, a tenant GUID,
/// or a verified domain name.
pub fn validate_microsoft_graph_tenant(tenant: &str) -> Result<&str, MicrosoftGraphTenantError> {
    let tenant = tenant.trim();
    let tenant = if tenant.is_empty() { "common" } else { tenant };
    if matches!(tenant, "common" | "consumers" | "organizations")
        || is_microsoft_tenant_guid(tenant)
        || is_verified_domain_name(tenant)
    {
        Ok(tenant)
    } else {
        Err(MicrosoftGraphTenantError)
    }
}

fn is_microsoft_tenant_guid(tenant: &str) -> bool {
    tenant.len() == 36
        && tenant.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

fn is_verified_domain_name(tenant: &str) -> bool {
    tenant.len() <= 253
        && tenant.contains('.')
        && tenant.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::MicrosoftGraphCloud;

    #[test]
    fn microsoft_graph_cloud_resolves_global_endpoints() {
        let cloud = MicrosoftGraphCloud::Global;

        assert_eq!(cloud.graph_base_url(), "https://graph.microsoft.com");
        assert_eq!(
            cloud.authorization_endpoint("organizations").unwrap(),
            "https://login.microsoftonline.com/organizations/oauth2/v2.0/authorize"
        );
        assert_eq!(
            cloud.token_endpoint("").unwrap(),
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
            cloud.authorization_endpoint("common").unwrap(),
            "https://login.chinacloudapi.cn/common/oauth2/v2.0/authorize"
        );
        assert_eq!(
            cloud
                .token_endpoint("11111111-2222-3333-4444-555555555555")
                .unwrap(),
            "https://login.chinacloudapi.cn/11111111-2222-3333-4444-555555555555/oauth2/v2.0/token"
        );
    }

    #[test]
    fn microsoft_graph_tenant_validation_accepts_supported_identifiers() {
        for tenant in [
            "common",
            "consumers",
            "organizations",
            "11111111-2222-3333-4444-555555555555",
            "contoso.onmicrosoft.com",
            "contoso.partner.onmschina.cn",
            " verified.example ",
        ] {
            assert!(
                super::validate_microsoft_graph_tenant(tenant).is_ok(),
                "{tenant}"
            );
        }
    }

    #[test]
    fn microsoft_graph_tenant_validation_rejects_endpoint_control_characters() {
        for tenant in [
            "common/../../evil",
            "common?redirect_uri=https://evil.example",
            "common#fragment",
            "//evil.example",
            "tenant-id",
            ".contoso.com",
            "contoso..com",
            "contoso.com.",
            "-contoso.com",
            "contoso_.com",
        ] {
            assert!(
                MicrosoftGraphCloud::Global
                    .authorization_endpoint(tenant)
                    .is_err(),
                "{tenant}"
            );
            assert!(
                MicrosoftGraphCloud::Global.token_endpoint(tenant).is_err(),
                "{tenant}"
            );
        }
    }
}
