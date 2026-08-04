use chrono::Utc;

use crate::runtime::StorageConnectorRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use aster_drive_model::types::{AuditAction, AuditEntityType, StorageCredentialProvider};

pub(crate) const OAUTH_AUDIT_ACTION_NAME: &str = "storage_credential_oauth";
pub(crate) const OAUTH_AUDIT_RESULT_SUCCESS: &str = "success";
pub(crate) const OAUTH_AUDIT_RESULT_FAILED: &str = "failed";
pub(crate) const OAUTH_AUDIT_RESULT_RECOVERED: &str = "recovered";
pub(crate) const OAUTH_AUDIT_EVENT_AUTHORIZATION_STARTED: &str = "authorization_started";
pub(crate) const OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED: &str = "authorization_completed";
pub(crate) const OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED: &str = "authorization_failed";
pub(crate) const OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED: &str = "credential_refreshed";
pub(crate) const OAUTH_AUDIT_EVENT_REAUTH_REQUIRED: &str = "reauth_required";

#[derive(Clone, Debug, Default)]
pub(crate) struct StorageCredentialOauthAuditDetails<'a> {
    pub(crate) event: &'a str,
    pub(crate) result: &'a str,
    pub(crate) policy_id: Option<i64>,
    pub(crate) connector_id: Option<&'a str>,
    pub(crate) provider: Option<StorageCredentialProvider>,
    pub(crate) reason: Option<&'a str>,
    pub(crate) fields: Option<&'a serde_json::Map<String, serde_json::Value>>,
}

pub(crate) fn storage_credential_oauth_audit_details(
    input: StorageCredentialOauthAuditDetails<'_>,
) -> serde_json::Value {
    let mut details = serde_json::json!({
        "action": OAUTH_AUDIT_ACTION_NAME,
        "used_draft_values": false,
        "mutates_remote_state": false,
        "oauth_event": input.event,
        "result": input.result,
    });
    if let Some(policy_id) = input.policy_id {
        details["policy_id"] = serde_json::Value::from(policy_id);
    }
    if let Some(connector_id) = input.connector_id {
        details["connector_id"] = serde_json::Value::String(connector_id.to_string());
    }
    if let Some(provider) = input.provider {
        details["provider"] = serde_json::Value::String(provider.as_str().to_string());
    }
    if let Some(reason) = input.reason {
        details["reason"] = serde_json::Value::String(reason.to_string());
    }
    if let Some(fields) = input.fields {
        for (key, value) in fields {
            details[key] = value.clone();
        }
    }
    details
}

pub(crate) async fn log_storage_credential_oauth_audit(
    state: &impl StorageConnectorRuntimeState,
    ctx: &AuditContext,
    details: StorageCredentialOauthAuditDetails<'_>,
) {
    let policy_id = details.policy_id;
    audit::log_with_db_and_config(
        state.writer_db(),
        state.runtime_config(),
        audit::AuditLogInput {
            ctx,
            action: audit::AuditAction::AdminTriggerStorageAction,
            entity_type: audit::AuditEntityType::StoragePolicy,
            entity_id: policy_id,
            entity_name: None,
        },
        || Some(storage_credential_oauth_audit_details(details)),
    )
    .await;
}

pub(crate) async fn write_storage_credential_oauth_audit(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    details: StorageCredentialOauthAuditDetails<'_>,
) {
    let now = Utc::now();
    let policy_id = details.policy_id;
    let request = aster_forge_db::AuditLogCreate {
        user_id,
        action: AuditAction::AdminTriggerStorageAction.as_str().to_string(),
        entity_type: AuditEntityType::StoragePolicy.as_str().to_string(),
        entity_id: policy_id,
        entity_name: None,
        details: Some(storage_credential_oauth_audit_details(details).to_string()),
        ip_address: None,
        user_agent: None,
        created_at: now,
    };
    if let Err(error) = aster_forge_db::create_audit_log_row(db, request).await {
        tracing::warn!("failed to write storage credential OAuth audit log: {error}");
    }
}
