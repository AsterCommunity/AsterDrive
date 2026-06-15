//! OAuth-managed storage policy credential service.

pub(crate) mod crypto;
mod management;
mod oauth;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::entities::storage_policy_credential;
use crate::types::{
    MicrosoftGraphCloud, StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
};

pub use management::{
    StoragePolicyCredentialValidationResult, list_policy_credentials, validate_policy_credential,
};
pub use oauth::{
    StorageAuthorizationCallbackOutcome, StorageAuthorizationCallbackQuery,
    StorageAuthorizationStartInput, StorageAuthorizationStartResponse,
    finish_authorization_callback, start_authorization,
};

const FLOW_TTL_SECS: u64 = 300;
const DEFAULT_MICROSOFT_GRAPH_SCOPES: &str =
    "offline_access Files.ReadWrite.All Sites.ReadWrite.All";
const REDACTED_SECRET: &str = "***REDACTED***";

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StoragePolicyCredentialInfo {
    pub id: i64,
    pub policy_id: i64,
    pub provider: StorageCredentialProvider,
    pub credential_kind: StorageCredentialKind,
    pub account_label: Option<String>,
    pub subject: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: Vec<String>,
    pub status: StorageCredentialStatus,
    pub status_reason: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub authorized_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_refreshed_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_validated_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageCredentialProviderInfo {
    pub provider: StorageCredentialProvider,
    pub display_name: String,
    pub supported: bool,
    pub default_scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MicrosoftGraphAuthorizationContext {
    pub cloud: MicrosoftGraphCloud,
    pub tenant: String,
    pub client_id: String,
    pub client_secret_configured: bool,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MicrosoftGraphAuthorizationInput {
    pub cloud: Option<MicrosoftGraphCloud>,
    pub tenant: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scopes: Option<Vec<String>>,
}

impl From<storage_policy_credential::Model> for StoragePolicyCredentialInfo {
    fn from(model: storage_policy_credential::Model) -> Self {
        Self {
            id: model.id,
            policy_id: model.policy_id,
            provider: model.provider,
            credential_kind: model.credential_kind,
            account_label: model.account_label,
            subject: model.subject,
            tenant_id: model.tenant_id,
            scopes: parse_scopes_json(&model.scopes),
            status: model.status,
            status_reason: model.status_reason,
            expires_at: model.expires_at,
            authorized_at: model.authorized_at,
            last_refreshed_at: model.last_refreshed_at,
            last_validated_at: model.last_validated_at,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

pub fn list_supported_providers() -> Vec<StorageCredentialProviderInfo> {
    vec![
        StorageCredentialProviderInfo {
            provider: StorageCredentialProvider::MicrosoftGraph,
            display_name: "Microsoft Graph".to_string(),
            supported: true,
            default_scopes: normalize_scopes(None),
        },
        StorageCredentialProviderInfo {
            provider: StorageCredentialProvider::GoogleDrive,
            display_name: "Google Drive".to_string(),
            supported: false,
            default_scopes: Vec::new(),
        },
    ]
}

fn normalize_scopes(value: Option<Vec<String>>) -> Vec<String> {
    let input = value.unwrap_or_else(|| {
        DEFAULT_MICROSOFT_GRAPH_SCOPES
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect()
    });
    let mut scopes = Vec::new();
    for scope in input {
        let scope = scope.trim();
        if !scope.is_empty() && !scopes.iter().any(|existing| existing == scope) {
            scopes.push(scope.to_string());
        }
    }
    if scopes.is_empty() {
        DEFAULT_MICROSOFT_GRAPH_SCOPES
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect()
    } else {
        scopes
    }
}

fn scopes_to_json(scopes: &[String]) -> crate::errors::Result<String> {
    serde_json::to_string(scopes).map_err(|err| {
        crate::errors::AsterError::internal_error(format!(
            "failed to serialize storage credential scopes: {err}"
        ))
    })
}

fn parse_scopes_json(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| {
        value
            .split_whitespace()
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_required_string(
    value: &str,
    field: &str,
    max_len: usize,
) -> crate::errors::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(crate::errors::AsterError::validation_error(format!(
            "{field} is required"
        )));
    }
    if value.len() > max_len {
        return Err(crate::errors::AsterError::validation_error(format!(
            "{field} must be at most {max_len} bytes"
        )));
    }
    Ok(value.to_string())
}
