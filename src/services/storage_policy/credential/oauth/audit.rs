use chrono::Utc;
use sea_orm::ActiveValue::Set;

use crate::db::repository::audit_log_repo;
use crate::entities::audit_log;
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use crate::types::{AuditAction, AuditEntityType, MicrosoftGraphCloud, StorageCredentialProvider};

pub(super) const OAUTH_AUDIT_ACTION_NAME: &str = "storage_credential_oauth";
pub(super) const OAUTH_AUDIT_DRIVER_TYPE: &str = "onedrive";
pub(super) const OAUTH_AUDIT_PROVIDER: StorageCredentialProvider =
    StorageCredentialProvider::MicrosoftGraph;
pub(super) const OAUTH_AUDIT_RESULT_SUCCESS: &str = "success";
pub(super) const OAUTH_AUDIT_RESULT_FAILED: &str = "failed";
pub(super) const OAUTH_AUDIT_RESULT_RECOVERED: &str = "recovered";
pub(super) const OAUTH_AUDIT_EVENT_AUTHORIZATION_STARTED: &str = "authorization_started";
pub(super) const OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED: &str = "authorization_completed";
pub(super) const OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED: &str = "authorization_failed";
pub(super) const OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED: &str = "credential_refreshed";
pub(super) const OAUTH_AUDIT_EVENT_REAUTH_REQUIRED: &str = "reauth_required";

#[derive(Clone, Debug, Default)]
pub(super) struct StorageCredentialOauthAuditDetails<'a> {
    pub(super) event: &'a str,
    pub(super) result: &'a str,
    pub(super) policy_id: Option<i64>,
    pub(super) cloud: Option<MicrosoftGraphCloud>,
    pub(super) tenant: Option<&'a str>,
    pub(super) reason: Option<&'a str>,
    pub(super) client_secret_configured: Option<bool>,
    pub(super) refresh_token_rotated: Option<bool>,
    pub(super) recovered_from_token_rotation: Option<bool>,
}

pub(super) fn storage_credential_oauth_audit_details(
    input: StorageCredentialOauthAuditDetails<'_>,
) -> serde_json::Value {
    let mut details = serde_json::json!({
        "action": OAUTH_AUDIT_ACTION_NAME,
        "driver_type": OAUTH_AUDIT_DRIVER_TYPE,
        "used_draft_values": false,
        "mutates_remote_state": false,
        "oauth_event": input.event,
        "provider": OAUTH_AUDIT_PROVIDER.as_str(),
        "result": input.result,
    });
    if let Some(policy_id) = input.policy_id {
        details["policy_id"] = serde_json::Value::from(policy_id);
    }
    if let Some(cloud) = input.cloud {
        details["cloud"] = serde_json::to_value(cloud).unwrap_or(serde_json::Value::Null);
    }
    if let Some(tenant) = input.tenant {
        details["tenant"] = serde_json::Value::String(tenant.to_string());
    }
    if let Some(reason) = input.reason {
        details["reason"] = serde_json::Value::String(reason.to_string());
    }
    if let Some(client_secret_configured) = input.client_secret_configured {
        details["client_secret_configured"] = serde_json::Value::Bool(client_secret_configured);
    }
    if let Some(refresh_token_rotated) = input.refresh_token_rotated {
        details["refresh_token_rotated"] = serde_json::Value::Bool(refresh_token_rotated);
    }
    if let Some(recovered_from_token_rotation) = input.recovered_from_token_rotation {
        details["recovered_from_token_rotation"] =
            serde_json::Value::Bool(recovered_from_token_rotation);
    }
    details
}

pub(super) async fn log_storage_credential_oauth_audit(
    state: &impl SharedRuntimeState,
    ctx: &AuditContext,
    details: StorageCredentialOauthAuditDetails<'_>,
) {
    let policy_id = details.policy_id;
    audit::log_with_details(
        state,
        ctx,
        audit::AuditAction::AdminTriggerStorageAction,
        audit::AuditEntityType::StoragePolicy,
        policy_id,
        None,
        || Some(storage_credential_oauth_audit_details(details)),
    )
    .await;
}

pub(super) async fn write_storage_credential_oauth_audit(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    details: StorageCredentialOauthAuditDetails<'_>,
) {
    let now = Utc::now();
    let policy_id = details.policy_id;
    let model = audit_log::ActiveModel {
        id: Default::default(),
        user_id: Set(user_id),
        action: Set(AuditAction::AdminTriggerStorageAction),
        entity_type: Set(AuditEntityType::StoragePolicy.as_str().to_string()),
        entity_id: Set(policy_id),
        entity_name: Set(None),
        details: Set(Some(
            storage_credential_oauth_audit_details(details).to_string(),
        )),
        ip_address: Set(None),
        user_agent: Set(None),
        created_at: Set(now),
    };
    if let Err(error) = audit_log_repo::create(db, model).await {
        tracing::warn!("failed to write storage credential OAuth audit log: {error}");
    }
}
