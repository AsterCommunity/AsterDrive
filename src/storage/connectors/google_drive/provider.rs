use super::{GoogleDriveConnector, GoogleDriveCredentialV1};
use crate::services::storage_policy::credential::{
    OAUTH_AUDIT_EVENT_CREDENTIAL_REFRESHED, OAUTH_AUDIT_EVENT_REAUTH_REQUIRED,
    OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_SUCCESS, StorageCredentialOauthAuditDetails,
    write_storage_credential_oauth_audit,
};
use crate::{
    db::repository::storage_policy_connector_credential_repo as repo,
    errors::{AsterError, Result, storage_driver_error},
    services::storage_policy::credential::crypto,
    storage::drivers::google_drive::{GoogleDriveTokenProvider, http_client},
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
        if self.access_token.trim().is_empty()
            || self.expires_in.is_none_or(|s| s <= 0 || s > 86400)
            || self
                .token_type
                .as_deref()
                .is_none_or(|s| !s.eq_ignore_ascii_case("Bearer"))
        {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Google OAuth returned invalid token metadata",
            ));
        }
        Ok(())
    }
    pub fn expires_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.expires_in
            .and_then(|s| now.checked_add_signed(chrono::Duration::seconds(s)))
    }
}

pub(super) struct GoogleTokenProvider {
    // Runtime uses the authoritative encrypted row; cleanup uses an encrypted snapshot
    // decoded by the connector after the policy and its credential row have been removed.
    db: Option<sea_orm::DatabaseConnection>,
    key: String,
    policy_id: i64,
    cache: Mutex<GoogleDriveCredentialV1>,
    token_url: String,
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
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            key,
            policy_id,
            cache: Mutex::new(payload),
            token_url: super::GOOGLE_TOKEN.into(),
        })
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
            if auth.status != StorageCredentialStatus::Authorized {
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    "Google Drive requires administrator reauthorization",
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
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Auth,
                        "Google Drive refresh token is missing; reauthorize",
                    )
                })?;
            let response = http_client()
                .post(&self.token_url)
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
                let kind = match status.as_u16() {
                    429 => StorageErrorKind::RateLimited,
                    408 | 500..=599 => StorageErrorKind::Transient,
                    _ => StorageErrorKind::Auth,
                };
                if kind == StorageErrorKind::Auth {
                    let mut failed = cache.clone();
                    let auth = failed.authorization.as_mut().expect("authorized payload");
                    auth.status = StorageCredentialStatus::ReauthRequired;
                    auth.status_reason =
                        Some("Google rejected the refresh token; reauthorize".into());
                    if let (Some(db), Some(row)) = (&self.db, row.as_ref()) {
                        if !persist(db, &self.key, row, &failed).await? {
                            continue;
                        }
                    }
                    *cache = failed;
                    self.audit(OAUTH_AUDIT_EVENT_REAUTH_REQUIRED, OAUTH_AUDIT_RESULT_FAILED)
                        .await;
                }
                return Err(storage_driver_error(
                    kind,
                    "Google OAuth token refresh failed",
                ));
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
            let auth = updated.authorization.as_mut().expect("authorized payload");
            auth.expires_at = token.expires_at(now);
            auth.access_token = token.access_token;
            if let Some(refresh) = token.refresh_token.filter(|s| !s.is_empty()) {
                auth.refresh_token = Some(refresh);
            }
            if let Some(scope) = token.scope {
                auth.scopes = scope.split_whitespace().map(str::to_owned).collect();
            }
            auth.last_refreshed_at = Some(now);
            auth.status_reason = None;
            let access = auth.access_token.clone();
            if let (Some(db), Some(row)) = (&self.db, row.as_ref()) {
                if !persist(db, &self.key, row, &updated).await? {
                    continue;
                }
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
