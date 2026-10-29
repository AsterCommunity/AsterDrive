//! Product-owned encrypted credential lifecycle for the Google Drive connector.
//!
//! The mutex coalesces refreshes inside one provider. The writer-side revision
//! CAS protects separate providers and Primary instances: after losing a race,
//! always reload the authoritative row instead of publishing the stale token.
use super::{GoogleDriveConnector, GoogleDriveCredentialV1};
use crate::services::storage_policy::credential::{
    OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED, OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
    OAUTH_AUDIT_EVENT_REAUTH_REQUIRED, OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, write_storage_credential_oauth_audit,
};
use crate::{
    db::repository::storage_policy_connector_credential_repo as repo,
    errors::{AsterError, Result, storage_driver_error},
    services::storage_policy::credential::crypto,
    storage::drivers::google_drive::{GoogleDriveTokenProvider, client::http_client},
};
use aster_drive_model::types::StorageCredentialStatus;
use aster_drive_storage::{ConnectorId, StorageErrorKind};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Deserialize)]
pub(super) struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
}
impl TokenResponse {
    pub fn validate(&self) -> Result<()> {
        if self.access_token.is_empty()
            || self
                .access_token
                .bytes()
                .any(|byte| !byte.is_ascii_graphic())
            || self.expires_at(Utc::now()).is_none()
            || self
                .token_type
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("Bearer"))
        {
            return Err(storage_driver_error(
                StorageErrorKind::Transient,
                "Google OAuth returned invalid token metadata",
            ));
        }
        Ok(())
    }
    pub fn expires_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.expires_in
            .filter(|seconds| *seconds > 0)
            .and_then(chrono::Duration::try_seconds)
            .and_then(|duration| now.checked_add_signed(duration))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CredentialUse {
    Runtime,
    /// An explicit admin probe may retry after folder permissions/configuration
    /// were repaired. It must still reject revoked or reauthorization-required
    /// grants, and never re-enable the runtime before root validation succeeds.
    Validation,
}

pub(super) struct GoogleTokenProvider {
    // Runtime uses the authoritative encrypted row; cleanup uses an encrypted snapshot
    // decoded by the connector after the policy and its credential row have been removed.
    db: Option<sea_orm::DatabaseConnection>,
    key: String,
    policy_id: i64,
    cache: Mutex<GoogleDriveCredentialV1>,
    token_url: String,
    http: reqwest::Client,
    usage: CredentialUse,
}
impl std::fmt::Debug for GoogleTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleTokenProvider")
            .field("policy_id", &self.policy_id)
            .finish_non_exhaustive()
    }
}
impl GoogleTokenProvider {
    pub(super) fn new(
        db: Option<sea_orm::DatabaseConnection>,
        key: String,
        policy_id: i64,
        payload: GoogleDriveCredentialV1,
        usage: CredentialUse,
    ) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            db,
            key,
            policy_id,
            cache: Mutex::new(payload),
            token_url: super::GOOGLE_TOKEN.into(),
            http: http_client()?,
            usage,
        }))
    }
    async fn audit(&self, event: &str, result: &str) {
        if let Some(db) = &self.db {
            write_storage_credential_oauth_audit(
                db,
                0,
                StorageCredentialOauthAuditDetails {
                    event,
                    result,
                    policy_id: Some(self.policy_id),
                    connector_id: Some(GoogleDriveConnector::ID),
                    provider: Some(
                        aster_drive_model::types::StorageCredentialProvider::GoogleDrive,
                    ),
                    ..Default::default()
                },
            )
            .await;
        }
    }
    async fn resolve(&self) -> Result<String> {
        let mut cache = self.cache.lock().await;
        for _ in 0..3 {
            let row = if let Some(db) = &self.db {
                let row = repo::find_by_policy(db, self.policy_id)
                    .await?
                    .ok_or_else(|| {
                        storage_driver_error(
                            StorageErrorKind::Auth,
                            "Google Drive credential was removed",
                        )
                    })?;
                *cache = super::super::decode_typed_connector_credential(
                    &self.key,
                    &row,
                    &ConnectorId::declared(GoogleDriveConnector::ID),
                    1,
                )?;
                Some(row)
            } else {
                None
            };
            let auth = cache.authorization.as_ref().ok_or_else(|| {
                storage_driver_error(StorageErrorKind::Auth, "Google Drive is not authorized")
            })?;
            let may_probe = self.usage == CredentialUse::Validation
                && matches!(
                    auth.status,
                    StorageCredentialStatus::PermissionDenied | StorageCredentialStatus::Invalid
                );
            if auth.status != StorageCredentialStatus::Authorized && !may_probe {
                return Err(storage_driver_error(
                    match auth.status {
                        StorageCredentialStatus::Invalid => StorageErrorKind::Misconfigured,
                        StorageCredentialStatus::PermissionDenied => StorageErrorKind::Permission,
                        _ => StorageErrorKind::Auth,
                    },
                    "Google Drive authorization is not usable; check credential status",
                ));
            }
            if !auth.access_token.is_empty()
                && auth
                    .expires_at
                    .is_some_and(|s| s > Utc::now() + chrono::Duration::seconds(60))
            {
                return Ok(auth.access_token.clone());
            }
            let refresh = auth
                .refresh_token
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Auth,
                        "Google Drive refresh token is missing; reauthorize",
                    )
                })?;
            let response = self
                .http
                .post(&self.token_url)
                .timeout(std::time::Duration::from_secs(30))
                .form(&[
                    ("client_id", cache.application.client_id.as_str()),
                    ("client_secret", cache.application.client_secret.as_str()),
                    ("refresh_token", refresh),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
                .map_err(|_| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        "Google OAuth token endpoint is unavailable",
                    )
                })?;
            let status = response.status();
            if !status.is_success() {
                let failure = classify_token_failure(response).await;
                if let Some(next_status) = failure.status {
                    let mut failed = cache.clone();
                    let auth = failed.authorization.as_mut().ok_or_else(|| {
                        storage_driver_error(
                            StorageErrorKind::Auth,
                            "Google Drive authorization was removed",
                        )
                    })?;
                    auth.status = next_status;
                    auth.status_reason = Some(failure.reason.into());
                    if let (Some(db), Some(row)) = (&self.db, row.as_ref())
                        && !persist(db, &self.key, row, &failed).await?
                    {
                        continue;
                    }
                    *cache = failed;
                    self.audit(
                        if next_status == StorageCredentialStatus::ReauthRequired {
                            OAUTH_AUDIT_EVENT_REAUTH_REQUIRED
                        } else {
                            OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED
                        },
                        OAUTH_AUDIT_RESULT_FAILED,
                    )
                    .await;
                }
                return Err(storage_driver_error(failure.kind, failure.reason));
            }
            let token: TokenResponse = response.json().await.map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    "invalid Google OAuth token response",
                )
            })?;
            token.validate()?;
            let now = Utc::now();
            let mut updated = cache.clone();
            let auth = updated.authorization.as_mut().ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "Google Drive authorization was removed",
                )
            })?;
            auth.expires_at = token.expires_at(now);
            auth.access_token = token.access_token;
            if let Some(refresh) = token.refresh_token.filter(|s| !s.trim().is_empty()) {
                auth.refresh_token = Some(refresh);
            }
            if let Some(scope) = token.scope {
                auth.scopes = scope.split_whitespace().map(str::to_owned).collect();
            }
            auth.last_refreshed_at = Some(now);
            auth.status_reason = None;
            let access = auth.access_token.clone();
            if let (Some(db), Some(row)) = (&self.db, row.as_ref())
                && !persist(db, &self.key, row, &updated).await?
            {
                continue;
            }
            *cache = updated;
            self.audit(
                OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED,
                OAUTH_AUDIT_RESULT_SUCCESS,
            )
            .await;
            return Ok(access);
        }
        Err(storage_driver_error(
            StorageErrorKind::Transient,
            "Google Drive credential changed concurrently; retry",
        ))
    }
}

/// Only recognized OAuth failures justify changing durable credential state.
/// Google documents invalid_client as a bad application secret, while
/// invalid_grant requires a new user grant. A proxy's 403/404 or malformed body
/// says nothing about token validity. Provider descriptions may contain secrets
/// and are never copied into errors, status reasons, Debug or audit fields.
/// <https://developers.google.com/identity/protocols/oauth2/web-server#httprest_3>
pub(super) struct TokenFailure {
    pub(super) kind: StorageErrorKind,
    pub(super) status: Option<StorageCredentialStatus>,
    pub(super) reason: &'static str,
}

pub(super) async fn classify_token_failure(response: reqwest::Response) -> TokenFailure {
    let status = response.status();
    let code = response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| body["error"].as_str().map(str::to_owned));
    let (kind, next_status, reason) = match (status.as_u16(), code.as_deref()) {
        (429, _) => (
            StorageErrorKind::RateLimited,
            None,
            "Google OAuth token endpoint is rate limited",
        ),
        (408 | 500..=599, _) => (
            StorageErrorKind::Transient,
            None,
            "Google OAuth token endpoint is temporarily unavailable",
        ),
        (400 | 401 | 403, Some("invalid_grant")) => (
            StorageErrorKind::Auth,
            Some(StorageCredentialStatus::ReauthRequired),
            "Google authorization expired or was revoked; reauthorize",
        ),
        (400 | 401 | 403, Some("invalid_client" | "unauthorized_client")) => (
            StorageErrorKind::Misconfigured,
            Some(StorageCredentialStatus::Invalid),
            "Google OAuth client configuration is invalid; check client ID and secret",
        ),
        _ => (
            StorageErrorKind::Transient,
            None,
            "Google OAuth token endpoint returned an unrecognized failure",
        ),
    };
    TokenFailure {
        kind,
        status: next_status,
        reason,
    }
}
async fn persist(
    db: &sea_orm::DatabaseConnection,
    key: &str,
    row: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    payload: &GoogleDriveCredentialV1,
) -> Result<bool> {
    let plaintext = serde_json::to_string(payload)
        .map_err(|_| AsterError::internal_error("serialize Google Drive credential"))?;
    let ciphertext = crypto::encrypt_connector_credential(
        key,
        row.policy_id,
        GoogleDriveConnector::ID,
        1,
        &plaintext,
    )?;
    repo::update_if_revision(
        db,
        row.policy_id,
        GoogleDriveConnector::ID,
        1,
        row.revision,
        ciphertext,
    )
    .await
}
#[async_trait::async_trait]
impl GoogleDriveTokenProvider for GoogleTokenProvider {
    async fn access_token(&self) -> aster_drive_storage::Result<String> {
        self.resolve().await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests;
