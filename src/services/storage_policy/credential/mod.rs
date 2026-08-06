//! Connector credential orchestration and startup migration.
//!
//! Provider payloads, authorization protocol handling, and refresh state are
//! connector-owned. This module keeps only cross-connector persistence,
//! callback transaction boundaries, registry reloads, and audit dispatch.

pub(crate) mod crypto;
mod management;
mod migration;
mod oauth;

pub use management::{
    StoragePolicyCredentialValidationResult, list_policy_credentials, validate_policy_credential,
};
pub(crate) use oauth::audit::{
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
    OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_RECOVERED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, write_storage_credential_oauth_audit,
};
pub(crate) use oauth::finish_authorization_callback;
pub use oauth::{
    StorageAuthorizationCallbackOutcome, StorageAuthorizationCallbackQuery,
    StorageAuthorizationStartResponse, start_authorization,
};

pub(crate) use migration::migrate_legacy_storage_credentials;
